# Feature Research

**Domain:** Quality & Resilience for a multi-provider AI agent platform (Rust)
**Researched:** 2026-03-08
**Confidence:** HIGH

## Scope

This document covers only the NEW features targeted for v1.4 Quality & Resilience. All existing shipped features (FSM agent loop, 5 LLM providers, 8 channel adapters, FormatPipeline, ChannelCapabilities, StreamingBuffer, event bus, skill registry, gateway API, node system) are treated as foundation -- they are dependencies, not scope.

The v1.4 goal is to fix QA audit deviations: accurate token counting, circuit breakers, graceful degradation, typed errors, FormatPipeline wiring, ChannelCapabilities extension, and ADR documentation.

---

## Feature Landscape

### Table Stakes (Users Expect These)

Features operators assume exist once a system claims multi-provider LLM support with production resilience. Missing these means the system feels prototype-grade.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Accurate token counting per provider | Cost ledger accuracy, context window management, compaction trigger correctness. Current `len()/4` heuristic in `dynamic.rs:64` is off by 2-5x for CJK, code, and mixed content. Any system with a "cost ledger" that uses char-length heuristics is lying about costs. Compaction triggers at wrong thresholds, wasting money or truncating too aggressively. | MEDIUM | HuggingFace `tokenizers` 0.21 already in workspace (used by blufio-memory for embeddings). tiktoken-rs 0.9.1 covers OpenAI models (o200k_base for GPT-4o/o1/o3/o4, cl100k_base for GPT-4/3.5). Anthropic provides FREE `/v1/messages/count_tokens` API (100-8000 RPM by tier) but publishes NO local tokenizer for Claude 3+. Gemini provides FREE `:countTokens` endpoint (3000 RPM). Ollama returns `prompt_eval_count`/`eval_count` in responses and has `/api/tokenize` endpoint. |
| Per-dependency circuit breakers | Any system calling 5+ external APIs (LLM providers, channel platforms) without circuit breakers will cascade-fail. Provider goes down, all sessions stall, backlog grows, system OOMs. This is the number one cause of production outages in LLM agent systems. Blufio currently has ZERO circuit breakers -- every external call is fire-and-hope. | MEDIUM | Standard 3-state FSM (Closed/Open/HalfOpen). Build in-house: ~200 LOC of state machine logic. Existing Rust crates (circuitbreaker-rs, tower-circuitbreaker) add external dependencies for trivial logic. Blufio already uses tower but tower-circuitbreaker is a separate crate for what is a simple state machine. Per-dependency instances: 5 providers + 8 channels + MCP servers = ~15 circuit breakers. |
| Typed error hierarchy with retryability | Current `BlufioError` has 14 variants but no `is_retryable()`, `severity()`, or `category()`. Without this, the agent loop cannot make automated retry/fallback decisions. Every production system needs "should I retry this?" to be answerable without string-matching error messages. The circuit breaker needs error classification to decide whether a failure should count toward trip threshold. | MEDIUM | Extend existing `BlufioError` enum with methods. Add `ErrorSeverity` (Fatal/Degraded/Transient) and `ErrorCategory` (Network/Auth/RateLimit/Capacity/Internal/Config). Each variant maps to retryability via match arms. No breaking changes to existing code -- methods are additive. |
| FormatPipeline wired into adapters | FormatPipeline exists in `blufio-core/src/format.rs` but ZERO adapters use it. Grep confirms: no adapter crate imports FormatPipeline. Every adapter does its own ad-hoc string formatting. This defeats the purpose of having a centralized formatting pipeline and means format degradation is untested and inconsistent across all 8 channel adapters. | MEDIUM | Wire `FormatPipeline::format()` into each adapter's `send()` path. Adapters currently construct raw strings; they should construct `RichContent` and let the pipeline degrade based on `capabilities()`. The FormatPipeline already handles Embed and Image degradation. |
| ChannelCapabilities completeness | Current 9-field struct (`types.rs:107-126`) is missing critical metadata: streaming type (edit-in-place vs append-only vs no-streaming), formatting support (markdown vs html vs plaintext vs platform-specific), and rate limits. Without `streaming_type`, the agent loop cannot decide whether to use `StreamingBuffer` (edit-in-place) or append-only for a given channel. Without `formatting_support`, FormatPipeline cannot degrade markdown to plaintext for channels like IRC. | LOW | Add 3 fields to existing struct. All new fields can have sensible defaults via `Default` impl. No breaking changes. |

### Differentiators (Competitive Advantage)

Features that set Blufio apart from OpenClaw and other agent platforms. These address the "kill shot" weaknesses directly.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| 6-level graceful degradation ladder | Automatic escalation from "normal" through "reduced context" to "emergency static responses" with de-escalation when dependencies recover. OpenClaw has no degradation -- it either works or crashes (memory leaks to 300-800MB). This is the single most impactful resilience feature for a $4/month VPS that needs to run for months without restart. No other open-source AI agent platform implements structured degradation levels. | HIGH | Levels: L0-Normal, L1-ReducedContext (shrink context window), L2-SimplifiedModel (force Haiku), L3-NoTools (disable skill/MCP execution), L4-CachedResponses (use stored patterns), L5-StaticFallback (hardcoded "I'm having trouble, try again later"). Each level triggered by circuit breaker state + resource pressure. Automatic de-escalation when conditions clear. |
| Table and List content types in FormatPipeline | LLM responses frequently contain tabular data and bullet lists. Discord renders embeds with fields, Telegram uses HTML tables, IRC gets plain text rows, Signal gets plain text. No other agent platform handles structured content degradation across 8 channels. Adds `RichContent::Table` and `RichContent::List` to existing pipeline. | LOW | Table degrades: native table (if supported) -> markdown table -> plain text rows with alignment. List degrades: bullet points -> numbered text -> plain indented text. Each adapter's `ChannelCapabilities.formatting_support` drives which degradation path is taken. |
| Provider-aware token counting abstraction | Single `TokenCounter` trait with per-provider implementations: local BPE for OpenAI (tiktoken-rs), API call for Anthropic (free endpoint), API call for Gemini (free endpoint), response-based for Ollama, and configurable heuristic fallback. No other agent framework provides accurate token counting across 5 providers from Rust. | MEDIUM | Trait in blufio-core, implementations in each provider crate. Cache tokenizer instances (singletons for tiktoken-rs). The HuggingFace `tokenizers` crate already in workspace can serve as fallback for providers without dedicated tokenizers. Key insight: pre-call counting must be fast; use cached estimates for repeated system prompts and only API-count the dynamic portion. |
| Circuit breaker Prometheus integration | Every circuit breaker state transition emits Prometheus metrics. Operators see `blufio_circuit_breaker_state{dependency="anthropic"}` in Grafana and get alerted before users notice degradation. OpenClaw has no observability for dependency health. Blufio already ships Prometheus via the metrics crate. | LOW | Add gauge for state (0=closed, 1=half-open, 2=open), counter for trip events, histogram for recovery time. Integrates with existing blufio-prometheus crate. |
| Automatic degradation-level Prometheus metrics | `blufio_degradation_level` gauge lets operators set alerts at L2+ (model downgrade) and page at L4+ (cached responses). Combined with circuit breaker metrics, gives full picture of system health without log-diving. | LOW | Single gauge metric + labels per affected subsystem. |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Automatic provider failover in agent loop | "If Anthropic is down, auto-switch to OpenAI" | Different providers have different tokenizers, tool call formats, system prompt handling, and context window sizes. Mid-conversation failover produces incoherent responses. Provider-specific prompt caching is wasted. The cost model changes silently. Users cannot predict which model answered. | Circuit breaker + degradation ladder. When primary provider trips circuit, degrade to simpler behavior (smaller model on same provider at L2, then cached responses at L4) rather than silently switching providers. Let operators explicitly configure failover pairs if they want cross-provider failover. OpenRouter already handles multi-provider failover at the infrastructure level. |
| Per-character tokenizer for all providers | "Embed tokenizers for every provider locally so we never need API calls" | Anthropic deliberately does not publish Claude 3+ tokenizer. Embedding a stale or approximate tokenizer gives false confidence in counts. Gemini tokenizer changes between model versions. Maintaining tokenizer parity across 5 providers is a maintenance nightmare. | Use provider-native counting: tiktoken-rs for OpenAI (local, fast, exact), free API for Anthropic (accurate, server-side), free API for Gemini (accurate, server-side), response-based for Ollama (free, exact post-call), heuristic for OpenRouter (response-based after first call). Accept that pre-call counting for Anthropic/Gemini requires an API call. |
| Complex retry policies per error type | "Exponential backoff with jitter, different delays per error class, retry budgets" | Over-engineering for conversational agents. LLM API calls take 1-30 seconds. Retrying a failed 10-second call with exponential backoff means 20-60 seconds of user-visible latency. For a conversational agent, the user will re-send their message before any retry completes. Complex retry logic also masks the real problem (dependency is down) instead of surfacing it to the degradation ladder. | Simple retry: 1 immediate retry for transient errors (network glitches), then circuit breaker opens. For rate limits, respect Retry-After header with a single delay. For everything else, fail fast and let the degradation ladder handle it. |
| Dynamic format negotiation with channels | "Let channels declare formatting capabilities at runtime so new formats don't need code changes" | Channels have fixed capabilities -- Telegram's MarkdownV2 does not change at runtime. Adding runtime negotiation means capability discovery latency on every message. The 8 channels have well-known, stable capabilities that change only with platform updates (years apart). | Static `ChannelCapabilities` per adapter (already exists). Add new fields as needed. Compile-time is correct for capabilities that change once per platform major version. |
| Global error recovery orchestrator | "Central component that monitors all errors and orchestrates recovery across subsystems" | Single point of failure for recovery. Adds coordination overhead and distributed state. Makes error handling non-local and harder to reason about. A bug in the orchestrator takes down all recovery. | Per-dependency circuit breakers (local state machines) + degradation ladder (composable levels). Each subsystem handles its own errors; the degradation ladder is the composition mechanism that reads circuit breaker state. Local reasoning, global effect. |

---

## Feature Dependencies

```
TokenCounter trait
    |
    +--requires--> Provider-specific implementations (per provider crate)
    |                  |
    |                  +--requires--> tiktoken-rs dep (OpenAI crate)
    |                  +--requires--> HTTP client to /v1/messages/count_tokens (Anthropic crate)
    |                  +--requires--> HTTP client to :countTokens (Gemini crate)
    |                  +--requires--> Response parsing (Ollama -- already returns counts)
    |
    +--enables--> Accurate compaction threshold (blufio-context DynamicZone)
    +--enables--> Accurate cost ledger pre-estimation (blufio-cost)

TypedErrorHierarchy
    |
    +--requires--> BlufioError enhancement (blufio-core)
    |
    +--enables--> CircuitBreaker (needs is_retryable() to decide failure counting)
    +--enables--> GracefulDegradationLadder (needs severity() for escalation)

CircuitBreaker
    |
    +--requires--> TypedErrorHierarchy (is_retryable, severity)
    |
    +--enables--> GracefulDegradationLadder (circuit state drives level transitions)
    +--enhances--> Prometheus (state transition metrics)

GracefulDegradationLadder
    |
    +--requires--> CircuitBreaker (dependency health signal)
    +--requires--> TypedErrorHierarchy (error classification)
    |
    +--enhances--> Agent loop (adjusts behavior per level)
    +--enhances--> Prometheus (degradation level gauge)

FormatPipeline integration
    |
    +--requires--> Table + List content types (extend RichContent enum)
    +--requires--> ChannelCapabilities extension (formatting_support field)
    |
    +--enhances--> All 8 channel adapters (consistent formatting)

ChannelCapabilities extension
    |
    +--enhances--> FormatPipeline (formatting_support drives degradation path)
    +--enhances--> StreamingBuffer (streaming_type drives edit-vs-append decision)
    +--enhances--> Agent loop (rate_limits for throttling decisions)
```

### Dependency Notes

- **TypedErrorHierarchy must come before CircuitBreaker:** The circuit breaker's decision logic depends on error classification. It must know whether a failure should count toward the trip threshold. Network timeouts count; authentication errors do not (they will fail every time until config is fixed). Building them in reverse order would require retrofitting the circuit breaker's counting logic.
- **CircuitBreaker must come before GracefulDegradationLadder:** Degradation levels are driven by circuit breaker state. When the primary LLM provider circuit opens, the ladder escalates to L2 (simplified model). When multiple circuits are open, it escalates to L4-L5. Without circuit breakers, the ladder has no input signal.
- **TokenCounter is independent:** Can be built and shipped without waiting for circuit breakers or degradation ladder. Immediately fixes the `len()/4` heuristic and improves cost accuracy. No dependency on error hierarchy or resilience features.
- **FormatPipeline + ChannelCapabilities are independent:** No dependency on circuit breakers, error hierarchy, or token counting. Can be done in parallel with the resilience track.
- **Two parallel tracks emerge:** (1) Resilience: TypedErrors -> CircuitBreaker -> DegradationLadder. (2) Quality: TokenCounting + FormatPipeline + ChannelCapabilities. These tracks can be developed concurrently.

---

## MVP Definition

### Must Ship (v1.4)

- [x] **Typed error hierarchy** -- Foundation for all resilience features. Add `is_retryable()`, `severity()`, `category()` to `BlufioError`. Unblocks circuit breakers and degradation.
- [x] **Per-dependency circuit breakers** -- Prevents cascade failures. 3-state FSM per external dependency. Configurable thresholds in TOML. Prometheus metrics on state transitions.
- [x] **Graceful degradation ladder (6 levels)** -- The differentiator. Automatic escalation/de-escalation based on circuit breaker state and resource pressure. Solves the core value: "run for months without restart on $4/month VPS."
- [x] **Accurate token counting** -- Fixes the `len()/4` lie in cost ledger and compaction triggers. Provider-aware: tiktoken-rs for OpenAI, free API for Anthropic/Gemini, response-based for Ollama.
- [x] **FormatPipeline integration** -- Wire existing pipeline into all 8 adapters. Add Table + List content types. Fixes the gap where FormatPipeline exists but is unused.
- [x] **ChannelCapabilities extension** -- Add streaming_type, formatting_support, rate_limits. Enables informed decisions about streaming strategy and format degradation.

### Add After Validation (v1.5+)

- [ ] **Provider failover with session migration** -- Only after circuit breakers prove stable in production. Requires solving the mid-conversation context translation problem.
- [ ] **Adaptive token budget** -- Dynamically adjust context window budget based on provider response latency and error rates. Needs degradation ladder telemetry data first.
- [ ] **Custom degradation policies** -- Let operators define custom degradation behavior per level via TOML config.

### Future Consideration (v2+)

- [ ] **Cross-instance degradation coordination** -- When running multiple Blufio instances, share circuit breaker state via SQLite or gossip protocol.
- [ ] **ML-based anomaly detection** -- Use historical error patterns to predict failures before circuit breakers trip.

---

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Typed error hierarchy | HIGH | LOW | P1 -- Foundation, unblocks everything |
| Circuit breakers | HIGH | MEDIUM | P1 -- Prevents cascade failures |
| Token counting | HIGH | MEDIUM | P1 -- Fixes cost accuracy |
| Degradation ladder | HIGH | HIGH | P1 -- Core value differentiator |
| FormatPipeline integration | MEDIUM | MEDIUM | P1 -- Fixes existing dead code |
| ChannelCapabilities extension | MEDIUM | LOW | P1 -- Enables pipeline + streaming decisions |
| Table/List content types | LOW | LOW | P2 -- Polish, not critical for resilience |
| Circuit breaker Prometheus metrics | MEDIUM | LOW | P2 -- Observability enhancement |
| Degradation level Prometheus metrics | MEDIUM | LOW | P2 -- Observability enhancement |
| ORT stable upgrade + ADR | LOW | LOW | P2 -- Tech debt, not user-facing |
| Plugin architecture ADR | LOW | LOW | P2 -- Documentation, not code |

**Priority key:**
- P1: Must have for v1.4 milestone
- P2: Should have, add within v1.4 if time allows
- P3: Nice to have, future milestone

---

## Competitor Feature Analysis

| Feature | OpenClaw | LiteLLM | Portkey | Blufio v1.4 Approach |
|---------|----------|---------|---------|----------------------|
| Token counting | No counting -- injects ~35K tokens/turn regardless of query complexity | Provider-specific via `litellm.token_counter()` with local tokenizers | Uses provider APIs for billing | Provider-aware trait: tiktoken-rs local for OpenAI, free API for Anthropic/Gemini, response-based for Ollama. Cached system prompt estimates to minimize API calls. |
| Circuit breakers | None -- silent `catch {}` blocks hide failures | None built-in (relies on HTTP client retries) | Built-in circuit breakers with configurable thresholds | Per-dependency FSM with Closed/Open/HalfOpen, configurable thresholds (50% failure rate, 5 min calls, 30s open duration), Prometheus metrics on every transition |
| Graceful degradation | None -- either works or crashes with 300-800MB memory leak | Fallback to alternate models (provider failover only) | Provider failover + caching layer | 6-level ladder: L0 normal -> L1 context reduction -> L2 model simplification -> L3 tool disabling -> L4 cached responses -> L5 static fallback. Automatic escalation and de-escalation. |
| Error typing | Empty catch blocks, no classification | Basic HTTP status code retry logic | HTTP status code based with retry headers | Typed enum with `is_retryable()`, `severity()`, `category()` on every variant. Compile-time exhaustive matching ensures every error path is handled. |
| Format pipeline | None -- each channel adapter does ad-hoc formatting independently | N/A (not a channel system) | N/A (not a channel system) | Centralized FormatPipeline with capability-based degradation across 8 channels. RichContent types (Text, Embed, Image, CodeBlock, Table, List) degrade automatically. |
| Observability of resilience | Minimal logging, no structured metrics | Token usage logging only | Dashboard metrics for requests | Prometheus: circuit breaker state gauge, trip counter, recovery histogram, degradation level gauge, per-provider token counts, cost per provider |

---

## Token Counting Strategy Details

Because this is the most nuanced feature, additional detail on the per-provider strategy:

| Provider | Local Tokenizer | API Endpoint | Cost | Accuracy | Recommended Approach |
|----------|----------------|--------------|------|----------|---------------------|
| **OpenAI** | tiktoken-rs 0.9.1 (o200k_base for GPT-4o/o1/o3/o4/GPT-4.1/GPT-5, cl100k_base for GPT-4/3.5) | N/A needed | Free (local) | Exact | Use tiktoken-rs singleton. 2000+ projects use this crate. Proven at scale. Zero latency for pre-call estimates. |
| **Anthropic** | None for Claude 3+ (old claude-tokenizer crate covers pre-Claude-3 only) | `POST /v1/messages/count_tokens` | Free, 100-8000 RPM by tier | Exact (server-side) | API call for pre-estimation when needed. Response `usage` field for post-call tracking. Cache system prompt token count (does not change between calls). For DynamicZone compaction trigger, use HuggingFace tokenizers as local approximation to avoid API latency. |
| **Gemini** | None published | `POST models/{model}:countTokens` | Free, 3000 RPM | Exact (server-side) | API call for pre-estimation. Same caching pattern as Anthropic. Response includes `promptTokensDetails` with per-modality breakdown. |
| **Ollama** | Model-specific via `POST /api/tokenize` | Response includes `prompt_eval_count` + `eval_count` | Free (local) | Exact (model-native) | Use `/api/tokenize` for pre-estimation (model-aligned, no inference overhead). Response fields for post-call tracking. |
| **OpenRouter** | Varies by underlying model | Response includes `usage` field | Free (in response) | Exact (post-call) | For pre-estimation, use heuristic based on known model's tokenizer (tiktoken-rs if OpenAI model, HuggingFace if known). Post-call, use response usage. OpenRouter bills per underlying provider's tokenizer. |
| **Fallback** | HuggingFace `tokenizers` 0.21 (already in workspace via blufio-memory) | N/A | Free (local) | Approximate but much better than len()/4 | Load model-appropriate tokenizer.json if available. Use as local approximation when API calls would add unacceptable latency (e.g., compaction trigger). |

**Critical implementation note:** Pre-call token counting for Anthropic/Gemini adds network latency. Strategy to minimize impact:
1. Cache system prompt token count at session start (system prompt is static within a session).
2. For compaction threshold checks, use HuggingFace tokenizers locally (approximate but fast, ~10x better than len()/4).
3. Only use the provider API for precise pre-call budget gates where cost accuracy matters.
4. Always use response `usage` fields for post-call cost recording (zero additional latency).

---

## Circuit Breaker Configuration Defaults

Based on production patterns from Resilience4j, Microsoft Azure Architecture Center, and LLM-specific guidance:

| Parameter | Default | Rationale |
|-----------|---------|-----------|
| Failure rate threshold | 50% | Industry standard (Resilience4j default). Trip when half the calls in the sliding window fail. |
| Minimum call count | 5 | Do not trip on first few failures (could be transient startup issues). Ensures statistical significance. |
| Open state duration | 30s | LLM providers typically recover within 30s from transient issues. Long enough to avoid flapping, short enough to recover quickly for conversational use. |
| Half-open probe count | 3 | Allow 3 test calls before fully closing. Validates recovery is real, not a fluke. |
| Slow call threshold | 60s | LLM calls can legitimately take 30s+ for long responses with extended thinking. Only flag as slow above 60s. |
| Slow call rate threshold | 80% | If 80% of calls exceed 60s, something is systemically wrong (not just long responses). |
| Sliding window size | 20 calls | Enough history for meaningful rate calculation. Not so large that recovery is slow. |

These defaults should be configurable per-dependency in TOML:
```toml
[resilience.circuit_breaker.anthropic]
failure_rate_threshold = 0.5
min_call_count = 5
open_duration_secs = 30
half_open_probes = 3
```

---

## Degradation Ladder Level Details

| Level | Name | Trigger | Behavior Change | Recovery Condition |
|-------|------|---------|-----------------|---------------------|
| L0 | Normal | All circuits closed, resources normal | Full functionality | N/A (default state) |
| L1 | ReducedContext | Provider response latency >50% above baseline OR memory pressure >80% | Halve context window budget, skip conditional zone, trigger aggressive compaction | Latency returns to baseline for 2 minutes OR memory drops below 70% |
| L2 | SimplifiedModel | Provider circuit half-open OR 2+ consecutive slow call alerts | Force cheapest model (Haiku), disable model routing | Provider circuit fully closes |
| L3 | NoTools | Provider circuit open OR 3+ tool execution failures in 5 minutes | Disable skill/MCP tool execution, LLM-only text responses | Provider circuit enters half-open + tool errors clear |
| L4 | CachedResponses | Multiple provider circuits open OR budget >90% exhausted | Match user queries against stored response patterns, return cached answers | Any provider circuit enters half-open + budget headroom returns |
| L5 | StaticFallback | All provider circuits open OR system resource critical (OOM risk) | Return hardcoded "I'm experiencing issues, please try again later" with optional operator-customized message | Any provider circuit enters half-open |

**Escalation is immediate; de-escalation has hysteresis.** When conditions clear, wait for the recovery condition to hold for 2 minutes before de-escalating one level. This prevents flapping between levels during unstable recovery.

**Each level is additive.** L3 includes the effects of L1 and L2 (reduced context + simplified model + no tools).

---

## Sources

- [Anthropic Token Counting API](https://platform.claude.com/docs/en/build-with-claude/token-counting) -- FREE API, 100-8000 RPM by tier, supports all active models including images and PDFs (HIGH confidence, verified via official docs)
- [tiktoken-rs v0.9.1](https://github.com/zurawiki/tiktoken-rs) -- o200k_base for GPT-4o/o1/o3/o4/GPT-4.1/GPT-5, cl100k_base for GPT-4/3.5, actively maintained, 2000+ dependents (HIGH confidence)
- [Google Gemini countTokens API](https://ai.google.dev/api/tokens) -- Free, 3000 RPM, supports text + multimodal (HIGH confidence, verified via official docs)
- [Ollama tokenize/detokenize endpoints](https://github.com/ollama/ollama/pull/12030) -- `/api/tokenize` and `/api/detokenize` for model-aligned tokenization (MEDIUM confidence -- PR merged, verify endpoint availability in target Ollama version)
- [Token Counting Guide: tiktoken, Anthropic, Gemini](https://www.propelcode.ai/blog/token-counting-tiktoken-anthropic-gemini-guide-2025) -- Provider comparison and approach differences (MEDIUM confidence)
- [Resilience4j CircuitBreaker](https://resilience4j.readme.io/docs/circuitbreaker) -- Industry-standard thresholds, sliding window configuration, state machine design (HIGH confidence)
- [Microsoft Azure Circuit Breaker Pattern](https://learn.microsoft.com/en-us/azure/architecture/patterns/circuit-breaker) -- Production architecture guidance, state transition design (HIGH confidence)
- [Martin Fowler: Circuit Breaker](https://martinfowler.com/bliki/CircuitBreaker.html) -- Original pattern description (HIGH confidence)
- [Portkey: Retries, Fallbacks, and Circuit Breakers in LLM Apps](https://portkey.ai/blog/retries-fallbacks-and-circuit-breakers-in-llm-apps/) -- Layered resilience approach for LLM systems (MEDIUM confidence)
- [Circuit Breakers for LLM Services (Go implementation)](https://dasroot.net/posts/2026/02/implementing-circuit-breakers-for-llm-services-in-go/) -- LLM-specific circuit breaker implementation patterns (MEDIUM confidence)
- [Fail-Safe Patterns for AI Agent Workflows](https://engineersmeetai.substack.com/p/fail-safe-patterns-for-ai-agent-workflows) -- Agent-specific resilience patterns (MEDIUM confidence)
- [AWS Reliability Pillar: Graceful Degradation](https://docs.aws.amazon.com/wellarchitected/latest/reliability-pillar/rel_mitigate_interaction_failure_graceful_degradation.html) -- Production degradation patterns, soft vs hard dependencies (HIGH confidence)
- [Graceful Degradation with FeatureOps](https://www.getunleash.io/blog/graceful-degradation-featureops-resilience) -- Feature flag driven degradation design (MEDIUM confidence)
- [circuitbreaker-rs crate](https://lib.rs/crates/circuitbreaker-rs) -- Rust circuit breaker library, considered but not recommended (build in-house for ~200 LOC) (LOW confidence -- evaluated but not selected)
- [tower-circuitbreaker crate](https://lib.rs/crates/tower-circuitbreaker) -- Tower middleware circuit breaker, considered but adds dependency for trivial logic (LOW confidence -- evaluated but not selected)
- [Rust Error Type Design](https://nrc.github.io/error-docs/error-design/error-type-design.html) -- Best practices for error hierarchy design in Rust (HIGH confidence)
- [Error Handling in Correctness-Critical Rust (sled)](http://sled.rs/errors.html) -- Production error handling patterns from sled database (HIGH confidence)
- [OpenRouter API](https://openrouter.ai/docs/api/reference/overview) -- Token counting via response usage field, per-provider tokenizer billing (HIGH confidence)

---
*Feature research for: Blufio v1.4 Quality & Resilience*
*Researched: 2026-03-08*
