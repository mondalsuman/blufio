---
phase: 24-sd-notify-integration
plan: 02
subsystem: infra
tags: [sd-notify, systemd, serve, watchdog, unit-file, Type-notify]

requires:
  - phase: 24-sd-notify-integration
    plan: 01
    provides: sdnotify.rs wrapper module with notify_ready, notify_status, spawn_watchdog
provides:
  - STATUS= at vault initialization milestone in serve.rs
  - READY=1 with channel count and memory status after mux.connect()
  - Watchdog background task spawned alongside memory monitor
  - systemd unit file with Type=notify, WatchdogSec=30, TimeoutStartSec=90
affects: [deployment, systemd operations, monitoring]

tech-stack:
  added: []
  patterns: [Type=notify systemd integration, sd_notify lifecycle wiring]

key-files:
  created: []
  modified:
    - crates/blufio/src/serve.rs
    - contrib/blufio.service

key-decisions:
  - "READY=1 placed strictly after mux.connect().await? -- systemd marks active only when all channels connected"
  - "Removed ExecStartPost curl health-check loop -- redundant with Type=notify"
  - "TimeoutStartSec=90 covers first-run model download (30-60s)"

patterns-established:
  - "sd_notify milestone pattern: STATUS= at key init points, READY=1 at end of initialization"
  - "Type=notify systemd unit: WatchdogSec + NotifyAccess=main + no ExecStartPost"

requirements-completed: [SYSD-01, SYSD-04, SYSD-06]

duration: 3min
completed: 2026-03-03
---

# Plan 24-02: serve.rs READY/STATUS Wiring and systemd Unit File Summary

**READY=1 after channel connection with STATUS= milestones, watchdog task spawn, and Type=notify systemd unit with WatchdogSec=30**

## Performance

- **Duration:** 3 min
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- serve.rs sends STATUS= "Initializing: vault unlocked" after vault startup check
- serve.rs sends READY=1 with channel count and memory enabled status after mux.connect()
- Watchdog background task spawned alongside memory monitor using same CancellationToken pattern
- systemd unit file updated: Type=notify, NotifyAccess=main, WatchdogSec=30, TimeoutStartSec=90
- ExecStartPost health-check curl loop removed (redundant with sd_notify READY=1)

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire STATUS=, READY=1, and watchdog into serve.rs** - `67bb256` (feat)
2. **Task 2: Update systemd unit file for Type=notify with watchdog** - `c68437b` (feat)

## Files Created/Modified
- `crates/blufio/src/serve.rs` - Added notify_status at vault, notify_ready at channel connect, spawn_watchdog after memory monitor
- `contrib/blufio.service` - Type=notify, NotifyAccess=main, WatchdogSec=30, TimeoutStartSec=90, removed ExecStartPost

## Decisions Made
None - followed plan as specified

## Deviations from Plan
None - plan executed exactly as written

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Full sd_notify integration complete: READY=1, STOPPING=1, STATUS=, WATCHDOG=1
- systemctl start/status/stop will accurately reflect Blufio lifecycle

---
*Phase: 24-sd-notify-integration*
*Completed: 2026-03-03*
