---
phase: 63-code-quality-hardening
plan: 05
subsystem: testing
tags: [criterion, benchmarks, ci, performance, regression-detection]

# Dependency graph
requires:
  - phase: 53-pii-data-classification
    provides: PII detection engine (detect_pii, redact_pii, luhn_validate)
  - phase: 55-memory-system
    provides: Memory retrieval pipeline (RRF fusion, cosine similarity, MMR reranking)
  - phase: 56-compaction
    provides: Compaction quality scoring (weighted scores, gate evaluation)
provides:
  - Criterion benchmark suite for 4 core hot paths (PII, memory, context, compaction)
  - CI workflow for automated benchmark regression detection on main push
  - HTML benchmark reports as GitHub Actions artifacts
affects: [ci, performance, benchmarks]

# Tech tracking
tech-stack:
  added: [criterion 0.5]
  patterns: [criterion_group/criterion_main macros, deterministic benchmark data generation, workspace-level bench configuration]

key-files:
  created:
    - crates/blufio/benches/bench_pii.rs
    - crates/blufio/benches/bench_memory.rs
    - crates/blufio/benches/bench_context.rs
    - crates/blufio/benches/bench_compaction.rs
    - .github/workflows/bench.yml
  modified:
    - Cargo.toml
    - crates/blufio/Cargo.toml

key-decisions:
  - "Benchmarks placed in crates/blufio/benches/ (not workspace root) because workspace root has no [package] section"
  - "Benchmark CPU-bound hot paths only (no LLM calls, no DB I/O) for deterministic reproducible results"
  - "Regression detection via grep parsing of criterion output for >20% threshold"

patterns-established:
  - "Deterministic benchmark data generators using seeded LCG for reproducible embeddings"
  - "Criterion benchmark groups organized by subsystem (pii_detect_mixed, memory_rrf_fusion, etc.)"

requirements-completed: [QUAL-08]

# Metrics
duration: 11min
completed: 2026-03-13
---

# Phase 63 Plan 05: Criterion Benchmarks & CI Regression Detection Summary

**Criterion benchmarks for 4 core hot paths (PII detection, memory retrieval, context assembly, compaction scoring) with CI workflow detecting >20% regressions on main push**

## Performance

- **Duration:** 11 min
- **Started:** 2026-03-13T14:25:02Z
- **Completed:** 2026-03-13T14:36:02Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- 4 criterion benchmark files covering PII detection/redaction, memory retrieval (RRF, cosine similarity), context assembly (zone budget, token counting), and compaction quality scoring
- Each benchmark uses realistic input sizes (1KB, 5KB, 10KB for text; 50, 200, 500 entries for collections)
- CI workflow on main push with cached baselines, regression detection, and HTML artifact upload
- All benchmarks compile and link successfully

## Task Commits

Each task was committed atomically:

1. **Task 1: Criterion benchmarks for 4 core hot paths** - `26c67c0` (feat)
2. **Task 2: CI benchmark regression workflow** - `5410e55` (feat)

## Files Created/Modified
- `crates/blufio/benches/bench_pii.rs` - PII detection/redaction benchmarks (mixed PII + clean text, 1-10KB)
- `crates/blufio/benches/bench_memory.rs` - Memory retrieval benchmarks (RRF fusion, cosine similarity, batch)
- `crates/blufio/benches/bench_context.rs` - Context assembly benchmarks (zone budget, token counting, JSON blocks)
- `crates/blufio/benches/bench_compaction.rs` - Compaction quality scoring benchmarks (weighted score, gate, pipeline)
- `.github/workflows/bench.yml` - CI workflow for benchmark regression detection
- `Cargo.toml` - Added criterion workspace dev-dependency
- `crates/blufio/Cargo.toml` - Added criterion dev-dep and [[bench]] entries

## Decisions Made
- Placed benchmarks in `crates/blufio/benches/` rather than workspace root `benches/` because the workspace root Cargo.toml has no `[package]` section and therefore cannot host `[[bench]]` entries. The `blufio` crate already depends on all relevant sub-crates (blufio-security, blufio-context, blufio-memory).
- Benchmarked only CPU-bound hot paths (regex scanning, RRF fusion, cosine similarity, quality scoring) rather than full async assembly pipelines, ensuring deterministic and reproducible benchmark results without requiring LLM providers or database connections.
- Used grep-based parsing of criterion output for regression detection rather than external tooling (github-action-benchmark, critcmp) to minimize workflow dependencies.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Moved bench entries from workspace root to blufio crate**
- **Found during:** Task 1 (benchmark creation)
- **Issue:** Plan specified `benches/` at workspace root, but root Cargo.toml has no `[package]` section so `[[bench]]` entries cannot be placed there
- **Fix:** Created benchmarks in `crates/blufio/benches/` and added `[[bench]]` entries to `crates/blufio/Cargo.toml`
- **Files modified:** `crates/blufio/Cargo.toml`
- **Verification:** `cargo bench --no-run -p blufio` compiles all 4 benchmarks successfully
- **Committed in:** 26c67c0 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Benchmark location adapted to workspace structure. No functional impact -- all benchmarks work identically from the crate directory.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Benchmark suite ready for CI execution on main branch push
- Future phases can add new `[[bench]]` entries following the established pattern
- HTML reports will be available as GitHub Actions artifacts for performance investigation

## Self-Check: PASSED

All 6 created files verified present. Both task commits (26c67c0, 5410e55) verified in git log. All 4 benchmarks compile successfully with `cargo bench --no-run -p blufio`.

---
*Phase: 63-code-quality-hardening*
*Completed: 2026-03-13*
