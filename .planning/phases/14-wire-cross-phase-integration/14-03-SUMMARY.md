# Plan 14-03 Summary: Wire Prometheus Business Metric Call Sites

**Phase:** 14-wire-cross-phase-integration
**Plan:** 03
**Status:** Complete
**Duration:** ~15 min

## What Was Done

### Task 1: Agent loop metrics (lib.rs)
- Added `blufio-prometheus` as optional dependency to `blufio-agent/Cargo.toml` with `prometheus` feature flag
- Propagated prometheus feature from blufio to blufio-agent: `prometheus = ["dep:blufio-prometheus", "blufio-agent/prometheus"]`
- Added `record_message(&channel_name)` after debug log in `handle_inbound()`
- Added `record_error()` at two error sites: `handle_inbound` failure and channel receive error
- Added `set_active_sessions()` at both session creation paths (new and resumed)
- Added `record_latency()` using `Instant::now()` before `handle_message()` and elapsed time after first `consume_stream()`
- Added `classify_error_type()` helper function mapping BlufioError variants to prometheus labels: provider, security, storage, channel, skill, budget, timeout, agent

### Task 2: Session actor metrics (session.rs)
- Added `record_tokens()` in `persist_response()` after cost recording -- records per-model input/output tokens
- Added `set_budget_remaining()` in `persist_response()` using new `BudgetTracker::remaining_daily_budget()` method
- Added `record_tokens()` for compaction costs in `handle_message()`
- Added `record_tokens()` for extraction costs in `maybe_trigger_idle_extraction()`
- Added `remaining_daily_budget()` method to BudgetTracker in `blufio-cost/src/budget.rs`

### Task 3: Startup integration summary (serve.rs)
- Added integration status log before `agent_loop.run()` with security, redaction, and metrics status
- Integration summary displays: "Security: OK (TLS 1.2+ / SSRF protection) | Redaction: OK (RedactingWriter active) | Metrics: OK/WARN"
- Metrics status determined by prometheus adapter initialization (compiled vs. disabled vs. enabled)

## Files Modified

- `crates/blufio-agent/Cargo.toml` -- optional blufio-prometheus dep + prometheus feature
- `crates/blufio-agent/src/lib.rs` -- message, error, session, latency metrics + error classifier
- `crates/blufio-agent/src/session.rs` -- token and budget metrics in 3 cost recording paths
- `crates/blufio-cost/src/budget.rs` -- remaining_daily_budget() method
- `crates/blufio/Cargo.toml` -- feature propagation to blufio-agent
- `crates/blufio/src/serve.rs` -- integration status summary

## Verification

- `cargo build -p blufio-agent --features prometheus` -- compiles
- `cargo build -p blufio-agent` (no prometheus) -- compiles (zero overhead)
- `cargo build -p blufio` -- compiles with default features
- `cargo test --workspace` -- all tests pass, 0 failures
- All metric call sites gated behind `#[cfg(feature = "prometheus")]`

## Commit

`23ffd93` -- feat(14-03): wire Prometheus business metric call sites in agent code
