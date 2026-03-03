---
phase: 24-sd-notify-integration
plan: 01
subsystem: infra
tags: [sd-notify, systemd, shutdown, watchdog, lifecycle]

requires:
  - phase: 23-backup-integrity-verification
    provides: stable agent crate for lifecycle additions
provides:
  - sdnotify.rs wrapper module with notify_ready(), notify_stopping(), notify_status(), spawn_watchdog()
  - STOPPING=1 and STATUS= integration in shutdown.rs signal handler and drain flow
  - sd-notify 0.4 dependency in blufio-agent
affects: [24-02, serve.rs integration, systemd unit file]

tech-stack:
  added: [sd-notify 0.4]
  patterns: [best-effort sd_notify wrapper, centralized notification module]

key-files:
  created:
    - crates/blufio-agent/src/sdnotify.rs
  modified:
    - crates/blufio-agent/Cargo.toml
    - crates/blufio-agent/src/lib.rs
    - crates/blufio-agent/src/shutdown.rs

key-decisions:
  - "All sd_notify calls centralized in sdnotify.rs -- no direct sd_notify crate imports elsewhere"
  - "Best-effort error handling: debug log on failure, never propagate"
  - "Watchdog info-level log is the one exception to debug-only logging (operational significance)"

patterns-established:
  - "sd_notify wrapper pattern: typed helpers per notification, debug logging, false for unset_environment"
  - "Watchdog spawn pattern: check watchdog_enabled, ping at half interval, CancellationToken shutdown"

requirements-completed: [SYSD-02, SYSD-03, SYSD-05]

duration: 4min
completed: 2026-03-03
---

# Plan 24-01: sd_notify Wrapper Module and Shutdown Integration Summary

**sd_notify wrapper module with notify_ready/stopping/status/watchdog helpers, plus STOPPING=1 and STATUS= drain integration in shutdown.rs**

## Performance

- **Duration:** 4 min
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- Created sdnotify.rs with 4 public + 1 private helper wrapping the sd-notify crate
- All calls are silent no-ops on macOS/Docker (NOTIFY_SOCKET absent)
- shutdown.rs sends STOPPING=1 before token cancellation and STATUS= during drain lifecycle
- Unit tests confirm no-panic behavior and watchdog returns None without systemd

## Task Commits

Each task was committed atomically:

1. **Task 1: Create sdnotify.rs module and add sd-notify dependency** - `475cb35` (feat)
2. **Task 2: Integrate STOPPING=1 and STATUS= into shutdown.rs** - `00f950e` (feat)

## Files Created/Modified
- `crates/blufio-agent/src/sdnotify.rs` - New wrapper module: notify_ready, notify_stopping, notify_status, spawn_watchdog
- `crates/blufio-agent/Cargo.toml` - Added sd-notify = "0.4" dependency
- `crates/blufio-agent/src/lib.rs` - Added pub mod sdnotify declaration
- `crates/blufio-agent/src/shutdown.rs` - STOPPING=1 before cancel, STATUS= during drain start/complete/timeout

## Decisions Made
None - followed plan as specified

## Deviations from Plan
None - plan executed exactly as written

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- sdnotify module ready for Plan 02 to wire READY=1 and watchdog into serve.rs
- All 4 public functions exported and tested

---
*Phase: 24-sd-notify-integration*
*Completed: 2026-03-03*
