---
phase: 04-context-engine-cost-tracking
plan: 01
subsystem: cost-tracking
tags: [cost, budget, pricing, anthropic, sqlite, tokio-rusqlite]

# Dependency graph
requires:
  - phase: 02-storage-vault
    provides: Database, tokio-rusqlite patterns, SQLite migrations
  - phase: 01-core-config
    provides: BlufioError, TokenUsage, CostConfig
provides:
  - CostLedger for recording per-call token/cost data
  - BudgetTracker for in-memory daily/monthly cap enforcement
  - ModelPricing with get_pricing() and calculate_cost()
  - V2 migration for cost_ledger table
  - Extended TokenUsage with cache_read_tokens, cache_creation_tokens
  - BudgetExhausted error variant
affects: [agent-loop, context-engine, observability]

# Tech tracking
tech-stack:
  added: []
  patterns: [cost-per-call recording, in-memory budget enforcement with DB recovery, per-million-token pricing]

key-files:
  created:
    - crates/blufio-cost/src/lib.rs
    - crates/blufio-cost/src/pricing.rs
    - crates/blufio-cost/src/ledger.rs
    - crates/blufio-cost/src/budget.rs
    - crates/blufio-cost/Cargo.toml
    - crates/blufio-storage/migrations/V2__cost_ledger.sql
  modified:
    - crates/blufio-core/src/types.rs
    - crates/blufio-core/src/error.rs
    - crates/blufio-core/src/lib.rs

key-decisions:
  - "Used blufio_config::model::CostConfig import path (not re-exported from crate root)"
  - "Pricing uses substring matching (contains 'opus'/'haiku'/'sonnet') with Sonnet as fallback for unknown models"
  - "CostLedger uses in-memory DB for tests, matching blufio-storage test patterns"
  - "BudgetTracker is not thread-safe (single-threaded agent loop design) -- no Arc/Mutex needed"

patterns-established:
  - "Cost recording: CostRecord::new() auto-generates UUID and timestamp"
  - "Budget recovery: BudgetTracker::from_ledger() re-hydrates totals on restart"
  - "Pricing reference: code comment with URL and date at top of pricing.rs"

requirements-completed: [COST-01, COST-02, COST-03, COST-05, COST-06]

# Metrics
duration: 6min
completed: 2026-03-01
---

# Phase 4 Plan 01: Cost Tracking Summary

**blufio-cost crate with SQLite cost ledger, daily/monthly budget caps with 80% warnings, and Anthropic model pricing table**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-01T10:04:21Z
- **Completed:** 2026-03-01T10:10:41Z
- **Tasks:** 1
- **Files modified:** 11

## Accomplishments
- Created blufio-cost crate with pricing, ledger, and budget modules (19 tests)
- Extended TokenUsage with cache_read_tokens and cache_creation_tokens (backward compatible via serde(default))
- Added BudgetExhausted error variant to BlufioError for budget enforcement
- V2 migration creates cost_ledger table with session/date/model indexes
- BudgetTracker supports restart recovery via from_ledger()

## Task Commits

Each task was committed atomically:

1. **Task 1: Extend core types and create blufio-cost crate with pricing, ledger, and budget** - `3218808` (feat)

## Files Created/Modified
- `crates/blufio-cost/Cargo.toml` - Crate manifest with workspace deps
- `crates/blufio-cost/src/lib.rs` - Public API re-exports
- `crates/blufio-cost/src/pricing.rs` - Model pricing table and calculate_cost()
- `crates/blufio-cost/src/ledger.rs` - CostLedger with SQLite persistence, CostRecord, FeatureType enum
- `crates/blufio-cost/src/budget.rs` - BudgetTracker with daily/monthly caps, 80% warnings, reset logic
- `crates/blufio-storage/migrations/V2__cost_ledger.sql` - cost_ledger table with 3 indexes
- `crates/blufio-core/src/types.rs` - Extended TokenUsage with cache fields
- `crates/blufio-core/src/error.rs` - Added BudgetExhausted variant
- `crates/blufio-core/src/lib.rs` - Updated error variant test

## Decisions Made
- Used `blufio_config::model::CostConfig` import path since CostConfig is not re-exported at crate root
- Pricing uses substring matching with Sonnet as fallback for unknown models (safe default)
- BudgetTracker is not thread-safe by design -- matches single-threaded agent loop pattern
- CostRecord::new() auto-generates UUID and ISO 8601 timestamp for convenience

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Created stub lib.rs for blufio-context crate**
- **Found during:** Task 1 (workspace compilation)
- **Issue:** blufio-context crate had Cargo.toml and static_zone.rs but no lib.rs, preventing workspace build
- **Fix:** Created minimal lib.rs with `pub mod static_zone;` re-export
- **Files modified:** crates/blufio-context/src/lib.rs
- **Verification:** Workspace compiles successfully
- **Committed in:** 3218808 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Necessary to unblock workspace compilation. No scope creep.

## Issues Encountered
- Pre-existing clippy warnings in blufio-agent and blufio-context (collapsible_if) -- out of scope, not fixed

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Cost crate ready for integration with agent loop (SessionActor records costs after each LLM call)
- Context engine can import TokenUsage cache fields for prompt caching cost tracking
- Budget enforcement ready to gate LLM calls in the message processing pipeline

---
*Phase: 04-context-engine-cost-tracking*
*Completed: 2026-03-01*
