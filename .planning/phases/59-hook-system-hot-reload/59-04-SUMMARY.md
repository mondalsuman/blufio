---
phase: 59-hook-system-hot-reload
plan: 04
subsystem: hooks
tags: [hooks, hot-reload, serve-integration, lifecycle-hooks, doctor, arc-swap, event-bus]

# Dependency graph
requires:
  - phase: 59-hook-system-hot-reload plan 01
    provides: HookConfig, HookDefinition, HotReloadConfig types, ShellExecutor, RecursionGuard, HookEvent on BusEvent
  - phase: 59-hook-system-hot-reload plan 02
    provides: Config hot reload module with ArcSwap, spawn_config_watcher, load_config
  - phase: 59-hook-system-hot-reload plan 03
    provides: HookManager with BTreeMap priority dispatch, execute_lifecycle_hooks, validate_hook_events, spawn_tls_watcher, spawn_skill_watcher
provides:
  - HookManager fully wired in serve.rs with EventBus reliable channel subscription
  - Config hot reload spawned in serve.rs with XDG config path detection
  - TLS and skill hot reload watchers spawned when configured
  - All 4 lifecycle hooks invoked at correct serve.rs lifecycle points
  - Doctor check_hooks and check_hot_reload health checks
affects: [serve.rs, doctor.rs, session config isolation]

# Tech tracking
tech-stack:
  added: [blufio-hooks dependency in blufio crate]
  patterns: [Non-fatal subsystem init (warn + continue), Arc<HookManager> shared between run loop and direct lifecycle calls]

key-files:
  created: []
  modified:
    - crates/blufio/src/serve.rs
    - crates/blufio/src/doctor.rs
    - crates/blufio/Cargo.toml

key-decisions:
  - "HookManager::run takes &self (not self), so Arc<HookManager> shared between EventBus run loop and direct lifecycle calls"
  - "Config path determined at runtime from XDG hierarchy (local > user > system) for hot reload watcher"
  - "pre_shutdown fires after agent_loop.run() returns, post_shutdown fires after audit trail cleanup"
  - "Doctor check_hooks validates event names via validate_hook_events and checks absolute command paths"

patterns-established:
  - "Arc<HookManager> pattern: single manager shared between spawned run loop and direct lifecycle hooks"
  - "XDG config path detection: local blufio.toml > ~/.config/blufio/blufio.toml > /etc/blufio/blufio.toml"

requirements-completed: [HOOK-01, HOOK-05, HTRL-01, HTRL-02, HTRL-03, HTRL-05, HTRL-06]

# Metrics
duration: 6min
completed: 2026-03-12
---

# Phase 59 Plan 04: Serve Integration Summary

**HookManager and hot reload fully wired in serve.rs with lifecycle hooks (pre/post start/shutdown), config/TLS/skill watchers, and doctor health checks**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-12T20:05:28Z
- **Completed:** 2026-03-12T20:11:40Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- HookManager wired in serve.rs after EventBus with reliable channel subscription and CancellationToken graceful shutdown
- Config hot reload watcher spawned with XDG config path detection, TLS cert watcher stub, and skill directory watcher
- All 4 direct lifecycle hooks invoked at correct points: pre_start before main loop, post_start after all init, pre_shutdown after agent loop exit, post_shutdown after audit cleanup
- Doctor check_hooks validates enabled status, hook count, event names, and absolute command paths
- Doctor check_hot_reload reports enabled features (config debounce, TLS cert reload, skill watching)
- 7 new doctor tests covering disabled/enabled/invalid states for both checks
- All workspace tests pass (blufio-hooks 22, blufio-bus 17, doctor 7 new), clippy clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire HookManager and hot reload into serve.rs** - `6e49e41` (feat)
2. **Task 2: Doctor health checks and workspace test verification** - `2eb9d57` (feat)

## Files Created/Modified
- `crates/blufio/Cargo.toml` - Added blufio-hooks dependency
- `crates/blufio/src/serve.rs` - HookManager init, hot reload watchers, lifecycle hook calls at startup/shutdown
- `crates/blufio/src/doctor.rs` - check_hooks and check_hot_reload functions with 7 unit tests

## Decisions Made
- HookManager::run takes &self (not self), enabling a single Arc<HookManager> shared between the EventBus run loop and direct lifecycle hook calls (simpler than creating two instances per the plan)
- Config path for hot reload determined at runtime by checking XDG hierarchy paths in priority order (local > user > system)
- pre_shutdown hooks fire after agent_loop.run() returns (signal-triggered graceful shutdown complete), post_shutdown after audit trail flush
- Doctor check_hooks uses validate_hook_events from blufio_hooks::manager for event name validation and checks absolute command paths for existence

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Adapted plan for HookManager::run &self signature**
- **Found during:** Task 1 (wiring HookManager)
- **Issue:** Plan assumed HookManager::run takes `self` (consuming) and suggested creating two HookManager instances (Option A). Actual implementation takes `&self` (borrowing).
- **Fix:** Used single Arc<HookManager> shared between the spawned run loop and direct lifecycle calls, eliminating the need for Option A or Option B from the plan
- **Files modified:** `crates/blufio/src/serve.rs`
- **Verification:** `cargo check -p blufio` passes
- **Committed in:** 6e49e41 (Task 1 commit)

**2. [Rule 2 - Missing Critical] spawn_skill_watcher returns Result, not bare spawn**
- **Found during:** Task 1 (wiring skill hot reload)
- **Issue:** Plan showed spawning skill watcher via tokio::spawn, but actual function is async and returns Result<(), BlufioError>
- **Fix:** Called spawn_skill_watcher directly with .await and error handling matching non-fatal init pattern
- **Files modified:** `crates/blufio/src/serve.rs`
- **Verification:** `cargo check -p blufio` passes
- **Committed in:** 6e49e41 (Task 1 commit)

**3. [Rule 3 - Blocking] Adapted CheckResult to existing struct (no details field)**
- **Found during:** Task 2 (doctor health checks)
- **Issue:** Plan's CheckResult included a `details` field, but the existing struct only has name/status/message/duration
- **Fix:** Incorporated details into the message field, matching existing doctor check patterns
- **Files modified:** `crates/blufio/src/doctor.rs`
- **Verification:** All 7 new tests pass
- **Committed in:** 2eb9d57 (Task 2 commit)

---

**Total deviations:** 3 auto-fixed (1 bug, 1 missing critical, 1 blocking)
**Impact on plan:** All deviations were adaptations to actual code signatures vs plan assumptions. No scope creep. Final implementation is cleaner than planned (single Arc vs two instances).

## Issues Encountered
None beyond the auto-fixed deviations above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Phase 59 (Hook System & Hot Reload) is now complete across all 4 plans
- All hook system components fully integrated: config types, bus events, shell executor, recursion guard, HookManager, hot reload, serve.rs wiring, and doctor checks
- Ready for the next phase in the v1.5 PRD Gap Closure roadmap

## Self-Check: PASSED

- [x] crates/blufio/Cargo.toml updated (blufio-hooks dep)
- [x] crates/blufio/src/serve.rs updated (HookManager + hot reload wiring)
- [x] crates/blufio/src/doctor.rs updated (check_hooks + check_hot_reload)
- [x] Commit 6e49e41 exists (Task 1)
- [x] Commit 2eb9d57 exists (Task 2)

---
*Phase: 59-hook-system-hot-reload*
*Completed: 2026-03-12*
