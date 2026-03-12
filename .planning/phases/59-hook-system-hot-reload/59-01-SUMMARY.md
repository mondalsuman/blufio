---
phase: 59-hook-system-hot-reload
plan: 01
subsystem: hooks
tags: [hooks, shell-executor, recursion-guard, bus-events, config-types, arc-swap]

# Dependency graph
requires:
  - phase: 58-cron-retention
    provides: CronConfig/RetentionConfig patterns in model.rs, CronEvent on BusEvent
provides:
  - HookConfig, HookDefinition, HotReloadConfig config structs
  - HookEvent (Triggered/Completed) variant on BusEvent
  - blufio-hooks crate with ShellExecutor and RecursionGuard
  - arc-swap workspace dependency
affects: [59-02-hot-reload, 59-03-eventbus-wiring]

# Tech tracking
tech-stack:
  added: [arc-swap 1.8]
  patterns: [RAII recursion guard with AtomicU32, restricted-PATH shell execution]

key-files:
  created:
    - crates/blufio-hooks/Cargo.toml
    - crates/blufio-hooks/src/lib.rs
    - crates/blufio-hooks/src/executor.rs
    - crates/blufio-hooks/src/recursion.rs
  modified:
    - crates/blufio-config/src/model.rs
    - crates/blufio-bus/src/events.rs
    - Cargo.toml
    - crates/blufio-audit/src/subscriber.rs

key-decisions:
  - "io-util tokio feature added to blufio-hooks for async stdin/stdout pipe handling"
  - "child.wait() with manual stdout/stderr reads instead of wait_with_output() to support kill-on-timeout"
  - "HookEvent follows String-fields pattern (no cross-crate deps) matching all other BusEvent sub-enums"

patterns-established:
  - "RecursionGuard RAII: Arc<AtomicU32> counter with try_enter/Drop for depth-limited recursion"
  - "ShellExecutor: env_clear + restricted PATH + JSON stdin for secure hook dispatch"

requirements-completed: [HOOK-01, HOOK-02, HOOK-03, HOOK-04, HOOK-06]

# Metrics
duration: 8min
completed: 2026-03-12
---

# Phase 59 Plan 01: Hook System Foundation Summary

**Shell-based hook system with HookConfig/HotReloadConfig types, HookEvent bus variant, AtomicU32 recursion guard, and restricted-PATH shell executor**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-12T19:44:37Z
- **Completed:** 2026-03-12T19:52:46Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments
- HookConfig, HookDefinition, and HotReloadConfig structs with serde(deny_unknown_fields) in blufio-config
- HookEvent (Triggered/Completed) on BusEvent with event_type_string match arms
- blufio-hooks crate with ShellExecutor (JSON stdin, stdout capture, timeout, PATH restriction) and RecursionGuard (RAII AtomicU32 depth counter)
- 12 unit tests (6 recursion + 6 executor) plus 1 doctest, all passing, clippy clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Config types, BusEvent variant, and workspace setup** - `054e92f` (feat)
2. **Task 2: Shell executor and recursion guard with tests** - `5cb62b0` (feat)

## Files Created/Modified
- `crates/blufio-hooks/Cargo.toml` - New crate manifest with bus/config/tokio dependencies
- `crates/blufio-hooks/src/lib.rs` - Crate root with module declarations and re-exports
- `crates/blufio-hooks/src/executor.rs` - ShellExecutor with JSON stdin, timeout, PATH restriction
- `crates/blufio-hooks/src/recursion.rs` - RecursionGuard RAII with AtomicU32 depth counter
- `crates/blufio-config/src/model.rs` - HookConfig, HookDefinition, HotReloadConfig structs
- `crates/blufio-bus/src/events.rs` - HookEvent sub-enum on BusEvent
- `Cargo.toml` - arc-swap added to workspace dependencies
- `crates/blufio-audit/src/subscriber.rs` - Exhaustive HookEvent match arms for audit trail

## Decisions Made
- Used `child.wait()` with manual stdout/stderr reads instead of `wait_with_output()` to preserve ability to kill child on timeout (ownership issue)
- Added `io-util` tokio feature for `AsyncWriteExt`/`AsyncReadExt` on stdin/stdout pipes
- HookEvent follows established String-fields pattern (no cross-crate type dependencies)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed exhaustive match in blufio-audit subscriber**
- **Found during:** Task 1 (adding HookEvent to BusEvent)
- **Issue:** `convert_to_pending_entry` in `blufio-audit/src/subscriber.rs` uses exhaustive match on BusEvent; adding Hook variant caused `E0004` non-exhaustive pattern error
- **Fix:** Added `BusEvent::Hook(HookEvent::Triggered { .. })` and `BusEvent::Hook(HookEvent::Completed { .. })` match arms with appropriate audit entry conversion; added `HookEvent` to imports; updated test to include hook events
- **Files modified:** `crates/blufio-audit/src/subscriber.rs`
- **Verification:** `cargo check --workspace` passes, all blufio-audit tests pass
- **Committed in:** 054e92f (Task 1 commit)

**2. [Rule 3 - Blocking] Added io-util tokio feature for async pipe handling**
- **Found during:** Task 2 (implementing ShellExecutor)
- **Issue:** `AsyncWriteExt::write_all` not available on `ChildStdin` without `io-util` feature
- **Fix:** Added `io-util` to tokio features in `crates/blufio-hooks/Cargo.toml`
- **Files modified:** `crates/blufio-hooks/Cargo.toml`
- **Verification:** All 12 tests pass
- **Committed in:** 5cb62b0 (Task 2 commit)

**3. [Rule 1 - Bug] Fixed ownership issue in timeout handling**
- **Found during:** Task 2 (implementing ShellExecutor)
- **Issue:** `child.wait_with_output()` takes ownership of `child`, making `child.kill()` impossible in timeout branch
- **Fix:** Switched to `child.wait()` (borrows) with manual stdout/stderr reads via `AsyncReadExt::read_to_end`
- **Files modified:** `crates/blufio-hooks/src/executor.rs`
- **Verification:** Timeout test passes, process is properly killed
- **Committed in:** 5cb62b0 (Task 2 commit)

---

**Total deviations:** 3 auto-fixed (1 bug, 2 blocking)
**Impact on plan:** All auto-fixes necessary for compilation and correctness. No scope creep.

## Issues Encountered
None beyond the auto-fixed deviations above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- blufio-hooks crate foundation complete with executor and recursion guard
- Ready for Plan 02 (hot reload) which will add ArcSwap-based config swapping
- Ready for Plan 03 (EventBus wiring) which will wire hooks to the event bus

## Self-Check: PASSED

- All 4 created files verified present
- Commit 054e92f verified (Task 1)
- Commit 5cb62b0 verified (Task 2)

---
*Phase: 59-hook-system-hot-reload*
*Completed: 2026-03-12*
