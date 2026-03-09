# Architecture Patterns

**Domain:** Quality & Resilience for Rust AI Agent Platform (v1.4)
**Researched:** 2026-03-08
**Confidence:** HIGH (based on direct source code analysis of 35-crate workspace)

## Recommended Architecture

### Principle: Modify Existing Crates, No New Crates

All v1.4 features fit naturally into existing crate boundaries. Adding new crates would fragment the workspace and create unnecessary dependency complexity. The 35-crate workspace is already large; v1.4 is about depth (improving what exists), not breadth (adding new subsystems).

**Crate modification map:**

| Crate | Modifications | Why Here |
|-------|--------------|----------|
| `blufio-core` | Typed error hierarchy, extended `ChannelCapabilities`, `FormatPipeline` additions (Table/List content types) | Core traits/types crate; all adapters depend on it |
| `blufio-context` | Replace `len()/4` heuristic with accurate tokenizer-backed counting | Token estimation lives in `dynamic.rs` line 64 |
| `blufio-agent` | Circuit breaker wrapper around provider calls, degradation ladder integration | Agent loop is the call site for provider + channel + storage |
| `blufio-bus` | New `Resilience` event domain (circuit breaker state changes, degradation level changes) | Event bus is the pub/sub backbone |
| `blufio-prometheus` | Circuit breaker + degradation metrics | Observability is already centralized here |
| `blufio-telegram`, `blufio-discord`, `blufio-slack`, `blufio-whatsapp`, `blufio-signal`, `blufio-irc`, `blufio-matrix`, `blufio-gateway` | Wire `FormatPipeline`, update `capabilities()` return values | Each adapter owns its formatting; pipeline replaces ad-hoc formatting |

### Component Boundaries

| Component | Responsibility | Communicates With |
|-----------|---------------|-------------------|
| `blufio-core::error` | Typed error hierarchy with `is_retryable()`, `severity()`, `category()` | Every crate (all use `BlufioError`) |
| `blufio-core::format` | Content degradation pipeline + Table/List content types | Channel adapters (call `FormatPipeline::format()`) |
| `blufio-core::types` | Extended `ChannelCapabilities` (streaming_type, formatting_support, rate_limits) | Channel adapters (return from `capabilities()`) |
| `blufio-context::token_counter` | New module: accurate token counting via HuggingFace `tokenizers` crate | `blufio-context::dynamic` (replace heuristic), `blufio-agent` (budget checks) |
| `blufio-agent::circuit_breaker` | New module: per-dependency circuit breaker state machine | `blufio-agent::session` (wraps provider calls), `blufio-bus` (publishes state changes) |
| `blufio-agent::degradation` | New module: 6-level degradation ladder with auto-escalation | `blufio-agent::session` (adjusts behavior per level), `blufio-bus` (publishes level changes) |
| `blufio-bus::events` | Extended with `Resilience(ResilienceEvent)` domain | `blufio-agent` (publishes), `blufio-prometheus` (subscribes), webhooks (subscribes) |
| `blufio-prometheus::recording` | New circuit breaker + degradation gauge/counter metrics | `blufio-prometheus` (subscribes to bus events) |

### Data Flow

#### Normal Request Flow (unchanged)

```
InboundMessage -> ChannelMultiplexer -> SessionActor
  -> ContextEngine (dynamic zone: token counting HERE)
  -> ProviderAdapter.stream() (circuit breaker wraps HERE)
  -> StreamingBuffer -> ChannelAdapter.send() (FormatPipeline HERE)
```

#### Circuit Breaker Flow (new)

```
SessionActor.process_message()
  -> CircuitBreaker.call(provider.stream(request))
     -> [Closed] pass through, record success/failure
     -> [Open] immediate Err(BlufioError::Provider { is_retryable: false })
              -> DegradationLadder.escalate()
     -> [HalfOpen] allow probe, transition on result
  -> On state change: EventBus.publish(Resilience::CircuitStateChanged)
  -> Prometheus: gauge blufio_circuit_state{dependency="anthropic"}
```

#### Degradation Ladder Flow (new)

```
DegradationLadder (6 levels):
  L0: Normal         - full features, all models available
  L1: CostReduction  - force Haiku for all queries, skip memory search
  L2: Simplified     - disable streaming, plain text only, skip skills
  L3: CacheOnly      - serve from cached/compacted context only
  L4: StaticResponse - pre-configured "service degraded" messages
  L5: Offline        - reject new messages, drain existing sessions

Triggers:
  - CircuitBreaker opens -> escalate one level
  - CircuitBreaker closes -> de-escalate one level
  - Budget exhaustion -> jump to L4
  - Storage failure -> jump to L5

Each transition -> EventBus.publish(Resilience::DegradationChanged)
                -> Prometheus gauge blufio_degradation_level
```

#### Token Counting Flow (modified)

```
BEFORE (dynamic.rs:64):
  estimated_tokens = history.iter().map(|m| m.content.len() / 4).sum()

AFTER:
  estimated_tokens = token_counter.count_tokens_batch(&history_texts)
  // where token_counter is initialized per-provider from tokenizer.json
```

## Patterns to Follow

### Pattern 1: Typed Error Enrichment (Extend BlufioError)

**What:** Add `is_retryable()`, `severity()`, and `category()` methods to `BlufioError` without breaking existing variant construction. Add structured fields to Provider/Channel variants.

**When:** Every error construction and match site.

**Approach:** The existing `BlufioError` enum (15 variants, `thiserror`-derived) uses string messages for `Provider` and `Channel` variants. Rather than adding new variants (which would break every `match`), enrich the existing variants with optional typed metadata and add trait methods.

```rust
// blufio-core/src/error.rs

/// Error severity for automated response decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    /// Transient error, retry likely to succeed.
    Transient,
    /// Degraded operation, partial functionality available.
    Degraded,
    /// Fatal error, operation cannot succeed.
    Fatal,
}

/// Error category for metric labeling and routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    Network,
    Authentication,
    RateLimit,
    Capacity,
    Validation,
    Internal,
}

impl BlufioError {
    /// Whether this error is safe to retry automatically.
    pub fn is_retryable(&self) -> bool {
        match self {
            BlufioError::Provider { message, .. } => {
                message.contains("429")
                    || message.contains("500")
                    || message.contains("503")
                    || message.contains("timeout")
            }
            BlufioError::Channel { message, .. } => {
                message.contains("rate limit")
                    || message.contains("timeout")
            }
            BlufioError::Timeout { .. } => true,
            BlufioError::Storage { .. } => false,
            _ => false,
        }
    }

    /// Error severity for circuit breaker and degradation decisions.
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            BlufioError::Timeout { .. } => ErrorSeverity::Transient,
            BlufioError::Provider { .. } if self.is_retryable() => ErrorSeverity::Transient,
            BlufioError::BudgetExhausted { .. } => ErrorSeverity::Fatal,
            BlufioError::Security(_) => ErrorSeverity::Fatal,
            BlufioError::Storage { .. } => ErrorSeverity::Fatal,
            _ => ErrorSeverity::Degraded,
        }
    }

    /// Category for metric labeling.
    pub fn category(&self) -> ErrorCategory {
        match self {
            BlufioError::Provider { message, .. } if message.contains("429") => {
                ErrorCategory::RateLimit
            }
            BlufioError::Provider { .. } => ErrorCategory::Network,
            BlufioError::Channel { .. } => ErrorCategory::Network,
            BlufioError::Timeout { .. } => ErrorCategory::Network,
            BlufioError::Security(_) => ErrorCategory::Authentication,
            BlufioError::BudgetExhausted { .. } => ErrorCategory::Capacity,
            BlufioError::Config(_) => ErrorCategory::Validation,
            _ => ErrorCategory::Internal,
        }
    }
}
```

**Why this approach:** Adding methods to the existing enum is backward-compatible. No existing `match` arms break. Provider crates can construct errors exactly as before. The `is_retryable()` check uses message heuristics initially -- these can be refined to use HTTP status codes extracted from the source error later, but starting with string matching avoids changing 17 `BlufioError::Provider` construction sites in blufio-anthropic alone.

### Pattern 2: Circuit Breaker as Wrapper, Not Trait Extension

**What:** Circuit breaker lives in `blufio-agent`, wrapping provider calls at the call site. It does NOT modify the `ProviderAdapter` trait.

**When:** Every `provider.stream()` and `provider.complete()` call in `SessionActor`.

**Why not a trait extension:** The `ProviderAdapter` trait is implemented by 5 provider crates. Adding circuit breaker logic to the trait would require changes in all 5 crates. Instead, the circuit breaker wraps the call in the agent loop -- the single place where all provider calls funnel through.

```rust
// blufio-agent/src/circuit_breaker.rs

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitBreakerInner>>,
    dependency: String,
}

struct CircuitBreakerInner {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    last_failure: Option<Instant>,
    failure_threshold: u32,
    cooldown: Duration,
    success_threshold: u32,
}
```

**Integration point in SessionActor:**

```rust
// In SessionActor::process_message(), replace:
//   let stream = self.provider.stream(request).await?;
// With:
//   let stream = self.circuit_breaker.call(
//       || self.provider.stream(request.clone())
//   ).await?;
```

### Pattern 3: Token Counter as Injectable Service

**What:** A `TokenCounter` that wraps the HuggingFace `tokenizers` crate, living in `blufio-context` because token counting is a context-engine concern (budget estimation, compaction thresholds).

**When:** Dynamic zone assembly (replacing `len()/4`), cost estimation, context budget checks.

**Why blufio-context, not blufio-memory:** The `tokenizers` crate is already a workspace dependency (used by blufio-memory for embedder tokenization). Token counting is fundamentally a context-engine operation -- the dynamic zone needs accurate counts for compaction thresholds. The blufio-memory OnnxEmbedder uses a sentence-transformer tokenizer (all-MiniLM-L6-v2), which is different from the LLM tokenizers needed here (Claude uses a custom BPE tokenizer, OpenAI uses cl100k_base/o200k_base). The token counter should be separate from the embedding tokenizer.

```rust
// blufio-context/src/token_counter.rs

use tokenizers::Tokenizer;

/// Accurate token counter using HuggingFace tokenizers.
///
/// Wraps a tokenizer loaded from tokenizer.json. Each provider
/// can ship its own tokenizer definition. Falls back to len()/4
/// if no tokenizer is available (graceful degradation).
pub struct TokenCounter {
    tokenizer: Option<Tokenizer>,
}

impl TokenCounter {
    /// Load from a tokenizer.json file path.
    pub fn from_file(path: &Path) -> Result<Self, BlufioError> { /* ... */ }

    /// Fallback counter that uses the len()/4 heuristic.
    pub fn heuristic() -> Self {
        Self { tokenizer: None }
    }

    /// Count tokens in a single string.
    pub fn count(&self, text: &str) -> usize {
        match &self.tokenizer {
            Some(t) => t.encode(text, false)
                .map(|enc| enc.len())
                .unwrap_or_else(|_| text.len() / 4),
            None => text.len() / 4,
        }
    }

    /// Count tokens in a batch of strings (more efficient).
    pub fn count_batch(&self, texts: &[&str]) -> usize {
        texts.iter().map(|t| self.count(t)).sum()
    }
}
```

**Key decision: Ship tokenizer.json files bundled with the binary or download on first use.**
Recommendation: Use `include_bytes!()` to embed a default Claude tokenizer.json at compile time (~1.5MB). For other providers, download on first use and cache in data directory. This keeps the binary self-contained for the default (Anthropic) use case while supporting other providers.

### Pattern 4: FormatPipeline Integration via Explicit Call Sites

**What:** Channel adapters call `FormatPipeline::format()` before sending outbound messages, using their own `ChannelCapabilities` to determine degradation.

**When:** In each adapter's `send()` implementation and streaming finalize.

**Why not middleware:** The format pipeline is not middleware -- it requires the adapter's specific capabilities which are known at construction time. Each adapter calls it explicitly before its platform-specific send.

```rust
// Example: blufio-telegram/src/lib.rs send() modification

async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> {
    // NEW: If the message contains RichContent, format through pipeline
    let formatted = FormatPipeline::format_text(&msg.content, &self.capabilities());
    let escaped = markdown::format_for_telegram(&formatted);
    // ... rest of existing send logic
}
```

The `FormatPipeline` gains a convenience method `format_text()` that detects content type markers in plain text (e.g., table/list patterns) and formats them appropriately per channel capabilities. This avoids changing the `OutboundMessage` type (which would require changes in all adapters simultaneously).

### Pattern 5: Event Bus Resilience Domain

**What:** New `Resilience` variant in `BusEvent` for circuit breaker and degradation events.

**When:** Circuit breaker state transitions, degradation level changes.

```rust
// blufio-bus/src/events.rs additions

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResilienceEvent {
    CircuitStateChanged {
        event_id: String,
        timestamp: String,
        dependency: String,
        from_state: String,
        to_state: String,
        failure_count: u32,
    },
    DegradationChanged {
        event_id: String,
        timestamp: String,
        from_level: u8,
        to_level: u8,
        reason: String,
    },
}

// Add to BusEvent enum:
pub enum BusEvent {
    Session(SessionEvent),
    Channel(ChannelEvent),
    Skill(SkillEvent),
    Node(NodeEvent),
    Webhook(WebhookEvent),
    Batch(BatchEvent),
    Resilience(ResilienceEvent),  // NEW
}
```

**Impact:** The `BusEvent::event_type_string()` match is exhaustive, so adding this variant forces updating the match -- which is exactly what we want (compile-time verification that all consumers handle the new domain).

## Anti-Patterns to Avoid

### Anti-Pattern 1: Modifying ProviderAdapter Trait

**What:** Adding `circuit_breaker()`, `is_healthy()`, or retry logic to the `ProviderAdapter` trait.

**Why bad:** 5 provider crates (anthropic, openai, ollama, openrouter, gemini) implement this trait. Modifying the trait requires changes in all 5. The circuit breaker is a cross-cutting concern that belongs in the call site (agent loop), not in each provider. Each provider already has its own internal retry logic (e.g., `AnthropicClient::max_retries: 1`).

**Instead:** Wrap provider calls with `CircuitBreaker` in `SessionActor`.

### Anti-Pattern 2: Token Counter as a New Crate

**What:** Creating a `blufio-tokenizer` crate for token counting.

**Why bad:** Token counting is a utility function for context assembly. It has exactly one primary consumer (`blufio-context::dynamic`). Creating a separate crate adds a workspace member, a Cargo.toml, CI configuration, and cross-crate dependency management for what amounts to a ~50-line module.

**Instead:** Add `token_counter.rs` module to `blufio-context`.

### Anti-Pattern 3: Changing OutboundMessage to Include RichContent

**What:** Adding a `rich_content: Option<RichContent>` field to `OutboundMessage`.

**Why bad:** `OutboundMessage` is constructed in the agent loop and consumed by 8 channel adapters. Changing its shape requires updating construction sites in `blufio-agent` and all 8 adapters simultaneously. The agent loop produces text; the format pipeline converts text to channel-appropriate output.

**Instead:** `FormatPipeline` operates on text content with a `format_text()` convenience method that detects content type markers. The pipeline sits inside each adapter's `send()`, not in the message type.

### Anti-Pattern 4: Global Circuit Breaker

**What:** A single circuit breaker for all external dependencies.

**Why bad:** The LLM provider being down does not mean Telegram is down. A global circuit breaker would disable all channels when one provider fails.

**Instead:** Per-dependency circuit breakers: one for each provider, one for each external service (storage, MCP servers). The degradation ladder aggregates signals from individual breakers to determine the overall system level.

### Anti-Pattern 5: Degradation as Middleware

**What:** Implementing degradation as Tower middleware that intercepts requests.

**Why bad:** Degradation affects different aspects of the system differently (model selection, streaming behavior, skill availability). It is not a simple request/response transform. Tower middleware works for simple concerns (rate limiting, CORS) but degradation requires deep integration with the session actor's decision points.

**Instead:** `DegradationLadder` is a shared state object queried at decision points within the session actor. The ladder exposes `current_level()` and the session actor checks it before each significant operation (model selection, streaming mode, skill invocation).

## Scalability Considerations

| Concern | At 10 sessions | At 100 sessions | At 1000 sessions |
|---------|---------------|-----------------|------------------|
| Token counting | Negligible (~1ms per count) | Still negligible | Consider caching tokenizer instance per thread |
| Circuit breaker state | 1 per provider (~5 instances) | Same (shared across sessions) | Same (Arc + RwLock) |
| Degradation ladder | 1 global instance | Same | Same |
| Event bus load | +2 event types, minimal | Some events per circuit trip | Batch resilience events to reduce bus pressure |
| Prometheus metrics | +6 metrics | Same metrics, different label values | Cardinality bounded by dependency count |

## Dependency Graph (Build Order)

The following build order respects crate dependencies and ensures each phase can compile and test independently.

```
Phase 1: blufio-core (error enrichment + ChannelCapabilities extension + FormatPipeline Table/List)
   |
   +-- No new crate dependencies
   +-- All downstream crates recompile but DO NOT break (additive changes only)
   |
Phase 2: blufio-bus (add Resilience event domain)
   |
   +-- Depends on: Phase 1 complete (for ErrorSeverity/ErrorCategory if referenced)
   +-- Consumers (blufio-agent, blufio-prometheus) can ignore new variant until Phase 4/5
   |
Phase 3: blufio-context (token counter module + DynamicZone integration)
   |
   +-- Depends on: Phase 1 (uses BlufioError)
   +-- Independent of Phase 2
   +-- Adds workspace dep: tokenizers (already in workspace Cargo.toml)
   |
Phase 4: blufio-agent (circuit breaker + degradation ladder + wiring)
   |
   +-- Depends on: Phase 1 (typed errors), Phase 2 (resilience events), Phase 3 (token counter)
   +-- This is the integration phase -- brings it all together
   |
Phase 5: blufio-prometheus (resilience metrics)
   |
   +-- Depends on: Phase 2 (subscribes to Resilience events)
   +-- Can run in parallel with Phase 4
   |
Phase 6: Channel adapters (FormatPipeline + capabilities update)
   |
   +-- Depends on: Phase 1 (extended FormatPipeline + ChannelCapabilities)
   +-- 8 adapters, can be done in parallel
   +-- Each adapter is a leaf crate (no downstream dependents)
```

### Integration Points Summary

| Integration Point | Source | Target | Mechanism |
|-------------------|--------|--------|-----------|
| Typed errors -> Circuit breaker | `BlufioError::is_retryable()` | `CircuitBreaker::call()` | Method call on error |
| Circuit breaker -> Event bus | `CircuitBreaker` | `EventBus::publish()` | `Arc<EventBus>` injected |
| Circuit breaker -> Degradation | `CircuitBreaker` state change | `DegradationLadder::escalate()` | Callback / event handler |
| Degradation -> Session actor | `DegradationLadder::current_level()` | `SessionActor::process_message()` | Shared `Arc<RwLock<DegradationLadder>>` |
| Token counter -> Dynamic zone | `TokenCounter::count()` | `DynamicZone::assemble_messages()` | Injected via `DynamicZone::new()` |
| FormatPipeline -> Channel send | `FormatPipeline::format()` | `ChannelAdapter::send()` | Direct call in each adapter |
| Event bus -> Prometheus | `BusEvent::Resilience` | `PrometheusAdapter` subscriber | Broadcast subscription |
| ChannelCapabilities -> FormatPipeline | `ChannelAdapter::capabilities()` | `FormatPipeline::format()` | Passed as parameter |

## Configuration (TOML)

New configuration sections needed in `blufio-config`:

```toml
[resilience]
# Circuit breaker settings (per-dependency defaults)
circuit_failure_threshold = 5      # failures before opening
circuit_cooldown_secs = 30         # seconds before half-open probe
circuit_success_threshold = 2      # successes before closing

# Degradation ladder
degradation_auto_escalate = true   # auto-escalate on circuit open
degradation_auto_deescalate = true # auto-de-escalate on circuit close

[context]
# Token counting (extends existing context config)
tokenizer_path = ""  # custom tokenizer.json path; empty = use built-in
```

## Sources

- Direct analysis of 35-crate workspace source code (HIGH confidence)
- `blufio-core/src/error.rs`: Current 15-variant `BlufioError` enum
- `blufio-core/src/format.rs`: Existing `FormatPipeline` (defined but unused by adapters)
- `blufio-core/src/types.rs`: Current 9-field `ChannelCapabilities`
- `blufio-context/src/dynamic.rs:64`: The `len()/4` heuristic to replace
- `blufio-bus/src/events.rs`: Current 6-domain event bus (Session, Channel, Skill, Node, Webhook, Batch)
- `blufio-memory/src/embedder.rs`: HuggingFace `tokenizers` crate already in use for embedding tokenization
- `blufio-agent/src/session.rs`: `SessionActor` as the integration point for provider calls
- `blufio-anthropic/src/client.rs`: 17 `BlufioError::Provider` construction sites showing current error pattern
- `blufio-telegram/src/lib.rs`: Representative channel adapter with custom `format_for_telegram()` (to be replaced by pipeline)
- [HuggingFace tokenizers crate](https://github.com/huggingface/tokenizers) - Already in workspace deps (v0.21)
- [tiktoken-rs](https://docs.rs/tiktoken-rs/latest/tiktoken_rs/) - OpenAI tokenizer (not recommended; HF tokenizers covers all providers)
- [Circuit breaker patterns in Rust](https://github.com/dmexe/failsafe-rs) - Reference implementation (custom implementation preferred to avoid new dependency)
