// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Token counting abstractions for accurate LLM token estimation.
//!
//! Provides the [`TokenCounter`] trait, a [`HeuristicCounter`] fallback,
//! and a [`TokenizerCache`] for caching provider-specific counters by model ID.

// Placeholder module -- tests below define the contract. Implementation follows in GREEN phase.

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
        let tokens = counter.count_tokens("\u{4F60}\u{597D}\u{4E16}\u{754C}\u{5440}").await.unwrap();
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
        let tokens = counter.count_tokens("abcdefghij\u{4F60}\u{597D}").await.unwrap();
        assert_eq!(tokens, 4);
    }

    #[tokio::test]
    async fn heuristic_counter_mixed_cjk_latin_above_threshold() {
        let counter = HeuristicCounter::default();
        // 3 ASCII chars + 5 CJK chars = 8 total, CJK fraction = 5/8 = 0.625 > 0.30
        // Uses chars/2.0: ceil(8 / 2.0) = 4
        let tokens = counter.count_tokens("abc\u{4F60}\u{597D}\u{4E16}\u{754C}\u{5440}").await.unwrap();
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
}
