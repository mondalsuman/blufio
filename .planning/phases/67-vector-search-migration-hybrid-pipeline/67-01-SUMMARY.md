---
phase: 67-vector-search-migration-hybrid-pipeline
plan: 01
subsystem: database
tags: [sqlite-vec, vec0, memory, migration, embedding, startup]

# Dependency graph
requires:
  - phase: 65-sqlite-vec-foundation
    provides: vec0 schema, dual-write, populate_vec0(), ensure_sqlite_vec_registered()
provides:
  - Startup vec0 wiring in both serve and shell paths
  - get_embeddings_by_ids() for lightweight embedding-only fetch (MMR optimization)
  - vec0_enabled defaults to true for new installs
affects: [67-02-hybrid-pipeline, 67-03-parity-validation, 68-performance-benchmarks]

# Tech tracking
tech-stack:
  added: []
  patterns: [startup-vec0-registration-before-connection, graceful-population-fallback]

key-files:
  created: []
  modified:
    - crates/blufio/src/serve/storage.rs
    - crates/blufio/src/shell.rs
    - crates/blufio-memory/src/store.rs
    - crates/blufio-config/src/model.rs
    - crates/blufio-memory/src/retriever.rs

key-decisions:
  - "populate_vec0 failure logs warning but does not crash startup -- retriever falls back to in-memory search"
  - "vec0_enabled defaults to true for new installs; existing installs with explicit vec0_enabled=false retain their setting via serde"

patterns-established:
  - "Startup vec0 wiring: ensure_sqlite_vec_registered() BEFORE open_connection(), then MemoryStore::with_vec0(), then populate_vec0()"
  - "Graceful migration: log warn on population failure, continue with in-memory fallback"

requirements-completed: [VEC-04, VEC-08]

# Metrics
duration: 4min
completed: 2026-03-14
---

# Phase 67 Plan 01: Startup Vec0 Wiring Summary

**Both startup paths wire vec0 population with graceful fallback, get_embeddings_by_ids added for MMR optimization, vec0_enabled defaults to true**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-14T10:14:15Z
- **Completed:** 2026-03-14T10:18:30Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- Both serve and shell startup paths register sqlite-vec before connection and call populate_vec0() when vec0_enabled
- Added get_embeddings_by_ids() -- lightweight embedding-only fetch for MMR pairwise cosine similarity (Plan 02 prerequisite)
- Flipped vec0_enabled default to true for new installs; existing configs with explicit false retain their setting
- Population failure degrades gracefully (warning log, not crash)

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire vec0 into startup paths and add get_embeddings_by_ids** - `3819fba` (feat)
2. **Task 2: Change vec0_enabled default to true and update tests** - `d9bea52` (feat)

## Files Created/Modified
- `crates/blufio/src/serve/storage.rs` - Added vec0 registration, with_vec0 constructor, populate_vec0 call in initialize_memory
- `crates/blufio/src/shell.rs` - Same vec0 wiring as storage.rs for shell startup path
- `crates/blufio-memory/src/store.rs` - Added get_embeddings_by_ids() method + 3 unit tests
- `crates/blufio-config/src/model.rs` - Changed vec0_enabled default to true, updated test assertions
- `crates/blufio-memory/src/retriever.rs` - Updated vec0_enabled_propagates_from_config test for new default

## Decisions Made
- populate_vec0() failure logs a warning but does not crash startup -- the retriever will fall back to in-memory brute-force search
- vec0_enabled defaults to true for new installs; serde default means TOML files without explicit vec0_enabled=false will use vec0

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Updated retriever test for new vec0_enabled default**
- **Found during:** Task 2 (config default flip)
- **Issue:** retriever::tests::vec0_enabled_propagates_from_config asserted default was false
- **Fix:** Updated assertion to expect true, added explicit false toggle test
- **Files modified:** crates/blufio-memory/src/retriever.rs
- **Verification:** cargo test -p blufio-memory -- --test-threads=1 (139 passed)
- **Committed in:** d9bea52 (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug fix)
**Impact on plan:** Test assertion fix was a direct consequence of the default flip. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Both startup paths fully wired for vec0 -- ready for Plan 02 (partial JOIN elimination using get_embeddings_by_ids)
- get_embeddings_by_ids() ready for retriever to fetch only embeddings when vec0 provides metadata
- Config default flip complete -- new installs will use vec0 out of the box

## Self-Check: PASSED

All 5 modified files verified on disk. Both task commits (3819fba, d9bea52) verified in git log.

---
*Phase: 67-vector-search-migration-hybrid-pipeline*
*Completed: 2026-03-14*
