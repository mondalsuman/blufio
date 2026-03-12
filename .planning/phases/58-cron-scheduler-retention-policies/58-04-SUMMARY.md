---
phase: 58-cron-scheduler-retention-policies
plan: 04
subsystem: cron, retention, integration
tags: [cron-scheduler, doctor, health-check, serve-lifecycle, cancellation-token, soft-delete]

# Dependency graph
requires:
  - phase: 58-cron-scheduler-retention-policies
    plan: 02
    provides: "CronScheduler, register_builtin_tasks, CronEvent, history module"
  - phase: 58-cron-scheduler-retention-policies
    plan: 03
    provides: "CLI subcommand, systemd timer generation"
provides:
  - "CronScheduler spawned inside blufio serve with CancellationToken for graceful shutdown"
  - "Built-in cron tasks registered from config on startup via register_builtin_tasks()"
  - "Doctor check_cron: active jobs, stale locks, recent failures from cron_history"
  - "Doctor check_retention: config status, pending soft-deleted records across tables"
affects: [58-05, integration-testing, operator-diagnostics]

# Tech tracking
tech-stack:
  added: []
  patterns: [CronScheduler lifecycle in serve.rs, doctor health check for cron and retention subsystems]

key-files:
  created: []
  modified:
    - "crates/blufio/src/serve.rs"
    - "crates/blufio/src/doctor.rs"

key-decisions:
  - "CronScheduler opens its own DB connection (not shared) following audit/archive pattern for connection isolation"
  - "Scheduler init failure is non-fatal (warn and continue) following audit/memory resilience pattern"
  - "Doctor cron check uses sync connection and gracefully handles missing cron_jobs table (pre-V14 migration)"
  - "Retention doctor check counts pending deletions across messages, sessions, and memories tables"

patterns-established:
  - "Background subsystem spawn pattern: config.X.enabled -> open_connection -> init -> spawn with child_token -> warn on failure"
  - "Doctor check table existence pattern: query sqlite_master before querying data tables"

requirements-completed: [CRON-05, CRON-06, RETN-02, RETN-04]

# Metrics
duration: 4min
completed: 2026-03-12
---

# Phase 58 Plan 04: Serve Integration & Doctor Health Checks Summary

**CronScheduler wired into blufio serve lifecycle with CancellationToken shutdown, and doctor health checks for cron job status (active/stale/failed) and retention policy enforcement (config/pending deletions)**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-12T18:50:02Z
- **Completed:** 2026-03-12T18:54:02Z
- **Tasks:** 1
- **Files modified:** 2

## Accomplishments
- CronScheduler spawns in serve.rs when `config.cron.enabled` is true, with its own DB connection and CancellationToken
- Built-in tasks registered via `register_builtin_tasks()` factory from BlufioConfig
- Doctor `check_cron` reports active job count, stale locks, and failed jobs in last 24h from cron_history
- Doctor `check_retention` reports configured retention periods and counts soft-deleted records pending permanent deletion
- Full workspace compiles and all tests pass (0 failures)

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire CronScheduler into serve.rs and add doctor checks** - `cefc0e7` (feat)

## Files Created/Modified
- `crates/blufio/src/serve.rs` - CronScheduler spawn block after EventBus init, with connection, task registry, and CancellationToken
- `crates/blufio/src/doctor.rs` - check_cron and check_retention functions wired into run_doctor

## Decisions Made
- CronScheduler opens its own `tokio_rusqlite::Connection` via `blufio_storage::open_connection()` following the same isolation pattern as audit trail and archive provider
- Scheduler init failure (CronError) is non-fatal -- logs warning and continues serving, matching audit/memory resilience pattern
- Doctor cron check queries `sqlite_master` to detect if cron_jobs table exists before querying (handles pre-V14 databases)
- Doctor retention check inspects `pragma_table_info` for `deleted_at` column existence to handle pre-migration schemas

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- CronScheduler fully operational inside blufio serve
- Doctor can diagnose cron and retention health
- Ready for Plan 05 (integration testing and end-to-end verification)

## Self-Check: PASSED

All 2 modified files verified present. Task commit (cefc0e7) verified in git log.

---
*Phase: 58-cron-scheduler-retention-policies*
*Completed: 2026-03-12*
