---
phase: 58-cron-scheduler-retention-policies
plan: 02
subsystem: cron, retention, scheduler
tags: [croner, cron-scheduling, retention-enforcement, soft-delete, permanent-delete, single-instance-lock]

# Dependency graph
requires:
  - phase: 58-cron-scheduler-retention-policies
    plan: 01
    provides: "CronTask trait, CronConfig/RetentionConfig, CronEvent, V14 migration, soft-delete filtering"
provides:
  - "CronScheduler with 60s dispatch loop, single-instance locking, CronEvent emission"
  - "CronError enum for scheduler-level errors"
  - "Job execution history (record_start, record_finish, query_history, cleanup_old_history)"
  - "5 built-in CronTask implementations (memory_cleanup, backup, cost_report, health_check, retention_enforcement)"
  - "register_builtin_tasks() factory creating task registry from BlufioConfig"
  - "RetentionEnforcer with two-phase soft-delete + permanent delete"
  - "Classification-aware retention (restricted vs non-restricted periods)"
affects: [58-03, 58-04, 58-05]

# Tech tracking
tech-stack:
  added: []
  patterns: [CronScheduler dispatch loop, single-instance lock via atomic DB update, two-phase retention enforcement]

key-files:
  created:
    - "crates/blufio-cron/src/scheduler.rs"
    - "crates/blufio-cron/src/history.rs"
    - "crates/blufio-cron/src/tasks/memory_cleanup.rs"
    - "crates/blufio-cron/src/tasks/backup.rs"
    - "crates/blufio-cron/src/tasks/cost_report.rs"
    - "crates/blufio-cron/src/tasks/health_check.rs"
    - "crates/blufio-cron/src/tasks/retention.rs"
    - "crates/blufio-cron/src/retention/mod.rs"
    - "crates/blufio-cron/src/retention/soft_delete.rs"
    - "crates/blufio-cron/src/retention/permanent.rs"
  modified:
    - "crates/blufio-cron/src/lib.rs"
    - "crates/blufio-cron/src/tasks/mod.rs"
    - "crates/blufio-cron/Cargo.toml"

key-decisions:
  - "CronScheduler uses simple String errors for history module (no BlufioError cross-crate dependency)"
  - "croner::Cron parsed via FromStr trait (schedule.parse::<Cron>())"
  - "find_next_occurrence returns Result (croner v3 API), handled with Ok/Err"
  - "Scheduler dispatches tasks inline (not tokio::spawn) since CronTask is not Clone"
  - "Memory cleanup uses soft-delete (UPDATE deleted_at) not hard-delete for consistency"
  - "Backup uses VACUUM INTO for atomic backup with SQL injection prevention (quote escaping)"
  - "Cost report aggregates by model field as provider proxy"
  - "Retention soft-delete queries use format!() for table/days interpolation (safe: internal values only)"

patterns-established:
  - "CronScheduler lock pattern: UPDATE SET running=1 WHERE running=0 with changes()==1 check"
  - "History truncation: output capped at 4096 chars to prevent DB bloat"
  - "Retention two-phase: soft_delete::run_soft_delete() then permanent::run_permanent_delete()"
  - "Classification-aware retention: separate queries for restricted vs non-restricted records"
  - "register_builtin_tasks() factory pattern for task registry creation"

requirements-completed: [CRON-04, CRON-05, CRON-06, RETN-02, RETN-03, RETN-04, RETN-05]

# Metrics
duration: 17min
completed: 2026-03-12
---

# Phase 58 Plan 02: Scheduler Core & Retention Engine Summary

**CronScheduler with 60s dispatch loop, single-instance locking, job history persistence, 5 built-in CronTask implementations, and two-phase RetentionEnforcer with classification-aware soft-delete and permanent deletion**

## Performance

- **Duration:** 17 min
- **Started:** 2026-03-12T18:17:56Z
- **Completed:** 2026-03-12T18:35:13Z
- **Tasks:** 2
- **Files modified:** 13

## Accomplishments
- CronScheduler with 60s interval dispatch loop, config-to-DB sync, graceful shutdown via CancellationToken
- Single-instance job locking via atomic `UPDATE SET running=1 WHERE running=0` pattern
- Job execution history with start/finish recording, query, and per-job cleanup
- 5 built-in tasks: memory_cleanup, backup, cost_report, health_check, retention_enforcement
- RetentionEnforcer orchestrating two-phase deletion with classification-aware queries
- Retention architecturally isolates audit.db (no retention queries touch audit tables)

## Task Commits

Each task was committed atomically:

1. **Task 1: Scheduler loop, job history, and single-instance locking** - `b995a49` (feat)
2. **Task 2: Built-in tasks and retention enforcement engine** - `c06babb` (feat)

## Files Created/Modified
- `crates/blufio-cron/src/scheduler.rs` - CronScheduler with run(), run_now(), dispatch loop, lock management
- `crates/blufio-cron/src/history.rs` - record_start, record_finish, query_history, cleanup_old_history
- `crates/blufio-cron/src/tasks/memory_cleanup.rs` - Evicts lowest-scored memories over max_entries
- `crates/blufio-cron/src/tasks/backup.rs` - VACUUM INTO database backup
- `crates/blufio-cron/src/tasks/cost_report.rs` - 24h cost aggregation by provider
- `crates/blufio-cron/src/tasks/health_check.rs` - DB connectivity and count verification
- `crates/blufio-cron/src/tasks/retention.rs` - CronTask wrapper for RetentionEnforcer
- `crates/blufio-cron/src/retention/mod.rs` - RetentionEnforcer, RetentionReport, TableBreakdown
- `crates/blufio-cron/src/retention/soft_delete.rs` - Phase 1: classification-aware soft-delete
- `crates/blufio-cron/src/retention/permanent.rs` - Phase 2: permanent delete past grace period
- `crates/blufio-cron/src/lib.rs` - Added scheduler, history, retention module exports
- `crates/blufio-cron/src/tasks/mod.rs` - Added 5 task sub-modules and register_builtin_tasks()
- `crates/blufio-cron/Cargo.toml` - Added tokio macros feature for select!

## Decisions Made
- CronScheduler uses simple String errors for history module to avoid BlufioError cross-crate complexity
- croner::Cron parsed via FromStr trait (`schedule.parse::<Cron>()`) per croner v3 API
- Scheduler dispatches tasks inline (not tokio::spawn) since CronTask is not Clone and tasks are expected to be quick
- Memory cleanup uses soft-delete (UPDATE deleted_at) for consistency with retention model
- Backup uses VACUUM INTO with SQL quote escaping for injection safety
- Cost report aggregates by model field as a provider proxy
- Retention soft-delete uses format!() for table/days interpolation (safe: only internal constant values)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] tokio macros feature missing for select!**
- **Found during:** Task 1 (cargo check)
- **Issue:** `tokio::select!` requires the `macros` feature flag which was not in Cargo.toml
- **Fix:** Added `macros` to tokio features in Cargo.toml
- **Files modified:** crates/blufio-cron/Cargo.toml
- **Verification:** cargo check passes
- **Committed in:** b995a49 (Task 1 commit)

**2. [Rule 1 - Bug] tokio_rusqlite closure type inference failure**
- **Found during:** Task 1 (cargo check)
- **Issue:** `conn.call()` closures needed explicit return type annotations for `tokio_rusqlite::Connection::call` to infer error type parameter E
- **Fix:** Added `-> Result<T, rusqlite::Error>` return type annotations to all conn.call closures
- **Files modified:** crates/blufio-cron/src/history.rs, crates/blufio-cron/src/scheduler.rs
- **Verification:** cargo check passes
- **Committed in:** b995a49 (Task 1 commit)

**3. [Rule 1 - Bug] croner v3 API: find_next_occurrence returns Result not Option**
- **Found during:** Task 1 (cargo check)
- **Issue:** Plan specified `find_next_occurrence` returning Option, but croner v3 returns `Result<DateTime<Utc>, CronError>`
- **Fix:** Changed pattern match from `Some/None` to `Ok/Err`
- **Files modified:** crates/blufio-cron/src/scheduler.rs
- **Verification:** cargo check passes
- **Committed in:** b995a49 (Task 1 commit)

**4. [Rule 1 - Bug] Clippy warnings: collapsible_if, redundant_closure, bind_instead_of_map**
- **Found during:** Task 2 (cargo clippy)
- **Issue:** 4 clippy warnings in retention soft_delete, scheduler, backup, and retention task
- **Fix:** Collapsed nested if with let-chain, used map instead of and_then, used CronTaskError::ExecutionError directly, allowed type_complexity
- **Files modified:** soft_delete.rs, scheduler.rs, backup.rs, retention.rs
- **Verification:** cargo clippy passes with 0 warnings
- **Committed in:** c06babb (Task 2 commit)

---

**Total deviations:** 4 auto-fixed (3 bugs, 1 blocking)
**Impact on plan:** All auto-fixes necessary for compilation and lint compliance. No scope creep.

## Issues Encountered
None beyond the auto-fixed deviations above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- CronScheduler ready for integration into `blufio serve` (Plan 03+)
- register_builtin_tasks() ready for wiring in serve.rs
- RetentionEnforcer ready for testing with actual data
- All 5 built-in tasks implement CronTask and return descriptive output strings
- Job history module ready for CLI `blufio cron history` command

## Self-Check: PASSED

All 13 files verified present. Both task commits (b995a49, c06babb) verified in git log.

---
*Phase: 58-cron-scheduler-retention-policies*
*Completed: 2026-03-12*
