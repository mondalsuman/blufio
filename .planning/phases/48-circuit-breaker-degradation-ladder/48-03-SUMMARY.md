---
phase: 48-circuit-breaker-degradation-ladder
plan: 03
subsystem: resilience
tags: [circuit-breaker, degradation, fallback, cost-tagging, sd-notify, prometheus, session-actor]

# Dependency graph
requires:
  - phase: 48-01
    provides: CircuitBreakerRegistry, CircuitBreaker FSM, ResilienceEvent types
  - phase: 48-02
    provides: DegradationManager, Prometheus resilience metrics, health endpoint degradation fields
provides:
  - Full resilience wiring in serve.rs (registry construction, manager spawn, L5 shutdown, sd-notify STATUS)
  - SessionActor circuit breaker check/record around provider calls
  - L4+ canned response when degradation severe or higher
  - Cost fallback tagging with CostRecord.fallback field
  - AgentLoop and GatewayChannel resilience field injection
affects: [48-04-testing, future-observability, future-multi-provider]

# Tech tracking
tech-stack:
  added: [tokio_util::sync::CancellationToken]
  patterns: [circuit-breaker-check-record-around-calls, l5-shutdown-via-cancellation-token, sd-notify-status-from-event-bus, cost-fallback-tagging]

key-files:
  created: []
  modified:
    - crates/blufio/src/serve.rs
    - crates/blufio-agent/src/session.rs
    - crates/blufio-agent/src/lib.rs
    - crates/blufio-agent/src/delegation.rs
    - crates/blufio-cost/src/ledger.rs
    - crates/blufio-cost/src/budget.rs
    - crates/blufio-gateway/src/lib.rs
    - crates/blufio-test-utils/src/harness.rs
    - crates/blufio/Cargo.toml
    - crates/blufio-agent/Cargo.toml

key-decisions:
  - "sd-notify STATUS updates via EventBus subscriber in serve.rs rather than inside blufio-resilience crate -- keeps resilience crate free of sd-notify dependency"
  - "Provider name detection from config.providers feature flags (ollama model presence, openai/openrouter/gemini api_key presence) rather than hardcoded list"
  - "L4+ canned response returns a synthetic stream instead of calling any provider -- avoids unnecessary API calls during severe degradation"
  - "CostRecord.fallback field uses serde(default) for backward compatibility with existing stored records"
  - "AgentLoop resilience wiring via setter methods (set_circuit_breaker_registry, set_degradation_manager, set_provider_name) matching existing EventBus pattern"

patterns-established:
  - "Check/record pattern: registry.check() before external call, registry.record_result() after, with trips_circuit_breaker() to filter non-retryable errors"
  - "CancellationToken L5 propagation: DegradationManager cancels token, serve.rs select! branch drains and shuts down"
  - "EventBus-driven sd-notify: subscribe_reliable(256) in serve.rs, match DegradationLevelChanged, call notify_status()"
  - "Cost tagging via builder method: record.with_fallback(true) when last_call_was_fallback"

requirements-completed: [DEG-05, DEG-06]

# Metrics
duration: 20min
completed: 2026-03-09
---

# Phase 48 Plan 03: Application Wiring Summary

**Circuit breaker check/record around SessionActor provider calls, L5 shutdown via CancellationToken, sd-notify STATUS updates from EventBus, and cost fallback tagging with CostRecord.fallback field**

## Performance

- **Duration:** ~20 min
- **Started:** 2026-03-09T13:40:00Z
- **Completed:** 2026-03-09T14:00:00Z
- **Tasks:** 2
- **Files modified:** 11

## Accomplishments

- serve.rs constructs CircuitBreakerRegistry from ResilienceConfig, spawns DegradationManager background task, wires L5 shutdown via CancellationToken into main select!, spawns sd-notify STATUS updater and Prometheus event recorder from EventBus
- SessionActor checks circuit breaker before provider calls, records results after with trips_circuit_breaker() filtering, sends L4+ canned responses when degradation is severe
- CostRecord extended with fallback field for operator visibility into fallback provider spend
- AgentLoop, GatewayChannel, delegation.rs, and test harness all updated with resilience field plumbing

## Task Commits

Each task was committed atomically:

1. **Task 1: serve.rs wiring -- registry construction, manager spawn, L5 shutdown, sd-notify** - `17b6084` (feat)
2. **Task 2: SessionActor integration -- check/record, fallback routing, notifications, cost tagging** - `d7487e6` (feat)

## Files Created/Modified

- `crates/blufio/Cargo.toml` - Added blufio-resilience dependency
- `crates/blufio/src/serve.rs` - CircuitBreakerRegistry construction from config, DegradationManager spawn, EscalationConfig from provider feature flags, CancellationToken L5 shutdown in select!, sd-notify STATUS subscriber, Prometheus resilience event recorder, AgentLoop resilience wiring, GatewayChannel resilience injection
- `crates/blufio-agent/Cargo.toml` - Added blufio-resilience dependency
- `crates/blufio-agent/src/session.rs` - Circuit breaker check/record around provider.stream() calls, L4+ canned response, fallback cost tagging via last_call_was_fallback flag, Prometheus transition metrics
- `crates/blufio-agent/src/lib.rs` - AgentLoop resilience fields (circuit_breaker_registry, degradation_manager, provider_name) with setters, wired into SessionActorConfig construction
- `crates/blufio-agent/src/delegation.rs` - Default resilience fields (None/None/"anthropic") in delegation SessionActorConfig
- `crates/blufio-cost/src/ledger.rs` - CostRecord.fallback field with serde(default), with_fallback() builder method, test helper updated
- `crates/blufio-cost/src/budget.rs` - Test CostRecord construction updated with fallback field
- `crates/blufio-gateway/src/lib.rs` - GatewayChannel resilience fields (degradation_manager, circuit_breaker_registry) with Mutex<Option<Arc<_>>> pattern and setters, passed to GatewayState on connect()
- `crates/blufio-test-utils/src/harness.rs` - TestHarness SessionActorConfig updated with default resilience fields
- `Cargo.lock` - Updated for new workspace dependencies

## Decisions Made

1. **sd-notify STATUS via EventBus subscriber** -- Keeps blufio-resilience crate free of sd-notify dependency. serve.rs subscribes to DegradationLevelChanged events and calls notify_status() directly.
2. **Provider name detection from config feature flags** -- Detects available providers by checking config.providers.ollama.default_model.is_some(), config.providers.openai.api_key.is_some(), etc. rather than maintaining a separate provider list.
3. **L4+ canned response as synthetic stream** -- When degradation level reaches Severe (L4) or higher, SessionActor returns a pre-built "temporarily unavailable" stream without calling any provider, avoiding unnecessary API spend during outages.
4. **CostRecord.fallback with serde(default)** -- New boolean field defaults to false via serde, maintaining backward compatibility with records already stored in SQLite without migration.
5. **Setter-based resilience wiring on AgentLoop** -- Follows existing pattern established by set_event_bus(), keeping AgentLoop::new() signature stable and resilience opt-in.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] OllamaConfig has no `enabled` field**
- **Found during:** Task 1 (serve.rs wiring)
- **Issue:** Initial code referenced `config.ollama.enabled` but OllamaConfig only has `base_url` and `default_model` fields
- **Fix:** Changed to detect Ollama availability via `config.providers.ollama.default_model.is_some()`
- **Files modified:** crates/blufio/src/serve.rs
- **Committed in:** 17b6084

**2. [Rule 1 - Bug] Provider configs under `config.providers.*` not `config.*`**
- **Found during:** Task 1 (serve.rs wiring)
- **Issue:** BlufioConfig has `providers: ProvidersConfig` which contains `openai`, `ollama`, `openrouter`, `gemini` -- referenced as `config.openai` instead of `config.providers.openai`
- **Fix:** Updated all provider config references to use `config.providers.*` path
- **Files modified:** crates/blufio/src/serve.rs
- **Committed in:** 17b6084

**3. [Rule 3 - Blocking] Use of moved value `config`**
- **Found during:** Task 1 (serve.rs wiring)
- **Issue:** `config` is moved into `AgentLoop::new()` but L5 shutdown code needed `config.resilience.drain_timeout_secs` after the move
- **Fix:** Extracted `resilience_drain_secs` into a local variable before the config move
- **Files modified:** crates/blufio/src/serve.rs
- **Committed in:** 17b6084

**4. [Rule 3 - Blocking] Missing resilience fields in delegation.rs SessionActorConfig**
- **Found during:** Task 2 (SessionActor integration)
- **Issue:** delegation.rs constructs SessionActorConfig without new circuit_breaker_registry, degradation_manager, provider_name fields
- **Fix:** Added `circuit_breaker_registry: None, degradation_manager: None, provider_name: "anthropic".to_string()`
- **Files modified:** crates/blufio-agent/src/delegation.rs
- **Committed in:** d7487e6

**5. [Rule 3 - Blocking] Missing resilience fields in test harness SessionActorConfig**
- **Found during:** Task 2 (SessionActor integration)
- **Issue:** TestHarness constructs SessionActorConfig without new resilience fields
- **Fix:** Added same default fields as delegation.rs
- **Files modified:** crates/blufio-test-utils/src/harness.rs
- **Committed in:** d7487e6

**6. [Rule 3 - Blocking] Missing `fallback` field in CostRecord test constructors**
- **Found during:** Task 2 (cost fallback tagging)
- **Issue:** Two test files construct CostRecord directly (not via ::new()) and were missing the new `fallback` field -- caused test compilation failure
- **Fix:** Added `fallback: false` to both struct literals in budget.rs and ledger.rs test code
- **Files modified:** crates/blufio-cost/src/budget.rs, crates/blufio-cost/src/ledger.rs
- **Committed in:** d7487e6

---

**Total deviations:** 6 auto-fixed (2 bugs, 4 blocking)
**Impact on plan:** All auto-fixes necessary for compilation correctness. No scope creep.

## Issues Encountered

None beyond the auto-fixed deviations above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Full resilience pipeline is wired: breakers protect provider calls, degradation manager escalates/de-escalates based on circuit breaker state, L5 triggers safe shutdown, sd-notify STATUS reflects current level, Prometheus metrics record all transitions
- Ready for Phase 48-04 (integration testing) if planned, or complete as the final plan in the phase
- Fallback routing scaffolding is in place (L4+ canned response works); full multi-provider fallback chain routing would require additional provider adapter wiring in a future phase

## Self-Check: PASSED

All 11 modified files verified present. Both task commits (17b6084, d7487e6) verified in git log.

---
*Phase: 48-circuit-breaker-degradation-ladder*
*Completed: 2026-03-09*
