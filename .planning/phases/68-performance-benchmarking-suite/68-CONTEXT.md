# Phase 68: Performance Benchmarking Suite - Context

**Gathered:** 2026-03-14
**Status:** Ready for planning

<domain>
## Phase Boundary

Quantified performance baselines validating binary size, memory usage, retrieval latency, injection throughput, and token efficiency claims. CI regression detection prevents performance degradation. Comparative benchmark against OpenClaw validates competitive positioning. Requirements: PERF-01 through PERF-07.

</domain>

<decisions>
## Implementation Decisions

### Binary size tracking
- cargo-bloat for per-crate size breakdown, installed in CI via `cargo install cargo-bloat`
- CI gate in bench.yml: warn at 50MB, fail at 55MB (release binary)
- Verify LTO=thin in Cargo.toml [profile.release] and verify strip=true for release builds
- Feature-flag CI comparison: build with and without sqlite-vec feature each run, report the delta
- Track both debug and release binary sizes
- New BenchmarkKind::BinarySize in blufio bench CLI that reads binary's own file size via std::env::current_exe()
- Per-crate breakdown: report-only (not stored in bench_results), shown in CI logs and CLI output
- Binary size job added to existing bench.yml workflow (not a separate workflow)
- sqlite-vec delta documented as one-time finding AND tracked via feature-flag comparison in CI

### Memory RSS profiling
- jemalloc global stats (allocated, active, resident, mapped) via tikv-jemalloc stats API
- getrusage peak RSS via existing get_peak_rss() function
- New BenchmarkKind::MemoryProfile in blufio bench CLI
- "Under load" defined as: single session with 1000 memory saves + 100 hybrid retrievals (vec0 + BM25 + RRF + MMR)
- ONNX model loading included in memory measurements (idle measurement includes loaded model)
- RSS sampling every 100 operations during sustained workload for leak detection
- If RSS grows monotonically without plateau, flag as potential memory leak
- Memory targets (50-80MB idle, 100-200MB loaded) are informational only, not CI-enforced
- jemalloc stats: global totals only, no per-arena breakdown (PERF-F02 is deferred)
- OpenClaw's documented 300-800MB range shown side-by-side in bench output
- vec0 vs in-memory memory comparison: back-to-back runs with same 1000-save workload, vec0_enabled=true vs false

### OpenClaw comparison
- Full feature matrix: memory RSS, token usage, startup time, binary size, dependency count, deployment size, security posture
- Hybrid methodology: run Blufio measurements ourselves, use OpenClaw's published metrics where available, document what was measured vs cited
- Both docs/benchmarks.md (persistent reference) and `blufio bench` CLI inline comparison
- Full methodology section documenting test environment, commands, data sources, reproducibility instructions
- Token efficiency: count actual prompt tokens using existing tiktoken/HuggingFace tokenizers for standard query
- Monthly cost comparison at 100/500/1000 turns/day for both platforms
- Heartbeat cost comparison: monthly cost at 5min/15min/30min intervals (Blufio Haiku skip-when-unchanged ~500 tokens vs OpenClaw ~35K tokens full context)
- npm dependency count comparison (Blufio <80 crates vs OpenClaw's npm dep tree)
- Deployment size comparison (single binary vs node_modules folder)
- Security posture: factual feature matrix (Feature | Blufio | OpenClaw), no value judgments
- Name OpenClaw directly with version cited, transparent and verifiable
- Latest stable OpenClaw version at time of writing, documented in benchmarks.md
- docs/benchmarks.md refreshed per milestone (v1.7, v1.8, etc.)
- Manually maintained document, not auto-generated in CI
- Startup comparison: time from process start to ready-to-handle-messages for both platforms

### CI regression detection
- New criterion benchmarks added to bench.yml:
  - vec0 KNN at 5K and 10K entries (extending existing 100/1K in bench_vec0.rs)
  - Injection classifier throughput at 1KB/5KB/10KB inputs (new bench_injection.rs) with detection rate verification
  - End-to-end hybrid retrieval pipeline including ONNX embedding generation (embed -> vec0 -> BM25 -> RRF -> decay -> importance -> MMR)
- ONNX model downloaded in CI from HuggingFace, cached with actions/cache
- Uniform 20% regression threshold for all benchmarks
- Benchmarks run on both PRs and main push
- PR benchmarks are informational only (don't block merge) -- CI runner variance makes hard gates unreliable
- PR comments via github-action-benchmark: markdown table with benchmark name, current median, baseline median, % change, emoji status indicators
- Main push benchmarks: hard failure on >20% regression (current behavior)
- 30-minute CI timeout for full benchmark suite
- 30-day artifact retention for criterion HTML reports
- Baseline management: cache key includes bench file hash (current approach) -- new baselines start fresh when benches change
- 20% threshold serves as the noise buffer -- no additional flaky handling needed (criterion's built-in statistical analysis handles variance)

### Claude's Discretion
- Exact jemalloc stats API calls and output formatting
- bench_injection.rs internal structure and test data generation
- How to integrate ONNX model download step into bench.yml
- github-action-benchmark configuration details
- PR comment styling beyond the table format
- Whether BinarySize and MemoryProfile kinds reuse existing bench.rs infrastructure or need separate modules
- Exact cargo-bloat CLI flags for per-crate breakdown
- How to structure the feature-flag comparison (separate CI job vs matrix strategy)

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `bench.rs` (src/bench.rs): Existing BenchmarkKind enum with run_bench(), get_peak_rss(), format_duration(), format_bytes(), collect_system_info(), save_results(), load_previous_results(), load_baseline_results()
- `bench_vec0.rs` (benches/): Criterion benchmarks for vec0 KNN vs in-memory at 100/1K entries with setup_bench_db() and make_embedding() helpers
- `bench_pii.rs` (benches/): Criterion benchmarks for PII detection/redaction at 1KB/5KB/10KB with text generators
- `bench_context.rs`, `bench_memory.rs`, `bench_compaction.rs` (benches/): Additional criterion benchmarks
- `bench.yml` (.github/workflows/): CI workflow with criterion regression detection (20% threshold, cache, artifact upload)
- `V11__bench_results.sql` (migrations): Existing bench_results SQLite table schema
- `BenchmarkResult` struct: name, median, min, max, peak_rss, iterations
- tikv-jemalloc-sys: Already the allocator, stats API available

### Established Patterns
- Criterion benchmarks in crates/blufio/benches/ with criterion_group!/criterion_main! macros
- BenchmarkKind enum with FromStr, Display, run dispatch
- System info collection via sysinfo crate
- Results stored in bench_results SQLite table with is_baseline flag
- CI regression check: grep criterion output for "regressed" lines, parse percentages
- In-memory SQLite for benchmark setup (no file-based DBs)
- Deterministic embeddings via make_embedding(seed) for reproducible benchmarks

### Integration Points
- bench.rs:run_bench() -- extend with new BenchmarkKind variants (BinarySize, MemoryProfile)
- bench.yml -- add binary size job, ONNX model download, PR trigger, github-action-benchmark
- bench_vec0.rs -- extend count range to include 5K and 10K entries
- blufio-injection crate -- expose screen_content() for benchmark invocation
- blufio-memory HybridRetriever -- full pipeline benchmark needs test harness

</code_context>

<specifics>
## Specific Ideas

- The OpenClaw comparison is the "kill shot" document -- should be professional, factual, and verifiable
- Heartbeat cost comparison directly validates the "$769/month on Opus" claim from PROJECT.md
- Memory leak detection (RSS over time) directly counters OpenClaw's "300-800MB in 24h" weakness
- vec0 vs in-memory memory comparison should show that disk-backed KNN uses LESS memory at scale
- Token efficiency comparison validates the "68-84% token reduction" claim from the context engine
- The full hybrid pipeline benchmark (with ONNX) is the most realistic user-facing latency measurement
- Feature-flag sqlite-vec comparison quantifies the exact binary size cost of disk-backed vector search

</specifics>

<deferred>
## Deferred Ideas

None -- discussion stayed within phase scope

</deferred>

---

*Phase: 68-performance-benchmarking-suite*
*Context gathered: 2026-03-14*
