---
phase: 58-cron-scheduler-retention-policies
plan: 01
subsystem: cron, retention, database
tags: [croner, cron-scheduling, soft-delete, retention-policies, sqlite-migration, async-trait]

# Dependency graph
requires:
  - phase: 57-injection-defense
    provides: "BusEvent enum with Security variant, InjectionDefenseConfig"
  - phase: 54-audit-trail
    provides: "AuditSubscriber exhaustive BusEvent match"
provides:
  - "blufio-cron crate with CronTask trait and CronTaskError"
  - "CronConfig, CronJobConfig, RetentionConfig, RetentionPeriods in BlufioConfig"
  - "CronEvent (Completed/Failed) variant on BusEvent"
  - "V14 migration: cron_jobs, cron_history tables + deleted_at soft-delete columns"
  - "Soft-delete filtering on all existing read queries"
affects: [58-02, 58-03, 58-04, 58-05]

# Tech tracking
tech-stack:
  added: [croner v3]
  patterns: [soft-delete via deleted_at column, CronTask async trait]

key-files:
  created:
    - "crates/blufio-cron/Cargo.toml"
    - "crates/blufio-cron/src/lib.rs"
    - "crates/blufio-cron/src/tasks/mod.rs"
    - "crates/blufio-storage/migrations/V14__cron_retention.sql"
  modified:
    - "Cargo.toml"
    - "crates/blufio-config/src/model.rs"
    - "crates/blufio-bus/src/events.rs"
    - "crates/blufio-audit/src/subscriber.rs"
    - "crates/blufio-storage/src/queries/messages.rs"
    - "crates/blufio-storage/src/queries/sessions.rs"
    - "crates/blufio-storage/src/queries/classification.rs"
    - "crates/blufio-cost/src/ledger.rs"
    - "crates/blufio-cost/src/budget.rs"
    - "crates/blufio-memory/src/store.rs"
    - "crates/blufio-memory/src/eviction.rs"
    - "crates/blufio-memory/src/validation.rs"
    - "crates/blufio-memory/src/background.rs"
    - "crates/blufio-mcp-server/src/resources.rs"

key-decisions:
  - "CronConfig and RetentionConfig added as serde(default) fields on BlufioConfig following existing pattern"
  - "CronEvent uses String fields following established bus event pattern (no cross-crate deps)"
  - "Soft-delete filtering added to classification queries in addition to CRUD queries"
  - "Test DB schemas across 6 files updated with deleted_at column for consistency"

patterns-established:
  - "deleted_at IS NULL filter: all SELECT queries against messages/sessions/cost_ledger/memories must include this"
  - "CronTask trait: async-trait pattern with name(), description(), execute(), timeout()"

requirements-completed: [CRON-01, CRON-06, RETN-01, RETN-03]

# Metrics
duration: 18min
completed: 2026-03-12
---

# Phase 58 Plan 01: Cron & Retention Foundation Summary

**New blufio-cron crate with CronTask trait, CronConfig/RetentionConfig models, CronEvent on EventBus, V14 migration for cron tables + soft-delete columns, and deleted_at filtering on all existing read queries**

## Performance

- **Duration:** 18 min
- **Started:** 2026-03-12T17:54:48Z
- **Completed:** 2026-03-12T18:12:48Z
- **Tasks:** 2
- **Files modified:** 19

## Accomplishments
- Created blufio-cron crate with CronTask trait and CronTaskError types
- Added CronConfig (with CronJobConfig) and RetentionConfig (with RetentionPeriods) to BlufioConfig
- Added CronEvent (Completed/Failed) to BusEvent enum with event_type_string, audit subscriber, and tests
- Created V14 migration with cron_jobs, cron_history tables and deleted_at columns on 4 existing tables
- Added deleted_at IS NULL filtering to all existing SELECT queries across messages, sessions, cost_ledger, memories, and classification tables
- Updated 6 test schema definitions with deleted_at column
- Full workspace test suite passes (1800+ tests, 0 failures)

## Task Commits

Each task was committed atomically:

1. **Task 1: Create blufio-cron crate, config types, CronEvent, and V14 migration** - `1c8881b` (feat)
2. **Task 2: Add soft-delete filtering to existing storage queries** - `55390ed` (feat)

## Files Created/Modified
- `crates/blufio-cron/Cargo.toml` - New crate manifest with croner and workspace deps
- `crates/blufio-cron/src/lib.rs` - Public API re-exports CronTask, CronTaskError
- `crates/blufio-cron/src/tasks/mod.rs` - CronTask trait and CronTaskError enum
- `crates/blufio-storage/migrations/V14__cron_retention.sql` - Cron tables + deleted_at columns
- `Cargo.toml` - Added croner = "3" to workspace dependencies
- `crates/blufio-config/src/model.rs` - CronConfig, CronJobConfig, RetentionConfig, RetentionPeriods
- `crates/blufio-bus/src/events.rs` - CronEvent enum + BusEvent::Cron variant
- `crates/blufio-audit/src/subscriber.rs` - CronEvent match arms for audit trail
- `crates/blufio-storage/src/queries/messages.rs` - deleted_at IS NULL on message SELECTs
- `crates/blufio-storage/src/queries/sessions.rs` - deleted_at IS NULL on session SELECTs
- `crates/blufio-storage/src/queries/classification.rs` - deleted_at IS NULL on classification queries
- `crates/blufio-cost/src/ledger.rs` - deleted_at IS NULL on cost aggregate queries
- `crates/blufio-cost/src/budget.rs` - deleted_at in test schema
- `crates/blufio-memory/src/store.rs` - deleted_at IS NULL on all memory queries + test schema
- `crates/blufio-memory/src/eviction.rs` - deleted_at in test schema
- `crates/blufio-memory/src/validation.rs` - deleted_at in test schema
- `crates/blufio-memory/src/background.rs` - deleted_at in test schema
- `crates/blufio-mcp-server/src/resources.rs` - deleted_at in test schema

## Decisions Made
- CronConfig and RetentionConfig use `#[serde(default)]` on BlufioConfig fields following established pattern
- CronEvent uses String fields to avoid cross-crate dependencies (following ClassificationEvent, CompactionEvent pattern)
- Soft-delete filtering applied to classification queries (get/set/list/bulk_update) in addition to CRUD queries
- Test DB schemas updated with `deleted_at TEXT` column across 6 files to prevent "no such column" errors
- Migration queries in migrate.rs intentionally excluded from filtering (operate on old schema formats)
- DELETE queries intentionally excluded from filtering per plan instructions

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Non-exhaustive BusEvent match in audit subscriber**
- **Found during:** Task 2 (cargo test --workspace)
- **Issue:** Adding CronEvent variant to BusEvent broke exhaustive match in blufio-audit/subscriber.rs
- **Fix:** Added CronEvent::Completed and CronEvent::Failed match arms with appropriate PendingEntry conversion, added CronEvent import
- **Files modified:** crates/blufio-audit/src/subscriber.rs
- **Verification:** cargo test --workspace passes
- **Committed in:** 55390ed (Task 2 commit)

**2. [Rule 3 - Blocking] Test DB schemas missing deleted_at column**
- **Found during:** Task 2 (cargo test --workspace)
- **Issue:** 6 test files create in-memory DB schemas without deleted_at column, causing "no such column: deleted_at" errors
- **Fix:** Added `deleted_at TEXT` to test schemas in budget.rs, ledger.rs, store.rs, eviction.rs, validation.rs, background.rs, resources.rs
- **Files modified:** 7 test schema definitions across 6 files
- **Verification:** Full workspace test suite passes
- **Committed in:** 55390ed (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Both auto-fixes necessary for compilation and test success. No scope creep.

## Issues Encountered
None beyond the auto-fixed deviations above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- CronTask trait ready for built-in task implementations (Plan 02+)
- CronConfig/RetentionConfig ready for scheduler integration
- V14 migration ready for DB deployment
- All existing queries properly filter soft-deleted records
- CronEvent ready for bus integration in scheduler

---
*Phase: 58-cron-scheduler-retention-policies*
*Completed: 2026-03-12*
