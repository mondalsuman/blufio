# Project Research Summary

**Project:** v1.6 Performance & Scalability Validation
**Domain:** Vector search migration, performance benchmarking suite, injection defense hardening
**Researched:** 2026-03-13
**Confidence:** MEDIUM-HIGH

## Executive Summary

This milestone migrates Blufio's memory retrieval from in-memory brute-force vector search to disk-backed sqlite-vec KNN queries, establishes comprehensive performance benchmarks, and hardens injection defenses against Unicode evasion attacks. The current approach loads all embeddings into memory for every search (O(n) scan, ~15MB at 10K entries), which hits memory limits at scale. sqlite-vec pushes vector search into SQLite's C layer with SIMD acceleration, eliminating the O(n) memory overhead while maintaining sub-10ms query latency at 10K scale.

The recommended approach uses sqlite-vec's vec0 virtual tables with metadata column filtering, compiled directly against SQLCipher to preserve encryption-at-rest. The existing hybrid retrieval pipeline (BM25 + vector search + RRF fusion) remains unchanged except for the vector search implementation. The critical risk is SQLCipher compatibility — sqlite-vec must be registered per-connection after PRAGMA key, never via auto_extension. The second major risk is injection pattern false positives — expanding from 11 to ~25 patterns requires benign corpus validation to prevent blocking legitimate user messages.

Performance benchmarking establishes binary size (<50MB), memory usage (50-80MB idle, 100-200MB loaded), and retrieval latency baselines. The existing 4 criterion benchmarks are extended with vec0 KNN comparison, injection classifier throughput, and binary/RSS tracking. Injection defense adds Unicode normalization, base64 decoding, and 14 new patterns covering prompt leaking, jailbreaks, and encoding obfuscation (OWASP LLM01:2025 vectors).

## Key Findings

### Recommended Stack

sqlite-vec 0.1.6 provides FFI bindings for native SQLite vector search, replacing the current in-memory `get_active_embeddings()` scan with vec0 virtual table KNN queries. The extension compiles via `cc` at build time and statically links, maintaining single-binary deployment. zerocopy 0.8 eliminates allocation overhead for vector-to-blob conversion (1536 bytes saved per query/insert). No ML-based injection classifier — regex patterns remain deterministic, auditable, and <1ms.

**Core technologies:**
- **sqlite-vec 0.1.6**: vec0 virtual tables with cosine KNN search, metadata column filtering — eliminates O(n) memory load, pushes search to C with SIMD (AVX/NEON), maintains encryption via SQLCipher
- **zerocopy 0.8**: zero-copy f32 slice to byte conversion — avoids 1536-byte allocation per query for 384-dim vectors
- **cargo-bloat (dev tool)**: binary size analysis by crate — tracks sqlite-vec's impact on 25MB target
- **criterion 0.5 (existing)**: statistical benchmarks — extends with vec0 KNN, injection throughput, and hybrid pipeline end-to-end tests
- **unicode-normalization 0.1**: NFKC normalization + confusable mapping — defeats Unicode tag injection and homoglyph attacks

**Critical dependencies already satisfied:**
- rusqlite 0.37 with bundled-sqlcipher-vendored-openssl provides the SQLite engine
- tokio-rusqlite 0.7 wraps async operations
- regex 1.0 handles pattern matching (RegexSet for fast-path)
- jemalloc with stats feature measures RSS

### Expected Features

**Must have (table stakes):**
- **vec0 virtual table with KNN queries** — Replace in-memory scan; users expect vector search to scale beyond 10K entries without hitting RAM limits
- **SQLCipher compatibility** — vec0 data encrypted at rest alongside existing memories table; critical for GDPR/privacy
- **Metadata column filtering** — Filter `status='active'` and `classification!='restricted'` during KNN, not post-query (current JOINs execute AFTER KNN and return wrong result counts)
- **Migration from BLOB embeddings to vec0** — Backfill existing memories; rollback strategy if migration fails
- **BM25 + vec0 hybrid search preserved** — RRF fusion, temporal decay, importance boost, MMR reranking unchanged
- **Binary size measurement** — Track 25-50MB target; validate sqlite-vec adds <2MB
- **Memory usage profiling** — Validate 50-80MB idle, 100-200MB loaded targets with jemalloc stats
- **Unicode/encoding evasion detection** — NFKC normalization, zero-width stripping, base64 decode + re-scan (OWASP LLM01:2025)
- **Expanded injection patterns** — Cover prompt leaking, jailbreak modes, delimiter manipulation (11 -> 25 patterns)

**Should have (competitive):**
- **vec0 partition keys by session_id** — 3x faster within-session search via index sharding (minor optimization, defer if complex)
- **Auxiliary columns for content** — Single-query retrieval without JOIN to memories table (reduces query count 2->1)
- **End-to-end hybrid retrieval benchmark** — Full pipeline (embed -> vec0 -> BM25 -> RRF -> MMR) for marketing claims
- **Comparative benchmark vs OpenClaw** — Validate "300-800MB vs 100-200MB" claim with reproducible numbers
- **Multi-language injection patterns** — French, German, Spanish, Chinese, Japanese for polyglot attacks (OWASP emerging vector)
- **Canary token detection** — Plant canary in system prompt, check if output echoes it (novel defense against prompt leaking)

**Defer (v2+):**
- **ANN indexes (HNSW/IVF)** — sqlite-vec brute-force is <10ms at 10K entries; ANN overkill for current scale
- **Quantized vectors (int8/bit)** — 4x/32x compression not needed until 100K+ entries
- **ML-based injection classifier** — Adds ONNX model weight, inference latency; regex is sufficient (OWASP guidance)
- **GPU-accelerated search** — CUDA/ROCm adds 100MB+ to binary, violates edge deployment model
- **Load testing suite (wrk/k6)** — HTTP perf not a v1.6 concern; micro-benchmarks cover memory/injection subsystems

### Architecture Approach

Three independent integration areas: (1) sqlite-vec in blufio-memory (store.rs, retriever.rs) + migration, (2) benchmarks in crates/blufio/benches + CI enhancements, (3) injection hardening in blufio-injection (patterns.rs, classifier.rs). Minimal cross-dependency allows parallel development. The vec0 table is a shadow index — existing memories.embedding BLOB retained for MMR reranking, GDPR export, and fallback. Dual-write pattern maintains sync (no trigger support for virtual tables).

**Major components:**
1. **blufio-memory/store.rs** — Add `knn_search(query_embedding, k)` method querying vec0 with metadata filters; dual-write in `save()`, `soft_delete()`, `batch_evict()` to keep memories + memories_vec in sync
2. **blufio-storage/migrations/V15** — Create vec0 virtual table, backfill from memories.embedding BLOBs with status/classification metadata columns; batched 500 rows to avoid single-writer starvation
3. **blufio-injection/classifier.rs** — Add sanitization pre-pass (Unicode normalization, zero-width stripping, base64 decode) before RegexSet fast-path; expand PATTERNS array with 14 new entries across 3 new categories
4. **crates/blufio/benches/** — New bench_injection.rs (classifier throughput), extend bench_memory.rs with vec0 KNN group; CI tracks binary size via `cargo bloat`, RSS via jemalloc stats

### Critical Pitfalls

1. **SQLCipher + sqlite-vec init order conflict** — NEVER use `sqlite3_auto_extension` with SQLCipher. The extension runs during `sqlite3_open()` before `PRAGMA key` is applied, causing "file is encrypted" failure. Register sqlite-vec per-connection AFTER `PRAGMA key` via manual `sqlite3_vec_init()` call. Modify `database.rs::open_connection()` to accept extension initializer closure. Integration test with encrypted DB required.

2. **vec0 distance vs similarity inversion** — sqlite-vec returns cosine **distance** (0=identical, 2=opposite), but existing pipeline expects cosine **similarity** (0=opposite, 1=identical). Failure to convert causes reversed ranking. Convert immediately: `similarity = 1.0 - distance`. Overfetch `k = max_results * 2` to account for threshold filtering. Regression test comparing top-5 results from old vs new path.

3. **vec0 metadata column requirement for filtering** — The current code filters `status='active' AND classification!='restricted'` via JOIN to memories table. Virtual table JOINs execute AFTER KNN, returning wrong result counts (includes deleted/restricted). Use vec0 metadata columns (`status TEXT`, `classification TEXT`, `is_deleted INTEGER`) and filter during KNN with WHERE clause. Maintain metadata via dual-write, not triggers (triggers don't work with virtual tables).

4. **Injection pattern false positive epidemic** — Expanding from 11 to 25 patterns increases recall but also false positives. Regex matches byte patterns, not semantics (e.g., `"discard prior directives"` also matches `"discard all prior reservations"`). Maintain benign corpus (100+ realistic messages), test every new pattern against it, use dry_run mode first, track FP rate as Prometheus metric. OWASP warns that 191 patterns achieved only 23% recall with high FP rate.

5. **tokio-rusqlite single-writer starvation during migration** — Migrating 10K entries in one `conn.call()` blocks all other DB operations (saves, queues, audit) for 5-30 seconds on slow VPS I/O. Batch migration in 500-row chunks, yield between batches, log progress, consider CLI command instead of startup migration.

## Implications for Roadmap

Based on research, suggested phase structure groups features by risk profile and dependency order:

### Phase 1: sqlite-vec Integration (HIGH priority, HIGH risk)
**Rationale:** This is the foundation for all performance claims. Must validate SQLCipher compatibility FIRST before committing to approach. If integration fails, fallback to in-memory path with optimizations. Highest technical risk due to extension loading, encryption interaction, and data migration.

**Delivers:** Disk-backed vector search via vec0 virtual tables, eliminating O(n) memory overhead; KNN queries <10ms at 10K scale; migration from existing BLOB embeddings; dual-write sync between memories and memories_vec tables.

**Addresses:**
- vec0 virtual table with KNN queries (table stakes)
- SQLCipher compatibility (table stakes)
- Metadata column filtering (table stakes)
- Migration with rollback (table stakes)
- BM25 + vec0 hybrid preserved (table stakes)

**Avoids:**
- Pitfall 1: SQLCipher init order (register per-connection after PRAGMA key)
- Pitfall 2: Bundled SQLite conflict (vendor C source, compile against SQLCipher headers)
- Pitfall 3: Distance/similarity inversion (convert immediately, regression test)
- Pitfall 4: External filter failure (metadata columns for all filters)
- Pitfall 5: Migration data loss (BLOB format verification, post-migration validation)
- Pitfall 10: Single-writer starvation (batch in 500-row chunks)

**Research flag:** Phase will need `/gsd:research-phase` if SQLCipher compatibility test fails — research alternative: sqlite-vec scalar functions without vec0 virtual table.

### Phase 2: Injection Defense Hardening (MEDIUM priority, MEDIUM risk)
**Rationale:** Can proceed independently of vec0 integration. Addresses OWASP LLM01:2025 vectors with minimal code changes. Main risk is false positives — mitigated by benign corpus validation and dry_run rollout. No new crate dependencies (unicode-normalization is small, battle-tested).

**Delivers:** Unicode/homoglyph normalization, base64 decode + re-scan, 14 new injection patterns (prompt leaking, jailbreak, encoding obfuscation), multi-language patterns (5 languages), expanded credential detection in OutputScreener.

**Uses:**
- unicode-normalization crate for NFKC + confusable mapping (STACK.md)
- regex crate (existing) for expanded PATTERNS array
- base64 crate (existing) for encoding detection

**Implements:**
- InjectionClassifier pre-processing pipeline (ARCHITECTURE.md)
- EncodingObfuscation, DelimiterManipulation, IndirectInjection categories
- PII + credential pattern expansion without double-redaction

**Addresses:**
- Unicode/encoding evasion (table stakes)
- Expanded injection patterns (table stakes)
- Multi-language patterns (differentiator)
- Configurable severity weights via TOML (differentiator)

**Avoids:**
- Pitfall 9: False positive epidemic (benign corpus testing, dry_run mode, FP metrics)
- Pitfall 13: Boundary token self-match (allowlist HMAC format)
- Pitfall 14: Double-redaction (test overlapping PII + credential patterns)
- Pitfall 17: Regex compile slowdown (benchmark compilation, prefer ASCII)

**Research flag:** Standard pattern expansion, skip `/gsd:research-phase`. OWASP documentation provides clear patterns.

### Phase 3: Performance Benchmarking Suite (MEDIUM priority, LOW risk)
**Rationale:** Depends on Phase 1 (KNN benchmarks need vec0) and Phase 2 (injection benchmarks need expanded patterns). Extends existing criterion infrastructure with new benchmark groups. Low risk — criterion patterns already established in codebase.

**Delivers:** Binary size tracking (<50MB), memory RSS tracking (50-80MB idle, 100-200MB loaded), vec0 vs in-memory A/B comparison, injection classifier throughput, end-to-end hybrid retrieval benchmark, regression baselines in bench_results table.

**Uses:**
- criterion 0.5 (existing) for statistical benchmarks
- cargo-bloat (dev tool) for binary size by crate
- jemalloc stats (existing) for heap RSS measurement
- sysinfo (existing) for total RSS including mmap

**Addresses:**
- Binary size measurement (table stakes)
- Memory usage profiling (table stakes)
- vec0 vs in-memory comparison (table stakes)
- Injection classifier throughput (table stakes)
- End-to-end hybrid pipeline (differentiator)
- Comparative vs OpenClaw (differentiator)

**Avoids:**
- Pitfall 8: Artifact measurement (file-backed encrypted SQLite, include ONNX model, strip binary)
- Pitfall 11: Flaky CI on shared runners (20% threshold, store baselines in bench_results)
- Pitfall 15: Debug info inflates size (strip=true, lto=true in release profile)

**Research flag:** Standard benchmarking patterns, skip `/gsd:research-phase`. Criterion documentation is comprehensive.

### Phase Ordering Rationale

- **Phase 1 first:** SQLCipher compatibility is a gate — if sqlite-vec doesn't work with encryption, the entire approach changes. Must validate before building benchmarks that depend on it.
- **Phase 2 parallel:** Injection hardening is independent of vec0. Can develop in parallel or second — no dependency on Phase 1 completion.
- **Phase 3 last:** Benchmarks measure Phases 1 and 2. Requires both complete to establish accurate baselines. Criterion infrastructure already exists, so low risk even at end.
- **Grouping rationale:** Each phase touches distinct crates with minimal coupling (memory, injection, benches). Enables parallel development if resources allow.
- **Pitfall avoidance:** Phase 1 addresses all vec0 integration pitfalls upfront. Phase 2 uses dry_run + benign corpus to catch false positives before production. Phase 3 tracks regressions to ensure optimizations don't degrade.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 1 (sqlite-vec integration):** Only if SQLCipher compatibility test fails. Research alternative: sqlite-vec scalar functions (`vec_distance_cosine()`) in ORDER BY clause instead of vec0 virtual table with MATCH clause. Medium effort, loses metadata column filtering optimization.

Phases with standard patterns (skip research-phase):
- **Phase 2 (injection hardening):** OWASP LLM Top 10 2025 provides explicit patterns and evasion techniques. Unicode normalization is well-documented in unicode-normalization crate. Regex expansion follows existing patterns.rs structure.
- **Phase 3 (benchmarking suite):** Criterion benchmarking is well-established in Rust. Existing 4 benchmarks provide template. Binary size and RSS measurement are standard cargo practices.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | MEDIUM-HIGH | sqlite-vec Rust docs + crates.io confirm API; SQLCipher compatibility unconfirmed (no explicit docs, needs integration test). zerocopy is battle-tested. unicode-normalization is mature. |
| Features | HIGH | Table stakes identified from current code (get_active_embeddings() bottleneck, OWASP LLM01:2025 vectors). Differentiators align with PROJECT.md performance goals. Anti-features correctly exclude ANN/ML overkill. |
| Architecture | HIGH | Direct codebase analysis of retriever.rs, store.rs, classifier.rs, patterns.rs. Dual-write pattern verified as necessary (trigger limitation documented). Migration batching prevents starvation. |
| Pitfalls | HIGH | Pitfalls 1-5 grounded in SQLCipher docs (PRAGMA key first), sqlite-vec docs (distance not similarity, metadata columns required), tokio-rusqlite architecture (single writer). Injection FP risk confirmed by OWASP guidance. |

**Overall confidence:** MEDIUM-HIGH

SQLCipher compatibility is the one LOW confidence area requiring explicit validation. All other findings are grounded in official documentation, codebase analysis, or established patterns.

### Gaps to Address

**Gap 1: SQLCipher + sqlite-vec compatibility**
- **Status:** No explicit documentation confirms this combination works
- **Architecture is sound:** Extension registration via `sqlite3_auto_extension` is engine-agnostic, should work with SQLCipher
- **Validation strategy:** Integration test MUST verify during Phase 1 before bulk migration
- **Fallback:** If vec0 virtual table fails, use sqlite-vec scalar functions (`vec_distance_cosine()`) in ORDER BY — loses MATCH syntax but keeps disk-backed search
- **How to handle:** Gate Phase 1 on compatibility test passing. If test fails, pivot to scalar function approach or research alternative vector extensions.

**Gap 2: False positive rate for expanded injection patterns**
- **Status:** OWASP warns that broad regex patterns have high FP rates; 191 patterns achieved only 23% recall with poor precision
- **Validation strategy:** Maintain benign corpus (100+ realistic messages), test every new pattern, use dry_run mode for 1 week minimum before promoting to log/block
- **How to handle:** Phase 2 includes benign corpus creation task. Track FP rate as Prometheus metric. Iteratively tune pattern specificity vs recall.

**Gap 3: Binary size impact of sqlite-vec**
- **Status:** sqlite-vec C source is ~50KB; compiled extension likely <500KB; impact on 25MB target is negligible
- **Validation strategy:** Measure with cargo-bloat before/after
- **How to handle:** Phase 3 benchmark tracks this. If sqlite-vec adds >2MB, investigate compile flags (strip unused distance metrics, disable AVX if targeting older CPUs).

**Gap 4: Performance at 100K+ scale**
- **Status:** sqlite-vec benchmarks show brute-force <75ms at 100K x 384-dim. Blufio targets 10K-50K. 100K+ may need ANN indexes (not available in stable sqlite-vec yet).
- **Validation strategy:** Phase 3 benchmarks measure actual KNN latency at 1K/5K/10K. Extrapolate to 100K.
- **How to handle:** If 100K extrapolation exceeds 100ms target, research quantization (int8/bit for 4x-32x speedup) or wait for sqlite-vec ANN support (roadmap item). Not a v1.6 blocker.

## Sources

### Primary (HIGH confidence)
- [sqlite-vec official documentation](https://alexgarcia.xyz/sqlite-vec/) — vec0 virtual table API, KNN syntax, metadata columns, distance metrics
- [sqlite-vec Rust integration guide](https://alexgarcia.xyz/sqlite-vec/rust.html) — `sqlite3_vec_init` registration pattern, rusqlite compatibility
- [sqlite-vec GitHub repository](https://github.com/asg017/sqlite-vec) — v0.1.6 source code, issue #196 on JOIN/WHERE filtering limitation
- [sqlite-vec crates.io](https://crates.io/crates/sqlite-vec) — version 0.1.6 stable, API reference
- [OWASP LLM01:2025 Prompt Injection](https://genai.owasp.org/llmrisk/llm01-prompt-injection/) — attack taxonomy, Unicode evasion, multi-language vectors
- [OWASP Prompt Injection Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/LLM_Prompt_Injection_Prevention_Cheat_Sheet.html) — regex limitations, sanitization pre-pass, encoding detection
- [SQLCipher API documentation](https://www.zetetic.net/sqlcipher/sqlcipher-api/) — PRAGMA key must be first statement, no operations before key
- [rusqlite crates.io](https://crates.io/crates/rusqlite) — v0.37 with bundled-sqlcipher-vendored-openssl, libsqlite3-sys 0.36
- Direct codebase analysis: blufio-memory (retriever.rs, store.rs, types.rs, eviction.rs), blufio-injection (classifier.rs, patterns.rs, pipeline.rs, output_screen.rs), benches (bench_memory.rs, bench_context.rs, bench_pii.rs, bench_compaction.rs), blufio-storage (database.rs, migrations V3/V11)

### Secondary (MEDIUM confidence)
- [sqlite-vec stable release blog](https://alexgarcia.xyz/blog/2024/sqlite-vec-stable-release/index.html) — performance benchmarks: <75ms for 100K x 384-dim queries
- [sqlite-vec metadata release blog](https://alexgarcia.xyz/blog/2024/sqlite-vec-metadata-release/index.html) — metadata column filtering, partition keys
- [Cisco Unicode Tag Injection](https://blogs.cisco.com/ai/understanding-and-mitigating-unicode-tag-prompt-injection) — zero-width characters, invisible Unicode attacks
- [Palo Alto Indirect Injection](https://unit42.paloaltonetworks.com/ai-agent-prompt-injection/) — tool output poisoning, MCP result injection
- [zerocopy docs.rs](https://docs.rs/zerocopy) — v0.8 IntoBytes trait (formerly AsBytes), f32 support
- [cargo-bloat GitHub](https://github.com/RazrFalcon/cargo-bloat) — binary size analysis per crate
- [Criterion.rs documentation](https://bheisler.github.io/criterion.rs/book/) — statistical benchmarking, regression detection

### Tertiary (LOW confidence)
- [sqlite-vec compiling docs](https://alexgarcia.xyz/sqlite-vec/compiling.html) — AVX/NEON compile flags (not tested with SQLCipher)
- [Google prompt injection defense blog](https://security.googleblog.com/2025/06/mitigating-prompt-injection-attacks.html) — layered defense strategy (no Blufio-specific details)

---
*Research completed: 2026-03-13*
*Ready for roadmap: yes*
