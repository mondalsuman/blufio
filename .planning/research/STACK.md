# Technology Stack: v1.4 Quality & Resilience

**Project:** Blufio
**Researched:** 2026-03-08
**Scope:** NEW stack additions/changes for v1.4 only -- accurate token counting, circuit breakers, ORT upgrade path, degradation management, typed errors, FormatPipeline integration, ChannelCapabilities extension.

> Existing stack validated through v1.3 (71,808 LOC, 35 crates, 219 requirements) is UNCHANGED. This document covers ONLY what v1.4 adds or modifies.

---

## 1. Accurate Token Counting

### New Dependency

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| `tiktoken-rs` | 0.9.1 | BPE tokenization for OpenAI models (GPT-4o, o1, o3, o4-mini via o200k_base encoding) | Only Rust crate with correct OpenAI vocabulary files. Provides o200k_base, cl100k_base encodings. Verified on crates.io, released 2025-11-09. |

### Existing Dependency (Reused)

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| `tokenizers` (HuggingFace) | 0.21.4 (keep current, do NOT upgrade to 0.22) | BPE tokenization for Anthropic Claude and Ollama HF models | Already in workspace for ONNX embedding tokenization in blufio-memory. Claude tokenizer.json from Xenova/claude-tokenizer is HF tokenizers-compatible and loadable via `Tokenizer::from_file()`. |

**Confidence:** HIGH -- verified via official Anthropic docs, crates.io, HuggingFace, docs.rs

### Architecture: Multi-Provider Token Counting

Blufio supports 5 LLM providers (Anthropic, OpenAI, Ollama, OpenRouter, Gemini), each with different tokenizers. The `len() / 4` heuristic currently used in `blufio-context/src/dynamic.rs:64` gives only ~75-85% accuracy for English text and degrades further for CJK, code, and mixed content.

**Provider-to-tokenizer mapping:**

| Provider | Tokenizer Strategy | Accuracy vs len/4 | Rationale |
|----------|-------------------|-------------------|-----------|
| Anthropic (Claude) | `tokenizers` crate + `Xenova/claude-tokenizer` tokenizer.json (65K vocab, BPE) | ~80-95% vs ~75-85% | Anthropic does not publish an official tokenizer. The Xenova/claude-tokenizer on HuggingFace is the best available approximation. For Claude 3/4, accuracy is ~80-95% (the Xenova tokenizer matches Claude 2.x exactly; Claude 3+ vocabulary may differ slightly). The Anthropic `count_tokens` API is free but requires a network call -- not viable as primary path for context budget decisions. |
| OpenAI | `tiktoken-rs` with `o200k_base` encoding | ~100% vs ~75-85% | tiktoken-rs 0.9.1 is the canonical OpenAI tokenizer in Rust. Supports GPT-4o, o1, o3, o4 via o200k_base. This matches OpenAI's server-side counting exactly. |
| Ollama | `tokenizers` crate + per-model tokenizer.json from HuggingFace | ~95-100% vs ~75-85% | Ollama models are HuggingFace models; each has its own tokenizer.json downloadable from HF. Load at model init time. Fallback: Ollama API returns exact token counts in responses (prompt_eval_count, eval_count). |
| OpenRouter | Same as underlying model | varies | OpenRouter proxies to various models. Use the tokenizer matching the configured backend model. |
| Gemini | Calibrated heuristic (len/3.5 for multilingual, len/4 for English) | ~80% vs ~75% | Google does not publish Gemini tokenizers. Their API returns exact token counts in responses (promptTokenCount, candidatesTokenCount). For pre-flight estimation, a calibrated heuristic is the only viable offline option. |

**Why NOT tiktoken-rs alone for all providers:** tiktoken-rs supports ONLY OpenAI models. It has zero support for Anthropic Claude, Gemini, or Ollama models. Using it for Claude would give worse results than the current len/4 heuristic because OpenAI and Claude vocabularies overlap only ~70%.

**Why NOT tokenizers alone for all providers:** The `tokenizers` crate cannot load tiktoken BPE files natively. OpenAI's vocabulary format (rank-based BPE) differs from HuggingFace's tokenizer.json format (merge-based BPE). tiktoken-rs wraps OpenAI's vocabulary files correctly.

**Why NOT the Anthropic count_tokens API as primary:** It requires a network call (adds latency), is rate-limited (100-8000 RPM depending on tier), and does not work offline. Use it for calibration/verification only, not per-message counting.

### Token Counter Trait Design

```rust
// New: blufio-core/src/tokenizer.rs
pub trait TokenCounter: Send + Sync {
    /// Count tokens in text. Must be fast (microseconds, not milliseconds).
    fn count_tokens(&self, text: &str) -> usize;
    /// Provider name this counter is calibrated for.
    fn provider_name(&self) -> &str;
}

// Implementations:
// ClaudeTokenCounter    -- tokenizers crate + bundled claude tokenizer.json
// OpenAiTokenCounter    -- tiktoken-rs + o200k_base
// HfModelTokenCounter   -- tokenizers crate + model-specific tokenizer.json (Ollama)
// HeuristicTokenCounter -- calibrated chars-per-token ratio (Gemini fallback)
```

### Claude Tokenizer File Strategy

The `Xenova/claude-tokenizer` tokenizer.json is 1.77MB. Options for bundling:

**Recommendation: Ship alongside model files in `~/.blufio/models/`.**

The ONNX embedder already downloads `model.onnx` (8.5MB) and `tokenizer.json` (for the embedder) to `~/.blufio/models/` at first run. Adding a separate `claude-tokenizer.json` (1.77MB) to this download step is consistent and avoids inflating the binary with `include_bytes!()`.

Fallback: If the tokenizer file is not available (first run before download, offline), fall back to the calibrated heuristic (len/3.5). This gracefully degrades without losing functionality.

### Integration Points

| Location | Current | After v1.4 |
|----------|---------|------------|
| `blufio-context/src/dynamic.rs:64` | `m.content.len() / 4` | `token_counter.count_tokens(&m.content)` |
| `blufio-cost` | Uses post-response API counts | Can also use pre-flight estimates for budget checks |
| `blufio-router` | Heuristic context budget | Accurate context budget based on real token counts |
| `blufio-agent/src/session.rs` | token_count stored as Option<i64> from API response | Also populate from pre-flight counter |

### Why Keep tokenizers at 0.21 (NOT 0.22)

The workspace currently uses `tokenizers = { version = "0.21", ... }` and resolves to 0.21.4. The latest available is 0.22.1. Reasons to stay:

1. `ort` =2.0.0-rc.11 is pinned and depends on `ndarray` 0.17. Upgrading tokenizers to 0.22 could introduce dependency conflicts.
2. `tokenizers` 0.21.4 loads tokenizer.json files correctly. No feature gap for our use case.
3. The upgrade path is: when ort stable 2.0.0 releases, evaluate tokenizers upgrade simultaneously to ensure ndarray compatibility.

---

## 2. Circuit Breaker Pattern

### Recommendation: Custom Implementation (~200 LOC)

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| **Custom implementation** | N/A | Per-dependency circuit breakers for LLM APIs, MCP servers, channel adapters | Best fit for Blufio's adapter-trait architecture; no suitable crate covers all use cases; integrates directly with existing metrics and tracing |

**Confidence:** HIGH -- analyzed all Rust circuit breaker crates, none fit Blufio's adapter-based architecture

### Crate Evaluation

| Crate | Version | Last Updated | Downloads | Verdict |
|-------|---------|-------------|-----------|---------|
| `failsafe` | 1.3.0 | ~2024 (>1yr ago) | Moderate | **REJECT:** Unmaintained, pre-async-await API style, futures-support is a bolted-on feature |
| `circuitbreaker-rs` | 0.1.1 | 2025 | Low | **REJECT:** Too new (0.1.x), unproven in production, low adoption |
| `tower-circuitbreaker` | 0.2.0 | Oct 2025 | Low | **CONSIDERED:** Good tower integration, but only works for tower Service implementations |
| `rssafecircuit` | - | Unknown | Very low | **REJECT:** Minimal adoption, unknown maintenance |

### Why Custom Wins Over tower-circuitbreaker

`tower-circuitbreaker` 0.2.0 is the strongest external option but fails on a critical architectural mismatch:

1. **Blufio's adapter calls are async trait methods, not tower Services.** The `ProviderAdapter::send()`, `ChannelAdapter::send_message()`, and `EmbeddingAdapter::embed()` calls go through `dyn` trait objects with `async-trait`. Wrapping each in a tower Service just to use tower-circuitbreaker adds boilerplate without benefit.

2. **Circuit breakers protect outbound calls to diverse backends.** These include: teloxide (Telegram), serenity (Discord), slack-morphism (Slack), matrix-sdk (Matrix), irc crate, signal-cli JSON-RPC, and reqwest HTTP calls. Each has its own connection management. tower-circuitbreaker would only work for the reqwest-based calls.

3. **tower-circuitbreaker brings tower-resilience-core 0.2 as a dependency.** This is a new transitive dependency for a pattern that is ~200 lines of straightforward Rust.

4. **Integration with existing infrastructure is free with custom.** Blufio already has `metrics` 0.24 (Prometheus), `tracing` 0.1 (structured logging), `dashmap` 6 (concurrent maps). The custom implementation wires directly into these without adaptation layers.

### Custom Circuit Breaker Design

```rust
// blufio-core/src/circuit_breaker.rs

use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CircuitState {
    Closed = 0,    // Normal operation, calls pass through
    Open = 1,      // Failures exceeded threshold, calls rejected
    HalfOpen = 2,  // Testing recovery with limited calls
}

pub struct CircuitBreaker {
    name: String,
    state: AtomicU8,
    failure_count: AtomicU32,
    success_count: AtomicU32,
    last_failure_ts: AtomicU64,  // epoch millis
    config: CircuitBreakerConfig,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening the circuit.
    pub failure_threshold: u32,         // default: 5
    /// Duration to stay Open before transitioning to HalfOpen.
    pub reset_timeout: Duration,        // default: 30s
    /// Number of test calls allowed in HalfOpen state.
    pub half_open_max_calls: u32,       // default: 3
    /// Successes needed in HalfOpen to close the circuit.
    pub success_threshold: u32,         // default: 2
}
```

**No new crate dependencies.** Uses existing:
- `std::sync::atomic` for lock-free state management
- `tokio::time::Instant` for timeout tracking
- `metrics::counter!("blufio_circuit_breaker_trips_total", "dependency" => name)` for Prometheus
- `tracing::warn!("circuit breaker opened for {}", name)` for logging

### Per-Dependency Circuit Breaker Registry

```rust
// blufio-core/src/circuit_breaker.rs
pub struct CircuitBreakerRegistry {
    breakers: DashMap<String, Arc<CircuitBreaker>>,
    default_config: CircuitBreakerConfig,
}

impl CircuitBreakerRegistry {
    pub fn get_or_create(&self, name: &str) -> Arc<CircuitBreaker>;
    pub fn record_success(&self, name: &str);
    pub fn record_failure(&self, name: &str);
    pub fn is_call_permitted(&self, name: &str) -> bool;
}
```

**Integration with TOML config:**
```toml
[resilience.circuit_breaker]
failure_threshold = 5
reset_timeout_secs = 30
half_open_max_calls = 3

[resilience.circuit_breaker.overrides.anthropic]
failure_threshold = 3
reset_timeout_secs = 60
```

---

## 3. ORT Stable Upgrade Path

### Recommendation: Stay Pinned at rc.11

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| `ort` | =2.0.0-rc.11 (KEEP PINNED) | ONNX Runtime bindings for local embedding inference | No stable 2.0.0 release exists. rc.12 has breaking API changes. Upgrading twice (rc.11 -> rc.12 -> stable) wastes effort. |

**Confidence:** HIGH -- verified via GitHub releases page (pykeio/ort), crates.io

### Current State

| Property | Value |
|----------|-------|
| Pinned version | `=2.0.0-rc.11` (released January 7, 2025) |
| Latest RC | `2.0.0-rc.12` (released March 5, 2025) |
| Stable 2.0.0 | **Does NOT exist** |
| Maintainer statement | rc.11 notes: "the next big release of ort should be, finally, 2.0.0" |

### Breaking Changes in rc.12 (Impact Analysis)

| rc.12 Change | Blufio Impact | Action Needed |
|-------------|--------------|---------------|
| `ORT_LIB_LOCATION` env var renamed to `ORT_LIB_PATH` | **NONE** -- Blufio uses `download-binaries` feature, not env var | None |
| Items moved from `ort::tensor` to `ort::value` | **NONE** -- Blufio already uses `ort::value::TensorRef` (see blufio-memory/src/embedder.rs) | None |
| `with_denormal_as_zero` renamed to `with_flush_to_zero` | **NONE** -- not used by Blufio | None |
| `with_device_allocator_for_initializers` renamed | **NONE** -- not used by Blufio | None |
| `api-24` feature may be required for some capabilities | **LOW** -- need to verify feature flags at upgrade time | Test at upgrade |

### Recommendation Details

**Do NOT upgrade to rc.12 in v1.4.** Rationale:

1. Zero functional benefit: rc.12 changes are naming/organizational, not feature additions that Blufio needs.
2. Double migration risk: If we upgrade to rc.12 now, we will need another migration when stable 2.0.0 lands.
3. The current rc.11 works correctly: `OnnxEmbedder` in blufio-memory/src/embedder.rs passes all tests, produces correct 384-dim embeddings.

**ADR (Architecture Decision Record) for v1.4:**
- **Decision:** Pin `ort` at `=2.0.0-rc.11` until stable 2.0.0 releases
- **Context:** No stable release exists; rc.12 has minor API renames but zero new features needed by Blufio
- **Consequences:** Must monitor `pykeio/ort` GitHub releases for stable 2.0.0 announcement
- **Migration plan when stable lands:** Update version pin, adjust any renamed APIs (likely minimal based on rc.12 analysis), run embedding inference tests, update ONNX model download URLs if needed

### Version Compatibility Lock

These three crates must upgrade together:

| Crate | Current | Constraint |
|-------|---------|------------|
| `ort` | =2.0.0-rc.11 | Requires ndarray 0.17 |
| `ndarray` | 0.17 | Required by ort for tensor arrays |
| `tokenizers` | 0.21.4 | No direct ort dependency, but shares ndarray transitively |

When ort stable 2.0.0 releases, check whether it bumps ndarray to 0.18. If so, tokenizers may need upgrading simultaneously.

---

## 4. Degradation Level Management

### Recommendation: No New Dependencies

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| **No new dependencies** | N/A | 6-level degradation ladder with automatic escalation/de-escalation | Pure state machine pattern using existing workspace primitives |

**Confidence:** HIGH -- architectural pattern, no external deps needed

### Degradation Levels

```
Level 0: Normal       -- all systems operational
Level 1: Elevated     -- non-critical failures detected (e.g., memory search slow)
Level 2: Degraded     -- some features disabled (e.g., memory search off, skills limited)
Level 3: Limited      -- model downgraded (Opus -> Sonnet -> Haiku)
Level 4: Minimal      -- text-only responses, no tools, no skills
Level 5: Emergency    -- canned responses, no LLM calls
```

### Implementation Uses Only Existing Primitives

| Need | Existing Solution |
|------|-------------------|
| Atomic level storage | `std::sync::atomic::AtomicU8` |
| Level change notifications | `tokio::sync::watch::channel` |
| Prometheus gauge | `metrics::gauge!("blufio_degradation_level")` (already using metrics 0.24) |
| System-wide event broadcast | `EventBus` (blufio-bus, already uses tokio::sync::broadcast) |
| Structured logging | `tracing::warn!()` (already using tracing 0.1) |
| Health status per-adapter | `HealthStatus::Degraded(String)` already exists in blufio-core |

### Integration with Circuit Breakers

The degradation ladder and circuit breakers compose:
- Circuit breaker opens on dependency X -> emit `CircuitBreakerTripped(x)` event via EventBus
- Degradation manager listens to events, counts open breakers
- 1 open breaker = Level 1 (Elevated), 2+ = Level 2 (Degraded), primary LLM breaker open = Level 3+ (Limited)
- When breakers close -> de-escalate levels automatically

---

## 5. Typed Error Hierarchy

### Recommendation: No New Dependencies

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| `thiserror` | 2 (already in workspace) | Derive Error implementations | Already used for `BlufioError` in blufio-core/src/error.rs |

**Confidence:** HIGH -- extends existing pattern

### Current Error Enum

`BlufioError` in `blufio-core/src/error.rs` has 13 variants (Config, Storage, Channel, Provider, AdapterNotFound, HealthCheckFailed, Timeout, Vault, Security, Signature, BudgetExhausted, Skill, Mcp, Update, Migration, Internal). None carry retryability or severity metadata.

### Extension Approach

Add methods to `BlufioError` without new dependencies:

```rust
impl BlufioError {
    pub fn is_retryable(&self) -> bool { ... }
    pub fn severity(&self) -> ErrorSeverity { ... }
    pub fn category(&self) -> ErrorCategory { ... }
}

#[derive(Debug, Clone, Copy)]
pub enum ErrorSeverity { Low, Medium, High, Critical }

#[derive(Debug, Clone, Copy)]
pub enum ErrorCategory { Network, Auth, RateLimit, Internal, Configuration, External }
```

No new crates. `thiserror` 2 (existing) handles the derive macros. The new enums are pure Rust with no dependencies.

---

## 6. FormatPipeline Integration + ChannelCapabilities Extension

### Recommendation: No New Dependencies

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| **No new dependencies** | N/A | Wire FormatPipeline into channel adapters, add Table/List content types, extend ChannelCapabilities | All types already exist in blufio-core; this is wiring work |

**Confidence:** HIGH -- code already exists, needs extension and wiring

### Existing Code

- `FormatPipeline` struct in `blufio-core/src/format.rs` -- already implements format/degrade for Text, Embed, Image, CodeBlock
- `ChannelCapabilities` struct in `blufio-core/src/types.rs` -- 9 boolean/option fields (supports_edit, supports_typing, supports_images, etc.)
- Every channel adapter already implements `fn capabilities(&self) -> ChannelCapabilities`

### Extensions Needed (No New Crates)

**New RichContent variants:** `Table { headers, rows }`, `List { items, ordered }`
**New ChannelCapabilities fields:** `streaming_type: StreamingType`, `formatting_support: FormattingSupport`, `rate_limits: Option<ChannelRateLimits>`

These are pure struct/enum additions to existing blufio-core types. Zero dependency impact.

---

## Complete New Dependencies Summary

| Crate | Version | Added To | Purpose | Binary Size Impact |
|-------|---------|----------|---------|-------------------|
| `tiktoken-rs` | 0.9.1 | workspace + blufio-context or new blufio-tokenizer crate | OpenAI model token counting | ~1-2MB (includes embedded BPE vocabulary data) |

**Total new workspace dependency count: 1** (`tiktoken-rs`)

Everything else uses existing workspace dependencies:
- `tokenizers` 0.21 (already in workspace for ONNX embedding)
- `thiserror` 2 (already in workspace for error derives)
- `metrics` 0.24 (already in workspace for Prometheus)
- `dashmap` 6 (already in workspace for concurrent maps)
- `tracing` 0.1 (already in workspace for structured logging)
- `tokio` (already in workspace; watch, broadcast channels for degradation/events)

Or requires no external crate at all:
- Circuit breaker: custom ~200 LOC, `std::sync::atomic`
- Degradation ladder: custom state machine, `AtomicU8` + `tokio::sync::watch`
- Error hierarchy: method additions to existing `BlufioError` enum
- FormatPipeline integration: type extensions + wiring in existing crates
- ChannelCapabilities extension: struct field additions

---

## Alternatives Considered

| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| Claude token counting | `tokenizers` + Xenova/claude-tokenizer | Anthropic `count_tokens` API | API requires network call; adds 100-500ms latency; rate-limited (100-8000 RPM by tier); not available offline; not suitable for per-message context budget decisions |
| Claude token counting | `tokenizers` + Xenova/claude-tokenizer | `tiktoken-rs` with cl100k_base approximation | Only ~70% vocabulary overlap with Claude; worse accuracy than dedicated Claude tokenizer |
| OpenAI token counting | `tiktoken-rs` 0.9.1 | `tokenizers` + OpenAI tokenizer.json | OpenAI tokenizer files use tiktoken BPE format (rank-based), not HuggingFace JSON format (merge-based); tiktoken-rs handles this natively |
| Circuit breaker | Custom (~200 LOC) | `tower-circuitbreaker` 0.2.0 | Only works for tower Service implementations; Blufio adapter calls are async trait methods via dyn dispatch, not tower Services; does not cover teloxide, serenity, slack-morphism, matrix-sdk, irc crate calls |
| Circuit breaker | Custom (~200 LOC) | `failsafe` 1.3.0 | Unmaintained (>1yr no updates); pre-async-await API design; future-support bolted on |
| Circuit breaker | Custom (~200 LOC) | `circuitbreaker-rs` 0.1.1 | v0.1.x; unproven in production; low adoption; too early to trust |
| ORT upgrade | Stay on rc.11 | Upgrade to rc.12 | Breaking API renames for zero functional benefit; stable 2.0.0 expected soon; avoids double migration |
| ORT upgrade | Stay on rc.11 | Downgrade to 1.x stable | 1.x API is completely different; would require full rewrite of OnnxEmbedder |
| Degradation state | Custom state machine | `tower` middleware layers | Degradation is system-wide state, not per-request middleware; crosses all adapter boundaries |
| Token counting arch | Dual-crate (tokenizers + tiktoken-rs) | Single crate for all providers | No single crate handles both OpenAI tiktoken-format and HuggingFace-format vocabularies |

---

## What NOT to Add

| Crate | Why Not |
|-------|---------|
| `tower-circuitbreaker` 0.2.0 | Adds tower-resilience-core 0.2 dep; only works for tower Service; Blufio's adapter calls are async trait methods through dyn dispatch |
| `failsafe` 1.3.0 | Unmaintained; pre-async design; last release >1yr ago |
| `circuitbreaker-rs` 0.1.1 | v0.1.x maturity; too immature; minimal downloads |
| `rssafecircuit` | Minimal adoption; unproven; unknown maintenance status |
| `bpe` crate | Lower-level BPE implementation without vocabulary files; tiktoken-rs and tokenizers are higher-level with correct vocabs included |
| `tokenizers` 0.22.x upgrade | Risk of ndarray version conflict with pinned ort =2.0.0-rc.11; zero feature gap in 0.21.4 for our use case |
| `token-counter` | CLI tool, not a library; not embeddable in workspace |
| `ort` 2.0.0-rc.12 | Breaking API renames with zero new features needed; stable expected soon |
| `another-tiktoken-rs` | Fork of tiktoken-rs with no additional value; use the original |
| `tiktoken` (different crate) | Different from tiktoken-rs; less maintained |
| Any resilience framework (tower-resilience full suite) | Overkill; Blufio needs only circuit breakers, not bulkheads/rate-limiters/retries from a framework |

---

## Installation

```toml
# Workspace Cargo.toml -- add to [workspace.dependencies]
tiktoken-rs = "0.9"

# No other new workspace dependencies needed for v1.4
```

```toml
# blufio-context/Cargo.toml (or new blufio-tokenizer crate)
[dependencies]
tiktoken-rs.workspace = true
tokenizers.workspace = true  # already available via workspace
```

---

## Version Compatibility Matrix

| Crate | Current | v1.4 Target | Constraint |
|-------|---------|-------------|------------|
| `tokenizers` | 0.21.4 | 0.21.x (keep) | Compatible with ort rc.11 and ndarray 0.17; loads Claude tokenizer.json |
| `ort` | =2.0.0-rc.11 | =2.0.0-rc.11 (keep) | Pin until stable 2.0.0 releases |
| `ndarray` | 0.17 | 0.17 (keep) | Required by ort rc.11 |
| `tiktoken-rs` | (new) | 0.9.1 | No dependency conflicts with existing workspace |
| `tower` | 0.5 | 0.5 (keep) | NOT used for circuit breaker pattern |
| `metrics` | 0.24 | 0.24 (keep) | Used for circuit breaker + degradation Prometheus metrics |
| `dashmap` | 6 | 6 (keep) | Used for CircuitBreakerRegistry concurrent map |
| `thiserror` | 2 | 2 (keep) | Used for typed error hierarchy |
| `tracing` | 0.1 | 0.1 (keep) | Used for circuit breaker/degradation state change logging |

---

## Dependency Budget

| Metric | v1.3 | v1.4 |
|--------|------|------|
| Direct workspace deps | ~51 | ~52 (+tiktoken-rs only) |
| Within <80 constraint | Yes | Yes (comfortable margin) |
| New crates | 0 custom + 1 external | Minimal audit surface |

---

## Sources

### Verified via Official Documentation (HIGH confidence)
- [Anthropic Token Counting API](https://platform.claude.com/docs/en/build-with-claude/token-counting) -- Free API, rate-limited, returns estimates
- [tiktoken-rs on crates.io](https://crates.io/crates/tiktoken-rs) -- v0.9.1, released 2025-11-09
- [tiktoken-rs API docs](https://docs.rs/tiktoken-rs/latest/tiktoken_rs/) -- OpenAI-only: o200k_base, cl100k_base, p50k_base confirmed
- [ort GitHub releases](https://github.com/pykeio/ort/releases) -- rc.12 (March 5, 2025), rc.11 (Jan 7, 2025), no stable 2.0.0
- [ort crate docs](https://docs.rs/crate/ort/latest) -- rc.12 API changes documented

### Verified via crates.io / docs.rs (HIGH confidence)
- [circuitbreaker-rs](https://docs.rs/circuitbreaker-rs) -- v0.1.1, async support, new crate
- [tower-circuitbreaker](https://lib.rs/crates/tower-circuitbreaker) -- v0.2.0, Oct 2025, deps include tower-resilience-core
- [failsafe on crates.io](https://crates.io/crates/failsafe) -- v1.3.0, last update >1yr ago
- [tokenizers on crates.io](https://crates.io/crates/tokenizers) -- v0.22.1 latest, 0.21.4 in use

### Verified via Community Sources (MEDIUM confidence)
- [Xenova/claude-tokenizer on HuggingFace](https://huggingface.co/Xenova/claude-tokenizer) -- tokenizer.json (1.77MB), HF-compatible, ~2yr old
- [ctoc: Reverse Engineering Claude's Token Counter](https://grohan.co/2026/02/10/ctoc/) -- 36K vocab, ~96% accuracy on Claude 4.x
- [Token Counting Guide](https://www.propelcode.ai/blog/token-counting-tiktoken-anthropic-gemini-guide-2025) -- len/4 accuracy ~75-85% for English

### Local Codebase Verification
- `blufio-context/src/dynamic.rs:64` -- confirmed `m.content.len() / 4` heuristic
- `blufio-memory/src/embedder.rs` -- confirmed `tokenizers::Tokenizer::from_file()` usage with ort
- `blufio-core/src/error.rs` -- confirmed 13-variant BlufioError without retryability/severity
- `blufio-core/src/format.rs` -- confirmed FormatPipeline exists with RichContent/FormattedOutput
- `blufio-core/src/types.rs:107` -- confirmed ChannelCapabilities with 9 fields
- `Cargo.toml` -- confirmed tokenizers 0.21, ort =2.0.0-rc.11, ndarray 0.17

---

*Stack research for: Blufio v1.4 Quality & Resilience*
*Researched: 2026-03-08*
