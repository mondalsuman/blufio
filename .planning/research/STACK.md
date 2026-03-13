# Stack Research: v1.6 Performance & Scalability Validation

**Domain:** sqlite-vec vector search migration, performance benchmarking suite, injection defense hardening
**Researched:** 2026-03-13
**Confidence:** MEDIUM-HIGH (sqlite-vec Rust crate verified via official docs + crates.io; SQLCipher compatibility is the one gap requiring integration testing)

## Existing Stack (DO NOT ADD -- Already in Workspace)

These are already in the workspace and cover their respective domains. Listed to prevent accidental duplication and to show which v1.6 features they serve.

| Already Have | Version | v1.6 Feature It Serves |
|---|---|---|
| `rusqlite` | 0.37 | SQLCipher connection factory, sqlite-vec registration via FFI |
| `tokio-rusqlite` | 0.7 | Async wrapper for vec0 virtual table queries |
| `criterion` | 0.5 | Performance benchmarking suite (4 benches already exist) |
| `regex` | 1 | Injection defense L1 classifier pattern expansion |
| `sysinfo` | 0.33 | Memory usage measurement in benchmarks |
| `metrics` | 0.24 | Prometheus counters for vector search latency |
| `insta` | 1 | Snapshot testing for injection pattern outputs |
| `proptest` | 1 | Property-based testing for pattern expansion |
| `tracing` | 0.1 | OTel spans for vector search operations |
| `ndarray` | 0.17 | Embedding tensor operations (existing ONNX pipeline) |
| `ring` | 0.17 | HMAC boundary tokens (existing L3 defense) |
| `hmac` | 0.12 | HMAC boundary tokens (existing L3 defense) |
| `sha2` | 0.10 | Hashing for benchmark result integrity |

---

## New Dependencies Required

### 1. sqlite-vec Integration (Vector Search Migration)

| Technology | Version | Purpose | Why Recommended |
|---|---|---|---|
| `sqlite-vec` | 0.1.6 | FFI bindings for sqlite-vec SQLite extension -- enables `vec0` virtual tables with native KNN search inside SQLite | Replaces the current in-memory brute-force `get_active_embeddings()` + Rust-side cosine similarity loop that loads ALL embeddings into memory (O(n) scan). sqlite-vec pushes vector search into the SQLite engine via `vec0` virtual tables, enabling: (1) disk-backed vector storage (no more ~15MB RAM ceiling for 10K x 384-dim embeddings), (2) KNN queries with `MATCH` + `k` clause, (3) cosine distance metric built-in (`distance_metric=cosine`), (4) metadata column support for filtering by status/classification without post-load filtering. Pure C, zero dependencies, compiles via `cc` crate at build time and statically links into the binary. |

**Critical Integration Detail -- SQLCipher Compatibility:**

The sqlite-vec crate compiles the sqlite-vec C source code via the `cc` build crate and exposes a single function `sqlite3_vec_init`. This function is registered into the already-linked SQLite engine via `rusqlite::ffi::sqlite3_auto_extension()`. Because blufio uses `bundled-sqlcipher-vendored-openssl`, the SQLite engine is SQLCipher. The `sqlite3_auto_extension` call registers the vec0 extension into whichever SQLite is linked -- it does NOT bring its own SQLite. This means sqlite-vec operates inside the SQLCipher engine, and all vec0 virtual table data is encrypted at rest along with everything else.

**Confidence: MEDIUM** -- This architecture is sound in principle (extension registration is engine-agnostic), but no explicit documentation confirms sqlite-vec + SQLCipher compatibility. An integration test must verify this during the first phase. If it fails, the fallback is calling `conn.load_extension()` or using sqlite-vec's scalar functions (`vec_distance_cosine()`) on BLOB columns without the `vec0` virtual table.

**Registration Pattern:**

```rust
use rusqlite::ffi::sqlite3_auto_extension;
use sqlite_vec::sqlite3_vec_init;

// Must be called BEFORE any connection is opened.
// Safe: sqlite3_auto_extension is thread-safe and idempotent.
unsafe {
    sqlite3_auto_extension(Some(
        std::mem::transmute(sqlite3_vec_init as *const ())
    ));
}
```

**vec0 Virtual Table Schema (replaces in-memory vector index):**

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS memories_vec USING vec0(
    memory_id TEXT PRIMARY KEY,
    embedding float[384] distance_metric=cosine
);
```

**KNN Query (replaces Rust-side cosine similarity loop):**

```sql
SELECT memory_id, distance
FROM memories_vec
WHERE embedding MATCH ?1 AND k = ?2;
```

Where `?1` is the query embedding as a binary BLOB (384 x 4 = 1536 bytes, little-endian f32) and `?2` is the max results count.

**Where this lives:** `blufio-memory` crate. The `MemoryStore` gains vec0 table management. The `HybridRetriever::vector_search()` method changes from loading all embeddings + Rust cosine scan to a single SQL KNN query.

**Migration strategy:** A new V15 migration creates the `memories_vec` virtual table and backfills from existing `memories.embedding` BLOBs. The existing `memories` table and `memories_fts` FTS5 table remain unchanged -- BM25 keyword search + RRF fusion pipeline is preserved.

### 2. Zero-Copy Vector Passing

| Technology | Version | Purpose | Why Recommended |
|---|---|---|---|
| `zerocopy` | 0.8 | Zero-copy conversion from `Vec<f32>` to byte slices for sqlite-vec queries | sqlite-vec expects vectors as compact binary BLOBs. The existing `vec_to_blob()` / `blob_to_vec()` functions in `blufio-memory/src/types.rs` copy every embedding through `flat_map(to_le_bytes)`. zerocopy's `IntoBytes` trait (formerly `AsBytes` in 0.7) enables passing `&[f32]` directly as `&[u8]` without any allocation or copy. For a 384-dim vector, this eliminates a 1536-byte allocation per query and per insert. At scale (10K+ memories), this matters. |

**Note:** zerocopy 0.8 renamed `AsBytes` to `IntoBytes` and `FromBytes` to `FromBytes` (unchanged). The derive macros now require `#[derive(IntoBytes)]` instead of `#[derive(AsBytes)]`. Since `f32` already implements `IntoBytes` in zerocopy 0.8, the usage is straightforward:

```rust
use zerocopy::IntoBytes;

// Pass Vec<f32> to sqlite-vec without copying
let embedding: &[f32] = &query_embedding;
let blob: &[u8] = embedding.as_bytes();
// blob is now a 1536-byte slice pointing to the same memory
```

**Where this lives:** `blufio-memory` crate, replacing `vec_to_blob()` calls in `store.rs` and `retriever.rs`.

**Confidence: HIGH** -- zerocopy is from the Fuchsia team (Google), 200M+ downloads, battle-tested. `f32` is guaranteed little-endian on all Blufio target platforms (x86_64, aarch64). sqlite-vec expects little-endian f32 arrays.

---

## Benchmarking Suite Additions

No new crate dependencies needed. The existing stack already covers benchmarking needs:

| Existing | Version | How It Serves v1.6 Benchmarks |
|---|---|---|
| `criterion` | 0.5 | Core benchmark framework. 4 benches already exist: `bench_context`, `bench_memory`, `bench_pii`, `bench_compaction`. Extend with sqlite-vec KNN benchmarks, binary size tracking, and injection classifier benchmarks. |
| `sysinfo` | 0.33 | RSS measurement for memory usage benchmarks (already a dependency in the binary crate). |
| `insta` | 1 | Snapshot assertions for benchmark regression detection in CI. |

### Binary Size Tracking (Development Tool, Not a Dependency)

| Tool | Purpose | Notes |
|---|---|---|
| `cargo bloat --release --crates` | Analyze per-crate binary size contribution | Install as cargo subcommand: `cargo install cargo-bloat`. Run in CI to track sqlite-vec's impact on binary size (~25MB constraint). Not a Cargo.toml dependency -- a development/CI tool. |

### New Benchmarks to Add (Using Existing criterion)

1. **`bench_vec_search`** -- KNN query via sqlite-vec vec0 virtual table at [100, 1K, 5K, 10K] memory entries
2. **`bench_injection_classify`** -- L1 classifier throughput on clean, suspicious, and adversarial inputs
3. **`bench_hybrid_retrieval`** -- Full pipeline: embed query + vec0 KNN + BM25 + RRF + MMR (end-to-end latency)
4. **`bench_binary_size`** -- Track binary size in bench_results table (not a criterion bench, but a CI check)
5. **`bench_rss_idle`** -- Measure steady-state RSS after startup with [0, 1K, 10K] memory entries loaded

**Where these live:** `crates/blufio/benches/` (extend existing bench suite). Results persisted to `bench_results` table (V11 migration already exists).

---

## Injection Defense Pattern Expansion

No new crate dependencies needed. The existing `regex` crate with `RegexSet` handles all pattern matching. The expansion is purely additive patterns.

### Pattern Categories to Add

Based on OWASP LLM Top 10 2025 (LLM01: Prompt Injection) and the OWASP Cheat Sheet for LLM Prompt Injection Prevention:

| Category | Current Count | Patterns to Add | Source |
|---|---|---|---|
| `RoleHijacking` | 4 patterns | +3: developer mode, DAN/jailbreak, hypothetical scenario | OWASP Cheat Sheet |
| `InstructionOverride` | 4 patterns | +4: `<\|endoftext\|>`, `### Instruction:`, `<<SYS>>`, `[/INST]` format tokens | OWASP Cheat Sheet + model format token research |
| `DataExfiltration` | 3 patterns | +3: URL-based exfil, markdown image injection, base64 encoding attempts | OWASP LLM01:2025 |
| `EncodingEvasion` (NEW) | 0 patterns | +3: base64 payload detection, hex-encoded instructions, Unicode homoglyph detection | OWASP Cheat Sheet "Encoding and Obfuscation" |
| `SystemPromptExtraction` (NEW) | 0 patterns | +3: "reveal your prompt", "print system message", "what are your instructions" | OWASP Cheat Sheet |

**Total expansion:** 11 patterns -> 27 patterns (+16 patterns, +2 new categories)

**Where this lives:** `blufio-injection/src/patterns.rs` (add to `PATTERNS` array). `InjectionCategory` enum gains `EncodingEvasion` and `SystemPromptExtraction` variants. No new crates needed -- `regex::RegexSet` already handles arbitrary pattern counts efficiently (single-pass DFA).

### Classifier Tuning

The scoring formula in `classifier.rs::calculate_score()` currently uses:
- Base severity per pattern (0.1 - 0.5)
- Positional bonus (up to 0.1 for early-message patterns)
- Multi-match bonus (+0.1 per additional match)

Tuning for v1.6:
- Add **category diversity bonus**: matches across 2+ distinct categories score higher than same-category matches (multi-vector attacks are more suspicious than false positives from a single category)
- Add **encoding evasion severity escalation**: any `EncodingEvasion` match boosts total score by 0.15 (encoding is always intentional)
- Verify existing thresholds (0.95 user, 0.98 MCP) remain appropriate with expanded pattern set

**No new dependencies for any of this.** The existing `regex` + `RegexSet` architecture handles the expanded pattern set identically to the current 11 patterns.

---

## What NOT to Add

| Avoid | Why | What to Do Instead |
|---|---|---|
| `sqlite-vss` (FAISS-based) | Deprecated by the sqlite-vec author. sqlite-vss requires FAISS C++ library (~200MB), incompatible with single-binary constraint. sqlite-vec replaced it. | Use `sqlite-vec` 0.1.6 |
| `qdrant-client` / `pinecone` / any external vector DB | Violates single-binary SQLite-only architecture. Adds network dependency, operational complexity. | sqlite-vec inside existing SQLite engine |
| `hnsw_rs` / `instant-distance` / any Rust ANN library | sqlite-vec's brute-force approach is fast enough for Blufio's scale (10K-50K memories, 384-dim). ANN libraries add complexity and memory overhead for marginal gain at this scale. Brute-force 384-dim KNN at 10K entries takes <10ms on commodity hardware. | sqlite-vec brute-force via `vec0` virtual table |
| `dhat-rs` (heap profiler) | Good for debugging but not for production benchmarks. Adds a custom global allocator that conflicts with `tikv-jemallocator`. | Use `sysinfo` for RSS tracking + `cargo bloat` for binary analysis |
| `pprof` / `flamegraph` in Cargo.toml | Development profiling tools should be installed as cargo subcommands, not workspace dependencies. | `cargo install flamegraph` as a dev tool |
| `aho-corasick` for injection patterns | The existing `regex::RegexSet` already uses Aho-Corasick internally for multi-pattern matching. Adding it explicitly would be redundant. | `regex::RegexSet` (already used in `patterns.rs`) |
| `vectorlite` / `sqlite-vector` | Alternative vector extensions that duplicate sqlite-vec's role. sqlite-vec is the most portable (pure C, no deps) and has first-class Rust bindings. | Use `sqlite-vec` 0.1.6 |

---

## Installation

```toml
# In workspace Cargo.toml [workspace.dependencies]
sqlite-vec = "0.1.6"
zerocopy = { version = "0.8", features = ["derive"] }

# In crates/blufio-memory/Cargo.toml [dependencies]
sqlite-vec.workspace = true
zerocopy.workspace = true
```

```bash
# Development tools (not Cargo.toml deps)
cargo install cargo-bloat
```

---

## Alternatives Considered

| Recommended | Alternative | Why Not |
|---|---|---|
| `sqlite-vec` 0.1.6 | `sqlite-vss` (FAISS) | Deprecated by same author, requires FAISS C++ (~200MB), impossible for single binary |
| `sqlite-vec` 0.1.6 | In-memory Rust cosine similarity (current) | Current approach loads ALL embeddings into memory. At 10K entries x 384 dims x 4 bytes = ~15MB just for vectors. At 100K entries = ~150MB, exceeding the 100-200MB budget. sqlite-vec is disk-backed with O(n) query but no O(n) memory |
| `sqlite-vec` 0.1.6 | `hnsw_rs` (Rust ANN) | ANN is overkill for <50K entries. Adds ~2MB binary size, complex index maintenance, and the quality/recall tradeoff is unnecessary when brute-force takes <10ms |
| `sqlite-vec` vec0 virtual table | `sqlite-vec` scalar functions only | vec0 virtual tables handle insert/delete/update automatically with metadata columns. Scalar functions (`vec_distance_cosine()`) require manual management and don't support `MATCH`+`k` KNN syntax. vec0 is the recommended approach per official docs |
| `zerocopy` 0.8 | Manual `unsafe { std::slice::from_raw_parts() }` | zerocopy provides safety guarantees, is well-audited, and the `IntoBytes` derive validates alignment at compile time. Manual unsafe is error-prone and unnecessary |
| `zerocopy` 0.8 | Keep existing `vec_to_blob()` / `blob_to_vec()` | Existing functions allocate a new Vec for every conversion. At 10K memories, each retrieval allocates ~15MB just for the vector scan. zerocopy eliminates this entirely. For the vec0 migration specifically, we still need efficient blob conversion for INSERT operations |
| Expanded regex patterns | ML-based classifier (e.g., ONNX injection model) | The existing ONNX embedding model is 15MB and takes ~50ms per inference. Adding a second model for injection classification doubles model load time and memory. Regex patterns are <1ms, deterministic, and auditable. ML classifier is a v2.0 consideration per OWASP guidance |
| `criterion` (existing) | `iai` (instruction-count benchmarks) | `iai` uses Valgrind (Linux-only, unavailable on macOS dev). criterion already works on both platforms and provides statistical analysis. Keep criterion for consistency with existing 4 benchmarks |

---

## Version Compatibility

| Package | Compatible With | Notes |
|---|---|---|
| `sqlite-vec` 0.1.6 | `rusqlite` 0.31+ | sqlite-vec lists rusqlite 0.31 as dev-dependency. Blufio uses 0.37 which is newer and compatible. The `sqlite3_auto_extension` FFI is stable across rusqlite versions. |
| `sqlite-vec` 0.1.6 | `bundled-sqlcipher-vendored-openssl` | **NEEDS INTEGRATION TEST.** The extension registers via `sqlite3_auto_extension()` into the linked SQLite engine (SQLCipher in our case). Architecture is sound but no explicit documentation confirms this combination. Test in phase 1. |
| `zerocopy` 0.8 | Rust 1.85+ | zerocopy 0.8 MSRV is 1.56. Workspace MSRV is 1.85. No conflict. |
| `sqlite-vec` 0.1.6 | SQLite 3.51.1 (via libsqlite3-sys 0.36) | sqlite-vec is pure C extension compiled against standard SQLite API. Compatible with any SQLite 3.x. |
| `sqlite-vec` 0.1.6 | FTS5 (existing `memories_fts`) | vec0 and FTS5 are independent virtual table types. Both can coexist in the same database. The hybrid retrieval pipeline queries both and fuses via RRF. |

---

## Stack Patterns by Variant

**If sqlite-vec + SQLCipher integration test passes (expected):**
- Register via `sqlite3_auto_extension` before any connection opens
- Create `memories_vec` virtual table in V15 migration
- Replace `get_active_embeddings()` + Rust cosine scan with `MATCH` KNN query
- Keep `memories_fts` for BM25, fuse results with existing RRF pipeline
- Use zerocopy for zero-copy blob passing in queries

**If sqlite-vec + SQLCipher integration test FAILS (fallback):**
- Skip `vec0` virtual table approach
- Use sqlite-vec's scalar functions: `vec_distance_cosine(embedding, ?1)` in ORDER BY
- Store embeddings in existing `memories.embedding` BLOB column (no change)
- Still eliminates loading ALL embeddings into memory (query pushes distance computation to SQL)
- Loses `MATCH` + `k` syntax but gains the same core benefit (disk-backed vector search)

**If 10K entries proves too slow for brute-force (<100ms target not met):**
- sqlite-vec supports binary quantization (`bit[384]` instead of `float[384]`)
- Reduces storage 32x and query time ~10x at the cost of some recall quality
- Add quantized shadow table: `memories_vec_quantized` with `bit[384]`
- Use quantized table for initial top-100 candidates, rescore with full float vectors
- This is a future optimization -- benchmarks will determine if it's needed

---

## Sources

- [sqlite-vec official Rust documentation](https://alexgarcia.xyz/sqlite-vec/rust.html) -- Integration method, `sqlite3_vec_init`, Cargo setup (MEDIUM confidence)
- [sqlite-vec GitHub repository](https://github.com/asg017/sqlite-vec) -- v0.1.6 features, vec0 virtual table syntax, distance metrics (HIGH confidence)
- [sqlite-vec KNN documentation](https://alexgarcia.xyz/sqlite-vec/features/knn.html) -- MATCH clause, distance_metric=cosine, k parameter (HIGH confidence)
- [sqlite-vec crates.io](https://crates.io/crates/sqlite-vec) -- Version 0.1.6 stable, 0.1.7-alpha.10 latest alpha (HIGH confidence)
- [sqlite-vec docs.rs](https://docs.rs/crate/sqlite-vec/latest) -- API: single `sqlite3_vec_init` function, cc build dep (HIGH confidence)
- [sqlite-vec compiling documentation](https://alexgarcia.xyz/sqlite-vec/compiling.html) -- SQLITE_VEC_ENABLE_AVX, SQLITE_VEC_ENABLE_NEON compile flags (MEDIUM confidence)
- [sqlite-vec stable release blog post](https://alexgarcia.xyz/blog/2024/sqlite-vec-stable-release/index.html) -- Performance benchmarks: <75ms for 100K x 384-dim queries (MEDIUM confidence)
- [rusqlite crates.io](https://crates.io/crates/rusqlite) -- v0.37 with bundled-sqlcipher-vendored-openssl, libsqlite3-sys 0.36.0, SQLite 3.51.1 (HIGH confidence)
- [OWASP LLM Prompt Injection Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/LLM_Prompt_Injection_Prevention_Cheat_Sheet.html) -- 13 attack types, regex patterns, fuzzy matching, encoding detection (HIGH confidence)
- [OWASP LLM01:2025 Prompt Injection](https://genai.owasp.org/llmrisk/llm01-prompt-injection/) -- Attack taxonomy, mitigation strategies (HIGH confidence)
- [zerocopy docs.rs](https://docs.rs/zerocopy) -- v0.8 API, IntoBytes trait (formerly AsBytes), f32 support (HIGH confidence)
- [cargo-bloat GitHub](https://github.com/RazrFalcon/cargo-bloat) -- Binary size analysis, per-crate breakdown (HIGH confidence)

---
*Stack research for: v1.6 Performance & Scalability Validation*
*Researched: 2026-03-13*
