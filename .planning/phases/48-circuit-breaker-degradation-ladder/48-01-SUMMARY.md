---
phase: 48-circuit-breaker-degradation-ladder
plan: 01
subsystem: resilience
tags: [circuit-breaker, fsm, state-machine, mutex, registry, event-bus, config]

# Dependency graph
requires:
  - phase: 46-error-hierarchy-refactor
    provides: BlufioError with trips_circuit_breaker() and suggested_backoff()
provides:
  - CircuitBreaker 3-state FSM with configurable thresholds
  - CircuitBreakerRegistry with per-dependency Mutex-protected breakers
  - Clock trait with RealClock and MockClock for deterministic testing
  - CircuitBreakerSnapshot and CircuitBreakerTransition types
  - BusEvent::Resilience variant with CircuitBreakerStateChanged and DegradationLevelChanged
  - ResilienceConfig with defaults and per-dependency overrides
  - BlufioError::CircuitOpen variant for fast-fail signaling
affects: [48-02, 48-03, blufio-agent, blufio-gateway, blufio-prometheus]

# Tech tracking
tech-stack:
  added: [blufio-resilience crate]
  patterns: [clock-trait-injection, mutex-per-breaker, lazy-open-to-halfopen, serialized-probing]

key-files:
  created:
    - crates/blufio-resilience/Cargo.toml
    - crates/blufio-resilience/src/lib.rs
    - crates/blufio-resilience/src/circuit_breaker.rs
    - crates/blufio-resilience/src/clock.rs
    - crates/blufio-resilience/src/snapshot.rs
    - crates/blufio-resilience/src/registry.rs
  modified:
    - crates/blufio-core/src/error.rs
    - crates/blufio-bus/src/events.rs
    - crates/blufio-config/src/model.rs
    - Cargo.lock

key-decisions:
  - "BlufioError::CircuitOpen uses FailureMode::Internal so trips_circuit_breaker()=false and is_retryable()=false"
  - "MockClock wrapped in Arc for shared test access via ArcClock adapter pattern"
  - "Registry uses unwrap_or_else(|e| e.into_inner()) for mutex poisoning recovery"
  - "ResilienceConfig validates fallback_chain via separate validate_providers() method for startup cross-reference"

patterns-established:
  - "Clock trait injection: Box<dyn Clock + Send> for deterministic time testing"
  - "ArcClock adapter: allows shared MockClock across breaker and test code"
  - "Registry clock factory: new_with_clock_factory(configs, || Box::new(clock)) for test injection"
  - "record_probe_complete() before record_result() for HalfOpen probe serialization"

requirements-completed: [CB-01, CB-02, CB-03, CB-06, CB-07]

# Metrics
duration: 13min
completed: 2026-03-09
---

# Phase 48 Plan 01: Core Circuit Breaker FSM Summary

**Custom 3-state circuit breaker FSM with configurable thresholds, breaker registry, ResilienceEvent on EventBus, and ResilienceConfig with per-dependency overrides**

## Performance

- **Duration:** 13 min
- **Started:** 2026-03-09T13:06:13Z
- **Completed:** 2026-03-09T13:19:02Z
- **Tasks:** 2
- **Files modified:** 10

## Accomplishments
- CircuitBreaker FSM implementing Closed->Open->HalfOpen->Closed with consecutive failure model, lazy timeout transitions, and serialized HalfOpen probing
- CircuitBreakerRegistry holding independent breakers per dependency with Mutex protection and poisoning recovery
- BusEvent::Resilience as 7th event variant with CircuitBreakerStateChanged and DegradationLevelChanged sub-events
- ResilienceConfig with global defaults, per-dependency CircuitBreakerOverride, fallback chain validation (max 2)
- BlufioError::CircuitOpen for fast-fail signaling that does not trip breakers or count as retryable

## Task Commits

Each task was committed atomically:

1. **Task 1: Create blufio-resilience crate with CircuitBreaker FSM, Clock, Snapshot types** - `69c994c` (feat)
2. **Task 2: Registry, ResilienceEvent, ResilienceConfig, and config validation** - `e2775a5` (feat)

## Files Created/Modified
- `crates/blufio-resilience/Cargo.toml` - New crate manifest with blufio-core and blufio-bus dependencies
- `crates/blufio-resilience/src/lib.rs` - Module declarations and re-exports
- `crates/blufio-resilience/src/clock.rs` - Clock trait, RealClock, MockClock (cfg(test))
- `crates/blufio-resilience/src/snapshot.rs` - CircuitBreakerState, CircuitBreakerSnapshot, CircuitBreakerTransition
- `crates/blufio-resilience/src/circuit_breaker.rs` - CircuitBreaker FSM with check(), record_result(), snapshot()
- `crates/blufio-resilience/src/registry.rs` - CircuitBreakerRegistry with HashMap<String, Mutex<CircuitBreaker>>
- `crates/blufio-core/src/error.rs` - Added CircuitOpen variant with circuit_open() constructor
- `crates/blufio-bus/src/events.rs` - Added Resilience(ResilienceEvent) 7th BusEvent variant
- `crates/blufio-config/src/model.rs` - Added ResilienceConfig, CircuitBreakerDefaults, CircuitBreakerOverride

## Decisions Made
- BlufioError::CircuitOpen maps to FailureMode::Internal to ensure is_retryable()=false and trips_circuit_breaker()=false, since Internal is neither server-side nor retryable
- MockClock uses Arc wrapper (ArcClock) so test code can advance the clock while the breaker holds a Box<dyn Clock + Send>
- Registry uses unwrap_or_else(|e| e.into_inner()) for mutex poisoning recovery per research recommendation
- ResilienceConfig.validate() checks chain length; validate_providers() does cross-reference against known provider names (separate because provider list comes from different config section)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed proptest generator for CircuitOpen variant**
- **Found during:** Task 1 (adding CircuitOpen to error.rs)
- **Issue:** Used Just(BlufioError::CircuitOpen{..}) but BlufioError does not implement Clone, which Just requires
- **Fix:** Changed to Just("test-dep".to_string()).prop_map(|dep| BlufioError::CircuitOpen { dependency: dep })
- **Files modified:** crates/blufio-core/src/error.rs
- **Verification:** All 166 blufio-core tests pass including proptest properties
- **Committed in:** 69c994c (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Trivial fix for proptest compatibility. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- CircuitBreaker FSM and Registry ready for DegradationManager integration (Plan 02)
- ResilienceEvent ready for EventBus subscription by DegradationManager
- ResilienceConfig ready for startup wiring in serve.rs
- Clock trait ready for injection in integration tests

---
*Phase: 48-circuit-breaker-degradation-ladder*
*Completed: 2026-03-09*
