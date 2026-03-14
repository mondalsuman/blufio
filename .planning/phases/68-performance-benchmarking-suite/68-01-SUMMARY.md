---
phase: 68-performance-benchmarking-suite
plan: 01
subsystem: benchmarking
tags: [jemalloc, cargo-bloat, binary-size, memory-profiling, rss, leak-detection]

# Dependency graph
requires:
  - phase: 65-sqlite-vec-integration
    provides: "vec0 module and tikv-jemalloc-ctl dependency with stats feature"
provides:
  - "BenchmarkKind::BinarySize variant measuring binary file size with cargo-bloat support"
  - "BenchmarkKind::MemoryProfile variant reporting jemalloc idle stats with leak detection framework"
  - "sample_rss(), check_leak(), print_rss_summary() helper functions for RSS monitoring"
affects: [68-04-ci-regression-detection, 68-03-openclaw-comparison]

# Tech tracking
tech-stack:
  added: []
  patterns: ["Non-timing BenchmarkKind variants handled separately from iteration-based benchmarks"]

key-files:
  created: []
  modified:
    - "crates/blufio/src/bench.rs"

key-decisions:
  - "BinarySize and MemoryProfile bypass the iteration-based run_benchmark() loop -- dispatched directly in run_bench()"
  - "Binary size stored in peak_rss field (repurposed) of BenchmarkResult since no timing is involved"
  - "jemalloc stats::*::read() returns usize, cast to u64 for consistency with format_bytes()"
  - "Leak detection threshold: monotonic growth AND >10% of initial RSS"

patterns-established:
  - "Non-timing benchmarks: handle in run_bench() dispatch, not run_single_benchmark()"
  - "RSS sampling pattern: epoch::advance() + stats::resident::read() per sample"

requirements-completed: [PERF-01, PERF-02]

# Metrics
duration: 4min
completed: 2026-03-14
---

# Phase 68 Plan 01: Binary Size & Memory Profile Benchmarks Summary

**BinarySize and MemoryProfile BenchmarkKind variants with jemalloc idle stats, cargo-bloat per-crate breakdown, and RSS leak detection framework**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-14T12:08:48Z
- **Completed:** 2026-03-14T12:12:48Z
- **Tasks:** 2
- **Files modified:** 1

## Accomplishments
- BenchmarkKind::BinarySize reports own binary size via current_exe(), compares against <50MB target, attempts cargo-bloat per-crate breakdown with graceful fallback
- BenchmarkKind::MemoryProfile reports jemalloc idle stats (allocated, active, resident, mapped) with target comparison (50-80MB) and OpenClaw baseline (300-800MB)
- RSS leak detection framework: sample_rss(), check_leak() (monotonic growth >10%), print_rss_summary() helpers ready for Plan 04 CI integration

## Task Commits

Each task was committed atomically:

1. **Task 1: Add BenchmarkKind::BinarySize variant** - `9fac9a4` (feat)
2. **Task 2: Add BenchmarkKind::MemoryProfile variant** - `79a3fcf` (feat)

## Files Created/Modified
- `crates/blufio/src/bench.rs` - Extended with BinarySize and MemoryProfile enum variants, bench_binary_size() and bench_memory_profile() implementations, sample_rss/check_leak/print_rss_summary helpers, updated Display/FromStr/dispatch

## Decisions Made
- BinarySize and MemoryProfile bypass the iteration-based run_benchmark() loop since they are not timing benchmarks -- dispatched directly in run_bench() with dedicated handler functions
- Binary size stored in peak_rss field of BenchmarkResult (repurposed for non-timing benchmarks)
- jemalloc stats return usize; cast to u64 at the API boundary for format_bytes() compatibility
- Leak detection uses dual criteria: monotonic growth AND >10% total growth to avoid false positives from minor jitter

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] jemalloc stats usize to u64 type mismatch**
- **Found during:** Task 2 (MemoryProfile implementation)
- **Issue:** tikv_jemalloc_ctl stats::resident::read() returns usize, but sample_rss() and format_bytes() expect u64
- **Fix:** Added `as u64` cast in sample_rss() return value
- **Files modified:** crates/blufio/src/bench.rs
- **Verification:** cargo build -p blufio succeeds
- **Committed in:** 79a3fcf (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Type cast necessary for compilation. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- BinarySize and MemoryProfile variants ready for `blufio bench --only binary_size` and `blufio bench --only memory_profile` CLI usage
- RSS sampling framework (sample_rss, check_leak, print_rss_summary) ready for Plan 04 CI integration with full workload
- Under-load measurement (1000 saves + 100 retrievals) deferred to application-context invocation

## Self-Check: PASSED

- FOUND: crates/blufio/src/bench.rs
- FOUND: 9fac9a4 (Task 1 commit)
- FOUND: 79a3fcf (Task 2 commit)
- FOUND: 68-01-SUMMARY.md

---
*Phase: 68-performance-benchmarking-suite*
*Completed: 2026-03-14*
