# Phase 69: Cross-Phase Integration Validation - Research

**Researched:** 2026-03-14
**Domain:** Rust workspace integration testing, cross-subsystem validation, milestone verification
**Confidence:** HIGH

## Summary

Phase 69 is the final phase of v1.6, serving as both cross-subsystem integration validation and milestone sign-off. The work breaks into four distinct domains: (1) extending bench_hybrid.rs with a full async ONNX E2E benchmark and combined vec0+injection benchmark, (2) writing new e2e_integration.rs cross-subsystem tests, (3) proactively scanning and fixing wiring gaps discovered during research, and (4) producing a milestone-level VERIFICATION.md with full traceability matrix.

Research uncovered several concrete wiring gaps already. GDPR erasure performs hard DELETE on memories table without cleaning vec0. Cron memory cleanup performs soft-delete without syncing vec0 status. MCP server memory search uses BM25-only (search_bm25) rather than HybridRetriever, so vec0 is bypassed. blufio.example.toml is missing `[memory]` with vec0_enabled and has no benchmark-related config. These are real gaps that need inline fixes during integration testing.

The existing test infrastructure is mature and well-patterned. e2e_vec0.rs provides reusable setup_test_db(), synthetic_embedding(), make_test_memory(), and vec0_count_async() helpers. bench_hybrid.rs provides make_embedding(), MEMORY_TOPICS, and setup_hybrid_bench_db(). Criterion patterns (sample_size(10), iter_batched) are established. The workspace has ~2622 tests across 37 crates, all compiling cleanly.

**Primary recommendation:** Start with wiring gap discovery/fixes (smallest scope, highest risk), then write e2e_integration.rs tests, extend bench_hybrid.rs, and finally produce the VERIFICATION.md with full evidence.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- E2E benchmark in bench_hybrid.rs (extend existing file, not new file)
- Entry counts: 100 and 1K
- Synthetic data with topic diversity (make_embedding(seed) + MEMORY_TOPICS)
- Combined vec0+injection benchmark with full injection pipeline on retrieved content
- Attack flow scenario: store memory with injection payload, retrieve via vec0, verify injection scanner detects
- Separate ONNX model load time from per-query latency using iter_batched
- Graceful skip if ONNX model not found
- Smoke test: bench CLI commands work with vec0_enabled=true
- TOML config integration test: load complete v1.6 config, validate all subsystems initialize
- Milestone-level VERIFICATION.md at 69-VERIFICATION.md
- Full cargo test --workspace pass as evidence
- Clippy --workspace + cargo doc zero warnings
- Full 23-requirement traceability matrix
- Actual benchmark results from running blufio bench
- Human verification items section
- Tech debt audit from STATE.md carry-forwards
- Update PROJECT.md: tokei LOC, crate count, requirements total (357+23=380), v1.6 validated
- Full cargo test --workspace pass (all 37 crates)
- Four targeted cross-subsystem regression tests in new e2e_integration.rs
- EventBus event flow test for v1.6 events
- Prometheus metric name uniqueness validation
- Feature gate check: default features AND no-default-features
- cargo deny check
- blufio.example.toml validation
- Proactive wiring gap scan with inline fixes

### Claude's Discretion
- Exact e2e_integration.rs test structure and infrastructure reuse from e2e_vec0.rs
- VERIFICATION.md format and section ordering
- How to implement concurrent hot reload + vec0 search test (tokio::spawn vs join!)
- Whether to trace gateway API code path via static analysis or LSP
- How to measure ONNX model load time separately (iter_batched setup vs separate bench group)
- Tech debt audit depth
- Wiring gap fix approach

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| VEC-05 | Hybrid retrieval (BM25 + vec0 KNN + RRF fusion + temporal decay + importance boost + MMR diversity) preserved and functionally identical | E2E benchmark validates full pipeline; e2e_integration.rs tests verify cross-subsystem behavior |
| PERF-05 | End-to-end hybrid retrieval benchmark measures full pipeline (embed -> vec0 -> BM25 -> RRF -> MMR) | bench_hybrid.rs extension with ONNX embedding, iter_batched for model load separation |
| PERF-06 | Comparative benchmark vs OpenClaw validates memory usage and token efficiency claims with reproducible numbers | VERIFICATION.md captures blufio bench output; docs/benchmarks.md already exists from Phase 68 |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| criterion | workspace | Benchmark harness for Rust | Industry standard, already used in bench_vec0/injection/hybrid |
| tokio | workspace (full) | Async runtime | Project standard, all async tests use #[tokio::test] |
| rusqlite | workspace | Synchronous SQLite for benchmarks | bench_hybrid.rs pattern: synchronous conn for criterion |
| tokio-rusqlite | workspace | Async SQLite for integration tests | e2e_vec0.rs pattern: Connection::open_in_memory().await |
| blufio-injection | workspace | Injection classifier + pipeline | Combined vec0+injection benchmark |
| blufio-memory | workspace | MemoryStore, vec0 module, HybridRetriever | Core integration target |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| chrono | workspace | Timestamp generation for test data | Parity test patterns use chrono::Utc::now() |
| serde_json | 1 | Config validation tests | TOML config load -> verify fields |
| toml | workspace | Config deserialization | TOML config integration test |
| blufio-config | workspace | BlufioConfig struct | Config integration test |
| blufio-bus | workspace | EventBus + BusEvent types | Event flow tests |

## Architecture Patterns

### Recommended Project Structure
```
crates/blufio/
  benches/
    bench_hybrid.rs       # EXTEND: add ONNX E2E + combined vec0+injection groups
  tests/
    e2e_integration.rs    # NEW: cross-subsystem integration tests
.planning/phases/69-cross-phase-integration-validation/
    69-VERIFICATION.md    # NEW: milestone verification report
```

### Pattern 1: Criterion iter_batched for ONNX Model Separation
**What:** Use iter_batched to separate ONNX model loading (setup) from per-query latency (measured)
**When to use:** ONNX E2E benchmark where model load is one-time cost
**Example:**
```rust
// Source: criterion docs + established project pattern
group.bench_function("onnx_e2e_pipeline/100_entries", |b| {
    b.iter_batched(
        || {
            // Setup: load ONNX model + prepare DB (NOT measured)
            let embedder = OnnxEmbedder::new(&model_path).unwrap();
            let (conn, _, query_text) = setup_hybrid_bench_db(100);
            (embedder, conn, query_text)
        },
        |(embedder, conn, query_text)| {
            // Measured: embed query + vec0 KNN + BM25 + RRF
            let query_emb = embedder.embed_sync(&query_text);
            let vec0_res = vec0::vec0_search(&conn, &query_emb, 10, 0.3, None).unwrap();
            let bm25_res = bm25_search(&conn, &query_text, 10);
            let fused = reciprocal_rank_fusion(&vec0_res_pairs, &bm25_res);
            black_box(fused)
        },
        BatchSize::SmallInput,
    );
});
```

### Pattern 2: Reuse e2e_vec0.rs Test Infrastructure
**What:** Import shared helpers from e2e_vec0.rs patterns for e2e_integration.rs
**When to use:** All cross-subsystem integration tests that need DB setup
**Example:**
```rust
// Source: e2e_vec0.rs established patterns
async fn setup_test_db() -> Connection {
    vec0::ensure_sqlite_vec_registered();
    let conn = Connection::open_in_memory().await.unwrap();
    conn.call(|conn| -> Result<(), rusqlite::Error> {
        // Same schema as e2e_vec0.rs: sessions, memories, FTS5 triggers, vec0
        conn.execute_batch("...");
        Ok(())
    }).await.unwrap();
    conn
}
```

### Pattern 3: Graceful ONNX Skip
**What:** Skip benchmark when ONNX model files not found on disk
**When to use:** Any benchmark that depends on model.onnx + tokenizer.json
**Example:**
```rust
// Source: established project pattern (Phase 68 decision)
let model_path = dirs::data_dir()
    .unwrap_or_default()
    .join("blufio/models/all-MiniLM-L6-v2-quantized/model.onnx");
if !model_path.exists() {
    eprintln!("Skipping ONNX E2E benchmark: model not found at {}", model_path.display());
    return;
}
```

### Pattern 4: EventBus Test via Broadcast Subscribe
**What:** Subscribe to EventBus, trigger subsystem, assert events received
**When to use:** EventBus event flow validation tests
**Example:**
```rust
// Source: blufio-bus/src/lib.rs EventBus::subscribe()
let bus = Arc::new(EventBus::new());
let mut rx = bus.subscribe();
// Trigger subsystem that emits events...
// Then tokio::time::timeout to receive and match:
match tokio::time::timeout(Duration::from_secs(1), rx.recv()).await {
    Ok(Ok(event)) => match event {
        BusEvent::Memory(MemoryEvent::Vec0PopulationComplete { .. }) => { /* pass */ }
        _ => panic!("unexpected event"),
    },
    _ => panic!("no event received"),
}
```

### Anti-Patterns to Avoid
- **File-based DBs in integration tests:** Use in-memory SQLite (`:memory:`) per established pattern. File DBs create cleanup issues and parallelism conflicts.
- **Testing ONNX model in unit tests:** ONNX model is a runtime dependency. Use graceful skip, never require model files for cargo test.
- **Blocking assertions on EventBus:** Always use tokio::time::timeout -- EventBus is broadcast, events may be dropped if no subscriber is ready.
- **Duplicating test infrastructure:** e2e_integration.rs should define its own setup_test_db() by copy (not import, since test files are not library code in Rust's module system).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| ONNX embedding in benchmark | Custom model loading | OnnxEmbedder::new() from blufio-memory | Handles tokenizer, session builder, mean pooling |
| Criterion benchmark setup | Manual timing | iter_batched with BatchSize::SmallInput | Separates setup from measurement automatically |
| Config deserialization | Manual TOML parsing | BlufioConfig serde with figment | Validates deny_unknown_fields, defaults, all sections |
| vec0 operations | Raw SQL vec0 INSERT | vec0::vec0_insert(), vec0::vec0_search() | Handles rowid, embedding format, metadata columns |
| BM25 search in tests | Raw FTS5 SQL | store.search_bm25() or direct SQL from bench_hybrid.rs | Established patterns with correct JOIN + filtering |
| Metric name uniqueness | Manual string comparison | grep/Grep tool scan across workspace | Regex pattern `(counter|histogram|gauge)!.*"blufio_` captures all declarations |

## Common Pitfalls

### Pitfall 1: ONNX Model Not Available in CI
**What goes wrong:** Benchmark fails because model.onnx is not downloaded
**Why it happens:** CI caches model file but cache may miss; local dev environments usually lack the model
**How to avoid:** Always use graceful skip pattern (check model_path.exists() before creating OnnxEmbedder). Print eprintln skip message, not panic.
**Warning signs:** "No such file or directory" errors in criterion output

### Pitfall 2: Vec0 Table Not Created Before Tests
**What goes wrong:** vec0_search returns "no such table: memories_vec0"
**Why it happens:** setup_test_db() missing the CREATE VIRTUAL TABLE statement
**How to avoid:** Always include the full schema from e2e_vec0.rs setup_test_db() including the vec0 DDL
**Warning signs:** "no such table" rusqlite errors

### Pitfall 3: Synchronous vs Async Mismatch in Benchmarks
**What goes wrong:** Cannot use async code (OnnxEmbedder.embed()) inside Criterion's synchronous benchmark closure
**Why it happens:** Criterion benches run synchronously; the HybridRetriever.retrieve() is async
**How to avoid:** For the ONNX E2E benchmark, either (a) use tokio::runtime::Runtime::block_on inside iter_batched, or (b) call the embedding synchronously via the ONNX session directly (avoiding the async EmbeddingAdapter wrapper). Pattern (b) is cleaner for benchmarking.
**Warning signs:** "Cannot start a runtime from within a runtime" panic

### Pitfall 4: FTS5 MATCH Requires Non-Empty Tokens
**What goes wrong:** BM25 search panics or returns error with certain query strings
**Why it happens:** FTS5 MATCH requires at least one valid token; empty or all-punctuation queries fail
**How to avoid:** Use known-good query strings from MEMORY_TOPICS in benchmarks
**Warning signs:** "fts5: syntax error" rusqlite errors

### Pitfall 5: cargo test --workspace May Timeout
**What goes wrong:** Full workspace test run takes >10 minutes
**Why it happens:** 2622 tests across 37 crates, some with DB setup and crypto operations
**How to avoid:** Run without --release flag (debug mode is fine for correctness). For CI, set a generous timeout (30+ minutes).
**Warning signs:** CI timeout with partial test output

### Pitfall 6: GDPR Erasure Does Not Sync Vec0
**What goes wrong:** After GDPR erasure, vec0 table still contains rows for deleted memories. KNN search returns ghost entries.
**Why it happens:** erasure.rs line 94 does `DELETE FROM memories` but never touches memories_vec0. The FTS5 triggers fire automatically (content-sync mode), but vec0 has no equivalent auto-sync.
**How to avoid:** The wiring gap fix must add `DELETE FROM memories_vec0 WHERE rowid IN (SELECT rowid FROM memories WHERE session_id IN (...))` before the main memories DELETE, since the rowid reference will be lost after DELETE.
**Warning signs:** vec0_count returns non-zero after full GDPR erasure

### Pitfall 7: Cron Memory Cleanup Bypasses Vec0
**What goes wrong:** Cron soft-deletes memories but vec0 status remains 'active'. Deleted memories continue appearing in KNN search.
**Why it happens:** memory_cleanup.rs line 64 does `UPDATE memories SET deleted_at = ...` but never calls vec0 UPDATE. It uses raw SQL on Connection, not MemoryStore.soft_delete() which has vec0 sync.
**How to avoid:** Either (a) change cron task to use MemoryStore.batch_evict() (preferred, has vec0 sync), or (b) add inline vec0 status UPDATE in the cron task SQL.
**Warning signs:** vec0 count mismatch in doctor check after cron cleanup

## Code Examples

### E2E ONNX Benchmark (bench_hybrid.rs extension)
```rust
// Source: established patterns in bench_hybrid.rs + embedder.rs
fn bench_onnx_e2e_pipeline(c: &mut Criterion) {
    // Graceful skip if ONNX model not found
    let model_path = dirs::data_dir()
        .unwrap_or_default()
        .join("blufio/models/all-MiniLM-L6-v2-quantized/model.onnx");
    if !model_path.exists() {
        eprintln!("Skipping ONNX E2E benchmark: model not found at {}", model_path.display());
        return;
    }

    let mut group = c.benchmark_group("onnx_e2e_pipeline");
    group.sample_size(10);

    for count in [100, 1000] {
        let (conn, _, query_text) = setup_hybrid_bench_db(count);

        group.bench_with_input(
            BenchmarkId::new("full_pipeline", format!("{count}_entries")),
            &(&conn, &query_text, &model_path),
            |b, &(conn, query_text, model_path)| {
                b.iter_batched(
                    || {
                        // Setup: load ONNX model (NOT measured)
                        blufio_memory::embedder::OnnxEmbedder::new(model_path).unwrap()
                    },
                    |embedder| {
                        // Measured: embed + vec0 + BM25 + RRF
                        // (synchronous path to avoid async runtime issues in criterion)
                        // ... embed query_text, search, fuse, black_box result
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }
    group.finish();
}
```

### Combined Vec0+Injection Benchmark (bench_hybrid.rs extension)
```rust
// Source: bench_injection.rs + bench_hybrid.rs patterns
fn bench_vec0_injection_combined(c: &mut Criterion) {
    let mut group = c.benchmark_group("vec0_injection_combined");
    group.sample_size(10);

    let config = blufio_injection::config::InjectionDefenseConfig::default();
    let classifier = blufio_injection::classifier::InjectionClassifier::new(&config);

    for count in [100, 1000] {
        let (conn, query_emb, query_text) = setup_hybrid_bench_db(count);

        group.bench_with_input(
            BenchmarkId::new("retrieve_then_scan", format!("{count}_entries")),
            &(&conn, &query_emb, &query_text),
            |b, &(conn, query_emb, query_text)| {
                b.iter(|| {
                    // Step 1: vec0 KNN retrieval
                    let results = vec0::vec0_search(conn, query_emb, 10, 0.3, None).unwrap();
                    // Step 2: Run injection scanner on each retrieved memory content
                    for result in &results {
                        let scan = classifier.classify(&result.content, "user");
                        black_box(scan);
                    }
                    black_box(results)
                });
            },
        );
    }
    group.finish();
}
```

### Cross-Subsystem Integration Test (e2e_integration.rs)
```rust
// Source: e2e_vec0.rs patterns
#[tokio::test]
async fn test_gdpr_erasure_with_vec0_sync() {
    let conn = setup_test_db().await;
    let store = blufio_memory::MemoryStore::with_vec0(conn, None, true);

    // Save memories with session_id
    for i in 0..5u64 {
        let mut mem = make_test_memory(&format!("mem-gdpr-{i}"), "GDPR test", i + 100);
        mem.session_id = Some("session-to-erase".to_string());
        store.save(&mem).await.unwrap();
    }

    // Verify vec0 has rows
    let initial_count = vec0_count_async(store.conn()).await;
    assert_eq!(initial_count, 5);

    // Perform soft_delete (simulating GDPR erasure)
    for i in 0..5 {
        store.soft_delete(&format!("mem-gdpr-{i}")).await.unwrap();
    }

    // Verify vec0 status updated (status = 'forgotten')
    // KNN search with status='active' filter should return 0 results
    let results = store.conn().call(|conn| {
        vec0::vec0_search(conn, &synthetic_embedding(105), 10, 0.0, None)
    }).await.unwrap();
    assert_eq!(results.len(), 0, "vec0 should return 0 after GDPR soft-delete");
}
```

## Wiring Gaps Discovered

Research identified these concrete wiring gaps requiring inline fixes:

### Gap 1: GDPR Erasure Misses Vec0 (HIGH priority)
**Location:** `crates/blufio-gdpr/src/erasure.rs` line 90-98
**Issue:** `DELETE FROM memories WHERE session_id IN (...)` does not delete from memories_vec0
**Impact:** After GDPR erasure, ghost entries remain in vec0 KNN search
**Fix approach:** Add `DELETE FROM memories_vec0 WHERE rowid IN (SELECT rowid FROM memories WHERE session_id IN (...))` BEFORE the memories DELETE (rowid must exist for the subquery)
**Confidence:** HIGH -- verified by reading erasure.rs source

### Gap 2: Cron Memory Cleanup Bypasses Vec0 Sync (HIGH priority)
**Location:** `crates/blufio-cron/src/tasks/memory_cleanup.rs` line 64
**Issue:** Uses raw `UPDATE memories SET deleted_at = ...` instead of MemoryStore.soft_delete() which has vec0 sync
**Impact:** Cron-deleted memories stay active in vec0, doctor check shows sync drift
**Fix approach:** Either add vec0 status UPDATE alongside the memories UPDATE, or restructure to call MemoryStore methods. The inline SQL approach is simpler and matches the existing pattern.
**Confidence:** HIGH -- verified by reading memory_cleanup.rs source

### Gap 3: MCP Server Uses BM25-Only Search (MEDIUM priority)
**Location:** `crates/blufio-mcp-server/src/resources.rs` line 135
**Issue:** `read_memory_search()` calls `store.search_bm25()` directly, not HybridRetriever
**Impact:** MCP memory search does not benefit from vec0 KNN similarity; only BM25 keyword matching
**Fix approach:** Document as a known limitation. MCP search is keyword-based by design (query parameter is text, not embedding). The HybridRetriever requires an OnnxEmbedder, which MCP resource reads don't have access to. This is an architectural choice, not a bug.
**Confidence:** HIGH -- verified, but this is arguably by design

### Gap 4: blufio.example.toml Missing V1.6 Config Sections (LOW priority)
**Location:** `contrib/blufio.example.toml`
**Issue:** Missing `[memory]` section with `vec0_enabled = true` and no benchmark config documentation
**Impact:** Operators cannot discover vec0 toggle or benchmark configuration from the example file
**Fix approach:** Add commented `[memory]` section showing vec0_enabled and related fields
**Confidence:** HIGH -- verified by reading example TOML

### Gap 5: Gateway API Code Path (Documentation only)
**Location:** Code path trace needed: /v1/chat/completions -> context assembly -> HybridRetriever
**Issue:** Need to verify the gateway API correctly uses HybridRetriever (vec0-enabled) for memory retrieval
**Impact:** If gateway uses in-memory search, production users miss vec0 benefits
**Fix approach:** Static code path trace during implementation. blufio-agent/src/session.rs line 433 sets current query for retrieval; context assembly goes through MemoryProvider which uses HybridRetriever. This appears correctly wired but needs explicit verification.
**Confidence:** MEDIUM -- partial trace done, full trace needed during implementation

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| In-memory brute-force cosine | sqlite-vec vec0 disk-backed KNN | v1.6 Phase 65 | Scalable to 10K+ entries |
| 11 injection patterns | 38 patterns across 8 categories | v1.6 Phase 66 | Multi-language coverage |
| No benchmarks | Criterion + CLI bench suite | v1.6 Phase 68 | CI regression detection |
| Sync-only hybrid bench | Full async ONNX E2E benchmark | v1.6 Phase 69 (this phase) | Validates actual user-facing latency |

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in) + criterion 0.5 |
| Config file | Cargo.toml [[bench]] entries |
| Quick run command | `cargo test --workspace --lib --no-fail-fast 2>&1 \| tail -5` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| VEC-05 | Hybrid retrieval preserved with vec0 | integration | `cargo test -p blufio --test e2e_integration -- test_hybrid_retrieval_e2e` | No -- Wave 0 |
| PERF-05 | E2E hybrid retrieval benchmark runs | benchmark | `cargo bench -p blufio --bench bench_hybrid -- onnx_e2e_pipeline --test` | No -- Wave 0 (bench_hybrid.rs exists, ONNX group does not) |
| PERF-06 | Comparative benchmark produces results | integration | `cargo test -p blufio --test e2e_integration -- test_bench_cli_smoke` | No -- Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --workspace --lib --no-fail-fast`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green + clippy + cargo doc before verify

### Wave 0 Gaps
- [ ] `crates/blufio/tests/e2e_integration.rs` -- new file for cross-subsystem tests
- [ ] `crates/blufio/benches/bench_hybrid.rs` -- extend with ONNX E2E + combined groups
- [ ] No new framework install needed -- criterion and tokio already in dev-dependencies

## Open Questions

1. **OnnxEmbedder synchronous access in Criterion**
   - What we know: OnnxEmbedder wraps ort::Session in Mutex, embed() is async. Criterion closures are sync.
   - What's unclear: Whether embed() can be called via tokio::runtime::Runtime::block_on() inside criterion, or if we need a direct synchronous path through the Mutex<Session>.
   - Recommendation: Create a small `embed_sync()` helper that locks the Mutex and runs inference directly. This avoids runtime-in-runtime issues and is closer to what iter_batched expects.

2. **Concurrent hot reload + vec0 search test**
   - What we know: Need to test that config reload doesn't break an in-flight vec0 search.
   - What's unclear: Whether ArcSwap config reload affects in-memory MemoryStore fields or only the config reference.
   - Recommendation: Use tokio::spawn for vec0 search + main task triggers config reload. Assert both complete without error. The real concern is whether the MemoryStore's vec0_enabled field can be toggled mid-flight -- it's set at construction time, so it's inherently safe.

3. **Backup/restore vec0 data safety**
   - What we know: SQLite backup copies the entire database file. vec0 uses shadow tables in the same DB.
   - What's unclear: Whether vec0 shadow tables survive a rusqlite backup + restore cycle intact.
   - Recommendation: Static review is sufficient. vec0 shadow tables are regular SQLite tables in the same file. `PRAGMA integrity_check` after backup already validates structural integrity. Document this finding.

## Sources

### Primary (HIGH confidence)
- `crates/blufio/benches/bench_hybrid.rs` -- existing hybrid benchmark patterns, make_embedding(), MEMORY_TOPICS, setup_hybrid_bench_db()
- `crates/blufio/tests/e2e_vec0.rs` -- 12 integration tests with full test infrastructure (setup_test_db, synthetic_embedding, make_test_memory, vec0_count_async)
- `crates/blufio-memory/src/retriever.rs` -- HybridRetriever implementation, score_from_vec0_data, score_from_memory_structs, reciprocal_rank_fusion, mmr_rerank
- `crates/blufio-memory/src/store.rs` -- MemoryStore with vec0_enabled, save, batch_evict, soft_delete, populate_vec0
- `crates/blufio-gdpr/src/erasure.rs` -- GDPR erasure SQL (DELETE FROM memories without vec0 sync)
- `crates/blufio-cron/src/tasks/memory_cleanup.rs` -- Cron cleanup (raw SQL without vec0 sync)
- `crates/blufio-mcp-server/src/resources.rs` -- MCP search uses search_bm25 only
- `crates/blufio-bus/src/events.rs` -- BusEvent enum with Vec0PopulationComplete, SecurityEvent variants
- `crates/blufio/src/doctor.rs` -- Health checks including check_vec0() and check_injection_defense()
- `contrib/blufio.example.toml` -- Missing memory.vec0_enabled section
- `.planning/phases/68-performance-benchmarking-suite/68-VERIFICATION.md` -- Verification format reference

### Secondary (MEDIUM confidence)
- `crates/blufio-memory/src/embedder.rs` -- OnnxEmbedder::new(Path) API, Mutex<Session> design
- `crates/blufio-agent/src/session.rs` -- Memory provider query setting for context assembly
- `crates/blufio/Cargo.toml` -- Feature flags, bench entries, dev-dependencies

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all libraries already in workspace, patterns established across 4 prior phases
- Architecture: HIGH -- test infrastructure is mature (e2e_vec0.rs, bench_hybrid.rs patterns well-documented)
- Pitfalls: HIGH -- wiring gaps verified by direct source code inspection, not speculation
- Wiring gaps: HIGH -- 5 gaps identified with exact file/line references

**Research date:** 2026-03-14
**Valid until:** 2026-04-14 (stable -- infrastructure is mature, no external dependency changes expected)
