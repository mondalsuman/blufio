# Phase 6: Model Routing & Smart Heartbeats - Context

**Gathered:** 2026-03-01
**Status:** Ready for planning

<domain>
## Phase Boundary

The agent automatically routes user-facing queries to the appropriate Claude model (Haiku for simple, Sonnet for standard, Opus for complex) based on query complexity classification, and runs background heartbeats cheaply on Haiku for proactive check-ins. Internal LLM calls (compaction, memory extraction) are NOT routed — they stay on their configured models.

</domain>

<decisions>
## Implementation Decisions

### Classification approach
- Heuristic rules only — zero latency, zero cost classification (no LLM pre-call)
- Fixed algorithm, Claude tunes internally — not user-configurable
- Classifier considers current message + recent context (last 2-3 messages) to track conversation momentum
- When classification is uncertain, default UP to Sonnet (prioritize quality over cost)

### Routing tiers
- Three tiers from day one: Haiku (simple), Sonnet (standard), Opus (complex)
- Matches success criteria directly — Opus reserved for multi-step reasoning, code generation, nuanced analysis
- Routing applies to user-facing messages only — internal calls (compaction, extraction) stay on configured models

### Model overrides
- Global config override: `routing.force_model = "sonnet"` bypasses classification entirely
- Per-message prefix override: user types `/opus analyze this...` or `/haiku what time is it` to force a model
- Both mechanisms coexist — global config for default behavior, per-message for power users

### Heartbeat purpose
- Proactive check-ins: review pending items, reminders, follow-ups ("You mentioned you'd review that doc today")
- Personal assistant behavior, not infrastructure monitoring
- Scheduling and skip-when-unchanged logic at Claude's discretion

### Heartbeat delivery
- User-configurable: `heartbeat.delivery = "immediate" | "on_next_message"`
- Immediate: heartbeat sends a Telegram message directly
- On next message: stores the proactive insight, weaves it into next user interaction
- Distinct visual format — prefix or header so user knows it's a check-in, not a response

### Budget-aware routing
- When daily budget >80% consumed: downgrade Opus to Sonnet, Sonnet to Haiku
- When daily budget >95% consumed: everything routes to Haiku
- Transparent notification to user when downgrades happen (e.g., "(Using Haiku — budget at 85%)")
- Graceful degradation before the existing hard kill switch at 100%

### Heartbeat budget
- Separate dedicated budget: `heartbeat.monthly_budget_usd = 10.0`
- Heartbeat budget cannot eat into conversation budget
- Matches $10/month success criterion

### Cost tracking
- Cost ledger tracks both `intended_model` and `actual_model` per call
- Enables reporting on how often budget forces downgrades
- Useful for tuning budget caps and routing thresholds over time

### Claude's Discretion
- Crate organization (standalone `blufio-router` vs module in `blufio-agent`)
- Heartbeat scheduling strategy (fixed interval vs event-driven vs hybrid)
- Skip-when-unchanged detection logic
- Heuristic classification algorithm details (keyword lists, length thresholds, complexity indicators)
- Max tokens per tier adjustments
- Heartbeat content generation prompt design
- Distinct heartbeat message format specifics

</decisions>

<specifics>
## Specific Ideas

- Per-message model override via prefix (e.g., `/opus`, `/haiku`, `/sonnet`) is Telegram-friendly — parse before routing
- Budget degradation thresholds (80%, 95%) should work with the existing BudgetTracker
- Heartbeat proactive messages should feel like a helpful assistant, not a cron job notification

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ProviderRequest.model` field: Already supports per-request model selection — routing just needs to set this before the call
- `blufio-cost/pricing.rs`: Has Haiku/Sonnet/Opus pricing tables with substring matching (`get_pricing()`)
- `BudgetTracker`: Existing budget checking and cost recording — extend for budget-aware routing queries
- `CostLedger` + `CostRecord`: Per-session, per-model attribution already works — extend schema for intended_model
- `FeatureType` enum: Has Message, Compaction, Extraction — add Heartbeat variant
- `AnthropicClient`: Already takes model per-request via `MessageRequest.model` — no changes needed

### Established Patterns
- Internal LLM calls already use cheap models: compaction uses `context.compaction_model` (Haiku), extraction uses `memory.extraction_model` (Haiku)
- Cost recording pattern: get pricing → calculate cost → create CostRecord → ledger.record() → tracker.record_cost()
- Config follows `deny_unknown_fields` pattern with sensible defaults

### Integration Points
- `SessionActor.handle_message()`: Currently uses `self.model` for all requests — routing intercepts here before context assembly
- `SessionActor.model` field: Replace with router call that returns the appropriate model per message
- `BlufioConfig`: Needs new `routing` and `heartbeat` sections
- `serve.rs`: Heartbeat background task spawns alongside Telegram polling and shutdown listener

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 06-model-routing-smart-heartbeats*
*Context gathered: 2026-03-01*
