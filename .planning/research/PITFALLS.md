# Domain Pitfalls: v1.4 Quality & Resilience

**Domain:** Adding circuit breakers, graceful degradation ladders, typed error hierarchy, accurate token counting, format pipeline integration, ChannelCapabilities extension, and ORT upgrade to an existing 71,808 LOC Rust async agent platform (35 crates)
**Researched:** 2026-03-08
**Confidence:** HIGH for Rust async/tokio/thiserror specifics (verified against codebase); HIGH for ORT RC-to-stable changes (verified against GitHub release notes); MEDIUM for circuit breaker/degradation patterns (established industry patterns + Rust-specific verification)

---

## Critical Pitfalls

Mistakes that cause rewrites, cascading failures, or make the system worse than before the "improvement."

---

### Pitfall 1: Token Counting on the Hot Path Blocks Tokio Worker Threads

**What goes wrong:**
The `tokenizers` crate's `encode()` method is a synchronous, CPU-bound operation. The current codebase calls it synchronously in the embedder (`OnnxEmbedder::embed_text` at `crates/blufio-memory/src/embedder.rs:83`). Replacing the `len() / 4` heuristic in `DynamicZone::assemble_messages` (`crates/blufio-context/src/dynamic.rs:64`) with accurate tokenizer-based counting means calling `tokenizer.encode()` for every message in the conversation history on every turn. For a 50-message history, that is 50 synchronous encode calls. Each call takes 50-200 microseconds for short text, but 1-5ms for long messages (1K+ chars, CJK text, or code). If these calls happen on a tokio worker thread (inside an async fn), they block the cooperative scheduler. With 4 worker threads (default on a 4-core VPS), 4 concurrent sessions doing token counting simultaneously stall the entire runtime -- no other tasks can make progress, including channel polling, heartbeats, and health checks.

**Why it happens:**
The `tokenizers` crate is a Rust library that performs CPU-bound work. It has no async interface. Developers wrap the call in an `async fn` and assume it is safe because "it's fast" (sub-millisecond per call). But cooperative scheduling in tokio means any work that does not yield at an `.await` point starves other tasks. The problem is invisible in testing (single session, fast messages) and manifests only under concurrent load.

**Consequences:**
- Heartbeat timer fires late, triggering unnecessary heartbeat messages (cost waste)
- Channel adapter `receive()` calls delayed, causing message delivery latency spikes
- P99 latency grows nonlinearly with session count
- sd-notify watchdog timeout triggers if worker threads are blocked for >30s

**Prevention:**
Use `tokio::task::spawn_blocking` for all token counting operations. Create a dedicated `TokenCounter` struct that owns a `tokenizers::Tokenizer` (it is `Send` but not `Sync`, so wrap in `Arc<Mutex<Tokenizer>>` or clone per call -- the tokenizer is lightweight to clone). Batch all messages into a single `spawn_blocking` call rather than spawning one per message:

```rust
let messages = messages.clone();
let tokenizer = self.tokenizer.clone();
let counts = tokio::task::spawn_blocking(move || {
    messages.iter().map(|m| tokenizer.encode(&m.content, false)
        .map(|e| e.get_ids().len())
        .unwrap_or(m.content.len() / 4) // fallback on error
    ).collect::<Vec<_>>()
}).await?;
```

Never call `tokenizer.encode()` directly inside an `async fn` body. Add a test that measures elapsed wall time of token counting with 100 messages and asserts it does not block the tokio runtime (use `tokio::time::timeout` around a concurrent health check).

**Detection:**
- `tokio::runtime::metrics::RuntimeMetrics::worker_noop_count` increasing (workers starving)
- P99 latency spikes correlating with conversation length
- Heartbeat firing late (cost ledger shows heartbeat intervals > 2x configured)

**Phase to address:**
Token counting phase (first phase of v1.4). Must be designed with spawn_blocking from the start.

---

### Pitfall 2: Circuit Breaker Thresholds Tuned for Development Cause Production Cascading Failures

**What goes wrong:**
Circuit breakers with thresholds set during development (e.g., "open after 3 failures in 60 seconds") are either too aggressive or too lenient for production. Too aggressive: a single Anthropic API hiccup (502 during deployment) opens the circuit, and the agent becomes unresponsive for the entire cooldown period. Too lenient: the circuit stays closed during a sustained outage, sending requests into a black hole while accumulating timeouts that eat the budget. The half-open state is particularly dangerous: if the single probe request happens to succeed (lucky timing), the circuit closes and immediately floods the recovering service with backed-up requests -- the thundering herd problem.

**Why it happens:**
Developers test circuit breakers with artificial failure injection (mock returning errors). Real API failures have different characteristics: they come in bursts (provider deployment), they affect some endpoints but not others (rate limit on streaming but not non-streaming), and recovery is gradual (first request succeeds but second times out because the provider is warming caches). Thresholds that "feel right" in testing (3 failures = open, 30s cooldown) don't account for the bursty, partial-failure reality.

**Consequences:**
- Agent goes dark for 30-60 seconds on every minor provider hiccup (too aggressive)
- Agent burns budget sending requests to a down provider for minutes (too lenient)
- Thundering herd after half-open probe success crashes the recovering provider
- All 5 providers (Anthropic, OpenAI, Ollama, OpenRouter, Gemini) have different failure characteristics but share the same thresholds

**Prevention:**
1. Make thresholds per-provider and configurable in TOML, not hardcoded:
   ```toml
   [provider.anthropic.circuit_breaker]
   failure_threshold = 5
   success_threshold = 3
   cooldown_secs = 30
   half_open_max_requests = 1
   ```
2. Use a sliding window (not a simple counter) to avoid the "3 failures across 3 hours trips the breaker" problem.
3. In half-open state, allow exactly 1 request through. If it succeeds, allow 2, then 4 (exponential ramp-up, not instant close). This prevents the thundering herd.
4. Add jitter to the cooldown period: `cooldown_secs * (1.0 + random(0.0, 0.3))` so that if multiple providers open their circuits simultaneously (e.g., during a network partition), they don't all probe at the same time.
5. Emit Prometheus metrics for circuit state transitions: `blufio_circuit_state{provider="anthropic"} 0|1|2` (closed/open/half_open).
6. The circuit breaker MUST be below the retry logic layer, not above it. Retries should not count as separate failures for the circuit breaker -- only the final retry failure should increment the failure count.

**Detection:**
- Provider that was briefly unavailable causes agent to be unresponsive for the full cooldown
- Cost spikes during provider outages (too lenient -- keeps sending)
- Prometheus `blufio_circuit_state` transitions happening too frequently (threshold too low)

**Phase to address:**
Circuit breaker phase. Thresholds must be configurable from day one; do not hardcode and plan to make configurable later.

---

### Pitfall 3: Degradation Ladder That Never Recovers (Ratchet Effect)

**What goes wrong:**
A 6-level degradation ladder escalates through levels (e.g., 0=Normal, 1=ReduceContext, 2=DisableMemory, 3=SimplifyRouting, 4=MinimalPrompt, 5=EmergencyMode). Escalation triggers are well-defined (budget threshold, provider errors, latency spikes). But de-escalation is forgotten or implemented with a simple "if condition cleared, go to level 0." The system escalates to level 3 during a brief provider outage, the outage resolves, but the de-escalation check only runs on the next message. If the operator's sessions are idle, the system stays at level 3 indefinitely. When the next message arrives, it gets the degraded experience even though the system is healthy. Worse: if de-escalation goes directly from level 3 to level 0, all the features snap back simultaneously, potentially triggering the same overload that caused escalation (memory queries + full context + complex routing all hitting at once).

**Why it happens:**
Escalation is event-driven (error occurs -> escalate). De-escalation requires proactive polling (is the system healthy now?). Developers implement the exciting escalation logic but treat de-escalation as "just set it back to 0." The asymmetry between event-driven escalation and polling-based de-escalation means recovery logic is always weaker than failure logic.

**Consequences:**
- System permanently stuck at a degraded level after a transient issue
- Users experience degraded service long after the underlying problem is resolved
- If de-escalation is too aggressive (jump to 0), it causes the same overload that triggered escalation -- creating an oscillation loop between levels 0 and 3
- Memory search disabled at level 2+ means the agent loses personality, which operators notice and report as a bug

**Prevention:**
1. Implement de-escalation as a background timer task (not message-driven). Every 60 seconds, check if conditions for the current level still hold. If not, drop ONE level (not to 0).
2. Step-wise de-escalation: 5->4->3->2->1->0, with a minimum dwell time at each level (e.g., 2 minutes) before dropping further. This prevents oscillation.
3. Add hysteresis: the condition to escalate from level 1 to 2 should be stricter than the condition to stay at level 2. For example, escalate at 80% budget utilization, but de-escalate only when below 70%.
4. Log every level transition with `tracing::info!` including the reason and dwell time at the previous level.
5. Expose current degradation level as a Prometheus gauge: `blufio_degradation_level{} 0-5`.
6. Add a `blufio status` CLI command that shows the current degradation level and reason.
7. Include a manual override: `blufio degrade reset` to force level 0 (operator escape hatch for stuck states).

**Detection:**
- `blufio_degradation_level` gauge stays above 0 for hours after the triggering condition cleared
- Users report "agent seems dumber than usual" or "agent forgot my preferences" (memory disabled)
- Oscillation visible as rapid level transitions in metrics (0->3->0->3 every few minutes)

**Phase to address:**
Degradation ladder phase. De-escalation timer and step-wise recovery must be in the initial design, not added later.

---

### Pitfall 4: Typed Error Hierarchy Creates Variant Explosion That Makes Match Ergonomics Painful

**What goes wrong:**
The current `BlufioError` enum has 14 variants (Config, Storage, Channel, Provider, AdapterNotFound, HealthCheckFailed, Timeout, Vault, Security, Signature, BudgetExhausted, Skill, Mcp, Update, Migration, Internal). Adding typed metadata (is_retryable, severity, category) seems to require either: (a) adding sub-enums to each variant (ProviderError::RateLimit, ProviderError::AuthFailed, ProviderError::ModelNotFound...), creating a two-level match nightmare, or (b) adding methods like `is_retryable()` to the enum, which requires matching all 14+ variants to determine retryability. Either approach makes every `match` statement across 35 crates that handles `BlufioError` require updating. Since `BlufioError` is in `blufio-core` (the root dependency), any new variant requires recompiling the entire workspace.

**Why it happens:**
The existing error type is designed as a flat enum -- simple to construct, simple to display. Adding behavioral traits (retryability, severity) to a flat enum forces each variant to carry enough context to answer the question. Developers either add nested enums (complex to construct, painful to match) or add fields to existing variants (breaking change across all crates), or add methods that use a giant match (fragile -- new variant means updating the method).

**Consequences:**
- Adding a new error variant forces updating `is_retryable()`, `severity()`, and `category()` methods -- each is a match with 15+ arms
- Downstream crates that match on `BlufioError` break when variants are added (non-exhaustive helps but makes matching weaker)
- Two-level matching (`BlufioError::Provider(ProviderError::RateLimit { ... })`) is verbose and discourages proper error handling -- developers use `_ => false` for `is_retryable`, making everything non-retryable by default
- Error context is lost: `BlufioError::Provider { message: "rate limited" }` has the same type as `BlufioError::Provider { message: "model not found" }` but completely different retry semantics

**Prevention:**
1. Do NOT nest enums. Keep `BlufioError` flat. Instead, add an `ErrorContext` struct that carries behavioral metadata:
   ```rust
   pub struct ErrorContext {
       pub retryable: bool,
       pub severity: ErrorSeverity,
       pub category: ErrorCategory,
       pub retry_after: Option<Duration>,
   }
   ```
2. Add a single new variant or modify existing variants to optionally carry context:
   ```rust
   Provider {
       message: String,
       source: Option<Box<dyn std::error::Error + Send + Sync>>,
       context: Option<ErrorContext>,
   }
   ```
3. Provide builder methods on `BlufioError` for constructing with context:
   ```rust
   BlufioError::provider("rate limited").retryable().with_retry_after(Duration::from_secs(30))
   ```
4. Make `is_retryable()` a method on `BlufioError` with sensible defaults: Timeout is always retryable, BudgetExhausted is never retryable, Provider uses the context if present or defaults to false. This avoids the exhaustive match problem -- defaults are safe, context overrides when available.
5. Mark the enum `#[non_exhaustive]` to prevent downstream crates from relying on exhaustive matching.

**Detection:**
- PR review shows 15-arm match statements in `is_retryable()`
- Developers adding `_ => false` wildcard arms in error classification
- New variant added to BlufioError but `is_retryable()` not updated -- silent regression

**Phase to address:**
Typed error hierarchy phase. Must be designed before circuit breakers (circuit breakers consume `is_retryable()`).

---

### Pitfall 5: ORT RC-to-Stable Upgrade Breaks Module Paths, Value Extraction, and Feature Flags

**What goes wrong:**
The current codebase pins `ort = "=2.0.0-rc.11"` and uses:
- `ort::session::Session` and `ort::session::builder::GraphOptimizationLevel` (embedder.rs:13-14)
- `ort::value::TensorRef` (embedder.rs:15)
- `ort::inputs!` macro (embedder.rs:128)
- `session.run()` with named inputs
- `outputs[0].try_extract_tensor::<f32>()` returning `(shape, data)` (embedder.rs:136-138)

ORT rc.12 (released 2026-03-05) introduced breaking changes: module reorganization (`ort::tensor` -> `ort::value`), env var renaming (`ORT_LIB_LOCATION` -> `ORT_LIB_PATH`), session option renames, and new feature flag requirements (`api-24` must be enabled for latest features). The rc.11-to-rc.12 changes also updated `ndarray` from 0.16 to 0.17 (already done in this codebase, so safe). The stable 2.0.0 release (when it ships) will remove backward-compatibility re-exports for renamed items that are still available in rc.12 as deprecated.

If upgrading to rc.12 or stable, the following code breaks:
1. `try_extract_tensor::<f32>()` API may change return type (rc.9 changed from owned to borrowed references)
2. `with_denormal_as_zero` renamed to `with_flush_to_zero` (not used in current code, but future usage would break)
3. Feature flags: current features `["std", "ndarray", "download-binaries", "copy-dylibs", "tls-rustls"]` may need `api-24` added for full feature access under the new multiversioning system
4. The `unsafe impl Send for OnnxEmbedder` (embedder.rs:38) relies on `Session` being non-Send -- if the stable release makes `Session: Send`, the unsafe impl becomes unsound

**Why it happens:**
RC versions are explicitly unstable. Each RC can break API surfaces. The project pinned to rc.11 with `=2.0.0-rc.11` (exact version) which prevents accidental upgrades but also means no bug fixes or security patches land. The ORT project is actively reorganizing APIs before the stable release.

**Consequences:**
- Compilation failure on upgrade (module paths changed)
- Potential unsoundness if `Session` Send/Sync status changes and the `unsafe impl` is not re-evaluated
- Docker builds break if `ORT_LIB_LOCATION` env var is used in CI (renamed to `ORT_LIB_PATH`)
- Upgraded binary may not load existing ONNX models if ONNX Runtime version expectations change (rc.12 supports v1.17-v1.24 via multiversioning)

**Prevention:**
1. Before upgrading, audit all `ort::` imports in the workspace: `grep -r "ort::" crates/blufio-memory/src/`
2. Check if `Session` gains `Send` in the target version. If it does, remove the `unsafe impl Send` and `unsafe impl Sync` and the `Mutex<Session>` wrapper.
3. Add `api-24` feature flag if upgrading to rc.12+ and needing latest ONNX Runtime features.
4. If stable 2.0.0 is not yet released when v1.4 ships, upgrade to rc.12 (not stable) and document the pinned RC status in the ADR. Do not block v1.4 on ORT stable -- it may not ship for months.
5. Write an integration test that loads the quantized all-MiniLM-L6-v2 model, embeds a test string, and asserts the output shape is `[384]`. This catches API breakage at test time, not production time.
6. Pin the exact version (`=2.0.0-rc.12`) to prevent cargo update from pulling a future RC.

**Detection:**
- `cargo build` fails with "unresolved import `ort::session::Session`"
- Runtime panic in embedder with "failed to extract tensor" (API return type changed)
- Docker build fails with "ORT_LIB_LOCATION: unknown environment variable"

**Phase to address:**
ORT upgrade phase (should be a separate, small phase with its own ADR). Do not combine with other feature work.

---

### Pitfall 6: FormatPipeline Integration Silently Changes Existing Adapter Output

**What goes wrong:**
The existing `FormatPipeline` (at `crates/blufio-core/src/format.rs`) converts `RichContent` to `FormattedOutput` based on `ChannelCapabilities`. Currently, no adapter uses it -- the pipeline exists but is not wired in. When wiring it into the 8 channel adapters, developers insert the pipeline into the send path: `RichContent -> FormatPipeline::format() -> FormattedOutput -> adapter.send()`. The problem: the existing adapters already handle formatting internally. The Telegram adapter applies MarkdownV2 escaping. The Discord adapter constructs embeds. The Slack adapter builds Block Kit structures. Inserting the FormatPipeline before the adapter's own formatting creates double-formatting: text gets MarkdownV2-escaped, then the embed degradation adds `**bold**` markers which themselves need escaping. The output is garbled: `\*\*Status\*\*` instead of **Status**.

**Why it happens:**
The FormatPipeline was designed as a standalone component. The adapters were designed as standalone components. Neither knows about the other's formatting. The pipeline outputs markdown-style formatting (`**bold**`) which is correct for channels that support markdown, but the adapters then apply their own channel-specific escaping on top.

**Consequences:**
- Telegram messages show raw markdown instead of formatted text (double-escaping)
- Discord embeds get degraded to text even though the channel supports embeds (pipeline runs before the adapter checks capabilities)
- Slack messages lose Block Kit formatting because the pipeline converts to plain text
- IRC messages get markdown syntax that IRC does not render

**Prevention:**
1. The FormatPipeline must be the LAST step before the raw send, NOT an input to the adapter's existing formatting.
2. Better: make each adapter opt-in to the FormatPipeline rather than inserting it globally. The adapter calls `FormatPipeline::format()` internally when it receives `RichContent`, using its own `capabilities()` to drive degradation. The adapter is responsible for converting `FormattedOutput` to its platform-specific format.
3. Do not change the existing `send(OutboundMessage)` signature. Instead, add a new method `send_rich(RichContent)` with a default implementation that calls `FormatPipeline::format()` then `send()`. Adapters override `send_rich()` to handle rich content natively (Discord embeds, Slack blocks).
4. Add a test for EACH adapter that sends a `RichContent::Embed` and verifies the output is formatted correctly for that platform (not garbled by double-escaping).

**Detection:**
- `**bold**` appearing literally in Telegram messages (not rendered as bold)
- Discord adapter producing text blocks where embeds are expected
- Test that compares send output before and after FormatPipeline wiring shows different results

**Phase to address:**
FormatPipeline integration phase. Must audit each adapter's existing formatting before wiring in the pipeline.

---

## Moderate Pitfalls

---

### Pitfall 7: ChannelCapabilities Extension Breaks All Existing Adapter Implementations

**What goes wrong:**
The current `ChannelCapabilities` struct has 9 boolean fields. Adding new fields (streaming_type, formatting_support, rate_limits) means every adapter that constructs a `ChannelCapabilities` value needs updating. There are 10 places in the codebase that construct `ChannelCapabilities` (8 adapters + gateway + test-utils mock). Since `ChannelCapabilities` has no `Default` impl and no builder pattern, adding a new field is a compile error in all 10 locations simultaneously. This is correct behavior (the compiler catches it), but it means a "simple" capability extension touches 10 files across 10 crates.

**Why it happens:**
`ChannelCapabilities` was designed as a simple struct with public fields. No `#[non_exhaustive]`, no `Default`, no builder. Every consumer constructs it with struct literal syntax: `ChannelCapabilities { supports_edit: true, ... }`. Adding a field breaks every construction site.

**Prevention:**
1. Add `#[derive(Default)]` to `ChannelCapabilities` with sensible defaults (all false, None for optionals, streaming_type defaults to None).
2. Add `#[non_exhaustive]` to prevent future breakage when adding more fields.
3. Provide a builder or `new()` method that returns the default, with chainable setters:
   ```rust
   ChannelCapabilities::default()
       .with_edit(true)
       .with_embeds(true)
       .with_streaming_type(StreamingType::EditInPlace)
   ```
4. In the PR that adds new fields, migrate all 10 construction sites to use `..Default::default()` syntax to future-proof against further additions.
5. Keep new fields as `Option<T>` not `T` -- this allows adapters to signal "I don't know" rather than forcing a potentially incorrect boolean.

**Detection:**
- Adding a field to ChannelCapabilities causes 10+ compilation errors across the workspace
- New adapters always construct ChannelCapabilities with all new fields set to false/None because the developer didn't know what to put

**Phase to address:**
ChannelCapabilities extension phase. Add Default + non_exhaustive BEFORE adding new fields.

---

### Pitfall 8: Circuit Breaker State Shared Across Sessions Causes One User's Failure to Block All Users

**What goes wrong:**
A global per-provider circuit breaker is the obvious design: one `CircuitBreaker` per provider, shared by all sessions via `Arc<Mutex<CircuitBreaker>>`. But if provider failures are user-specific (e.g., one user's API key is rate-limited because of their usage on another platform, or one user's content triggers a safety filter), the global circuit breaker opens for ALL users. One user's API key hitting rate limits causes the circuit to open, blocking a different user whose key has headroom.

**Why it happens:**
The existing architecture uses a single provider adapter per provider type (one `Arc<dyn ProviderAdapter>` for Anthropic, shared by all sessions). The circuit breaker wraps the provider, so it naturally becomes global.

**Prevention:**
1. Circuit breakers should be per-provider, not per-session. Per-session is too granular (1000 sessions = 1000 circuit breakers, hard to monitor). Per-provider is correct for the single-binary, single-API-key model that Blufio uses (all sessions share one Anthropic API key).
2. However, differentiate between errors that indicate provider-wide problems (503, network errors, timeout) and errors that indicate request-specific problems (400 bad request, 413 content too long, safety filter). Only provider-wide errors should increment the circuit breaker failure count. Request-specific errors should be returned to the caller without affecting the circuit.
3. This is where `is_retryable()` on `BlufioError` matters: the circuit breaker should only count errors where `is_retryable() == true` as failures. Non-retryable errors (auth failed, invalid request) are the caller's problem, not the provider's.
4. Emit a Prometheus counter `blufio_circuit_failure{provider, error_class}` that separates counted vs uncounted errors.

**Detection:**
- All sessions fail when one session gets a 400 error
- Circuit opens due to non-transient errors (auth failure, content filter)
- Prometheus shows circuit opening with error_class="client_error" (should only open on server errors)

**Phase to address:**
Circuit breaker phase. Must classify errors before counting them, which depends on the typed error hierarchy.

---

### Pitfall 9: Token Counter Initialization Fails Silently When Model File Missing

**What goes wrong:**
The `tokenizers` crate requires a `tokenizer.json` file for each model. The accurate token counter will need tokenizer files for Claude (Anthropic uses a custom BPE tokenizer), GPT-4/GPT-4o (cl100k_base or o200k_base), Gemini (SentencePiece), and Ollama models (varies). If any tokenizer file is missing at startup, the system has two bad options: (a) fail to start entirely (too aggressive -- the agent should work even without perfect counting), or (b) fall back to `len() / 4` silently (defeats the purpose of accurate counting, and the operator thinks they have accurate counting when they don't).

**Why it happens:**
Different LLM providers use different tokenizers. There is no universal tokenizer. The `tokenizers` crate supports BPE (OpenAI, Anthropic) and SentencePiece (Gemini), but each requires its own model file. Shipping all tokenizer files in the binary increases binary size. Downloading them at runtime requires network access at startup.

**Prevention:**
1. Ship the cl100k_base tokenizer (GPT-4) embedded in the binary using `include_bytes!`. It is ~1.5MB and covers OpenAI and Anthropic approximately (within 5% accuracy for English, 10% for CJK). This is the default fallback.
2. For other providers, use the provider's own token counting API if available (Anthropic's `/v1/messages/count_tokens`, Gemini's `countTokens` endpoint) for pre-flight budget checks, and the embedded tokenizer for hot-path estimation.
3. When a provider-specific tokenizer is missing, log `tracing::warn!` ONCE per provider (not per message) and fall back to the embedded tokenizer. Never fall back silently to `len() / 4` -- that defeats the purpose.
4. Add a Prometheus gauge `blufio_token_counter_accuracy{provider, method}` where method is "exact", "approximate", or "heuristic" so operators can see which counting method is active per provider.
5. The `blufio doctor` command should report tokenizer availability per provider.

**Detection:**
- Cost tracking shows significant over/under-estimation compared to provider invoices
- `blufio doctor` reports "token counting: heuristic" when operator expected "exact"
- Budget gates trigger too early or too late due to inaccurate counting

**Phase to address:**
Token counting phase. Fallback strategy must be designed before implementation.

---

### Pitfall 10: Degradation Ladder Interacts Badly with Model Router Budget Downgrade

**What goes wrong:**
The existing `ModelRouter` already downgrades models when budget utilization is high (e.g., Opus -> Sonnet when daily spend > 80%). The new degradation ladder also reacts to budget pressure (level 3 = SimplifyRouting). If both systems act independently, they compound: the router downgrades from Opus to Sonnet, AND the degradation ladder disables memory + reduces context. The user gets a dramatically worse experience (wrong model + no memory + short context) when either system alone would have been sufficient. Worse: the degradation ladder's "SimplifyRouting" might override the router's decision, creating a conflict about which model to use.

**Why it happens:**
The model router and the degradation ladder are designed as independent systems. Neither knows about the other's state. Both react to the same signal (budget utilization) but take different actions. Without coordination, they stack.

**Prevention:**
1. The degradation ladder MUST be aware of the model router's state. If the router has already downgraded the model, the degradation ladder should not apply routing-related degradation (level 3 SimplifyRouting becomes a no-op).
2. Define a clear precedence: the degradation ladder controls which features are available; the model router controls which model is used. They should not overlap in responsibility.
3. Budget utilization thresholds must be different: if the router downgrades at 80%, the degradation ladder should escalate at 90% (not also at 80%).
4. Add a combined state view: `blufio status` shows both router state and degradation level together so operators can see the full picture.
5. Test the interaction explicitly: set budget to 85% and verify that EITHER the router downgrades OR the ladder escalates, but not both simultaneously.

**Detection:**
- User experiences both model downgrade AND feature degradation simultaneously
- Cost savings from router downgrade are insufficient because degradation ladder also activates
- `blufio status` shows router at "downgraded" AND degradation at level 3

**Phase to address:**
Degradation ladder phase. Must integrate with ModelRouter, not operate independently.

---

### Pitfall 11: Format Pipeline Table/List Content Types Have No Fallback for Length-Limited Channels

**What goes wrong:**
Adding `RichContent::Table` and `RichContent::List` content types to the FormatPipeline seems straightforward: render to markdown tables/lists, fall back to text if the channel doesn't support them. But IRC has a 512-byte line limit. A table with 5 columns and 10 rows renders to ~800 characters of text. The fallback exceeds the channel's `max_message_length` and either gets truncated (losing data) or rejected by the channel API (silent failure). List items with long text have the same problem. The FormatPipeline does not currently check `max_message_length` during degradation.

**Why it happens:**
The existing FormatPipeline degrades based on capability booleans (supports_embeds, supports_images) but does not consider length constraints. Adding new content types that produce variable-length text output exposes this gap.

**Prevention:**
1. The FormatPipeline must check `caps.max_message_length` after every degradation and truncate or split if the output exceeds the limit.
2. For tables: truncate columns that are too wide, then truncate rows if still too long, adding a `... (N more rows)` footer.
3. For lists: truncate after N items with `... and N more`.
4. Add a `FormattedOutput::MultiPart(Vec<String>)` variant for cases where degraded content exceeds the length limit and must be split across multiple messages.
5. Test with IRC's 512-byte limit: send a 10-row table and verify the output fits in 512 bytes or is properly split.

**Detection:**
- IRC messages truncated mid-word after FormatPipeline wiring
- Table data silently lost on length-limited channels
- Adapter returns error on send because message exceeds platform limit

**Phase to address:**
FormatPipeline integration phase. Length-aware degradation must be added alongside new content types.

---

## Minor Pitfalls

---

### Pitfall 12: Token Counter Disagrees with Provider's Count, Causing Budget Miscalculation

**What goes wrong:**
Local token counting with the `tokenizers` crate uses a tokenizer model that may not exactly match the provider's tokenizer. Anthropic's tokenizer is proprietary (not published as a tokenizer.json). OpenAI's o200k_base (used for GPT-4o) is different from cl100k_base (GPT-4). If the local counter says a message is 1,000 tokens but Anthropic reports 1,100 tokens, the budget tracker under-counts by 10%. Over thousands of messages, this accumulates into real money.

**Prevention:**
Use the provider's reported `TokenUsage` from responses as the source of truth for cost tracking (already done in `session.rs:399`). Use local counting ONLY for pre-flight estimation (budget gate check, compaction threshold). Never use local counting for billing. Log the discrepancy between estimated and actual token counts as a metric: `blufio_token_estimate_error{provider} = (estimated - actual) / actual`. If discrepancy exceeds 15%, log a warning.

**Phase to address:**
Token counting phase. Pre-flight vs post-flight counting distinction must be clear in the architecture.

---

### Pitfall 13: Circuit Breaker Prometheus Metrics Use Labels That Cause Cardinality Explosion

**What goes wrong:**
Adding labels like `{provider, endpoint, model, session_id}` to circuit breaker metrics creates a unique time series per session per model per endpoint. With 100 sessions and 5 models, that is 500+ time series per metric. Prometheus scrape becomes slow and memory usage grows. On a $4/month VPS with limited RAM, Prometheus OOMs.

**Prevention:**
Labels should be `{provider}` only for circuit state metrics. Error counters can add `{error_class}` (transient/permanent) but NOT session_id or model. Keep total label cardinality under 50 per metric.

**Phase to address:**
Circuit breaker phase. Define metric labels in the design document before implementation.

---

### Pitfall 14: Error Hierarchy Breaking Change Not Gated Behind a Major Version

**What goes wrong:**
Modifying `BlufioError` variants (adding context fields, changing variant shapes) is a breaking change for any external consumer of `blufio-core`. If Blufio ever publishes crates to crates.io, this breaks semver. Even internally, it forces recompilation of all 35 crates.

**Prevention:**
Mark `BlufioError` as `#[non_exhaustive]` before making changes. Add new variants rather than modifying existing ones. Use `Option<ErrorContext>` in existing variants to add metadata without changing the variant shape. Document in the ADR that this is a one-time migration and future additions will use the non_exhaustive escape hatch.

**Phase to address:**
Typed error hierarchy phase. `#[non_exhaustive]` must be added as the FIRST commit.

---

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| Token counting | Blocking tokio workers with CPU-bound encode calls | `spawn_blocking` for all tokenizer operations |
| Token counting | Tokenizer file missing for specific provider | Embedded fallback tokenizer + per-provider accuracy metric |
| Token counting | Local count disagrees with provider count | Use provider count for billing, local for estimation only |
| Circuit breakers | Thresholds too aggressive/lenient for production | Per-provider configurable thresholds in TOML |
| Circuit breakers | Non-retryable errors counted as failures | Only count `is_retryable() == true` errors |
| Circuit breakers | Half-open thundering herd | Exponential ramp-up in half-open, not instant close |
| Circuit breakers | Prometheus cardinality explosion | Labels: `{provider}` only, not per-session |
| Degradation ladder | Ratchet effect -- never de-escalates | Background timer with step-wise de-escalation + hysteresis |
| Degradation ladder | Compounds with model router downgrade | Coordinate thresholds; don't overlap responsibilities |
| Typed errors | Variant explosion / match ergonomics | ErrorContext struct, not nested enums; non_exhaustive |
| Typed errors | Breaking change across 35 crates | Add #[non_exhaustive] first; use Option fields |
| FormatPipeline | Double-formatting with existing adapter output | Adapter calls pipeline internally, not externally imposed |
| FormatPipeline | Table/List exceeds max_message_length | Length-aware degradation with truncation/splitting |
| ChannelCapabilities | Adding fields breaks 10 construction sites | Add Default + non_exhaustive before new fields |
| ORT upgrade | Module path changes break compilation | Audit all ort:: imports; pin exact version |
| ORT upgrade | unsafe Send impl becomes unsound | Re-evaluate if Session gains Send in new version |
| ORT upgrade | Feature flags change | Add api-24 feature; test model loading in CI |

---

## Integration Pitfalls

Mistakes when these features interact with each other and with the existing system.

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| Circuit breaker + typed errors | Building circuit breaker before error hierarchy exists | Build typed errors first; circuit breaker consumes `is_retryable()` |
| Degradation ladder + circuit breaker | Degradation reacts to circuit state AND to errors independently | Degradation watches circuit state as an input, not raw errors |
| Token counting + context engine | Replacing `len()/4` in DynamicZone without maintaining backward-compatible threshold | Recalibrate compaction_threshold after switching to accurate counting (tokens != chars/4) |
| Token counting + cost ledger | Using estimated tokens for billing | Always use provider-reported TokenUsage for cost; local estimate for budget gate only |
| FormatPipeline + streaming | Running format pipeline on partial streaming chunks | Pipeline runs on complete messages only, not streaming deltas |
| ChannelCapabilities + FormatPipeline | Adding Table/List to FormatPipeline without adding corresponding capability flags | Add `supports_tables: bool`, `supports_lists: bool` to ChannelCapabilities first |
| Circuit breaker + degradation ladder | Both react to same signal (provider errors) | Circuit breaker owns provider health; ladder reads circuit state |
| ORT upgrade + token counting | Both touch blufio-memory dependencies | Do ORT upgrade in isolation first; then token counting in a separate phase |
| Typed errors + all provider crates | New error variants in blufio-core require all providers to update | Use #[non_exhaustive] and Option<ErrorContext> to minimize blast radius |
| Degradation level + heartbeat | Heartbeat continues at full cost during degradation | Degradation level 4+ should reduce heartbeat frequency or disable it |

---

## "Looks Done But Isn't" Checklist

- [ ] **Token counting:** Call `tokenizer.encode()` on CJK text (Chinese, Japanese, Korean) -- CJK has dramatically different token-to-char ratios (1 char = 1-3 tokens, not 0.25). If tests only use English, the counter "works" but is 4-12x wrong for CJK users.

- [ ] **Token counting on hot path:** Run a 50-message conversation and verify tokio worker threads are NOT blocked (use `tokio::runtime::Handle::current().metrics()` to check `worker_noop_count` or `tokio-console`).

- [ ] **Circuit breaker half-open:** After circuit opens and cools down, send exactly 1 probe request. If it fails, verify the circuit returns to OPEN (not stays in HALF_OPEN). If it succeeds, verify only a limited number of requests pass (ramp-up, not flood).

- [ ] **Circuit breaker error classification:** Send a 400 Bad Request to the provider and verify it does NOT increment the circuit breaker failure count. Only 429/500/502/503/timeout should count.

- [ ] **Degradation de-escalation:** Set degradation to level 3. Clear the triggering condition. Wait 5 minutes. Verify the system has de-escalated (not stuck at level 3).

- [ ] **Degradation + router interaction:** Set budget to 85%. Verify either the router downgrades OR the ladder escalates, not both.

- [ ] **Typed errors retryable:** Construct a `BlufioError::Timeout` and verify `is_retryable()` returns true. Construct `BlufioError::BudgetExhausted` and verify `is_retryable()` returns false.

- [ ] **FormatPipeline + Telegram:** Send a `RichContent::Embed` through the Telegram adapter. Verify the output does NOT contain raw `**bold**` markdown that Telegram doesn't render (Telegram uses MarkdownV2 with different escaping).

- [ ] **FormatPipeline + IRC:** Send a `RichContent::Table` with 10 rows through IRC. Verify the output fits within 512 bytes or is properly split across messages.

- [ ] **ChannelCapabilities Default:** After adding new fields, verify that an adapter constructed with `..Default::default()` has sensible values (not accidentally claiming capability it doesn't have).

- [ ] **ORT upgrade:** After upgrading, run the embedding integration test that loads all-MiniLM-L6-v2 and produces a 384-dim vector. Verify the vector is identical (within f32 epsilon) to the vector produced by the old version -- ORT upgrades should not change inference results.

- [ ] **ORT unsafe Send:** After upgrade, check if `ort::session::Session` implements `Send`. If yes, remove the `unsafe impl Send for OnnxEmbedder` and the `Mutex<Session>` wrapper (replace with direct field).

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Token counting blocking workers | LOW | Wrap in spawn_blocking; no data changes needed |
| Circuit breaker too aggressive | LOW | Adjust thresholds in TOML config; restart |
| Circuit breaker too lenient | MEDIUM | Tighten thresholds; may need to add sliding window if using simple counter |
| Degradation ladder stuck | LOW | `blufio degrade reset` manual override; fix de-escalation timer |
| Degradation + router double-downgrade | MEDIUM | Coordinate threshold ranges; may need to restructure both systems |
| Error hierarchy breaks 35 crates | HIGH | If done wrong (nested enums), requires rewriting all error handling; plan carefully |
| FormatPipeline double-formatting | MEDIUM | Revert pipeline wiring; redesign as adapter-internal rather than external |
| ChannelCapabilities breaks 10 files | LOW | Add Default impl retroactively; mechanical fix |
| ORT upgrade compilation failure | LOW | Mechanical import path fixes; audit all ort:: usages |
| ORT unsafe Send unsoundness | MEDIUM | Requires careful evaluation of Session thread safety; may need architectural change |

---

## Dependency Ordering (Critical)

These features have strict dependencies that constrain implementation order:

```
1. Typed Error Hierarchy (no deps -- enables everything else)
   |
   v
2. Circuit Breakers (depends on is_retryable() from typed errors)
   |
   v
3. Degradation Ladder (depends on circuit state as input)
   |
   |-- 4a. Token Counting (independent, but recalibrates context engine thresholds)
   |
   |-- 4b. ChannelCapabilities Extension (independent, but must precede FormatPipeline)
   |       |
   |       v
   |-- 5. FormatPipeline Integration (depends on ChannelCapabilities extension)
   |
   v
6. ORT Upgrade (independent -- do in isolation, ideally last to minimize risk)
```

**Do NOT** implement circuit breakers before typed errors. The circuit breaker needs `is_retryable()` to classify errors. Without it, all errors count as failures, making the breaker too aggressive.

**Do NOT** implement FormatPipeline before ChannelCapabilities extension. The pipeline needs capability flags for new content types (tables, lists).

---

## Sources

- ORT 2.0.0-rc.12 release notes (breaking changes): https://github.com/pykeio/ort/releases/tag/v2.0.0-rc.12
- ORT 2.0.0-rc.11 release notes (ndarray update, metadata API changes): https://github.com/pykeio/ort/releases/tag/v2.0.0-rc.11
- ORT documentation (Session API, value extraction): https://ort.pyke.io/
- tokio spawn_blocking documentation: https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html
- tokio cooperative scheduling and blocking: https://tokio.rs/tokio/tutorial/spawning
- HuggingFace tokenizers crate: https://crates.io/crates/tokenizers
- Circuit breaker pattern (Microsoft): https://learn.microsoft.com/en-us/azure/architecture/patterns/circuit-breaker
- Graceful degradation in distributed systems: https://www.geeksforgeeks.org/system-design/graceful-degradation-in-distributed-systems/
- AWS graceful degradation best practices: https://docs.aws.amazon.com/wellarchitected/latest/reliability-pillar/rel_mitigate_interaction_failure_graceful_degradation.html
- Rust error handling with thiserror (anti-patterns): https://nrc.github.io/error-docs/error-design/error-type-design.html
- Effective Rust error design: https://effective-rust.com/errors.html
- Iroh project error handling (backtraces + thiserror): https://www.iroh.computer/blog/error-handling-in-iroh
- Circuit breaker thundering herd prevention: https://iam.slys.dev/p/how-systems-handle-failure-retries
- tower-circuitbreaker crate: https://lib.rs/crates/tower-circuitbreaker
- Blufio codebase: `BlufioError` enum at `crates/blufio-core/src/error.rs` (14 variants, no behavioral methods)
- Blufio codebase: `FormatPipeline` at `crates/blufio-core/src/format.rs` (exists but not wired into adapters)
- Blufio codebase: `ChannelCapabilities` at `crates/blufio-core/src/types.rs:107` (9 fields, no Default, no non_exhaustive)
- Blufio codebase: `DynamicZone::assemble_messages` at `crates/blufio-context/src/dynamic.rs:64` (len()/4 heuristic)
- Blufio codebase: `OnnxEmbedder` at `crates/blufio-memory/src/embedder.rs` (unsafe Send, Mutex<Session>, ort rc.11 APIs)
- Blufio codebase: ort pinned at `=2.0.0-rc.11` in workspace Cargo.toml line 49
- Blufio codebase: tokenizers at `0.21` in workspace Cargo.toml line 50

---
*Pitfalls research for: v1.4 Quality & Resilience -- adding circuit breakers, graceful degradation, typed errors, accurate token counting, format pipeline integration, ChannelCapabilities extension, and ORT upgrade to existing 71,808 LOC Rust AI agent platform*
*Researched: 2026-03-08*
