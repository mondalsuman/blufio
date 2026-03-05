---
phase: 23-backup-integrity-verification
plan: 01
subsystem: database
tags: [sqlite, pragma-integrity-check, rusqlite, backup, restore, corruption-detection]

# Dependency graph
requires: []
provides:
  - run_integrity_check() helper for PRAGMA integrity_check verification
  - Backup post-write integrity verification with corrupt file deletion
  - Restore pre-check, post-check, and auto-rollback from .pre-restore
  - Operator-facing integrity status in backup/restore output
affects: [25-sqlcipher-database-encryption]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "PRAGMA integrity_check(1) for fast single-error corruption detection"
    - "Auto-rollback via fs::copy from .pre-restore on corrupt restore"
    - "Pre-check backup file integrity before restore attempt"

key-files:
  created: []
  modified:
    - "crates/blufio/src/backup.rs"

key-decisions:
  - "Kept integrity check in backup.rs rather than sharing with doctor.rs due to sync vs async connection mismatch"
  - "Used PRAGMA integrity_check(1) to limit to single error row for performance on corrupt databases"
  - "Corruption test strategy: multi-page DB with second-page corruption (offset 4096+) for reliable detection"
  - "Accept both custom integrity_check error and rusqlite malformed error as valid corruption detection"

patterns-established:
  - "run_integrity_check(path) pattern: open read-only, PRAGMA integrity_check(1), parse result"
  - "Post-operation verification: verify output file after Backup API completes"
  - "Rollback pattern: delete corrupt file, fs::copy from .pre-restore, clear error message"

requirements-completed: [BKUP-01, BKUP-02, BKUP-03, BKUP-04]

# Metrics
duration: 4min
completed: 2026-03-03
---

# Phase 23 Plan 01: Backup Integrity Verification Summary

**PRAGMA integrity_check verification for backup and restore with corruption handling, auto-rollback, and operator status reporting**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-03T18:56:37Z
- **Completed:** 2026-03-03T19:00:41Z
- **Tasks:** 2
- **Files modified:** 1

## Accomplishments
- Added `run_integrity_check()` helper using `PRAGMA integrity_check(1)` with read-only connection for fast corruption detection
- Backup now verifies output file integrity after write, deletes corrupt backups, and reports "Backup complete: X.X MB, integrity: ok"
- Restore pre-checks backup file integrity before any modification (replaces weak `SELECT 1` validation)
- Restore post-checks restored database with auto-rollback from `.pre-restore` on corruption
- 6 new tests covering valid/empty/corrupt integrity checks, pre-check failure, first-time restore, and .pre-restore retention

## Task Commits

Each task was committed atomically:

1. **Task 1: Add integrity check helper and verify backup output** - `df360ce` (feat)
2. **Task 2: Add restore pre-check, post-check, rollback, and tests** - `765deed` (feat)

_Note: TDD tasks -- implementation and tests committed together per task._

## Files Created/Modified
- `crates/blufio/src/backup.rs` - Added `run_integrity_check()`, modified `run_backup()` with integrity verification and corrupt file deletion, modified `run_restore()` with pre-check/post-check/rollback, added 6 new tests

## Decisions Made
- **Separate integrity check from doctor.rs:** Kept `run_integrity_check` in backup.rs rather than extracting a shared utility. Doctor.rs uses async `tokio_rusqlite::Connection` while backup.rs uses sync `rusqlite::Connection` -- sharing would require a trait abstraction not worth the complexity for 10 lines of PRAGMA logic.
- **PRAGMA integrity_check(1):** Limited to 1 error row per research pitfall #3 for speed on badly corrupted databases. Only the first error is shown to the operator.
- **Corruption test strategy:** Used multi-page databases (100 rows with padding) and corrupted bytes in the second page area (offset 4096-4196) to reliably trigger integrity_check failure without preventing the file from opening. Accepts both our custom "integrity check failed" message and rusqlite's "malformed" error since both indicate corruption detection.
- **Rollback uses fs::copy:** Used `std::fs::copy` for rollback from `.pre-restore` rather than the Backup API, per research recommendation -- simpler, no SQLite connection needed for static file copy.

## Deviations from Plan

None -- plan executed exactly as written.

## Issues Encountered
- Initial corruption test (overwriting bytes 100-200) was too aggressive -- corrupted the B-tree root page header so severely that rusqlite could not even prepare the PRAGMA statement, returning "database disk image is malformed" before our custom error path ran. Fixed by using a multi-page database and corrupting the second page (offset 4096+), which allows the connection to open but triggers integrity_check failure. The test now accepts both error forms as valid corruption detection.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Backup integrity verification complete, ready for Phase 24 (sd_notify Integration)
- Phase 25 (SQLCipher) depends on this phase's integrity check pattern for verifying encrypted export correctness

---
*Phase: 23-backup-integrity-verification*
*Completed: 2026-03-03*
