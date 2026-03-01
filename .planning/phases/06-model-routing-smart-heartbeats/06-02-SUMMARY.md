---
phase: 06-model-routing-smart-heartbeats
plan: 02
type: summary
status: complete
commit: retroactive
duration: ~15min
tests_added: 9
tests_total: 355
---

# Plan 06-02 Summary: HeartbeatRunner with Haiku, skip-when-unchanged, dedicated budget

**Retroactive: created during Phase 12 verification**

## What was built

Created the `HeartbeatRunner` in `blufio-agent` for proactive check-ins using Haiku. The heartbeat system runs on a configurable interval, generates reminders and follow-ups, and uses a dedicated budget tracker to enforce a $10/month cap.

### Changes

1. **HeartbeatRunner** (`crates/blufio-agent/src/heartbeat.rs`)
   - Periodic Haiku LLM calls with proactive check-in prompt
   - **Skip-when-unchanged logic**: `compute_state_hash()` hashes `(message_count, date)` via `DefaultHasher`; `should_skip()` returns true when hash unchanged since last execution
   - **Dedicated budget tracker**: Creates separate `BudgetTracker` with `monthly_budget_usd` from `HeartbeatConfig` (default $10.00); skips when budget exhausted
   - **Cost recording**: Records to `CostLedger` with `FeatureType::Heartbeat`
   - **Delivery modes**:
     - `on_next_message`: stores content in `pending_heartbeat` Mutex for the next user interaction
     - `immediate`: stores for external delivery
   - `take_pending_heartbeat()` returns and clears pending content
   - `notify_message_received()` increments internal counter for change detection
   - `gather_session_context()` reads recent messages from up to 5 active sessions
   - `build_heartbeat_prompt()` constructs system prompt with `NO_HEARTBEAT` sentinel for no-action responses

2. **FeatureType::Heartbeat** (already in `crates/blufio-cost/src/ledger.rs`)
   - `Heartbeat` variant enables separate cost tracking for heartbeat operations

### Key design decisions

- **Separate budget tracker**: Heartbeat costs are isolated from conversation costs, preventing heartbeats from consuming the main daily/monthly budget
- **Skip-when-unchanged**: Simple hash of (message_count, date) avoids unnecessary LLM calls when nothing has changed
- **NO_HEARTBEAT sentinel**: LLM returns "NO_HEARTBEAT" when nothing actionable, avoiding unnecessary notifications
- **on_next_message delivery**: Prepends heartbeat content to the next user response rather than sending out-of-band

## Verification

- `cargo build` compiles cleanly
- `cargo test --workspace` passes (9 tests covering state hash changes, sentinel detection, budget enforcement, counter increments)
