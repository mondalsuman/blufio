# Feature Landscape

**Domain:** sqlite-vec vector search migration, performance benchmarking suite, injection defense hardening
**Researched:** 2026-03-13
**Milestone:** v1.6 Performance & Scalability Validation

## Table Stakes

Features users/operators expect from production-grade AI agent memory, benchmarking, and security.

### sqlite-vec Integration

| Feature | Why Expected | Complexity | Dependencies | Notes |
|---------|--------------|------------|--------------|-------|
| vec0 virtual table for vector storage | Current approach loads ALL embeddings into Rust memory for brute-force cosine similarity (`get_active_embeddings()` in retriever.rs). At 10K entries x 384 dims x 4 bytes = 15MB per query scan. sqlite-vec pushes this to C with SIMD (AVX/NEON). | Med | rusqlite 0.37, sqlite-vec crate, migration V-next | vec0 supports `float[384]` columns with cosine distance metric |
| KNN query via `MATCH` + `k = N` | Replace `retriever.rs::vector_search()` which currently iterates all active embeddings in Rust. sqlite-vec does brute-force in C with SIMD, ~3-10x faster for 10K vectors. | Med | vec0 table created, embeddings migrated | Query: `WHERE embedding MATCH ?1 AND k = ?2` returns (rowid, distance) |
| Metadata columns for status/classification filtering | Current SQL queries filter `status = 'active' AND classification != 'restricted'` in separate queries. vec0 metadata columns support `=`, `!=`, `>`, `<`, `BETWEEN` in KNN WHERE clauses -- pre-filter before vector scan. | Med | vec0 table definition includes metadata columns | Supported types: boolean, integer, float, text. NULL not supported yet. |
| SQLCipher compatibility | Blufio uses `bundled-sqlcipher-vendored-openssl`. sqlite-vec compiles C source via `cc` crate. Must verify vec0 works when SQLite is actually SQLCipher. Extension loading via `sqlite3_auto_extension` should work since both link against the same SQLite. | High | Build-time integration testing | **CRITICAL RISK**: sqlite-vec C source compiled against SQLCipher headers, not stock SQLite. Needs explicit verification. |
| Migration from BLOB embeddings to vec0 | Existing `memories` table stores embeddings as raw BLOB. Need migration to create vec0 shadow table, copy embeddings, and ensure FTS5 triggers still work. | Med | New refinery migration (V-next), rollback strategy | vec0 is a separate virtual table -- can coexist with existing memories table |
| BM25 + vec0 hybrid search preserved | Current hybrid search (RRF fusion of cosine + BM25) must continue working. BM25 uses FTS5 `memories_fts`. vec0 replaces only the vector search leg. | Low | vec0 for vector, FTS5 for BM25, RRF fusion unchanged | RRF fusion code in `retriever.rs::reciprocal_rank_fusion()` stays identical |
| Eviction works with vec0 | `batch_evict()` currently DELETEs from `memories` table. Must also remove from vec0 virtual table. | Med | vec0 DELETE support, transactional consistency | vec0 supports DELETE. Need to wrap both deletes in same transaction. |
| Temporal decay + importance boost preserved | Scoring pipeline (`rrf_score * importance * decay`) remains in Rust. vec0 only replaces the raw similarity computation, not the post-processing. | Low | None -- Rust scoring code unchanged | No vec0 dependency for this. Just verify scores still comparable. |

### Performance Benchmarking Suite

| Feature | Why Expected | Complexity | Dependencies | Notes |
|---------|--------------|------------|--------------|-------|
| Binary size measurement and tracking | PROJECT.md specifies 25-50MB target. Must validate `cargo bloat --release` output and track regressions as dependencies change. | Low | cargo-bloat (dev tool), CI integration | Already have `release-musl` profile with `opt-level = "s"`, `strip = "symbols"` |
| Memory usage profiling (idle + load) | PROJECT.md specifies 50-80MB idle, 100-200MB under load. Must measure RSS with jemalloc stats (already has `tikv-jemalloc-ctl` with `stats` feature). | Med | jemalloc stats API, sysinfo crate (already dep) | jemalloc `epoch.advance()` + `stats.allocated` gives precise heap usage |
| Token reduction validation | Context engine claims 68-84% token reduction vs inject-everything. Must measure actual token counts before/after compaction across representative workloads. | Med | Token counter (tiktoken-rs/HuggingFace), compaction pipeline | Existing `bench_context.rs` has heuristic counting; need end-to-end validation |
| Criterion benchmarks for vec0 vs in-memory | Direct A/B comparison: current `get_active_embeddings()` + Rust cosine vs vec0 KNN query. At 100, 1K, 5K, 10K entries. | Med | Both implementations available during migration | Benchmarks should cover query latency and memory delta |
| Injection classifier throughput benchmark | Current `bench_pii.rs` covers PII detection. Need equivalent for injection classifier at various input sizes (1KB, 5KB, 10KB). | Low | blufio-injection classifier, criterion | Extends existing benchmark pattern; classifier is synchronous |
| Regression CI baselines | Store benchmark results as baselines; fail CI if perf degrades beyond threshold. | Med | criterion JSON output, CI script | Existing `bench_regression` CI mentioned in PROJECT.md Phase 63 |

### Injection Defense Hardening

| Feature | Why Expected | Complexity | Dependencies | Notes |
|---------|--------------|------------|--------------|-------|
| Unicode/invisible character detection | Current 11 patterns use standard regex. Attackers embed U+200B (zero-width space), U+200C/D, U+FEFF, Unicode tags (E0000-E007F) between injection keywords to bypass pattern matching. | Med | regex crate (already dep), Unicode normalization | Strip/detect zero-width chars BEFORE pattern matching. Regex: `[\u{200B}-\u{200F}\u{2060}-\u{2064}\u{FEFF}]` |
| Homoglyph normalization | Cyrillic 'a' (U+0430) visually identical to Latin 'a'. Attackers substitute characters to evade regex. NFKC normalization resolves most confusables. | Med | unicode-normalization crate (new dep, ~50KB) | Apply NFKC before classifier. Also map known confusables (Cyrillic/Greek -> Latin). |
| Base64/encoding detection | Attackers encode injection payloads as Base64 strings. Pattern: detect `[A-Za-z0-9+/]{40,}={0,2}` then decode and re-scan. | Low | base64 crate (already dep) | Decode candidate strings, re-run classifier on decoded content. False positive risk on legitimate base64 data. |
| Expanded pattern coverage | Current 11 patterns cover 3 categories. Missing: prompt leaking (`"repeat the above"`), jailbreak keywords (`"DAN mode"`, `"developer mode"`), multi-language patterns, delimiter manipulation (`"""`, `---`, triple backticks). | Med | regex crate, PATTERNS array expansion | OWASP LLM Top 10 2025 identifies these as active attack vectors |
| Indirect injection detection | MCP tool results and external content can contain embedded instructions. L4 OutputScreener already checks tool output, but needs patterns specific to indirect injection (instructions hidden in HTML comments, markdown, JSON). | Med | L4 OutputScreener, additional patterns | Check `<!-- ignore previous -->` in HTML, hidden markdown, JSON string injection |
| Configurable severity weights | Current severities are hardcoded (0.1-0.5). Operators should tune weights per deployment. TOML config for `[injection.pattern_weights]`. | Low | blufio-config, InjectionDefenseConfig | Extend existing InputDetectionConfig with optional per-category weights |

## Differentiators

Features that set Blufio apart from competitors. Not universally expected, but demonstrate quality.

### sqlite-vec Integration

| Feature | Value Proposition | Complexity | Dependencies | Notes |
|---------|-------------------|------------|--------------|-------|
| Partition key by session_id | vec0 `partition key` internally shards the index. Searching within a session is ~3x faster because only that partition is scanned. Competitors use flat vector stores. | Low | vec0 partition key declaration | `session_id text partition key` in table def. Only helps when querying within-session. |
| Auxiliary columns for content | vec0 `+content text` auxiliary columns avoid a JOIN to `memories` table when retrieving search results. Single query returns vectors + content. | Low | vec0 auxiliary column support | Reduces query count from 2 (vec0 KNN + memories lookup) to 1 |
| vec0 + FTS5 in single transaction | Both vec0 and FTS5 managed within same SQLite connection ensures ACID consistency. No external vector DB to sync. Single-file deployment preserved. | Low | Already using single SQLite connection | Key differentiator vs Chroma/Pinecone/Weaviate which require separate processes |
| Quantized vectors (int8/bit) | vec0 supports `int8[384]` (4x compression) and `bit[384]` (32x compression). Could reduce storage for cold/archived memories while maintaining search quality. | Med | vec0 quantization support, quality validation | int8 typically retains 95%+ recall. Defer to future if not needed. |

### Performance Benchmarking Suite

| Feature | Value Proposition | Complexity | Dependencies | Notes |
|---------|-------------------|------------|--------------|-------|
| End-to-end memory pipeline benchmark | Full pipeline: embed query -> vec0 KNN -> BM25 FTS5 -> RRF fusion -> temporal decay -> MMR rerank. No competitor benchmarks the complete retrieval pipeline. | High | ONNX model loaded, SQLite with data, full pipeline wired | Requires test fixture with pre-embedded data. ONNX model download in CI. |
| Comparative benchmark vs OpenClaw | Measure Blufio idle RSS vs OpenClaw (300-800MB/24h reported). Token usage per turn. Startup time. Concrete numbers for the "kill shot" narrative. | Med | OpenClaw instance for comparison, consistent workload | Powerful marketing material. Must be reproducible. |
| Binary size breakdown by crate | `cargo bloat --release --crates` showing each of 37 crates' contribution. Identify bloat candidates. Track across releases. | Low | cargo-bloat | Already have 37-crate workspace; breakdown is trivial |
| jemalloc allocation tracking per subsystem | Tag allocations by subsystem (memory, context, injection, gateway) using jemalloc arenas or Prometheus gauges. | High | jemalloc arena APIs or custom allocator tracking | Complex; may not be worth it for v1.6. Prometheus memory gauges may suffice. |

### Injection Defense Hardening

| Feature | Value Proposition | Complexity | Dependencies | Notes |
|---------|-------------------|------------|--------------|-------|
| Input sanitization layer (pre-classifier) | Strip zero-width chars, normalize Unicode, detect encoding before pattern matching. Runs in O(n) on input length, catches evasion that regex alone misses. | Med | unicode-normalization, custom sanitizer | OWASP recommends this as first defense line. Most competitors skip it. |
| Multi-language injection patterns | `"ignorez les instructions precedentes"` (French), `"ignoriere vorherige Anweisungen"` (German). Polyglot users and attackers use non-English. | Med | Expanded PATTERNS array, i18n-aware regex | OWASP 2025 flags multi-language as emerging vector. Start with top 5 languages. |
| Canary token detection | Detect if LLM output echoes a planted canary string, indicating the LLM was manipulated into revealing system prompt content. Insert canary in system prompt, check if output contains it. | Med | System prompt modification, output scanning | Novel defense against prompt leaking attacks. Low false positive rate. |
| Injection detection Prometheus metrics | Per-category detection counts, score distributions, false positive tracking. Enables operators to tune thresholds with data. | Low | metrics crate (already dep), Prometheus | Existing `metrics::record_input_detection()` provides foundation; expand with histograms |

## Anti-Features

Features to explicitly NOT build in this milestone.

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| ANN index (HNSW/IVF) in vec0 | sqlite-vec v0.1.7-alpha does not yet ship ANN indexes. Brute-force is sufficient for 10K entries (sub-50ms at 384 dims). ANN adds complexity for no gain at this scale. | Use brute-force vec0 KNN. Revisit at 100K+ entries. |
| Separate vector database (Chroma/Pinecone) | Violates single-binary, single-file deployment model. Adds network dependency, ops burden, and sync complexity. | sqlite-vec keeps everything in SQLite. Zero additional infrastructure. |
| ML-based injection classifier | Training/fine-tuning a small ML model for injection detection adds ONNX model weight, inference latency, and maintenance burden. Regex patterns are transparent, auditable, and fast. | Expand regex patterns + pre-processing. ML classifier is v2.0+ territory. |
| GPU-accelerated vector search | CUDA/ROCm adds 100MB+ to binary, platform-specific builds, driver dependencies. Blufio targets $4/month VPS. | CPU SIMD via sqlite-vec. GPU is anti-pattern for single-binary edge deployment. |
| Real-time adversarial training | Dynamically updating injection patterns based on detected attacks requires persistence, learning pipeline, and can be poisoned. | Static patterns + operator custom patterns via TOML config. Ship updated patterns in releases. |
| flamegraph/profiling in release binary | Embedding profiling instrumentation in release builds adds size and performance overhead. | Use `cargo flamegraph` on debug/release builds during development. CI benchmarks for regression. |
| Load testing suite (wrk/k6) | HTTP load testing is orthogonal to the benchmarking suite goal. Gateway performance is not a v1.6 concern. | Criterion for micro-benchmarks. Load testing deferred to future milestone. |
| Multimodal injection detection (images) | OWASP 2025 identifies image-based injection. Blufio is text-only for now (no vision input in agent loop). | Detect only text-based injection. Image injection becomes relevant when vision providers are added. |

## Feature Dependencies

```
sqlite-vec crate integration
  |-> vec0 virtual table creation (migration V-next)
  |    |-> SQLCipher compatibility verification (CRITICAL GATE)
  |    |-> Metadata columns (status, classification) in vec0
  |    |-> Partition key (session_id) in vec0
  |    |-> Auxiliary column (+content) in vec0
  |-> Embedding migration (BLOB -> vec0 INSERT)
  |    |-> Rollback strategy (keep memories table, add vec0 alongside)
  |-> HybridRetriever refactor
  |    |-> vector_search() uses vec0 KNN instead of get_active_embeddings()
  |    |-> BM25 search unchanged (FTS5)
  |    |-> RRF fusion unchanged
  |-> Eviction update
  |    |-> batch_evict() deletes from both memories + vec0

Unicode/homoglyph sanitization (pre-classifier)
  |-> InjectionClassifier.classify() calls sanitizer first
  |-> Zero-width character stripping
  |-> NFKC normalization
  |-> Confusable character mapping

Pattern expansion
  |-> New PATTERNS entries (prompt leaking, jailbreak, delimiters)
  |-> Multi-language patterns
  |-> Base64 detection + decode + re-scan
  |-> Updated severity weights

Benchmarking suite
  |-> Binary size: cargo bloat (no code dep)
  |-> Memory: jemalloc stats (existing dep)
  |-> vec0 vs in-memory: both implementations available
  |-> Injection classifier: existing classifier code
  |-> Token reduction: existing compaction + counters
```

## MVP Recommendation

### Phase 1: sqlite-vec Integration (highest priority, highest risk)

Prioritize:
1. **SQLCipher + sqlite-vec build compatibility** -- gate everything else. If sqlite-vec C source cannot compile/link against SQLCipher, the entire vec0 approach fails. Test this FIRST.
2. **vec0 virtual table with cosine distance** -- create alongside existing memories table. Add metadata columns (status text, classification text). Dual-write during migration period.
3. **Migration script** -- copy existing BLOB embeddings into vec0 table. Non-destructive: keep original memories table intact.
4. **HybridRetriever refactor** -- replace `vector_search()` to use vec0 KNN query. Keep BM25 + RRF unchanged.
5. **Eviction update** -- `batch_evict()` deletes from both tables in single transaction.

### Phase 2: Performance Benchmarking Suite (medium priority, low risk)

Prioritize:
1. **Binary size measurement** -- `cargo bloat --release` baseline. Measure delta from sqlite-vec addition.
2. **Memory RSS tracking** -- jemalloc stats for idle and under-load measurement. Compare before/after vec0.
3. **vec0 vs in-memory criterion benchmarks** -- A/B at 100, 1K, 5K, 10K entries.
4. **Injection classifier throughput benchmark** -- 1KB, 5KB, 10KB inputs.
5. **Token reduction validation** -- end-to-end compaction measurement.

### Phase 3: Injection Defense Hardening (medium priority, medium risk)

Prioritize:
1. **Input sanitization layer** -- Unicode normalization + zero-width stripping before classifier. Highest impact for evasion prevention.
2. **Pattern expansion** -- add prompt leaking, jailbreak, delimiter manipulation patterns. Increase from 11 to ~20-25 patterns.
3. **Base64 detection** -- detect encoded payloads, decode, re-scan.
4. **Multi-language patterns** -- top 5 languages (French, German, Spanish, Chinese, Japanese).
5. **Configurable severity weights** -- TOML config for operator tuning.

Defer:
- **Canary token detection**: Novel but not urgent. Requires system prompt modification and careful design.
- **End-to-end pipeline benchmark**: Requires ONNX model in CI. Complex fixture setup. Better as a follow-up.
- **jemalloc per-subsystem tracking**: Overkill for v1.6. Prometheus memory gauges suffice.
- **Quantized vectors (int8/bit)**: Brute-force at 10K entries is fast enough. Quantization is optimization for scale Blufio has not reached.

## Existing Code Impact Analysis

### blufio-memory crate (MAJOR changes)

| File | Change Type | Scope |
|------|-------------|-------|
| `Cargo.toml` | Add `sqlite-vec` dependency | New dep |
| `store.rs` | Add vec0 table initialization, vec0 insert/delete methods | ~100-150 LOC new |
| `retriever.rs` | Replace `vector_search()` with vec0 KNN query | ~50 LOC changed |
| `eviction.rs` | Update `batch_evict()` for dual-table delete | ~20 LOC changed |
| `types.rs` | Possibly add vec0-specific serialization (zerocopy) | ~10 LOC |
| `lib.rs` | No change (public API unchanged) | None |

### blufio-injection crate (MODERATE changes)

| File | Change Type | Scope |
|------|-------------|-------|
| `Cargo.toml` | Add `unicode-normalization` dependency | New dep (~50KB) |
| `patterns.rs` | Expand PATTERNS array from 11 to ~20-25 | ~100 LOC new |
| `classifier.rs` | Add sanitization pre-pass before pattern matching | ~50-80 LOC new |
| `config.rs` | Add configurable severity weights | ~30 LOC new |
| `output_screen.rs` | Expand indirect injection patterns | ~30 LOC new |

### blufio (binary crate) benchmarks (NEW files)

| File | Change Type | Scope |
|------|-------------|-------|
| `benches/bench_vec0.rs` | New criterion benchmark for vec0 vs in-memory | ~150 LOC |
| `benches/bench_injection.rs` | New criterion benchmark for classifier throughput | ~100 LOC |
| `benches/bench_binary_size.rs` | Measurement script (may be CI-only, not criterion) | ~50 LOC |

### blufio-storage crate (MINOR changes)

| File | Change Type | Scope |
|------|-------------|-------|
| `migrations/` | New migration V-next for vec0 table creation | ~30 LOC SQL |

### blufio-config crate (MINOR changes)

| File | Change Type | Scope |
|------|-------------|-------|
| model config | Add optional injection severity weight overrides | ~20 LOC |

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| sqlite-vec API and features | MEDIUM | Official docs + blog posts confirm vec0 MATCH syntax, metadata columns, cosine distance. Version is v0.1.7-alpha (not stable). |
| sqlite-vec + rusqlite integration | HIGH | Official Rust example uses `sqlite3_auto_extension`. Crate compiles C source via `cc`. |
| sqlite-vec + SQLCipher | LOW | No documentation found on compatibility. SQLCipher is a fork of SQLite with modified headers. Extension compilation against SQLCipher headers is untested. **Must validate before committing to approach.** |
| vec0 brute-force at 10K scale | HIGH | At 384 dims, brute-force under 50ms for 10K vectors confirmed by benchmarks. SIMD accelerated. |
| Injection patterns (OWASP) | HIGH | OWASP LLM Top 10 2025 + Cisco + Palo Alto research confirm Unicode evasion, homoglyphs, encoding bypass as active attack vectors. |
| Unicode normalization approach | HIGH | NFKC normalization + zero-width stripping is well-established defense. unicode-normalization crate is mature. |
| Criterion benchmarking | HIGH | Already using criterion 0.5 with existing benchmarks. Pattern is established in codebase. |
| Binary size/memory measurement | HIGH | cargo-bloat and jemalloc stats are standard Rust tooling. Already have jemalloc dep. |

## Sources

- [sqlite-vec GitHub repository](https://github.com/asg017/sqlite-vec)
- [sqlite-vec Rust integration guide](https://alexgarcia.xyz/sqlite-vec/rust.html)
- [sqlite-vec v0.1.0 stable release announcement](https://alexgarcia.xyz/blog/2024/sqlite-vec-stable-release/index.html)
- [sqlite-vec metadata columns release](https://alexgarcia.xyz/blog/2024/sqlite-vec-metadata-release/index.html)
- [OWASP LLM01:2025 Prompt Injection](https://genai.owasp.org/llmrisk/llm01-prompt-injection/)
- [OWASP Prompt Injection Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/LLM_Prompt_Injection_Prevention_Cheat_Sheet.html)
- [Cisco: Understanding and Mitigating Unicode Tag Prompt Injection](https://blogs.cisco.com/ai/understanding-and-mitigating-unicode-tag-prompt-injection)
- [Palo Alto Unit42: Indirect Prompt Injection in the Wild](https://unit42.paloaltonetworks.com/ai-agent-prompt-injection/)
- [Criterion.rs guide](https://bencher.dev/learn/benchmarking/rust/criterion/)
- [cargo-bloat-action for CI](https://github.com/orf/cargo-bloat-action)
