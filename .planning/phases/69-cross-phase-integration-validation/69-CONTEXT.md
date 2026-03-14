# Phase 69: Cross-Phase Integration Validation - Context

**Gathered:** 2026-03-14
**Status:** Ready for planning

<domain>
## Phase Boundary

Verify all v1.6 subsystems (vec0, injection defense, benchmarks) work together in production configuration. Add full async E2E benchmark with ONNX embedding, write cross-subsystem integration tests, proactively scan for wiring gaps and fix them inline, produce milestone-level verification report with traceability matrix, and update project documentation (PROJECT.md stats, REQUIREMENTS.md status). This is the final phase of v1.6.

</domain>

<decisions>
## Implementation Decisions

### E2E Pipeline Benchmark
- Add full async E2E benchmark in bench_hybrid.rs (extend existing file, not new file)
- Include ONNX embedding step: embed query via ONNX -> vec0 KNN -> BM25 -> RRF -> MMR
- Entry counts: 100 and 1K (matches parity test scales, avoids CI timeout with larger scales)
- Synthetic data with topic diversity (make_embedding(seed) + MEMORY_TOPICS) for reproducibility
- Add combined vec0+injection benchmark: retrieve memories via vec0 hybrid pipeline, then run injection scan on retrieved content
- Both integration test (e2e_integration.rs for correctness) AND criterion benchmark (bench_hybrid.rs for latency)
- Full injection pipeline on retrieved content: normalize -> decode -> extract -> scan (matches production code path)
- Include attack flow scenario: store memory with injection payload, retrieve via vec0, verify injection scanner detects it (detection assertion only, no classification enforcement)
- Separate ONNX model load time from per-query latency using Criterion iter_batched (warmup vs query split)
- Graceful skip if ONNX model not found: print skip message, don't fail (CI caches model, local devs may not have it)
- Smoke test: bench CLI commands (blufio bench --only binary_size/memory_profile) work with vec0_enabled=true
- TOML config integration test: load complete v1.6 config (vec0 + injection weights + benchmark settings), validate all subsystems initialize without conflict

### Verification Format
- Milestone-level VERIFICATION.md in .planning/phases/69-*/69-VERIFICATION.md (Phase 69's verification IS the milestone verification)
- Full cargo test --workspace pass required as evidence (record pass count + any failures)
- Clippy --workspace + cargo doc --workspace --no-deps: zero warnings as evidence
- Full requirement traceability matrix: all 23 v1.6 requirements mapped to phase, plan, test name, pass/fail
- Include actual benchmark results from running blufio bench (binary size, RSS, KNN latency, injection throughput)
- Human verification items section: list items requiring human review (OpenClaw comparison fairness, Swagger UI rendering, etc.)
- Tech debt audit: review carry-forward items from STATE.md, note which v1.6 resolved, document remaining
- Update PROJECT.md: run tokei for current LOC count, verify crate count, update requirements total (357 + 23 = 380)
- Mark v1.6 validated: move v1.6 requirements to 'Validated' section in PROJECT.md, add to shipped milestones list

### Regression Scope
- Full cargo test --workspace pass (all 37 crates)
- Four targeted cross-subsystem regression tests in new e2e_integration.rs:
  1. GDPR erasure + vec0 sync: soft_delete() and batch_evict() on store with vec0 enabled, verify vec0 counts match
  2. Hot reload + injection config: concurrent test -- spawn vec0 search while triggering config reload, verify both complete correctly
  3. Doctor checks + all v1.6 subsystems: unit test per health check function (vec0 sync drift, canary self-test, bench table)
  4. Compaction + vec0 consistency: compact a memory (status -> 'superseded'), verify vec0 metadata updated, verify KNN excludes superseded memories
- EventBus event flow test: subscribe to v1.6 events (Vec0PopulationComplete, SecurityEvent), trigger subsystems, verify events received
- Prometheus metric name validation: grep all metric declarations across v1.6 crates, verify unique names
- Feature gate check: cargo build with default features AND no-default-features (two builds)
- cargo deny check: verify dependency licensing and advisory compliance
- blufio.example.toml validation: verify all v1.6 config sections present with correct field names
- Injection corpus validation: covered by existing cargo test (corpus_validation.rs in test suite)

### Wiring Gap Discovery
- Proactive scan of all v1.6 integration points, fix gaps inline if small (wiring-only)
- Integration points to check:
  1. Retention + vec0: does permanent delete remove from vec0?
  2. Cron scheduler + vec0: does scheduled batch_evict sync to vec0?
  3. Audit trail + v1.6 events: are v1.6 events logged in audit trail? Hash chain integrity preserved?
  4. Node system + vec0: verify assumption that vec0 is node-local (each node independent). Document finding only
  5. MCP server + vec0: verify MCP memory search goes through HybridRetriever (vec0-enabled path)
  6. Gateway API + vec0: code path trace from /v1/chat/completions -> context assembly -> memory retrieval -> HybridRetriever. Verify vec0 path used
  7. Backup/restore + vec0: verify database file copy includes vec0 shadow table data. Critical for data safety
  8. OpenAPI spec: static check that /openapi.json is valid, no new undocumented routes from v1.6
  9. Docker: static review of Dockerfile/docker-compose.yml for v1.6 compatibility
  10. systemd: static review of service files for v1.6 compatibility (no new required env vars)
  11. CLI help text: run --help on all v1.6 commands, verify consistent and documented

### Claude's Discretion
- Exact e2e_integration.rs test structure and infrastructure reuse from e2e_vec0.rs
- VERIFICATION.md format and section ordering
- How to implement concurrent hot reload + vec0 search test (tokio::spawn vs join!)
- Whether to trace gateway API code path via static analysis or LSP
- How to measure ONNX model load time separately (iter_batched setup vs separate bench group)
- Tech debt audit depth (which carry-forward items to investigate)
- Wiring gap fix approach for any gaps discovered

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `e2e_vec0.rs`: 12 integration tests with setup_test_db(), comprehensive test infrastructure for vec0 + memory store testing
- `bench_hybrid.rs`: Criterion benchmarks with reciprocal_rank_fusion, make_embedding(), MEMORY_TOPICS -- extend for ONNX E2E
- `bench_vec0.rs`: Vec0 KNN benchmarks at 100/1K/5K/10K with setup_bench_db() -- established benchmark patterns
- `bench_injection.rs`: Injection throughput benchmarks at 1KB/5KB/10KB -- established patterns for InjectionClassifier
- `corpus_validation.rs`: 125 benign + 67 attack messages -- already validates FP/detection rates in cargo test
- `68-VERIFICATION.md`, `67-VERIFICATION.md`, `66-VERIFICATION.md`, `65-VERIFICATION.md`: Phase verification reports to aggregate

### Established Patterns
- tokio-rusqlite single-writer for DB operations
- In-memory SQLite for integration tests (setup_test_db pattern)
- Criterion iter_batched for setup/teardown separation
- ONNX model graceful skip when not available
- sample_size(10) for large-scale benchmarks to manage CI time
- VERIFICATION.md with Observable Truths tables, Artifact tables, Key Link tables, Requirements Coverage

### Integration Points
- `bench_hybrid.rs`: Add ONNX E2E benchmark group and vec0+injection combined benchmark
- `e2e_integration.rs` (new): Cross-subsystem integration tests
- `69-VERIFICATION.md`: Milestone verification report
- `PROJECT.md`: LOC count, crate count, requirements total, shipped milestones, v1.6 validated requirements
- `REQUIREMENTS.md`: v1.6 status updates
- `STATE.md`: Session tracking, milestone completion

</code_context>

<specifics>
## Specific Ideas

- Phase 69's VERIFICATION.md serves double duty: it's both the phase verification and the v1.6 milestone sign-off document
- The proactive wiring gap scan should be the FIRST step -- discovering gaps early allows integration tests to cover them
- The full ONNX E2E benchmark validates the actual user-facing latency claim, not just internal pipeline performance
- The combined vec0+injection benchmark proves subsystems compose without interference, which is the core integration validation
- Benchmark results in VERIFICATION.md capture a point-in-time snapshot as milestone evidence
- Tech debt audit prevents known issues from being silently forgotten across milestones

</specifics>

<deferred>
## Deferred Ideas

None -- discussion stayed within phase scope

</deferred>

---

*Phase: 69-cross-phase-integration-validation*
*Context gathered: 2026-03-14*
