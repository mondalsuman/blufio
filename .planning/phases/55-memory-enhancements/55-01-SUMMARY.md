---
phase: 55-memory-enhancements
plan: 01
subsystem: memory
tags: [memory-config, scoring, eviction, file-watcher, notify, serde, deny-unknown-fields]

# Dependency graph
requires: []
provides:
  - "Extended MemoryConfig with 10 new scoring/eviction/validation/watcher fields"
  - "FileWatcherConfig nested struct with deny_unknown_fields"
  - "MemorySource::FileWatcher variant with as_str/from_str_value round-trip"
  - "Bulk MemoryEvent::Evicted (count, lowest_score, highest_score)"
  - "notify 8.2 and notify-debouncer-mini 0.7 as workspace dependencies"
  - "sha2, tokio-util, metrics added to blufio-memory dependencies"
affects: [55-02-scoring, 55-03-eviction, 55-04-validation, 55-05-file-watcher, 55-06-cli-metrics]

# Tech tracking
tech-stack:
  added: [notify 8.2, notify-debouncer-mini 0.7]
  patterns: [FileWatcherConfig nested struct with manual Default impl, bulk eviction events]

key-files:
  created: []
  modified:
    - "crates/blufio-config/src/model.rs"
    - "crates/blufio-memory/src/types.rs"
    - "crates/blufio-bus/src/events.rs"
    - "crates/blufio-audit/src/subscriber.rs"
    - "crates/blufio-memory/Cargo.toml"
    - "Cargo.toml"

key-decisions:
  - "FileWatcherConfig uses manual Default impl (not derive) to ensure max_file_size defaults to 102400 instead of 0"
  - "MemorySource::from_str_value places file_watcher before fallback to maintain Extracted as catch-all"

patterns-established:
  - "Nested config structs with manual Default impl when non-zero defaults are needed"
  - "Bulk event format for sweep operations (count + score range instead of per-item)"

requirements-completed: [MEME-01, MEME-02, MEME-03, MEME-04, MEME-05, MEME-06]

# Metrics
duration: 8min
completed: 2026-03-11
---

# Phase 55 Plan 01: Type Foundation Summary

**Extended MemoryConfig with decay/boost/MMR/eviction/validation/watcher fields, added MemorySource::FileWatcher, and converted MemoryEvent::Evicted to bulk format**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-11T19:48:46Z
- **Completed:** 2026-03-11T19:57:25Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments
- Extended MemoryConfig with 10 new fields covering scoring (decay_factor, decay_floor, mmr_lambda, importance boosts), eviction (max_entries, sweep interval), validation (stale_threshold_days), and file watcher configuration
- Added FileWatcherConfig nested struct with paths, extensions, max_file_size and deny_unknown_fields enforcement
- Added MemorySource::FileWatcher variant with full as_str/from_str_value round-trip support
- Converted MemoryEvent::Evicted from per-memory format (memory_id, reason) to bulk format (count, lowest_score, highest_score)
- Updated audit subscriber to handle bulk eviction events correctly
- Added notify, notify-debouncer-mini, sha2, tokio-util, metrics dependencies for downstream plans

## Task Commits

Each task was committed atomically:

1. **Task 1: Extend MemoryConfig and add MemorySource::FileWatcher** - `7eadbcc` (feat)
2. **Task 2: Convert MemoryEvent::Evicted to bulk format and update consumers** - `392c93d` (feat)

## Files Created/Modified
- `crates/blufio-config/src/model.rs` - Extended MemoryConfig with 10 new fields, added FileWatcherConfig struct, 16 new tests
- `crates/blufio-memory/src/types.rs` - Added MemorySource::FileWatcher variant with as_str/from_str_value, updated tests
- `crates/blufio-bus/src/events.rs` - Changed MemoryEvent::Evicted to bulk format (count, lowest_score, highest_score)
- `crates/blufio-audit/src/subscriber.rs` - Updated Evicted pattern match for bulk format with batch:{count} resource_id
- `crates/blufio-memory/Cargo.toml` - Added sha2, tokio-util, notify, notify-debouncer-mini, metrics dependencies
- `Cargo.toml` - Added notify 8.2, notify-debouncer-mini 0.7 to workspace dependencies

## Decisions Made
- FileWatcherConfig uses manual Default impl (not `#[derive(Default)]`) to ensure max_file_size defaults to 102400 instead of 0. The derive(Default) approach would set usize fields to 0, which is incorrect for max_file_size.
- MemorySource::from_str_value matches "file_watcher" explicitly before the catch-all that returns Extracted, maintaining backward compatibility.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed FileWatcherConfig Default implementation**
- **Found during:** Task 1 (MemoryConfig extension)
- **Issue:** Using `#[derive(Default)]` on FileWatcherConfig set max_file_size to 0 instead of 102400
- **Fix:** Replaced derive(Default) with manual Default impl that calls default_max_file_size()
- **Files modified:** crates/blufio-config/src/model.rs
- **Verification:** memory_config_file_watcher_defaults test passes
- **Committed in:** 7eadbcc (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Essential fix for correct default behavior. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All type foundations in place for downstream plans (scoring, eviction, validation, file watcher)
- MemoryConfig fields ready for consumption by retriever.rs (Plan 02: scoring pipeline)
- MemoryEvent::Evicted bulk format ready for eviction sweep implementation (Plan 03)
- notify and notify-debouncer-mini available for file watcher implementation (Plan 05)
- Full workspace compiles cleanly with all changes
- All 201 tests pass across affected crates (89 config + 17 bus + 61 memory + 33 audit + 1 doctest)

## Self-Check: PASSED

All 7 files verified present. Both task commits (7eadbcc, 392c93d) confirmed in git log.

---
*Phase: 55-memory-enhancements*
*Completed: 2026-03-11*
