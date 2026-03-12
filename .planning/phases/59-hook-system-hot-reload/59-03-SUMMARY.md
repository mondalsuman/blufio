---
phase: 59-hook-system-hot-reload
plan: 03
subsystem: hooks
tags: [hooks, hook-manager, eventbus, btreemap, priority-dispatch, tls, skill-reload, hot-reload, file-watcher]

# Dependency graph
requires:
  - phase: 59-hook-system-hot-reload plan 01
    provides: HookConfig, HookDefinition, HotReloadConfig types, ShellExecutor, RecursionGuard, HookEvent on BusEvent
  - phase: 59-hook-system-hot-reload plan 02
    provides: Config hot reload module with ArcSwap, spawn_config_watcher, load_config
provides:
  - HookManager EventBus subscriber with BTreeMap priority-ordered dispatch
  - LIFECYCLE_EVENT_MAP resolving 7 TOML event names to EventBus type strings
  - execute_lifecycle_hooks for 4 direct-call lifecycle events
  - validate_hook_events for config validation
  - spawn_tls_watcher stub (pending direct rustls dependency)
  - spawn_skill_watcher for WASM file change detection and EventBus notification
affects: [59-04-serve-integration, serve.rs HookManager init, serve.rs skill reload]

# Tech tracking
tech-stack:
  added: []
  patterns: [BTreeMap priority dispatch with RecursionGuard, TOML-to-EventBus event name resolution, file watcher detection+notification layer]

key-files:
  created:
    - crates/blufio-hooks/src/manager.rs
  modified:
    - crates/blufio-hooks/src/lib.rs
    - crates/blufio/src/hot_reload.rs

key-decisions:
  - "LIFECYCLE_EVENT_MAP resolves TOML names to EventBus type strings at dispatch time (not constructor time)"
  - "TLS hot reload implemented as stub (rustls only available transitively through reqwest, not direct dep)"
  - "Skill watcher is detection+notification layer only; SkillStore update handled in Plan 04 serve.rs integration"
  - "Clippy for_kv_map lint: use values() iterator for BTreeMap when key is unused"

patterns-established:
  - "HookManager run loop: subscribe_reliable mpsc channel + tokio::select! with CancellationToken"
  - "Event name resolution: TOML lifecycle names mapped to EventBus type strings via const slice"
  - "Skill watcher: scan_wasm_files + known set diffing for add/modify/remove detection"

requirements-completed: [HOOK-05, HTRL-02, HTRL-03, HTRL-06]

# Metrics
duration: 5min
completed: 2026-03-12
---

# Phase 59 Plan 03: EventBus Wiring & Hot Reload Summary

**HookManager EventBus subscriber with BTreeMap priority dispatch, recursion guard, lifecycle event mapping, plus TLS cert stub and skill directory hot reload watcher**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-12T19:57:06Z
- **Completed:** 2026-03-12T20:02:35Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- HookManager subscribes to EventBus reliable channel, dispatches hooks in BTreeMap priority order with RecursionGuard depth limiting
- 11 lifecycle events mapped: 7 EventBus-driven (via LIFECYCLE_EVENT_MAP) + 4 direct-call (pre/post start/shutdown)
- HookEvent::Triggered/Completed emitted for every hook dispatch (success, failed, timeout, skipped)
- validate_hook_events catches unknown event names in config definitions
- TLS cert hot reload stub with documented TODO for when rustls becomes a direct workspace dependency
- Skill directory watcher detects .wasm file additions, modifications, and removals, checks for .sig files, emits ConfigEvent::Reloaded with source "skill_reload"
- 22 blufio-hooks tests + 15 blufio hot_reload tests passing, clippy clean

## Task Commits

Each task was committed atomically:

1. **Task 1: HookManager EventBus subscriber with priority dispatch** - `0cc5d88` (feat)
2. **Task 2: TLS certificate and plugin/skill hot reload** - `c06d343` (feat)

## Files Created/Modified
- `crates/blufio-hooks/src/manager.rs` - HookManager with BTreeMap priority dispatch, RecursionGuard, LIFECYCLE_EVENT_MAP, validate_hook_events, execute_lifecycle_hooks
- `crates/blufio-hooks/src/lib.rs` - Added pub mod manager and pub use HookManager re-export
- `crates/blufio/src/hot_reload.rs` - Added spawn_tls_watcher (stub), spawn_skill_watcher, scan_wasm_files, handle_skill_change

## Decisions Made
- LIFECYCLE_EVENT_MAP resolves TOML event names to EventBus type strings at dispatch time, keeping HookDefinition event field as the original TOML name for readability
- TLS hot reload implemented as a documented stub since rustls is only transitively available through reqwest; full implementation deferred to when direct rustls dependency is added
- Skill watcher serves as detection+notification layer only; the actual SkillStore update (re-loading WASM modules) will be wired in Plan 04 serve.rs integration
- resolve_event_name returns direct lifecycle events as-is (they won't match any EventBus event, preventing accidental double-dispatch)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed clippy for_kv_map lint in manager.rs**
- **Found during:** Task 2 (clippy verification)
- **Issue:** Two `for (_priority, hooks) in &self.hooks` loops in handle_event and execute_lifecycle_hooks triggered clippy for_kv_map warning since the key was unused
- **Fix:** Changed to `for hooks in self.hooks.values()` in both locations
- **Files modified:** `crates/blufio-hooks/src/manager.rs`
- **Verification:** `cargo clippy -p blufio-hooks -p blufio -- -D warnings` passes clean
- **Committed in:** c06d343 (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 code style)
**Impact on plan:** Trivial clippy fix for idiomatic Rust. No scope creep.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- HookManager ready for serve.rs integration (subscribe_reliable + tokio::spawn run loop)
- execute_lifecycle_hooks ready for direct calls in serve.rs pre/post start/shutdown
- spawn_skill_watcher ready for serve.rs spawning with skills_dir from SkillConfig
- Plan 04 (serve.rs integration) can wire everything together: HookManager init, TLS watcher, skill watcher, config reload event handling

## Self-Check: PASSED

- [x] crates/blufio-hooks/src/manager.rs exists
- [x] crates/blufio-hooks/src/lib.rs updated
- [x] crates/blufio/src/hot_reload.rs updated
- [x] Commit 0cc5d88 exists (Task 1)
- [x] Commit c06d343 exists (Task 2)

---
*Phase: 59-hook-system-hot-reload*
*Completed: 2026-03-12*
