---
phase: 68-performance-benchmarking-suite
plan: 02
subsystem: testing
tags: [criterion, benchmarks, vec0, injection, hybrid-retrieval, performance]

# Dependency graph
requires:
  - phase: 65-sqlite-vec-integration
    provides: vec0 virtual table, vec0_search, vec0_insert
  - phase: 66-injection-defense-expansion
    provides: InjectionClassifier with 38 patterns
  - phase: 67-vector-search-hybrid-pipeline
    provides: HybridRetriever, reciprocal_rank_fusion, score_from_vec0_data
provides:
  - Extended vec0 KNN benchmarks at 100/1K/5K/10K entries
  - Injection classifier throughput benchmarks at 1KB/5KB/10KB with attack/benign variants
  - Hybrid pipeline benchmarks (vec0 + BM25 + RRF) at 100/500/1K entries
affects: [68-04-ci-regression-detection, performance-monitoring]

# Tech tracking
tech-stack:
  added: []
  patterns: [criterion benchmark with size-parameterized groups, FTS5 BM25 in bench setup, reduced sample_size for expensive benchmarks]

key-files:
  created:
    - crates/blufio/benches/bench_injection.rs
    - crates/blufio/benches/bench_hybrid.rs
  modified:
    - crates/blufio/benches/bench_vec0.rs
    - crates/blufio/Cargo.toml

key-decisions:
  - "5K/10K vec0 benchmarks use sample_size(10) + 30s measurement time to avoid CI timeouts"
  - "Hybrid bench uses synchronous pipeline (vec0+BM25+RRF) without ONNX embedding -- full async pipeline deferred to when ONNX model CI caching is in place"
  - "bench_injection sanity check asserts detection after benchmark loop (not inside timed iteration)"

patterns-established:
  - "Reduced sample_size(10) for benchmarks with expensive setup or long iteration times"
  - "FTS5 content sync triggers replicated in bench setup for realistic BM25 search"

requirements-completed: [PERF-03, PERF-04, PERF-05]

# Metrics
duration: 12min
completed: 2026-03-14
---

# Phase 68 Plan 02: Criterion Benchmarks Summary

**Three criterion benchmark files: vec0 at 5K/10K entries, injection classifier at 1-10KB with attack/benign variants, hybrid retrieval pipeline with vec0+BM25+RRF fusion**

## Performance

- **Duration:** 12 min
- **Started:** 2026-03-14T12:09:15Z
- **Completed:** 2026-03-14T12:21:37Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- Extended bench_vec0.rs from [100, 1000] to [100, 1000, 5000, 10000] entries with reduced sample size for large counts
- Created bench_injection.rs benchmarking InjectionClassifier::classify() at 1KB/5KB/10KB with both attack payloads and benign text, plus post-benchmark detection sanity check
- Created bench_hybrid.rs benchmarking the full synchronous retrieval pipeline: vec0 KNN, BM25 FTS5 search, RRF fusion, and combined sync_pipeline at 100/500/1000 entries

## Task Commits

Each task was committed atomically:

1. **Task 1: Extend bench_vec0.rs to 5K/10K and create bench_injection.rs** - `1a3c57b` (feat)
2. **Task 2: Create bench_hybrid.rs for end-to-end hybrid retrieval pipeline** - `e155eff` (feat)

## Files Created/Modified
- `crates/blufio/benches/bench_vec0.rs` - Extended count array to [100, 1000, 5000, 10000] with sample_size(10) for >= 5000
- `crates/blufio/benches/bench_injection.rs` - New injection classifier throughput benchmark with attack/benign text generators
- `crates/blufio/benches/bench_hybrid.rs` - New hybrid pipeline benchmark with FTS5 setup, BM25 search, and RRF fusion
- `crates/blufio/Cargo.toml` - Added [[bench]] entries for bench_injection and bench_hybrid

## Decisions Made
- Used `sample_size(10)` + `measurement_time(30s)` for vec0 benchmarks >= 5000 entries to keep CI within timeout bounds
- Hybrid pipeline benchmark measures the synchronous hot path (vec0 KNN + BM25 + RRF fusion) without requiring ONNX model files, since full async HybridRetriever::retrieve() needs ONNX model + tokenizer on disk which Plan 04 (CI regression detection) will set up with model caching
- Post-benchmark sanity check in bench_injection.rs asserts that the classifier detects a known attack pattern, ensuring the benchmark isn't measuring a broken classifier
- bench_hybrid.rs benchmarks individual components (vec0_knn, bm25_search, rrf_fusion) AND the combined sync_pipeline for profiling which component dominates latency

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All criterion benchmarks registered and passing via `cargo bench -p blufio`
- bench_vec0 produces data at 100/1K/5K/10K for CI regression thresholds
- bench_injection produces data at 1KB/5KB/10KB for classifier throughput monitoring
- bench_hybrid produces data at 100/500/1K for pipeline latency monitoring
- Ready for Plan 04 (CI regression detection) to add these benchmarks to bench.yml with ONNX model caching

## Self-Check: PASSED

All files verified present, all commits found, all must_have content assertions confirmed.

---
*Phase: 68-performance-benchmarking-suite*
*Completed: 2026-03-14*
