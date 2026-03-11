---
phase: 55-memory-enhancements
plan: 03
subsystem: memory
tags: [sqlite, eviction, validation, background-task, cosine-similarity, tokio]

# Dependency graph
requires:
  - phase: 55-memory-enhancements/01
    provides: MemoryConfig with eviction/validation fields, MemoryEvent::Evicted variant, EventBus integration
provides:
  - count_active, get_all_active_with_embeddings, batch_evict store methods
  - Eviction sweep logic (triggers at count > max_entries, evicts to 90%)
  - Validation with duplicate/conflict/stale detection and auto-resolution
  - Combined background task with configurable eviction and daily validation timers
  - run_validation_dry_run for CLI dry-run mode
affects: [55-memory-enhancements/04, cli-memory-commands]

# Tech tracking
tech-stack:
  added: []
  patterns: [rust-side-scoring-for-sqlite, pairwise-embedding-comparison, cancellation-token-shutdown]

key-files:
  created:
    - crates/blufio-memory/src/eviction.rs
    - crates/blufio-memory/src/validation.rs
    - crates/blufio-memory/src/background.rs
  modified:
    - crates/blufio-memory/src/store.rs
    - crates/blufio-memory/src/lib.rs

key-decisions:
  - "Eviction scores computed in Rust (not SQL) because SQLite lacks native power() function"
  - "Pairwise O(n^2) comparison acceptable for validation since max_entries bounded at 10k"
  - "Conflict resolution uses newer-wins (created_at lexicographic comparison)"
  - "Orthogonal embedding test fixtures via single-hot-dimension for deterministic similarity control"

patterns-established:
  - "Rust-side scoring: load minimal fields from SQLite, compute score in Rust, then batch-DELETE by IDs"
  - "resolved_ids HashSet to prevent double-processing during pairwise validation"
  - "CancellationToken-based background task with skip-first-tick intervals"

requirements-completed: [MEME-04, MEME-05]

# Metrics
duration: 7min
completed: 2026-03-11
---

# Phase 55 Plan 03: Eviction & Validation Summary

**Bounded memory index with LRU eviction sweep, pairwise duplicate/conflict/stale validation, and combined background task**

## Performance

- **Duration:** 7 min
- **Started:** 2026-03-11T20:00:47Z
- **Completed:** 2026-03-11T20:08:03Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- Three new store methods: count_active (excludes restricted/non-active), get_all_active_with_embeddings, and batch_evict with Rust-side eviction scoring
- Eviction sweep that triggers when active count exceeds max_entries and evicts to 90% of max, emitting bulk MemoryEvent::Evicted
- Validation with three detection modes: duplicates (>0.9 cosine sim, supersede lower confidence), conflicts (0.7-0.9 sim, newer wins), stale (age + decay floor, soft-delete)
- Combined background task with 5min eviction and daily validation timers, respecting CancellationToken for graceful shutdown
- Dry-run validation mode returning counts without store modifications

## Task Commits

Each task was committed atomically:

1. **Task 1: Add store methods and implement eviction sweep** - `ba1ac2f` (feat)
2. **Task 2: Implement background validation and combined background task** - `f9129ec` (feat)

## Files Created/Modified
- `crates/blufio-memory/src/store.rs` - Added count_active, get_all_active_with_embeddings, batch_evict methods with inline tests
- `crates/blufio-memory/src/eviction.rs` - Eviction sweep logic: count check, target calculation, batch evict, event emission
- `crates/blufio-memory/src/validation.rs` - Duplicate/conflict/stale detection with auto-resolution and dry_run mode
- `crates/blufio-memory/src/background.rs` - Combined tokio task with eviction (5min) and validation (daily) interval timers
- `crates/blufio-memory/src/lib.rs` - Added eviction, validation, and background module declarations

## Decisions Made
- Eviction scores computed in Rust because SQLite lacks a native `power()` function; load (id, source, created_at) and compute `importance_boost * max(decay_factor^days, decay_floor)` in Rust
- Pairwise O(n^2) comparison in validation is acceptable since max_entries is bounded (default 10,000)
- Conflict resolution uses newer-wins by comparing created_at timestamps lexicographically (ISO 8601 format is lexicographically sortable)
- Test embeddings use single-hot-dimension vectors for deterministic similarity control (orthogonal for unrelated, identical for duplicates)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed borrow conflict in batch_evict**
- **Found during:** Task 1
- **Issue:** Rust borrow checker rejected mutable borrow of `conn.transaction()` while `stmt` (from `conn.prepare()`) was still live
- **Fix:** Scoped the `stmt` and `query_map` in a block so the immutable borrow drops before the transaction
- **Files modified:** crates/blufio-memory/src/store.rs
- **Verification:** Compilation succeeded
- **Committed in:** ba1ac2f

**2. [Rule 1 - Bug] Fixed implicit borrow pattern in eviction.rs**
- **Found during:** Task 1
- **Issue:** Rust 2024 edition rejects `ref` binding in implicitly-borrowing pattern (`if let Some(ref bus) = event_bus`)
- **Fix:** Removed explicit `ref` modifier
- **Files modified:** crates/blufio-memory/src/eviction.rs
- **Verification:** Compilation succeeded
- **Committed in:** ba1ac2f

**3. [Rule 1 - Bug] Fixed test embedding similarity calculation**
- **Found during:** Task 2
- **Issue:** Initial `make_embedding(base, dim)` function produced near-identical normalized vectors for different base values (offset difference negligible relative to magnitude), causing false duplicate/conflict detection in unrelated test cases
- **Fix:** Redesigned `make_embedding(seed, dim)` to use single-hot-dimension pattern (max weight at seed position) for deterministic orthogonal embeddings, plus `make_conflict_pair()` helper for analytical 0.8 cosine similarity
- **Files modified:** crates/blufio-memory/src/validation.rs
- **Verification:** All 8 validation tests pass with correct detection categories
- **Committed in:** f9129ec

---

**Total deviations:** 3 auto-fixed (3 bug fixes)
**Impact on plan:** All auto-fixes necessary for compilation and test correctness. No scope creep.

## Issues Encountered
None beyond the auto-fixed items above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Store methods (count_active, batch_evict, get_all_active_with_embeddings) ready for Plan 04 file watcher integration
- Background task spawn_background_task ready for integration in serve.rs startup
- Validation dry_run available for CLI `blufio memory validate --dry-run` command

---
*Phase: 55-memory-enhancements*
*Completed: 2026-03-11*
