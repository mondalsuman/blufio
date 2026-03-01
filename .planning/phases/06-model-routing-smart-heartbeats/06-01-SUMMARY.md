---
phase: 06-model-routing-smart-heartbeats
plan: 01
type: summary
status: complete
commit: retroactive
duration: ~20min
tests_added: 15
tests_total: 340
---

# Plan 06-01 Summary: QueryClassifier, ModelRouter, RoutingConfig

**Retroactive: created during Phase 12 verification**

## What was built

Created the `blufio-router` crate with heuristic query complexity classification and budget-aware model routing. This enables routing simple queries to Haiku, standard queries to Sonnet, and complex queries to Opus.

### Changes

1. **QueryClassifier** (`crates/blufio-router/src/classifier.rs`)
   - Classifies messages into `ComplexityTier::Simple`, `Standard`, or `Complex` using 7 heuristic signals:
     - Message length (word count)
     - Exact match against SIMPLE_EXACT patterns ("hi", "hello", "thanks", etc.)
     - Simple question patterns ("what time", "how are you")
     - Complex indicators ("analyze", "implement", "debug", "refactor")
     - Code block detection (triple backticks)
     - Multi-sentence detection
     - Conversation momentum (recent context complexity)
   - Default-up rule: when confidence is below threshold and tier is Simple, upgrades to Standard
   - Zero-cost, zero-latency: no LLM pre-call, no network

2. **ModelRouter** (`crates/blufio-router/src/router.rs`)
   - `route()` with priority: per-message override > global force_model > classify > budget downgrade
   - Per-message overrides: `/opus`, `/haiku`, `/sonnet` prefixes (stripped from message content)
   - Budget-aware downgrades: 80% -> one tier down, 95% -> everything to Haiku
   - `RoutingDecision` tracks `intended_model` and `actual_model` for cost attribution

3. **RoutingConfig** (`crates/blufio-config/src/model.rs`)
   - Added `RoutingConfig` with `enabled`, `simple_model`, `standard_model`, `complex_model`, `force_model`, `simple_max_tokens`, `standard_max_tokens`, `complex_max_tokens`
   - Added `HeartbeatConfig` with `enabled`, `interval_secs`, `model`, `monthly_budget_usd`, `delivery`

4. **CostRecord intended_model** (`crates/blufio-cost/src/ledger.rs`)
   - Added `intended_model` field to `CostRecord` for tracking routing decisions in cost reports

5. **V4 Migration** (`crates/blufio-storage/migrations/V4__cost_ledger_intended_model.sql`)
   - Adds `intended_model` column to cost_ledger table

### Key design decisions

- **Heuristic-only classification**: No LLM pre-call reduces latency and cost; heuristics are surprisingly effective for tier assignment
- **Default-up on uncertainty**: When the classifier is unsure, routes to a more capable model rather than risk degraded responses
- **Budget downgrades preserve intended_model**: Cost ledger can report "would have used Opus but downgraded to Sonnet" for transparency

## Verification

- `cargo build` compiles cleanly
- `cargo test --workspace` passes (15 tests covering classification tiers, budget downgrades, model overrides, routing decisions)
