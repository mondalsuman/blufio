# Phase 4: Context Engine & Cost Tracking - Context

**Gathered:** 2026-03-01
**Status:** Ready for planning

<domain>
## Phase Boundary

The agent assembles prompts intelligently using three-zone context (static/conditional/dynamic) with Anthropic prompt cache alignment, tracks every token spent across all features in a cost ledger with per-session and per-model attribution, and enforces configurable daily/monthly budget caps with warning thresholds and hard kill switches. Conversation history compacts automatically when approaching context window limits.

</domain>

<decisions>
## Implementation Decisions

### Context zone structure
- New `blufio-context` crate for the three-zone context engine, separate from blufio-agent
- Conditional zone stubbed with proper trait interface but kept empty — Phase 5 (Memory) and Phase 7 (Skills) will plug into it later
- Dynamic zone uses sliding window + compaction summary: recent messages verbatim, older messages summarized

### History compaction
- Compaction triggers on token threshold (e.g., when conversation history exceeds a percentage of the context window budget), checked before each LLM call
- LLM-generated summary using Haiku model for cost efficiency — higher quality than extractive summarization
- Compaction summaries persisted in SQLite (special message rows or metadata-tagged entries) — survives restarts without re-summarization

### Cost ledger & visibility
- Cost ledger stored as a new table in the existing blufio.db SQLite database — same backup story
- Per-LLM-call granularity: each API call gets a ledger row with session_id, model, feature type (message/compaction/tool), input_tokens, output_tokens, cache_read_tokens, calculated cost_usd, timestamp
- In-memory running daily/monthly totals for fast budget checks, plus structured tracing logs (info level) for audit trail
- New `blufio-cost` crate for cost tracking, budget enforcement, and ledger logic — independent from general storage

### Budget enforcement
- Warning notification at 80% of daily/monthly budget cap
- Hard stop at 100% — agent stops making LLM calls and returns a clear message ("Daily budget of $X reached. Resumes at midnight UTC.")
- Global caps only (daily + monthly) — no per-session limits for a personal agent
- Daily budget resets at midnight UTC

### Claude's Discretion
- Cache block structure: how to structure system prompt blocks with cache_control breakpoints for optimal Anthropic cache hit rates
- Compaction timing: inline (blocking before LLM call) vs background (after response) — balancing latency vs correctness
- Budget enforcement point: session actor gate vs context engine gate — wherever integration is cleanest
- Token counting strategy for cost calculation (tiktoken estimation vs post-response usage data)
- Exact token threshold percentage for compaction trigger
- Error handling and recovery patterns throughout

</decisions>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `context.rs` (blufio-agent): Current flat context assembly (system prompt + last 20 messages). Will be replaced by the three-zone engine but provides the integration pattern
- `TokenUsage` struct (blufio-core/types.rs): Already has input_tokens/output_tokens fields — extend for cache_read_tokens
- `CostConfig` (blufio-config/model.rs): Already has daily_budget_usd, monthly_budget_usd, track_tokens fields — wire to enforcement logic
- `SessionActor` (blufio-agent/session.rs): FSM session that calls assemble_context() and persist_response() with TokenUsage — integration point for cost recording
- `StorageAdapter` trait (blufio-core): Existing storage trait may need extension for cost ledger queries
- `AnthropicClient` (blufio-anthropic): HTTP client with retry — needs cache_control header support

### Established Patterns
- Trait-based adapter pattern: PluginAdapter + domain-specific traits (StorageAdapter, ProviderAdapter)
- Config via TOML with deny_unknown_fields — new crates follow this for their config sections
- Async tokio runtime with tokio-rusqlite for database access
- tracing crate for structured logging — cost events follow this pattern
- Result<T, BlufioError> everywhere — new crates extend BlufioError variants

### Integration Points
- `assemble_context()` in blufio-agent/context.rs — replaced by new blufio-context engine
- `SessionActor::handle_message()` — calls context engine, records cost after provider response
- `ProviderRequest` (blufio-core/types.rs) — needs extending to carry cache-aligned prompt structure
- `MessageRequest` (blufio-anthropic/types.rs) — system prompt field needs to support structured blocks with cache_control
- `ApiUsage` (blufio-anthropic/types.rs) — needs cache_creation_input_tokens and cache_read_input_tokens fields
- Database migrations — new cost_ledger table alongside existing sessions/messages/queue tables

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 04-context-engine-cost-tracking*
*Context gathered: 2026-03-01*
