---
phase: 58-cron-scheduler-retention-policies
plan: 03
subsystem: cron
tags: [cli, clap, systemd, timer, cron, rusqlite, croner]

# Dependency graph
requires:
  - phase: 58-01
    provides: "CronTask trait, V14 migration (cron_jobs table), soft-delete, CronConfig"
  - phase: 58-02
    provides: "CronScheduler, history module (record_start/record_finish/query_history), register_builtin_tasks, RetentionEnforcer"
provides:
  - "blufio cron CLI subcommand with 6 operations (list, add, remove, run-now, history, generate-timers)"
  - "Systemd timer/service file generation from cron job definitions"
  - "CronJobRow struct and generate_timers API in blufio-cron"
affects: [58-04, 58-05, integration-testing]

# Tech tracking
tech-stack:
  added: [croner (in blufio binary crate)]
  patterns: [CLI-to-cron-crate bridge via cron_cmd.rs, sync DB access for CLI per Phase 54 convention]

key-files:
  created:
    - crates/blufio/src/cron_cmd.rs
    - crates/blufio-cron/src/systemd.rs
  modified:
    - crates/blufio/src/main.rs
    - crates/blufio-cron/src/lib.rs
    - crates/blufio/Cargo.toml
    - Cargo.lock

key-decisions:
  - "croner v3 uses FromStr (schedule.parse::<Cron>()) not Cron::new() constructor"
  - "tokio_rusqlite conn.call closure returns Result<T, rusqlite::Error> (explicit type annotation required)"
  - "Main dispatch uses if-let-Err pattern (not ?) since main() returns ()"
  - "Cron-to-OnCalendar conversion is best-effort with hourly fallback for unsupported patterns"

patterns-established:
  - "CLI cron handler: sync DB for list/add/remove/generate, async DB for run-now/history"
  - "Systemd template: string replacement in const templates with safe name sanitization"

requirements-completed: [CRON-02, CRON-03]

# Metrics
duration: 12min
completed: 2026-03-12
---

# Phase 58 Plan 03: CLI Subcommand & Systemd Timer Generation Summary

**`blufio cron` CLI with 6 subcommands (list/add/remove/run-now/history/generate-timers) and systemd timer/service file generation with cron-to-OnCalendar conversion**

## Performance

- **Duration:** 12 min
- **Started:** 2026-03-12T18:30:00Z
- **Completed:** 2026-03-12T18:42:08Z
- **Tasks:** 1
- **Files modified:** 6

## Accomplishments
- Full `blufio cron` CLI with all 6 subcommands following existing project patterns (clap derive, sync DB access)
- Systemd timer/service unit file generation with best-effort cron-to-OnCalendar conversion
- 7 unit tests covering cron expression conversion (daily, every-15-min, every-6-hours, weekly, monthly, complex rejection) and file generation
- Integration with Plan 02's history module (query_history, record_start/record_finish) and task registry (register_builtin_tasks)

## Task Commits

Each task was committed atomically:

1. **Task 1: CLI subcommand and systemd timer generation** - `9c788aa` (feat)

## Files Created/Modified
- `crates/blufio/src/cron_cmd.rs` - CLI handler for all 6 cron subcommands with sync/async DB access
- `crates/blufio-cron/src/systemd.rs` - Systemd timer/service generation with cron-to-OnCalendar conversion and 7 tests
- `crates/blufio/src/main.rs` - Added `mod cron_cmd`, `Cron` variant to Commands enum, CronCommands enum, dispatch arm
- `crates/blufio-cron/src/lib.rs` - Added `pub mod systemd` and re-exports (CronJobRow, generate_timers)
- `crates/blufio/Cargo.toml` - Added blufio-cron and croner dependencies
- `Cargo.lock` - Dependency resolution update

## Decisions Made
- Used `schedule.parse::<croner::Cron>()` (FromStr) rather than `Cron::new()` -- croner v3.0.1 uses FromStr trait
- `conn.call` closure requires explicit `Result<T, rusqlite::Error>` return type annotation for tokio-rusqlite 0.7 type inference
- Main dispatch uses `if let Err(e) = ... { eprintln!; exit(1) }` pattern matching all other CLI commands (main returns `()`)
- Cron-to-OnCalendar conversion uses hourly fallback with warning for unsupported specifiers (L/W/#)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed croner API usage (Cron::new -> FromStr)**
- **Found during:** Task 1
- **Issue:** Plan specified `croner::Cron::from_str` but cron_cmd.rs initially used `croner::Cron::new(schedule).parse()` which doesn't exist in croner v3.0.1
- **Fix:** Changed to `schedule.parse::<croner::Cron>()` matching scheduler.rs pattern
- **Files modified:** crates/blufio/src/cron_cmd.rs
- **Verification:** cargo check passes
- **Committed in:** 9c788aa

**2. [Rule 1 - Bug] Fixed tokio_rusqlite Error type (no Rusqlite variant)**
- **Found during:** Task 1
- **Issue:** Used `tokio_rusqlite::Error::Rusqlite` which doesn't exist. tokio-rusqlite 0.7 has `Error<E>` with variants `ConnectionClosed`, `Close`, `Error(E)`
- **Fix:** Changed to explicit closure return type `Result<String, rusqlite::Error>` following scheduler.rs pattern
- **Files modified:** crates/blufio/src/cron_cmd.rs
- **Verification:** cargo check passes
- **Committed in:** 9c788aa

**3. [Rule 1 - Bug] Fixed main.rs dispatch pattern (? operator in non-Result fn)**
- **Found during:** Task 1
- **Issue:** Used `?` operator in main() which returns `()`, not `Result`
- **Fix:** Changed to `if let Err(e) = ... { eprintln!("error: {e}"); std::process::exit(1); }` matching all other dispatch arms
- **Files modified:** crates/blufio/src/main.rs
- **Verification:** cargo check passes
- **Committed in:** 9c788aa

---

**Total deviations:** 3 auto-fixed (3 bugs)
**Impact on plan:** All fixes were API correction issues discovered during compilation. No scope creep.

## Issues Encountered
- Concurrent Plan 02 execution in the same wave caused repeated file reverts (main.rs, lib.rs, Cargo.toml). Resolved by waiting for Plan 02 to complete both commits before re-applying all changes.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- All 6 cron CLI subcommands compile and are wired into the binary
- Systemd timer generation is functional with 7 passing tests
- Ready for Plan 04 (integration/wiring) and Plan 05 (testing)

## Self-Check: PASSED

- All 5 key files verified on disk
- Commit 9c788aa verified in git log
- cargo check -p blufio-cron -p blufio passes
- 7/7 systemd tests pass

---
*Phase: 58-cron-scheduler-retention-policies*
*Completed: 2026-03-12*
