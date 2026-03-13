---
phase: 65-sqlite-vec-foundation
plan: 01
subsystem: database
tags: [sqlite-vec, vec0, vector-search, knn, cosine-similarity, rusqlite, sqlcipher]

# Dependency graph
requires:
  - phase: 55-memory-enhancements
    provides: "MemoryStore with BLOB vectors, HybridRetriever, vec_to_blob/blob_to_vec, cosine_similarity"
  - phase: 25-encryption-at-rest
    provides: "SQLCipher with bundled-sqlcipher-vendored-openssl, PRAGMA key pattern"
provides:
  - "sqlite-vec 0.1.6 compiled into binary via SQLITE_CORE (no loadable extension)"
  - "vec0.rs module: ensure_sqlite_vec_registered(), check_vec0_available(), vec0_insert/delete/update_status/search/populate_batch/count/drop_and_recreate"
  - "Vec0SearchResult type with auxiliary columns for JOIN-free retrieval"
  - "V15 migration: memories_vec0 virtual table with metadata, partition key, embedding, auxiliary columns"
  - "MemoryConfig.vec0_enabled field (default false, serde-tagged)"
  - "MemoryEvent::Vec0Enabled, Vec0FallbackTriggered, Vec0PopulationComplete variants"
affects: [65-02-PLAN, 66-hybrid-retriever-wiring, 67-migration-pipeline]

# Tech tracking
tech-stack:
  added: [sqlite-vec 0.1.6]
  patterns: [sqlite3_auto_extension registration, vec0 KNN with metadata filtering, distance-to-similarity conversion in Rust]

key-files:
  created:
    - crates/blufio-memory/src/vec0.rs
    - crates/blufio-storage/migrations/V15__vec0_virtual_table.sql
  modified:
    - Cargo.toml
    - Cargo.lock
    - crates/blufio-memory/Cargo.toml
    - crates/blufio-memory/src/lib.rs
    - crates/blufio-config/src/model.rs
    - crates/blufio-bus/src/events.rs
    - crates/blufio-audit/src/subscriber.rs

key-decisions:
  - "vec0 auxiliary columns use 'float' not 'real' -- sqlite-vec 0.1.6 parser only accepts text/int/integer/float/double/blob"
  - "vec0 UPDATE on metadata columns works (tested) -- no need for DELETE+INSERT fallback pattern"
  - "Distance-to-similarity conversion (1.0 - distance) applied post-KNN in Rust, not in SQL"
  - "VEC-03 metadata filtering (status='active', classification!='restricted') applied during KNN via vec0 WHERE clause"

patterns-established:
  - "sqlite-vec registration: call ensure_sqlite_vec_registered() at process startup before any DB connections"
  - "vec0 schema: metadata text columns for in-query filtering, partition key for session scoping, auxiliary +prefixed columns for JOIN-free retrieval"
  - "KNN search pattern: embedding MATCH ?1 AND k = ?2 AND status = 'active' AND classification != 'restricted'"
  - "vec0 population: LEFT JOIN IS NULL pattern for idempotent batch insert from memories table"

requirements-completed: [VEC-01, VEC-02]

# Metrics
duration: 14min
completed: 2026-03-13
---

# Phase 65 Plan 01: sqlite-vec Foundation Summary

**sqlite-vec vec0 virtual table with registration, KNN search, metadata filtering, CRUD operations, and startup population -- all tested on in-memory SQLite**

## Performance

- **Duration:** 14 min
- **Started:** 2026-03-13T19:45:56Z
- **Completed:** 2026-03-13T20:00:37Z
- **Tasks:** 2
- **Files modified:** 10

## Accomplishments
- sqlite-vec 0.1.6 compiles into the binary alongside SQLCipher (SQLITE_CORE, no loadable extension)
- vec0 KNN search with VEC-03 metadata filtering: status and classification filtered during KNN, not post-query
- Complete CRUD: insert, delete, update_status (metadata column update confirmed working), KNN search with cosine distance-to-similarity conversion
- Idempotent batch population for startup eager loading (LEFT JOIN IS NULL skip pattern)
- 14 comprehensive unit tests covering all operations, including mixed status filtering, restricted classification exclusion, similarity threshold, session_id partition filtering

## Task Commits

Each task was committed atomically:

1. **Task 1: Add sqlite-vec dependency, config fields, event variants, V15 migration** - `d570b65` (feat)
2. **Task 2: Create vec0.rs module with extension registration and CRUD operations** - `7009ced` (feat)

## Files Created/Modified
- `crates/blufio-memory/src/vec0.rs` - sqlite-vec registration, vec0 CRUD operations, KNN search, batch population, 14 unit tests
- `crates/blufio-storage/migrations/V15__vec0_virtual_table.sql` - CREATE VIRTUAL TABLE memories_vec0 with metadata, partition key, embedding, auxiliary columns
- `Cargo.toml` - Added sqlite-vec 0.1.6 workspace dependency
- `crates/blufio-memory/Cargo.toml` - Added sqlite-vec.workspace = true
- `crates/blufio-memory/src/lib.rs` - Added pub mod vec0
- `crates/blufio-config/src/model.rs` - Added vec0_enabled field to MemoryConfig (default false) with tests
- `crates/blufio-bus/src/events.rs` - Added Vec0Enabled, Vec0FallbackTriggered, Vec0PopulationComplete variants with event_type_string and roundtrip tests
- `crates/blufio-audit/src/subscriber.rs` - Added audit handling for three new vec0 event variants

## Decisions Made
- **vec0 auxiliary column type "float" not "real":** sqlite-vec 0.1.6 parser only accepts text, int, integer, float, double, blob for auxiliary column types. Using "real" causes a spurious "chunk_size must be a non-zero positive integer" error because the unrecognized type falls through to the table options parser. Changed `+confidence real` to `+confidence float` in both the migration and Rust code.
- **vec0 UPDATE on metadata columns works:** Tested and confirmed that sqlite-vec 0.1.6 supports UPDATE on metadata columns (e.g., changing status from 'active' to 'forgotten'). No need for the DELETE+INSERT fallback pattern mentioned in the research.
- **KNN search uses k= syntax:** Using `k = ?2` parameter in WHERE clause rather than SQL LIMIT for explicit KNN candidate count control, per sqlite-vec documentation recommendation.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed vec0 auxiliary column type from "real" to "float"**
- **Found during:** Task 2 (vec0.rs creation and testing)
- **Issue:** vec0 CREATE VIRTUAL TABLE failed with "chunk_size must be a non-zero positive integer" when using `+confidence real`. The sqlite-vec 0.1.6 column parser does not recognize "real" as a valid auxiliary column type -- it only accepts text/int/integer/float/double/blob. Unrecognized types fall through to the table options parser which misinterprets the argument.
- **Fix:** Changed `+confidence real` to `+confidence float` in V15 migration, vec0_drop_and_recreate(), and test setup. Both "float" and "real" map to SQLITE_FLOAT at the SQLite level, so the semantic change is zero.
- **Files modified:** crates/blufio-storage/migrations/V15__vec0_virtual_table.sql, crates/blufio-memory/src/vec0.rs
- **Verification:** All 14 vec0 unit tests pass, workspace builds cleanly
- **Committed in:** 7009ced (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Essential fix for correctness. The plan specified `+confidence real` per standard SQL convention, but sqlite-vec requires `float` for the same type. No scope creep.

## Issues Encountered
- Pre-existing vault test failures (13 tests) in blufio-vault crate -- unrelated to this plan's changes, likely environment-specific issue with vault passphrase or system keyring. Not addressed.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- vec0.rs module is fully self-contained and tested in isolation
- Plan 02 can wire vec0 operations into MemoryStore (dual-write in save(), sync in batch_evict/soft_delete) and HybridRetriever (vec0 KNN replacing in-memory cosine when vec0_enabled=true)
- V15 migration creates the table; startup population via vec0_populate_batch() needs to be called from MemoryStore initialization when vec0_enabled=true
- sqlite-vec extension registration (ensure_sqlite_vec_registered()) needs to be called at process startup before Database::open()

## Self-Check: PASSED

All artifacts verified:
- 7 key files exist on disk
- 2 task commits found in git log (d570b65, 7009ced)
- vec0.rs: 677 lines, all 11 required symbols present
- MemoryConfig.vec0_enabled: 9 references in model.rs
- Vec0 event variants: 21 references in events.rs

---
*Phase: 65-sqlite-vec-foundation*
*Completed: 2026-03-13*
