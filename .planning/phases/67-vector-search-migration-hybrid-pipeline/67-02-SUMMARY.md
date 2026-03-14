---
phase: 67-vector-search-migration-hybrid-pipeline
plan: 02
subsystem: database
tags: [sqlite-vec, vec0, memory, retriever, scoring, hybrid-search, mmr]

# Dependency graph
requires:
  - phase: 67-vector-search-migration-hybrid-pipeline
    plan: 01
    provides: Startup vec0 wiring, get_embeddings_by_ids(), vec0_enabled default flip
  - phase: 65-sqlite-vec-foundation
    provides: vec0 schema with auxiliary columns, dual-write, vec0_search returning Vec0SearchResult
provides:
  - Vec0ScoringData struct carrying vec0 auxiliary data through scoring pipeline
  - Optimized retrieve() that uses vec0 data for scoring, only fetches embeddings for MMR
  - score_from_vec0_data() and score_from_memory_structs() scoring functions
  - parse_memory_source() and temporal_decay_from_str() helper functions
  - Partial JOIN elimination -- scoring no longer needs get_memories_by_ids when vec0 enabled
affects: [67-03-parity-validation, 68-performance-benchmarks]

# Tech tracking
tech-stack:
  added: []
  patterns: [vec0-auxiliary-scoring-path, dual-scoring-pipeline]

key-files:
  created: []
  modified:
    - crates/blufio-memory/src/retriever.rs

key-decisions:
  - "Scoring functions extracted as standalone async fns rather than HybridRetriever methods -- enables testing without ONNX embedder"
  - "Removed vector_search() and vec0_vector_search() dispatch wrappers -- retrieve() handles vec0/in-memory dispatch inline for clarity"
  - "BM25-only results (not in vec0 search) fall back to get_memories_by_ids for those specific IDs, ensuring complete coverage"

patterns-established:
  - "Vec0 scoring path: vec0_vector_search_rich -> Vec0ScoringData -> score_from_vec0_data (uses get_embeddings_by_ids for MMR only)"
  - "In-memory scoring path: in_memory_vector_search -> score_from_memory_structs (uses get_memories_by_ids for everything)"

requirements-completed: [VEC-05, VEC-08]

# Metrics
duration: 8min
completed: 2026-03-14
---

# Phase 67 Plan 02: Vec0 Auxiliary Scoring Pipeline Summary

**Optimized retriever scoring to use vec0 auxiliary column data for importance boost and temporal decay, fetching only embeddings from memories table for MMR reranking**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-14T10:22:18Z
- **Completed:** 2026-03-14T10:30:23Z
- **Tasks:** 1 (TDD: 3 commits -- RED, GREEN, REFACTOR)
- **Files modified:** 1

## Accomplishments
- Vec0 path now carries content, source, confidence, created_at through pipeline via Vec0ScoringData struct
- Scoring (importance_boost * temporal_decay) uses vec0 auxiliary data instead of re-fetching from memories table
- MMR still receives real embeddings via get_embeddings_by_ids (lightweight fetch)
- In-memory fallback path completely unchanged -- no regression risk
- BM25-only results (not in vec0 search) correctly fall back to get_memories_by_ids
- 14 new tests + 139 existing tests all pass (153 total)

## Task Commits

Each task was committed atomically (TDD pattern):

1. **Task 1 RED: Failing tests for vec0 auxiliary scoring** - `bcd5bcd` (test)
2. **Task 1 GREEN: Implement vec0 scoring pipeline** - `61f9aac` (feat)
3. **Task 1 REFACTOR: Update module doc** - `1c46b5c` (refactor)

## Files Created/Modified
- `crates/blufio-memory/src/retriever.rs` - Added Vec0ScoringData struct, parse_memory_source(), temporal_decay_from_str(), score_from_vec0_data(), score_from_memory_structs(), vec0_vector_search_rich(); modified retrieve() to branch on vec0_enabled; removed dead code (vector_search, vec0_vector_search)

## Decisions Made
- Extracted scoring functions as standalone async fns (`score_from_vec0_data`, `score_from_memory_structs`) rather than `HybridRetriever` methods. This enables direct testing without needing an ONNX embedder instance, and keeps the scoring logic decoupled from the retriever struct.
- Removed the `vector_search()` dispatch wrapper and `vec0_vector_search()` method. The `retrieve()` method now handles vec0/in-memory dispatch inline, which is clearer since the vec0 path now captures rich data (`Vec0ScoringData`) that the old `(String, f32)` return type couldn't express.
- `parse_memory_source()` delegates to the existing `MemorySource::from_str_value()` for consistency, rather than implementing a separate parsing function.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Removed dead code after refactoring**
- **Found during:** Task 1 GREEN (implementation)
- **Issue:** After refactoring retrieve() to use vec0_vector_search_rich directly, the old vector_search() and vec0_vector_search() methods became dead code, producing compiler warnings
- **Fix:** Removed both unused methods since retrieve() now handles dispatch inline
- **Files modified:** crates/blufio-memory/src/retriever.rs
- **Verification:** cargo build --workspace compiles with no warnings
- **Committed in:** 61f9aac (GREEN commit)

**2. [Rule 1 - Bug] Changed scoring functions from methods to standalone fns**
- **Found during:** Task 1 RED (test writing)
- **Issue:** Plan specified score_from_vec0 and score_from_memories as HybridRetriever methods, but constructing HybridRetriever in tests requires an OnnxEmbedder (needs ONNX model file). Tests that only exercise scoring logic cannot provide a real embedder.
- **Fix:** Implemented as standalone async fns (score_from_vec0_data, score_from_memory_structs) that accept store + config directly. HybridRetriever::retrieve() calls them with &self.store and &self.config.
- **Files modified:** crates/blufio-memory/src/retriever.rs
- **Verification:** All 153 tests pass including new scoring tests
- **Committed in:** 61f9aac (GREEN commit)

---

**Total deviations:** 2 auto-fixed (2 bug fixes)
**Impact on plan:** Both fixes maintain functional equivalence. Standalone scoring fns have same behavior as methods but are independently testable. Dead code removal keeps the codebase clean.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Optimized vec0 scoring pipeline is ready for Plan 03 parity validation
- Both scoring paths (vec0 and in-memory) produce identical results for same inputs
- get_embeddings_by_ids is exercised by the vec0 path for MMR (validated in score_from_vec0_data tests)

## Self-Check: PASSED

All 1 modified file verified on disk. All 3 task commits (bcd5bcd, 61f9aac, 1c46b5c) verified in git log.

---
*Phase: 67-vector-search-migration-hybrid-pipeline*
*Completed: 2026-03-14*
