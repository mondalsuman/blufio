// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Token counting abstractions for accurate LLM token estimation.
//!
//! Provides the [`TokenCounter`] trait, a [`HeuristicCounter`] fallback,
//! and a [`TokenizerCache`] for caching provider-specific counters by model ID.
//!
//! # Architecture
//!
//! - [`TokenCounter`] -- async trait for counting tokens in text
//! - [`HeuristicCounter`] -- O(n) char-scanning fallback (chars/3.5 default, chars/2.0 for CJK)
//! - [`TokenizerCache`] -- thread-safe cache mapping model IDs to `Arc<dyn TokenCounter>`
//! - [`TokenizerMode`] -- controls whether real tokenizers or heuristics are used
//!
//! - [`TiktokenCounter`] -- tiktoken-rs BPE tokenizer for OpenAI models
//! - [`HuggingFaceCounter`] -- HuggingFace tokenizer for Claude models (bundled vocabulary)
//! - [`count_with_fallback`] -- graceful degradation to heuristic on tokenizer failure
//!
//! Plan 03 integrates TokenizerCache into the context engine.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use async_trait::async_trait;
use tokenizers::Tokenizer;

use crate::error::BlufioError;

// ---------------------------------------------------------------------------
// TokenCounter trait
// ---------------------------------------------------------------------------

/// Async trait for counting tokens in text.
///
/// Implementations must be `Send + Sync` to allow sharing across tasks.
/// The `count_tokens` method is async to accommodate tokenizers that may
/// need blocking I/O (loaded via `spawn_blocking`).
#[async_trait]
pub trait TokenCounter: Send + Sync {
    /// Count the number of tokens in `text`.
    async fn count_tokens(&self, text: &str) -> Result<usize, BlufioError>;

    /// Human-readable name of this counter (for logging/debugging).
    fn counter_name(&self) -> &str;
}

// ---------------------------------------------------------------------------
// TokenizerMode
// ---------------------------------------------------------------------------

/// Controls whether real tokenizers or heuristics are used.
///
/// Set at startup via `PerformanceConfig.tokenizer_mode`. Not switchable at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenizerMode {
    /// Use real provider-specific tokenizers (tiktoken-rs, HuggingFace tokenizers).
    Accurate,
    /// Use character-count heuristic for all models (faster, less accurate).
    Fast,
}

// ---------------------------------------------------------------------------
// TiktokenEncoding
// ---------------------------------------------------------------------------

/// Tiktoken BPE encoding variants used by OpenAI models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TiktokenEncoding {
    /// Used by GPT-4o, GPT-4.1, GPT-5, o1, o3, o4 models.
    O200kBase,
    /// Used by GPT-4, GPT-3.5-turbo, older models.
    Cl100kBase,
}

// ---------------------------------------------------------------------------
// HeuristicCounter
// ---------------------------------------------------------------------------

/// Character-count heuristic token counter.
///
/// Estimates tokens as `ceil(chars / chars_per_token)`. When CJK characters
/// exceed the `cjk_threshold` fraction of total characters, uses the
/// `cjk_chars_per_token` ratio instead.
///
/// This is the fallback for Gemini, Ollama, and all models in `Fast` mode.
#[derive(Debug, Clone)]
pub struct HeuristicCounter {
    /// Characters per token for Latin/ASCII text (default: 3.5).
    pub chars_per_token: f64,
    /// Characters per token for CJK-heavy text (default: 2.0).
    pub cjk_chars_per_token: f64,
    /// Fraction of CJK characters above which `cjk_chars_per_token` is used (default: 0.30).
    pub cjk_threshold: f64,
}

impl Default for HeuristicCounter {
    fn default() -> Self {
        Self {
            chars_per_token: 3.5,
            cjk_chars_per_token: 2.0,
            cjk_threshold: 0.30,
        }
    }
}

#[async_trait]
impl TokenCounter for HeuristicCounter {
    async fn count_tokens(&self, text: &str) -> Result<usize, BlufioError> {
        let char_count = text.chars().count();
        if char_count == 0 {
            return Ok(0);
        }

        let cjk_count = text.chars().filter(|c| is_cjk(*c)).count();
        let cjk_fraction = cjk_count as f64 / char_count as f64;

        let ratio = if cjk_fraction > self.cjk_threshold {
            self.cjk_chars_per_token
        } else {
            self.chars_per_token
        };

        Ok((char_count as f64 / ratio).ceil() as usize)
    }

    fn counter_name(&self) -> &str {
        "heuristic"
    }
}

// ---------------------------------------------------------------------------
// TiktokenCounter (OpenAI)
// ---------------------------------------------------------------------------

/// Token counter using tiktoken-rs BPE encodings for OpenAI models.
///
/// Uses [`o200k_base_singleton`] for GPT-4o/4.1/5 and o1/o3/o4 models,
/// [`cl100k_base_singleton`] for GPT-4/3.5-turbo and older models.
///
/// The singletons are initialized once (lazy) and shared across all calls
/// with zero per-call allocation. The synchronous `encode_with_special_tokens`
/// call is wrapped in [`tokio::task::spawn_blocking`] to avoid blocking the
/// tokio runtime.
#[derive(Debug, Clone)]
pub struct TiktokenCounter {
    encoding: TiktokenEncoding,
}

impl TiktokenCounter {
    /// Create a `TiktokenCounter` with the encoding appropriate for the given model.
    pub fn for_model(model_id: &str) -> Self {
        Self {
            encoding: tiktoken_encoding_for_model(model_id),
        }
    }
}

#[async_trait]
impl TokenCounter for TiktokenCounter {
    async fn count_tokens(&self, text: &str) -> Result<usize, BlufioError> {
        let text = text.to_string();
        let encoding = self.encoding;

        tokio::task::spawn_blocking(move || {
            let tokens = match encoding {
                TiktokenEncoding::O200kBase => {
                    let bpe = tiktoken_rs::o200k_base_singleton();
                    bpe.encode_with_special_tokens(&text)
                }
                TiktokenEncoding::Cl100kBase => {
                    let bpe = tiktoken_rs::cl100k_base_singleton();
                    bpe.encode_with_special_tokens(&text)
                }
            };
            tokens.len()
        })
        .await
        .map_err(|e| BlufioError::Internal(format!("tiktoken spawn_blocking join: {e}")))
    }

    fn counter_name(&self) -> &str {
        match self.encoding {
            TiktokenEncoding::O200kBase => "tiktoken-o200k",
            TiktokenEncoding::Cl100kBase => "tiktoken-cl100k",
        }
    }
}

// ---------------------------------------------------------------------------
// HuggingFaceCounter (Claude)
// ---------------------------------------------------------------------------

/// Bundled Claude tokenizer vocabulary (Xenova/claude-tokenizer).
///
/// Embedded at compile time via `include_bytes!` to preserve the single-binary
/// deployment constraint. ~1.77 MB.
const CLAUDE_TOKENIZER_BYTES: &[u8] = include_bytes!("../assets/claude-tokenizer.json");

/// Global singleton for the Claude tokenizer instance.
///
/// Initialized once on first use via [`OnceLock`]. The `Arc` allows cheap
/// cloning into `spawn_blocking` closures.
static CLAUDE_TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();

/// Get (or initialize) the Claude tokenizer singleton.
fn get_claude_tokenizer() -> Result<Arc<Tokenizer>, BlufioError> {
    if let Some(tok) = CLAUDE_TOKENIZER.get() {
        return Ok(Arc::clone(tok));
    }

    let tok = Tokenizer::from_bytes(CLAUDE_TOKENIZER_BYTES)
        .map_err(|e| BlufioError::Internal(format!("Claude tokenizer init: {e}")))?;
    let arc = Arc::new(tok);

    // If another thread beat us, that's fine -- we just discard our instance.
    let _ = CLAUDE_TOKENIZER.set(Arc::clone(&arc));

    // Return whichever was stored (ours or the other thread's).
    Ok(CLAUDE_TOKENIZER.get().map(Arc::clone).unwrap_or(arc))
}

/// Token counter using the HuggingFace `tokenizers` crate with the bundled
/// Claude vocabulary.
///
/// The tokenizer is loaded once (lazy) via [`OnceLock`] and reused across
/// all calls. The synchronous `encode` call is wrapped in
/// [`tokio::task::spawn_blocking`] to avoid blocking the tokio runtime.
#[derive(Debug, Clone, Copy)]
pub struct HuggingFaceCounter;

#[async_trait]
impl TokenCounter for HuggingFaceCounter {
    async fn count_tokens(&self, text: &str) -> Result<usize, BlufioError> {
        let tokenizer = get_claude_tokenizer()?;
        let text = text.to_string();

        tokio::task::spawn_blocking(move || {
            let encoding = tokenizer
                .encode(text.as_str(), false)
                .map_err(|e| BlufioError::Internal(format!("Claude tokenizer encode: {e}")))?;
            Ok(encoding.get_ids().len())
        })
        .await
        .map_err(|e| BlufioError::Internal(format!("HuggingFace spawn_blocking join: {e}")))?
    }

    fn counter_name(&self) -> &str {
        "hf-claude"
    }
}

// ---------------------------------------------------------------------------
// count_with_fallback
// ---------------------------------------------------------------------------

/// Count tokens with graceful fallback to heuristic on tokenizer failure.
///
/// If the primary counter fails (e.g., tokenizer initialization error,
/// encoding error), logs a warning and falls back to [`HeuristicCounter`].
/// If even the heuristic fails (should not happen), returns `text.len() / 4`.
pub async fn count_with_fallback(primary: &dyn TokenCounter, text: &str) -> usize {
    match primary.count_tokens(text).await {
        Ok(count) => count,
        Err(e) => {
            tracing::warn!(
                error = %e,
                counter = primary.counter_name(),
                "tokenizer failed, using heuristic fallback"
            );
            HeuristicCounter::default()
                .count_tokens(text)
                .await
                .unwrap_or(text.len() / 4)
        }
    }
}

// ---------------------------------------------------------------------------
// CJK detection
// ---------------------------------------------------------------------------

/// Returns `true` if `c` is a CJK ideograph.
///
/// Covers the following Unicode blocks:
/// - CJK Unified Ideographs (U+4E00..U+9FFF)
/// - CJK Unified Ideographs Extension A (U+3400..U+4DBF)
/// - CJK Compatibility Ideographs (U+F900..U+FAFF)
/// - CJK Unified Ideographs Extension B+ (U+2F800..U+2FA1F)
pub(crate) fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}'
        | '\u{3400}'..='\u{4DBF}'
        | '\u{F900}'..='\u{FAFF}'
        | '\u{2F800}'..='\u{2FA1F}'
    )
}

// ---------------------------------------------------------------------------
// Model identification helpers
// ---------------------------------------------------------------------------

/// Returns `true` if the model ID belongs to OpenAI.
///
/// Matches: gpt-*, o1*, o3*, o4*, text-embedding-*, chatgpt-*
pub(crate) fn is_openai_model(model: &str) -> bool {
    model.starts_with("gpt-")
        || model.starts_with("o1")
        || model.starts_with("o3")
        || model.starts_with("o4")
        || model.starts_with("text-embedding-")
        || model.starts_with("chatgpt-")
}

/// Returns `true` if the model ID belongs to Anthropic.
///
/// Matches: claude-*
pub(crate) fn is_anthropic_model(model: &str) -> bool {
    model.starts_with("claude-")
}

/// Returns `true` if the model ID belongs to Google Gemini.
///
/// Matches: gemini-*
pub(crate) fn is_gemini_model(model: &str) -> bool {
    model.starts_with("gemini-")
}

/// Returns the appropriate tiktoken encoding for an OpenAI model.
///
/// Uses `o200k_base` for GPT-4o/4.1/5, o1/o3/o4 models.
/// Falls back to `cl100k_base` for GPT-4, GPT-3.5-turbo, and older models.
pub(crate) fn tiktoken_encoding_for_model(model: &str) -> TiktokenEncoding {
    // o200k_base models: GPT-4o family, o1/o3/o4 family
    if model.starts_with("gpt-4o")
        || model.starts_with("gpt-4.1")
        || model.starts_with("gpt-5")
        || model.starts_with("chatgpt-4o")
        || model.starts_with("o1")
        || model.starts_with("o3")
        || model.starts_with("o4")
    {
        TiktokenEncoding::O200kBase
    } else {
        TiktokenEncoding::Cl100kBase
    }
}

// ---------------------------------------------------------------------------
// TokenizerCache
// ---------------------------------------------------------------------------

/// Thread-safe cache mapping model IDs to their token counters.
///
/// In `Fast` mode, always returns [`HeuristicCounter`].
/// In `Accurate` mode, resolves provider-specific counters and caches them.
///
/// # Thread Safety
///
/// Uses `RwLock<HashMap<...>>` for concurrent reads with exclusive writes.
/// Counter creation is idempotent, so duplicate insertions are harmless.
pub struct TokenizerCache {
    counters: RwLock<HashMap<String, Arc<dyn TokenCounter>>>,
    mode: TokenizerMode,
}

impl std::fmt::Debug for TokenizerCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.counters.read().map(|c| c.len()).unwrap_or(0);
        f.debug_struct("TokenizerCache")
            .field("mode", &self.mode)
            .field("cached_counters", &count)
            .finish()
    }
}

impl TokenizerCache {
    /// Create a new cache with the given tokenizer mode.
    pub fn new(mode: TokenizerMode) -> Self {
        Self {
            counters: RwLock::new(HashMap::new()),
            mode,
        }
    }

    /// Get (or create) a token counter for the given model ID.
    ///
    /// Returns a cached `Arc<dyn TokenCounter>` on subsequent calls for the same model.
    pub fn get_counter(&self, model_id: &str) -> Arc<dyn TokenCounter> {
        // Fast path: check read lock
        {
            let cache = self.counters.read().expect("TokenizerCache lock poisoned");
            if let Some(counter) = cache.get(model_id) {
                return Arc::clone(counter);
            }
        }

        // Slow path: resolve and insert
        let counter = self.resolve_counter(model_id);
        {
            let mut cache = self.counters.write().expect("TokenizerCache lock poisoned");
            // Double-check after acquiring write lock (another thread may have inserted)
            cache
                .entry(model_id.to_string())
                .or_insert_with(|| Arc::clone(&counter));
        }

        // Re-read to return the canonical Arc (the one in the cache)
        let cache = self.counters.read().expect("TokenizerCache lock poisoned");
        Arc::clone(cache.get(model_id).expect("just inserted"))
    }

    /// Resolve a token counter for a model ID.
    ///
    /// In Fast mode, always returns HeuristicCounter.
    /// In Accurate mode, determines the provider from the model name and creates
    /// the appropriate counter:
    /// - OpenAI models: [`TiktokenCounter`] with appropriate encoding
    /// - Anthropic models: [`HuggingFaceCounter`] with bundled Claude vocabulary
    /// - Gemini/Ollama/unknown: [`HeuristicCounter`] fallback
    ///
    /// OpenRouter-style prefixed model IDs (e.g., `openai/gpt-4o`) are handled
    /// by stripping the provider prefix and routing to the correct tokenizer.
    fn resolve_counter(&self, model_id: &str) -> Arc<dyn TokenCounter> {
        if self.mode == TokenizerMode::Fast {
            return Arc::new(HeuristicCounter::default());
        }

        let lower = model_id.to_lowercase();

        // Handle OpenRouter-style provider prefixes
        if let Some(rest) = lower.strip_prefix("openai/") {
            return Arc::new(TiktokenCounter::for_model(rest));
        }
        if lower.starts_with("anthropic/") {
            return Arc::new(HuggingFaceCounter);
        }
        if lower.starts_with("google/") {
            return Arc::new(HeuristicCounter::default());
        }
        // Unknown provider prefix (meta-llama/, mistral/, etc.) -> heuristic
        if lower.contains('/') {
            return Arc::new(HeuristicCounter::default());
        }

        // Direct model name matching (no prefix)
        if is_openai_model(&lower) {
            Arc::new(TiktokenCounter::for_model(&lower))
        } else if is_anthropic_model(&lower) {
            Arc::new(HuggingFaceCounter)
        } else if is_gemini_model(&lower) {
            Arc::new(HeuristicCounter::default())
        } else {
            // Unknown model -> heuristic fallback
            Arc::new(HeuristicCounter::default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- HeuristicCounter tests ---

    #[tokio::test]
    async fn heuristic_counter_ascii_text_returns_ceil_chars_div_3_5() {
        let counter = HeuristicCounter::default();
        // "Hello, world!" = 13 chars => ceil(13 / 3.5) = ceil(3.714) = 4
        let tokens = counter.count_tokens("Hello, world!").await.unwrap();
        assert_eq!(tokens, 4);
    }

    #[tokio::test]
    async fn heuristic_counter_cjk_text_returns_ceil_chars_div_2_0() {
        let counter = HeuristicCounter::default();
        // All CJK characters -- fraction > 30%, uses 2.0
        // 5 CJK chars => ceil(5 / 2.0) = 3
        let tokens = counter
            .count_tokens("\u{4F60}\u{597D}\u{4E16}\u{754C}\u{5440}")
            .await
            .unwrap();
        assert_eq!(tokens, 3);
    }

    #[tokio::test]
    async fn heuristic_counter_empty_string_returns_zero() {
        let counter = HeuristicCounter::default();
        let tokens = counter.count_tokens("").await.unwrap();
        assert_eq!(tokens, 0);
    }

    #[tokio::test]
    async fn heuristic_counter_mixed_cjk_latin_below_threshold() {
        let counter = HeuristicCounter::default();
        // 10 ASCII chars + 2 CJK chars = 12 total, CJK fraction = 2/12 = 0.167 < 0.30
        // Uses chars/3.5: ceil(12 / 3.5) = ceil(3.428) = 4
        let tokens = counter
            .count_tokens("abcdefghij\u{4F60}\u{597D}")
            .await
            .unwrap();
        assert_eq!(tokens, 4);
    }

    #[tokio::test]
    async fn heuristic_counter_mixed_cjk_latin_above_threshold() {
        let counter = HeuristicCounter::default();
        // 3 ASCII chars + 5 CJK chars = 8 total, CJK fraction = 5/8 = 0.625 > 0.30
        // Uses chars/2.0: ceil(8 / 2.0) = 4
        let tokens = counter
            .count_tokens("abc\u{4F60}\u{597D}\u{4E16}\u{754C}\u{5440}")
            .await
            .unwrap();
        assert_eq!(tokens, 4);
    }

    #[test]
    fn is_cjk_identifies_cjk_unified_ideographs() {
        assert!(is_cjk('\u{4E00}')); // CJK Unified Ideographs start
        assert!(is_cjk('\u{9FFF}')); // CJK Unified Ideographs end
        assert!(is_cjk('\u{3400}')); // CJK Unified Ideographs Extension A start
        assert!(is_cjk('\u{4DBF}')); // CJK Unified Ideographs Extension A end
        assert!(is_cjk('\u{F900}')); // CJK Compatibility Ideographs start
        assert!(is_cjk('\u{FAFF}')); // CJK Compatibility Ideographs end
        assert!(!is_cjk('A'));
        assert!(!is_cjk('z'));
        assert!(!is_cjk('0'));
    }

    #[test]
    fn heuristic_counter_name() {
        let counter = HeuristicCounter::default();
        assert_eq!(counter.counter_name(), "heuristic");
    }

    // --- TokenizerCache tests ---

    #[test]
    fn tokenizer_cache_fast_mode_returns_heuristic() {
        let cache = TokenizerCache::new(TokenizerMode::Fast);
        let counter = cache.get_counter("gpt-4o");
        assert_eq!(counter.counter_name(), "heuristic");
    }

    #[test]
    fn tokenizer_cache_same_arc_for_repeated_calls() {
        let cache = TokenizerCache::new(TokenizerMode::Fast);
        let c1 = cache.get_counter("gpt-4o");
        let c2 = cache.get_counter("gpt-4o");
        // Pointer equality -- same Arc
        assert!(std::sync::Arc::ptr_eq(&c1, &c2));
    }

    // --- Model identification helper tests ---

    #[test]
    fn is_openai_model_identifies_gpt_4o() {
        assert!(is_openai_model("gpt-4o"));
        assert!(is_openai_model("gpt-4o-mini"));
        assert!(is_openai_model("gpt-4-turbo"));
    }

    #[test]
    fn is_openai_model_identifies_o1_o3_prefixes() {
        assert!(is_openai_model("o1"));
        assert!(is_openai_model("o1-mini"));
        assert!(is_openai_model("o3"));
        assert!(is_openai_model("o3-mini"));
        assert!(is_openai_model("o4-mini"));
    }

    #[test]
    fn is_openai_model_identifies_text_embedding() {
        assert!(is_openai_model("text-embedding-ada-002"));
        assert!(is_openai_model("text-embedding-3-large"));
    }

    #[test]
    fn is_openai_model_identifies_chatgpt() {
        assert!(is_openai_model("chatgpt-4o-latest"));
    }

    #[test]
    fn is_openai_model_rejects_non_openai() {
        assert!(!is_openai_model("claude-3-sonnet"));
        assert!(!is_openai_model("gemini-pro"));
    }

    #[test]
    fn is_anthropic_model_identifies_claude() {
        assert!(is_anthropic_model("claude-3-sonnet-20240229"));
        assert!(is_anthropic_model("claude-sonnet-4-20250514"));
        assert!(is_anthropic_model("claude-haiku-4-5-20250901"));
    }

    #[test]
    fn is_anthropic_model_rejects_non_anthropic() {
        assert!(!is_anthropic_model("gpt-4o"));
        assert!(!is_anthropic_model("gemini-pro"));
    }

    #[test]
    fn is_gemini_model_identifies_gemini() {
        assert!(is_gemini_model("gemini-2.0-flash"));
        assert!(is_gemini_model("gemini-pro"));
    }

    #[test]
    fn is_gemini_model_rejects_non_gemini() {
        assert!(!is_gemini_model("gpt-4o"));
        assert!(!is_gemini_model("claude-3-sonnet"));
    }

    // --- TiktokenEncoding tests ---

    #[test]
    fn tiktoken_encoding_o200k_for_gpt4o() {
        assert_eq!(
            tiktoken_encoding_for_model("gpt-4o"),
            TiktokenEncoding::O200kBase
        );
        assert_eq!(
            tiktoken_encoding_for_model("gpt-4o-mini"),
            TiktokenEncoding::O200kBase
        );
    }

    #[test]
    fn tiktoken_encoding_o200k_for_o1_o3_o4() {
        assert_eq!(
            tiktoken_encoding_for_model("o1"),
            TiktokenEncoding::O200kBase
        );
        assert_eq!(
            tiktoken_encoding_for_model("o3-mini"),
            TiktokenEncoding::O200kBase
        );
        assert_eq!(
            tiktoken_encoding_for_model("o4-mini"),
            TiktokenEncoding::O200kBase
        );
    }

    #[test]
    fn tiktoken_encoding_cl100k_for_gpt4() {
        assert_eq!(
            tiktoken_encoding_for_model("gpt-4"),
            TiktokenEncoding::Cl100kBase
        );
        assert_eq!(
            tiktoken_encoding_for_model("gpt-3.5-turbo"),
            TiktokenEncoding::Cl100kBase
        );
    }

    // --- TokenizerCache Accurate mode tests ---

    #[test]
    fn tokenizer_cache_accurate_mode_resolves_openai_model() {
        let cache = TokenizerCache::new(TokenizerMode::Accurate);
        let counter = cache.get_counter("gpt-4o");
        assert!(
            counter.counter_name().contains("tiktoken"),
            "Expected tiktoken counter for gpt-4o, got: {}",
            counter.counter_name()
        );
    }

    #[test]
    fn tokenizer_cache_accurate_mode_resolves_openrouter_prefix() {
        let cache = TokenizerCache::new(TokenizerMode::Accurate);
        let counter = cache.get_counter("openai/gpt-4o");
        assert!(
            counter.counter_name().contains("tiktoken"),
            "Expected tiktoken counter for openai/gpt-4o, got: {}",
            counter.counter_name()
        );
    }

    #[test]
    fn tokenizer_cache_accurate_mode_caches_different_models() {
        let cache = TokenizerCache::new(TokenizerMode::Accurate);
        let c1 = cache.get_counter("gpt-4o");
        let c2 = cache.get_counter("claude-3-sonnet");
        // Different models should get different Arc instances
        assert!(!std::sync::Arc::ptr_eq(&c1, &c2));
    }

    // --- TiktokenCounter tests (Plan 02) ---

    #[tokio::test]
    async fn tiktoken_o200k_tokenizes_hello_world() {
        let counter = TiktokenCounter::for_model("gpt-4o");
        let tokens = counter.count_tokens("Hello, world!").await.unwrap();
        assert!(tokens > 0, "Expected positive token count, got {tokens}");
    }

    #[tokio::test]
    async fn tiktoken_cl100k_tokenizes_hello_world() {
        let counter = TiktokenCounter::for_model("gpt-4");
        let tokens = counter.count_tokens("Hello, world!").await.unwrap();
        assert!(tokens > 0, "Expected positive token count, got {tokens}");
    }

    #[test]
    fn tiktoken_for_model_gpt4o_selects_o200k() {
        let counter = TiktokenCounter::for_model("gpt-4o");
        assert_eq!(counter.counter_name(), "tiktoken-o200k");
    }

    #[test]
    fn tiktoken_for_model_gpt4_selects_cl100k() {
        let counter = TiktokenCounter::for_model("gpt-4");
        assert_eq!(counter.counter_name(), "tiktoken-cl100k");
    }

    #[tokio::test]
    async fn tiktoken_empty_string_returns_zero() {
        let counter = TiktokenCounter::for_model("gpt-4o");
        let tokens = counter.count_tokens("").await.unwrap();
        assert_eq!(tokens, 0);
    }

    // --- HuggingFaceCounter tests (Plan 02) ---

    #[tokio::test]
    async fn hf_claude_tokenizes_hello_world() {
        let counter = HuggingFaceCounter;
        let tokens = counter.count_tokens("Hello, world!").await.unwrap();
        assert!(tokens > 0, "Expected positive token count, got {tokens}");
    }

    #[tokio::test]
    async fn hf_claude_reuses_singleton() {
        // Calling count_tokens twice should reuse the same OnceLock tokenizer.
        // If OnceLock is broken, the second call would fail or panic.
        let counter = HuggingFaceCounter;
        let t1 = counter.count_tokens("Hello").await.unwrap();
        let t2 = counter.count_tokens("Hello").await.unwrap();
        assert_eq!(t1, t2, "Singleton should produce identical results");
    }

    #[tokio::test]
    async fn hf_claude_empty_string_returns_zero_or_small() {
        let counter = HuggingFaceCounter;
        let tokens = counter.count_tokens("").await.unwrap();
        assert!(
            tokens <= 1,
            "Empty string should produce 0 or at most 1 token, got {tokens}"
        );
    }

    #[test]
    fn hf_claude_counter_name() {
        let counter = HuggingFaceCounter;
        assert_eq!(counter.counter_name(), "hf-claude");
    }

    // --- count_with_fallback tests (Plan 02) ---

    #[tokio::test]
    async fn count_with_fallback_uses_primary_on_success() {
        let counter = HeuristicCounter::default();
        let result = count_with_fallback(&counter, "Hello, world!").await;
        assert!(result > 0);
    }

    #[tokio::test]
    async fn count_with_fallback_degrades_to_heuristic_on_error() {
        // Create a counter that always fails
        struct FailingCounter;

        #[async_trait]
        impl TokenCounter for FailingCounter {
            async fn count_tokens(&self, _text: &str) -> Result<usize, BlufioError> {
                Err(BlufioError::Internal("intentional failure".into()))
            }
            fn counter_name(&self) -> &str {
                "failing"
            }
        }

        let counter = FailingCounter;
        let result = count_with_fallback(&counter, "Hello, world!").await;
        // Should fall back to heuristic -- 13 chars / 3.5 = ceil(3.71) = 4
        assert!(result > 0, "Fallback should produce a positive count");
    }

    // --- TokenizerCache Accurate mode: full routing tests (Task 2) ---

    #[test]
    fn cache_accurate_claude_returns_hf_claude() {
        let cache = TokenizerCache::new(TokenizerMode::Accurate);
        let counter = cache.get_counter("claude-sonnet-4-20250514");
        assert_eq!(
            counter.counter_name(),
            "hf-claude",
            "Expected hf-claude for claude model, got: {}",
            counter.counter_name()
        );
    }

    #[test]
    fn cache_accurate_gemini_returns_heuristic() {
        let cache = TokenizerCache::new(TokenizerMode::Accurate);
        let counter = cache.get_counter("gemini-2.0-flash");
        assert_eq!(
            counter.counter_name(),
            "heuristic",
            "Expected heuristic for gemini model, got: {}",
            counter.counter_name()
        );
    }

    #[test]
    fn cache_accurate_openrouter_openai_returns_tiktoken() {
        let cache = TokenizerCache::new(TokenizerMode::Accurate);
        let counter = cache.get_counter("openai/gpt-4o");
        assert!(
            counter.counter_name().contains("tiktoken"),
            "Expected tiktoken for openai/gpt-4o, got: {}",
            counter.counter_name()
        );
    }

    #[test]
    fn cache_accurate_openrouter_anthropic_returns_hf_claude() {
        let cache = TokenizerCache::new(TokenizerMode::Accurate);
        let counter = cache.get_counter("anthropic/claude-sonnet-4");
        assert_eq!(
            counter.counter_name(),
            "hf-claude",
            "Expected hf-claude for anthropic/claude-sonnet-4, got: {}",
            counter.counter_name()
        );
    }

    #[test]
    fn cache_accurate_openrouter_google_returns_heuristic() {
        let cache = TokenizerCache::new(TokenizerMode::Accurate);
        let counter = cache.get_counter("google/gemini-2.0-flash");
        assert_eq!(
            counter.counter_name(),
            "heuristic",
            "Expected heuristic for google/gemini-2.0-flash, got: {}",
            counter.counter_name()
        );
    }

    #[test]
    fn cache_accurate_unknown_model_returns_heuristic() {
        let cache = TokenizerCache::new(TokenizerMode::Accurate);
        let counter = cache.get_counter("some-unknown-model");
        assert_eq!(
            counter.counter_name(),
            "heuristic",
            "Expected heuristic for unknown model, got: {}",
            counter.counter_name()
        );
    }

    #[test]
    fn cache_accurate_meta_llama_returns_heuristic() {
        let cache = TokenizerCache::new(TokenizerMode::Accurate);
        let counter = cache.get_counter("meta-llama/llama-3.1-70b");
        assert_eq!(
            counter.counter_name(),
            "heuristic",
            "Expected heuristic for meta-llama/* model, got: {}",
            counter.counter_name()
        );
    }

    #[tokio::test]
    async fn cache_accurate_openrouter_model_produces_real_token_count() {
        let cache = TokenizerCache::new(TokenizerMode::Accurate);
        // DelegatingCounter behavior: OpenRouter model -> resolved counter -> real count
        let counter = cache.get_counter("openai/gpt-4o");
        let tokens = counter
            .count_tokens("The quick brown fox jumps over the lazy dog.")
            .await
            .unwrap();
        assert!(
            tokens > 0,
            "Expected positive token count for OpenRouter model, got {tokens}"
        );
        // tiktoken o200k should give a reasonable count (around 10 tokens for this sentence)
        assert!(
            tokens < 20,
            "Expected < 20 tokens for short sentence, got {tokens}"
        );
    }
}
