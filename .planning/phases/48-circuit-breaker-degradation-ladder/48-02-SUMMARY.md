---
phase: 48-circuit-breaker-degradation-ladder
plan: 02
subsystem: resilience
tags: [degradation-ladder, circuit-breaker, prometheus, health-api, cancellation-token, hysteresis]

# Dependency graph
requires:
  - phase: 48-circuit-breaker-degradation-ladder
    provides: CircuitBreaker FSM, CircuitBreakerRegistry, ResilienceEvent on EventBus
provides:
  - DegradationLevel 6-variant enum (L0-L5) with Display, from_u8/as_u8, name()
  - DegradationManager with AtomicU8 level, compute_level(), async run() event loop
  - EscalationConfig for primary_provider, primary_channel, hysteresis, drain timeout
  - Prometheus metrics (blufio_circuit_breaker_state, blufio_degradation_level, blufio_circuit_breaker_transitions_total)
  - Extended HealthResponse with degradation_level, degradation_name, circuit_breakers
  - /v1/health returns 503 for L4+ degradation
affects: [48-03, blufio-agent, blufio/src/serve.rs]

# Tech tracking
tech-stack:
  added: [tokio-util CancellationToken]
  patterns: [atomic-u8-level, select-hysteresis-timer, escalation-triggers-from-snapshots]

key-files:
  created:
    - crates/blufio-resilience/src/degradation.rs
  modified:
    - crates/blufio-resilience/src/lib.rs
    - crates/blufio-resilience/Cargo.toml
    - crates/blufio-prometheus/src/recording.rs
    - crates/blufio-gateway/src/handlers.rs
    - crates/blufio-gateway/src/server.rs
    - crates/blufio-gateway/src/lib.rs
    - crates/blufio-gateway/Cargo.toml
    - crates/blufio/tests/e2e_cross_contamination.rs

key-decisions:
  - "compute_level uses open_provider_count >= 2 for L3 (not total_open >= 2 with primary_provider), ensuring non-critical channel failures don't over-escalate"
  - "DegradationManager.run() uses tokio::select! with sleep_until for hysteresis timer concurrent with event reception"
  - "HealthResponse degradation fields are Option with skip_serializing_if for backward compatibility when resilience not wired"

patterns-established:
  - "AtomicU8 + Relaxed ordering for zero-cost level reads from agent loop"
  - "select! { event = rx.recv(), _ = sleep_until(deadline) } for concurrent event/timer handling"
  - "CancellationToken for L5 safe shutdown coordination between DegradationManager and serve.rs"
  - "register_resilience_metrics() called from register_metrics() for automatic metric registration"

requirements-completed: [CB-04, CB-05, DEG-01, DEG-02, DEG-03, DEG-04]

# Metrics
duration: 15min
completed: 2026-03-09
---

# Phase 48 Plan 02: DegradationManager + Prometheus Metrics + Health Extension Summary

**6-level DegradationManager with auto-escalation from circuit breaker state, hysteresis de-escalation, CancellationToken L5 shutdown, 3 Prometheus metrics, and /v1/health degradation visibility with 503 for L4+**

## Performance

- **Duration:** 15 min
- **Started:** 2026-03-09T13:23:31Z
- **Completed:** 2026-03-09T13:38:34Z
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments
- DegradationLevel enum (L0-L5) with DegradationManager processing circuit breaker events via reliable mpsc subscriber and computing escalation levels from registry snapshots
- De-escalation with configurable hysteresis timer (default 120s), one step at a time, with timer reset on any new escalation
- L5 SafeShutdown cancels CancellationToken (irreversible), shared with serve.rs for coordinated shutdown
- Three Prometheus metrics: blufio_circuit_breaker_state gauge, blufio_degradation_level gauge, blufio_circuit_breaker_transitions_total counter
- Extended HealthResponse with degradation_level, degradation_name, circuit_breakers map; 503 status for L4+

## Task Commits

Each task was committed atomically:

1. **Task 1: DegradationLevel enum and DegradationManager with escalation/de-escalation** - `e307f2c` (feat)
2. **Task 2: Prometheus metrics and /v1/health extension** - `2c6c188` (feat)

## Files Created/Modified
- `crates/blufio-resilience/src/degradation.rs` - DegradationLevel enum, EscalationConfig, DegradationManager with compute_level() and async run() event loop, 16 tests
- `crates/blufio-resilience/src/lib.rs` - Added pub mod degradation, re-exports
- `crates/blufio-resilience/Cargo.toml` - Added tokio and tokio-util dependencies
- `crates/blufio-prometheus/src/recording.rs` - 3 resilience metrics (register + record functions)
- `crates/blufio-gateway/Cargo.toml` - Added blufio-resilience dependency
- `crates/blufio-gateway/src/server.rs` - GatewayState extended with degradation_manager and circuit_breaker_registry
- `crates/blufio-gateway/src/lib.rs` - Gateway builder defaults new fields to None
- `crates/blufio-gateway/src/handlers.rs` - HealthResponse extended, get_health returns 503 for L4+
- `crates/blufio/tests/e2e_cross_contamination.rs` - Updated GatewayState construction

## Decisions Made
- compute_level() uses `open_provider_count >= 2` for L3 trigger, not a compound condition with total_open, to prevent non-critical channel failures from over-escalating
- DegradationManager.run() uses `tokio::select!` with `sleep_until(deadline)` for concurrent hysteresis timer and event reception
- HealthResponse degradation fields are `Option` with `skip_serializing_if = "Option::is_none"` for backward compatibility when resilience is not wired

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed L3 escalation trigger logic in compute_level()**
- **Found during:** Task 1 (DegradationManager implementation)
- **Issue:** Initial L3 condition `open_provider_count >= 2 || (primary_provider_open && total_open >= 2)` caused L3 when primary provider + non-critical channel were both open, which should only be L2
- **Fix:** Changed to `open_provider_count >= 2` only, requiring 2+ provider breakers open for L3
- **Files modified:** crates/blufio-resilience/src/degradation.rs
- **Verification:** All 16 degradation tests pass including hysteresis_resets_on_new_escalation
- **Committed in:** e307f2c (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Logic correction ensuring escalation triggers match the spec. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- DegradationManager ready for serve.rs wiring in Plan 03 (spawn background task, pass Arc to SessionActor)
- CancellationToken available for serve.rs select! with SIGTERM/SIGINT
- Prometheus metrics ready for recording from event handler wiring in Plan 03
- GatewayState.degradation_manager and circuit_breaker_registry ready for injection from serve.rs

## Self-Check: PASSED

- FOUND: crates/blufio-resilience/src/degradation.rs
- FOUND: crates/blufio-prometheus/src/recording.rs
- FOUND: crates/blufio-gateway/src/handlers.rs
- FOUND: .planning/phases/48-circuit-breaker-degradation-ladder/48-02-SUMMARY.md
- FOUND: commit e307f2c (Task 1)
- FOUND: commit 2c6c188 (Task 2)

---
*Phase: 48-circuit-breaker-degradation-ladder*
*Completed: 2026-03-09*
