---
phase: 48-circuit-breaker-degradation-ladder
plan: 04
subsystem: resilience
tags: [circuit-breaker, fallback, degradation, notifications, provider-routing, tier-mapping]

# Dependency graph
requires:
  - phase: 48-03
    provides: "CircuitBreakerRegistry, DegradationManager, SessionActor resilience integration, EventBus subscriber pattern"
provides:
  - "Fallback provider routing when primary circuit breaker opens (DEG-06)"
  - "Cross-provider tier-mapped model selection (map_model_to_tier)"
  - "Degradation level notification delivery to all active channels (DEG-05)"
  - "Notification dedup (max 1 per level per configurable window)"
affects: [resilience, agent-loop, session-actor, serve]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Fallback chain iteration with tier-mapped model switching"
    - "Best-effort notification broadcast to all connected channels"
    - "Per-level dedup with tokio::time::Instant tracking"

key-files:
  created: []
  modified:
    - "crates/blufio-agent/src/session.rs"
    - "crates/blufio-agent/src/lib.rs"
    - "crates/blufio-agent/src/delegation.rs"
    - "crates/blufio-test-utils/src/harness.rs"
    - "crates/blufio/src/serve.rs"

key-decisions:
  - "Clone ProviderRequest for fallback calls to preserve original for loop iteration"
  - "Tier mapping uses contains() pattern matching for model family detection (opus/sonnet/haiku)"
  - "Fallback provider registry reuses gateway's ConcreteProviderRegistry when available"
  - "Notification task spawned after mux.connect() but before mux is moved into AgentLoop"

patterns-established:
  - "map_model_to_tier: cross-provider model equivalence mapping (high/medium/low tiers)"
  - "Fallback chain iteration: skip providers with open breakers, record results in fallback's CB"

requirements-completed: [DEG-05, DEG-06]

# Metrics
duration: 12min
completed: 2026-03-09
---

# Phase 48 Plan 04: Gap Closure Summary

**Fallback provider routing with tier-mapped models when primary breaker opens, plus degradation notification delivery to all active channels with dedup**

## Performance

- **Duration:** 12 min
- **Started:** 2026-03-09T14:34:56Z
- **Completed:** 2026-03-09T14:46:45Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- SessionActor iterates fallback_chain when primary circuit breaker is open, mapping models to equivalent tiers (Opus->GPT-4o, Sonnet->GPT-4o-mini, Haiku->GPT-3.5)
- Fallback success sets last_call_was_fallback=true for cost record tagging
- Background notification task sends "[Blufio] Degraded/Recovered" messages to all active channels on level transitions
- Notification dedup prevents spam (max 1 per level per configurable 60-second window)
- Both escalation and recovery notifications are delivered

## Task Commits

Each task was committed atomically:

1. **Task 1: Fallback provider routing with tier-mapped models (DEG-06)** - `6c512f4` (feat)
2. **Task 2: Degradation notification delivery to all active channels with dedup (DEG-05)** - `b1a6149` (feat)

## Files Created/Modified
- `crates/blufio-agent/src/session.rs` - Added provider_registry, fallback_chain fields; map_model_to_tier() function; fallback chain iteration in handle_message
- `crates/blufio-agent/src/lib.rs` - Added provider_registry, fallback_chain fields to AgentLoop with setters; wired into SessionActorConfig
- `crates/blufio-agent/src/delegation.rs` - Added provider_registry: None, fallback_chain: Vec::new() to delegation SessionActorConfig
- `crates/blufio-test-utils/src/harness.rs` - Added provider_registry: None, fallback_chain: Vec::new() to test harness SessionActorConfig
- `crates/blufio/src/serve.rs` - Built fallback ConcreteProviderRegistry before config move; wired set_provider_registry/set_fallback_chain; spawned notification task after mux.connect()

## Decisions Made
- Clone ProviderRequest for each fallback attempt rather than consuming it, enabling multi-fallback iteration
- Tier mapping uses string contains() for model family detection -- simple, extensible pattern
- Fallback provider registry is built before config moves into AgentLoop, reusing gateway's registry when available
- Notification channels Arc is grabbed after mux.connect() and before mux is moved, matching bridge system pattern
- notification_dedup_secs extracted early in serve.rs before config is consumed

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed ProviderRequest borrow-after-move in fallback loop**
- **Found during:** Task 1 (Fallback routing implementation)
- **Issue:** assembled.request was consumed by fallback_provider.stream() in one loop iteration, making it unavailable for the next iteration
- **Fix:** Clone the request before passing to stream(), keeping the original intact for subsequent fallback attempts
- **Files modified:** crates/blufio-agent/src/session.rs
- **Verification:** cargo check --workspace passes
- **Committed in:** 6c512f4 (Task 1 commit)

**2. [Rule 3 - Blocking] Fixed config moved before fallback registry construction**
- **Found during:** Task 1 (serve.rs wiring)
- **Issue:** ConcreteProviderRegistry::from_config(&config) needed config reference, but config was moved into AgentLoop::new() before the fallback wiring
- **Fix:** Built fallback_provider_registry before config is moved, extracted fallback_chain and notification_dedup_secs early
- **Files modified:** crates/blufio/src/serve.rs
- **Verification:** cargo check --workspace passes
- **Committed in:** 6c512f4 (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Both auto-fixes necessary for correct Rust ownership semantics. No scope creep.

## Issues Encountered
- ConcreteProviderRegistry import was behind #[cfg(feature = "gateway")] -- moved to unconditional import since fallback routing needs it regardless of gateway feature

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- DEG-05 and DEG-06 verification gaps are now closed
- Phase 48 circuit breaker and degradation ladder is fully complete
- All resilience features (circuit breaker, degradation manager, fallback routing, notifications) are wired end-to-end

## Self-Check: PASSED

All 5 modified files verified present. Both task commits (6c512f4, b1a6149) verified in git log.

---
*Phase: 48-circuit-breaker-degradation-ladder*
*Completed: 2026-03-09*
