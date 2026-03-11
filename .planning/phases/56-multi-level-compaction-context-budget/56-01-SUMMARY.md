---
phase: 56-multi-level-compaction-context-budget
plan: 01
subsystem: context
tags: [compaction, context-budget, sqlite, events, config]

# Dependency graph
requires:
  - phase: 55-memory-enhancements
    provides: "Memory system with MemoryStore, FTS5 search, file watcher"
provides:
  - "Extended ContextConfig with 17 compaction/budget fields and serde defaults"
  - "CompactionEvent (Started/Completed) on BusEvent"
  - "StorageAdapter::delete_messages_by_ids trait method with SQLite impl"
  - "V13 compaction_archives migration with CRUD query module"
  - "ArchiveRow struct and 7 query functions (insert, list, get, delete, count, oldest, GDPR erasure)"
affects: [56-02, 56-03, 56-04, compaction-engine, quality-scoring, archive-system, cli]

# Tech tracking
tech-stack:
  added: [tracing (blufio-config)]
  patterns: [effective_soft_trigger() deprecation bridge, String-typed bus events for cross-crate isolation]

key-files:
  created:
    - "crates/blufio-storage/migrations/V13__compaction_archives.sql"
    - "crates/blufio-storage/src/queries/archives.rs"
  modified:
    - "crates/blufio-config/src/model.rs"
    - "crates/blufio-bus/src/events.rs"
    - "crates/blufio-core/src/traits/storage.rs"
    - "crates/blufio-storage/src/adapter.rs"
    - "crates/blufio-storage/src/queries/messages.rs"
    - "crates/blufio-storage/src/queries/mod.rs"
    - "crates/blufio-audit/src/subscriber.rs"
    - "crates/blufio-context/src/dynamic.rs"
    - "crates/blufio-mcp-server/src/resources.rs"
    - "crates/blufio/tests/e2e_mcp_server.rs"

key-decisions:
  - "compaction_threshold changed to Option<f64> with effective_soft_trigger() bridge for backward compat"
  - "CompactionEvent uses String fields (no cross-crate deps) following existing bus event pattern"
  - "delete_messages_by_ids uses parameterized IN clause with dynamic placeholder generation"
  - "Archive session_ids stored as JSON text with LIKE-based GDPR erasure"

patterns-established:
  - "Deprecation bridge: effective_soft_trigger() maps old field to new with tracing::warn"
  - "Archive CRUD: row_to_archive helper with unwrap_or_default for resilient parsing"

requirements-completed: [COMP-04, COMP-05, CTXE-01, CTXE-02]

# Metrics
duration: 7min
completed: 2026-03-11
---

# Phase 56 Plan 01: Foundation Types Summary

**Extended ContextConfig with 17 compaction/budget fields, CompactionEvent bus events, delete_messages_by_ids trait method, and V13 compaction_archives migration with full CRUD**

## Performance

- **Duration:** 7 min
- **Started:** 2026-03-11T22:40:00Z
- **Completed:** 2026-03-11T22:47:06Z
- **Tasks:** 2
- **Files modified:** 13

## Accomplishments
- ContextConfig extended with all compaction fields (soft/hard triggers, quality scoring weights/gates, L1-L3 token limits, zone budgets, archive settings) with correct serde defaults
- Backward-compatible deprecation of compaction_threshold via effective_soft_trigger() bridge
- CompactionEvent (Started/Completed) added to BusEvent with audit subscriber integration
- StorageAdapter::delete_messages_by_ids added to trait with SQLite and mock implementations
- V13 migration creates compaction_archives table with indexes on user_id and created_at
- Archive query module with 7 functions and 8 integration tests all passing

## Task Commits

Each task was committed atomically:

1. **Task 1: Extend ContextConfig, add CompactionEvent, add delete_messages_by_ids** - `bf7350e` (feat)
2. **Task 2: Create compaction_archives migration and archive query module** - `993903f` (feat)

## Files Created/Modified
- `crates/blufio-config/Cargo.toml` - Added tracing dependency for deprecation warnings
- `crates/blufio-config/src/model.rs` - Extended ContextConfig with 17 new fields, defaults, effective_soft_trigger()
- `crates/blufio-bus/src/events.rs` - Added CompactionEvent enum and Compaction variant to BusEvent
- `crates/blufio-core/src/traits/storage.rs` - Added delete_messages_by_ids to StorageAdapter trait
- `crates/blufio-storage/src/adapter.rs` - Implemented delete_messages_by_ids for SqliteStorage
- `crates/blufio-storage/src/queries/messages.rs` - Added delete_messages_by_ids SQL query function
- `crates/blufio-storage/src/queries/mod.rs` - Registered archives module
- `crates/blufio-storage/src/queries/archives.rs` - New: ArchiveRow struct, 7 CRUD functions, 8 tests
- `crates/blufio-storage/migrations/V13__compaction_archives.sql` - New: compaction_archives table DDL
- `crates/blufio-audit/src/subscriber.rs` - Added Compaction event conversion to audit entries
- `crates/blufio-context/src/dynamic.rs` - DynamicZone uses effective_soft_trigger()
- `crates/blufio-mcp-server/src/resources.rs` - Added delete_messages_by_ids to MockStorage
- `crates/blufio/tests/e2e_mcp_server.rs` - Added delete_messages_by_ids to MockStorage

## Decisions Made
- **compaction_threshold as Option<f64>**: Changed from required f64 to Option to maintain backward compat with deny_unknown_fields. Old configs with `compaction_threshold = 0.70` parse into `Some(0.70)` and are mapped via effective_soft_trigger()
- **CompactionEvent uses String fields**: Follows established pattern (ClassificationEvent, MemoryEvent) to avoid blufio-bus -> blufio-core dependency
- **Parameterized IN clause**: delete_messages_by_ids builds dynamic SQL with numbered placeholders to avoid SQL injection while supporting variable-length ID lists
- **JSON text LIKE for GDPR**: Archive session_ids stored as JSON text; GDPR erasure uses LIKE matching which is simple and correct for the bounded archive count

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed DynamicZone compilation after compaction_threshold type change**
- **Found during:** Task 1 (ContextConfig extension)
- **Issue:** crates/blufio-context/src/dynamic.rs accessed `config.compaction_threshold` as f64 directly; now Option<f64>
- **Fix:** Changed to use `config.effective_soft_trigger()` which handles the Option and deprecation warning
- **Files modified:** crates/blufio-context/src/dynamic.rs
- **Verification:** cargo check --workspace passes, test assertion updated from 0.70 to 0.50
- **Committed in:** bf7350e (Task 1 commit)

**2. [Rule 3 - Blocking] Added CompactionEvent to audit subscriber exhaustive match**
- **Found during:** Task 1 (CompactionEvent addition)
- **Issue:** crates/blufio-audit/src/subscriber.rs has exhaustive match on BusEvent; new Compaction variant caused E0004
- **Fix:** Added Compaction::Started and Compaction::Completed arms with appropriate audit entry fields
- **Files modified:** crates/blufio-audit/src/subscriber.rs
- **Verification:** cargo check --workspace passes
- **Committed in:** bf7350e (Task 1 commit)

**3. [Rule 3 - Blocking] Added tracing dependency to blufio-config**
- **Found during:** Task 1 (effective_soft_trigger deprecation warning)
- **Issue:** blufio-config needed tracing::warn! but had no tracing dependency
- **Fix:** Added `tracing.workspace = true` to Cargo.toml
- **Files modified:** crates/blufio-config/Cargo.toml
- **Verification:** cargo check passes
- **Committed in:** bf7350e (Task 1 commit)

---

**Total deviations:** 3 auto-fixed (3 blocking)
**Impact on plan:** All auto-fixes necessary for compilation correctness. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All foundation types in place for subsequent compaction plans
- ContextConfig ready for consumption by compaction engine (Plan 02)
- CompactionEvent ready for publishing during compaction passes
- Archive table and queries ready for archival storage
- delete_messages_by_ids ready for post-compaction cleanup

---
*Phase: 56-multi-level-compaction-context-budget*
*Completed: 2026-03-11*
