---
phase: 06-model-routing-smart-heartbeats
plan: 03
type: summary
status: complete
commit: retroactive
duration: ~15min
tests_added: 5
tests_total: 370
---

# Plan 06-03 Summary: Integration wiring -- SessionActor routing, heartbeat spawn, budget downgrades

**Retroactive: created during Phase 12 verification**

## What was built

Wired the model router and heartbeat system into the agent loop, making per-message routing and proactive check-ins live in the running agent.

### Changes

1. **SessionActor routing** (`crates/blufio-agent/src/session.rs`)
   - SessionActor uses `ModelRouter` to select model per-message instead of static `self.model`
   - Per-message override prefix (`/opus`, `/haiku`, `/sonnet`) stripped from message content before LLM call
   - Budget utilization queried via `BudgetTracker` before each routing decision for budget-aware downgrades
   - `RoutingDecision` stored per-session for tool follow-up requests (reuses same model tier)
   - `CostRecord` includes `intended_model` alongside `actual_model` for every message
   - When routing disabled (`config.routing.enabled = false`), falls back to `anthropic.default_model`

2. **Heartbeat spawn** (`crates/blufio/src/serve.rs`)
   - `HeartbeatRunner` created with config, provider, storage, cost_ledger
   - Spawned as background tokio task with configurable interval (`config.heartbeat.interval_secs`)
   - Respects `CancellationToken` for graceful shutdown alongside Telegram polling and memory monitor
   - Logs execution results at info/debug level

3. **AgentLoop plumbing** (`crates/blufio-agent/src/lib.rs`)
   - Added `router: Arc<ModelRouter>` and `heartbeat_runner: Option<Arc<HeartbeatRunner>>` to `AgentLoop`
   - Passes router and heartbeat_runner to each SessionActor on creation
   - On-next-message delivery: checks `take_pending_heartbeat()` and prepends to response

4. **Shell integration** (`crates/blufio/src/shell.rs`)
   - Shell REPL mode uses same routing logic as serve mode

### Key design decisions

- **Per-session routing decision caching**: Tool follow-ups reuse the same routing decision as the original message, preventing model switches mid-conversation
- **Background heartbeat task**: Runs independently of the main agent loop, using tokio::select! with cancellation for clean shutdown
- **Routing disabled fallback**: When routing is off, all messages use `anthropic.default_model` -- zero overhead

## Verification

- `cargo build` compiles cleanly
- `cargo test --workspace` passes (routing integration in SessionActor, heartbeat task lifecycle)
