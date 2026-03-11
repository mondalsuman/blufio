---
phase: 55-memory-enhancements
plan: 04
subsystem: memory
tags: [file-watcher, notify, sha256, cli, prometheus, background-task, tokio]

# Dependency graph
requires:
  - phase: 55-memory-enhancements/01
    provides: MemoryConfig with file_watcher fields, MemorySource::FileWatcher variant, FileWatcherConfig struct
  - phase: 55-memory-enhancements/02
    provides: HybridRetriever with temporal decay skipping for FileWatcher source
  - phase: 55-memory-enhancements/03
    provides: spawn_background_task, run_validation/run_validation_dry_run, MemoryStore count_active/batch_evict/get_all_active_with_embeddings
provides:
  - File watcher module with deterministic file:SHA-256 IDs, initial scan, and live watching
  - Background task and file watcher wired into serve.rs startup
  - blufio memory validate CLI with --dry-run and --json
  - Prometheus validation counters (duplicates/stale/conflicts) and active count gauge
affects: [cli-memory-commands, observability, serve-startup]

# Tech tracking
tech-stack:
  added: []
  patterns: [notify-debouncer-blocking-send, conn-accessor-for-hard-delete, file-memory-id-sha256]

key-files:
  created:
    - crates/blufio-memory/src/watcher.rs
  modified:
    - crates/blufio-memory/src/lib.rs
    - crates/blufio-memory/src/store.rs
    - crates/blufio/src/serve.rs
    - crates/blufio/src/main.rs
    - crates/blufio-prometheus/src/recording.rs
    - crates/blufio-prometheus/src/lib.rs

key-decisions:
  - "File memory IDs use file: prefix + SHA-256 of canonical path for deterministic, collision-free IDs"
  - "File update re-indexes by hard-deleting old row then saving new (FTS5 trigger consistency)"
  - "Recursive directory walking uses std::fs::read_dir (no walkdir dependency) with Box::pin for async recursion"
  - "notify callback uses tx.blocking_send (not async send) since notify runs on its own thread"

patterns-established:
  - "conn() accessor on MemoryStore for advanced SQL operations beyond CRUD"
  - "Child cancellation tokens for sub-tasks spawned from serve.rs"
  - "File watcher disabled by default (empty paths), enabled by TOML config"

requirements-completed: [MEME-04, MEME-05, MEME-06]

# Metrics
duration: 11min
completed: 2026-03-11
---

# Phase 55 Plan 04: File Watcher, CLI Validate & Prometheus Metrics Summary

**File watcher auto-indexes workspace files with SHA-256 IDs and 500ms debounce, CLI validates memory index with --dry-run/--json, Prometheus exports validation counters and active gauge**

## Performance

- **Duration:** 11 min
- **Started:** 2026-03-11T20:11:06Z
- **Completed:** 2026-03-11T20:22:41Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- File watcher module with deterministic `file:` + SHA-256(canonical_path) memory IDs, extension filtering, max_file_size enforcement, soft-delete on file removal, and initial scan + live watching via notify debouncer-mini
- Background eviction/validation task and file watcher wired into serve.rs with CancellationToken-based shutdown
- `blufio memory validate` CLI subcommand with --dry-run (preview without modifications) and --json (structured output) flags
- Prometheus metrics: 3 validation counters (duplicates, stale, conflicts) + 1 active count gauge registered and exported

## Task Commits

Each task was committed atomically:

1. **Task 1: Implement file watcher module** - `e5e09ff` (feat)
2. **Task 2: Wire background task, file watcher, CLI validate, Prometheus metrics** - `8d5183b` (feat)

## Files Created/Modified
- `crates/blufio-memory/src/watcher.rs` - File watcher: deterministic IDs, extension filter, process_file_change, initial_scan, start_file_watcher with 500ms debounce
- `crates/blufio-memory/src/lib.rs` - Added `pub mod watcher` declaration
- `crates/blufio-memory/src/store.rs` - Added `conn()` accessor for hard-delete on file update
- `crates/blufio/src/serve.rs` - Spawns background task and file watcher after cancel token; returns embedder Arc from initialize_memory
- `crates/blufio/src/main.rs` - Added `Memory { Validate { dry_run, json } }` CLI subcommand with handler
- `crates/blufio-prometheus/src/recording.rs` - Registered 3 validation counters + 1 active count gauge; added helper functions
- `crates/blufio-prometheus/src/lib.rs` - Re-exported new validation metric functions

## Decisions Made
- File memory IDs use `file:` prefix + SHA-256 of canonical path for deterministic, collision-free identification
- File updates hard-delete old row then save new (not UPDATE) to ensure FTS5 triggers fire correctly for content changes
- Used std::fs::read_dir recursive walk instead of adding walkdir dependency (keeps dependency count low)
- notify callback uses `tx.blocking_send()` (not `.send().await`) since the notify debouncer runs on its own thread, not in the tokio runtime
- Added `conn()` accessor to MemoryStore for advanced operations like hard-delete (the CRUD methods only do soft-delete)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed type inference for tokio_rusqlite Error in delete_memory_row**
- **Found during:** Task 1
- **Issue:** Rust could not infer the error type in `map_err(|e| BlufioError::storage_connection_failed(e))` for the `conn.call()` closure
- **Fix:** Added explicit type annotations: `Ok::<(), rusqlite::Error>(())` and `|e: tokio_rusqlite::Error|`
- **Files modified:** crates/blufio-memory/src/watcher.rs
- **Verification:** Compilation succeeded
- **Committed in:** e5e09ff

**2. [Rule 1 - Bug] Fixed debouncer mutability for watcher() call**
- **Found during:** Task 1
- **Issue:** `debouncer.watcher()` requires `&mut self` but debouncer was not declared `mut`
- **Fix:** Added `mut` to debouncer declaration
- **Files modified:** crates/blufio-memory/src/watcher.rs
- **Verification:** Compilation succeeded
- **Committed in:** e5e09ff

---

**Total deviations:** 2 auto-fixed (2 bug fixes)
**Impact on plan:** Both auto-fixes were minor compilation fixes. No scope creep.

## Issues Encountered
None beyond the auto-fixed items above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Phase 55 Memory Enhancements is now complete (all 4 plans executed)
- File watcher, background tasks, CLI, and metrics all wired and operational
- Ready for Phase 56 and beyond

---
*Phase: 55-memory-enhancements*
*Completed: 2026-03-11*
