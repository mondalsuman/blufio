# Architecture Patterns

**Domain:** v1.6 Performance & Scalability Validation -- sqlite-vec Migration, Benchmarking Suite, Injection Hardening
**Researched:** 2026-03-13
**Confidence:** HIGH (direct analysis of blufio-memory/retriever.rs, store.rs, types.rs; blufio-injection/classifier.rs, patterns.rs, pipeline.rs; 4 existing bench files; sqlite-vec API docs; OWASP LLM Top 10 2025)

## Recommended Architecture

Three integration areas into the existing 37-crate workspace, each touching different subsystems with minimal cross-dependency:

```
Feature 1: sqlite-vec
  blufio-memory/store.rs    -- vec0 virtual table + KNN queries replace get_active_embeddings()
  blufio-memory/retriever.rs -- vector_search() delegates to store instead of in-memory scan
  blufio-storage/migrations/ -- V15 migration creates vec0 shadow table
  Cargo.toml (workspace)     -- sqlite-vec dependency

Feature 2: Performance Benchmarking
  crates/blufio/benches/     -- new bench_injection.rs, expanded bench_memory.rs
  .github/workflows/bench.yml -- binary size + RSS tracking
  crates/blufio/src/bench_cmd.rs -- CLI bench subcommand enhancements

Feature 3: Injection Hardening
  blufio-injection/patterns.rs   -- expanded PATTERNS array
  blufio-injection/classifier.rs -- encoding detection, fuzzy matching
  blufio-injection/output_screen.rs -- expanded credential patterns
```

### Component Boundaries

| Component | Responsibility | Modified/New | Communicates With |
|-----------|---------------|--------------|-------------------|
| `blufio-memory/store.rs` | MODIFIED: Add vec0 table management, KNN query method | Modified | retriever.rs, blufio-storage (migrations) |
| `blufio-memory/retriever.rs` | MODIFIED: Replace in-memory cosine scan with store.knn_search() | Modified | store.rs |
| `blufio-memory/types.rs` | UNCHANGED: cosine_similarity() kept for MMR reranking | Unchanged | retriever.rs |
| `blufio-storage/migrations/V15` | NEW: Create vec0 virtual table shadowing memories.embedding | New | SQLite schema |
| `blufio-injection/patterns.rs` | MODIFIED: Expand PATTERNS array with new categories | Modified | classifier.rs |
| `blufio-injection/classifier.rs` | MODIFIED: Add encoding detection, fuzzy matching | Modified | patterns.rs, pipeline.rs |
| `blufio-injection/output_screen.rs` | MODIFIED: Expand credential patterns, PII sharing | Modified | classifier.rs |
| `crates/blufio/benches/bench_injection.rs` | NEW: Criterion benchmarks for injection pipeline | New | blufio-injection |
| `crates/blufio/benches/bench_memory.rs` | MODIFIED: Add sqlite-vec KNN benchmark group | Modified | blufio-memory |

### Data Flow Changes

**Current vector search flow (in-memory, O(n)):**
```
query -> embedder.embed() -> store.get_active_embeddings() -> [load ALL embeddings into memory]
  -> cosine_similarity() loop over all -> filter threshold -> sort -> truncate(k)
```

**New vector search flow (sqlite-vec, indexed KNN):**
```
query -> embedder.embed() -> store.knn_search(embedding, k) -> [sqlite-vec vec0 MATCH query]
  -> returns (id, distance) pairs directly from index -> no full table scan
```

**Key change:** `get_active_embeddings()` is no longer called during search. The vec0 virtual table maintains its own internal index that provides O(n*log(n)) KNN search without loading all embeddings into Rust process memory.

**What stays the same:**
- BM25 search via FTS5 (unchanged)
- RRF fusion of vector + BM25 results (unchanged)
- Temporal decay, importance boost, MMR reranking (unchanged)
- cosine_similarity() used in MMR reranking (unchanged -- operates on already-loaded Memory structs)
- Embedding storage as BLOB in memories table (unchanged -- vec0 is a shadow/parallel index)

## Integration Detail: sqlite-vec

### Extension Loading with tokio-rusqlite

The project uses `tokio-rusqlite` 0.7 wrapping `rusqlite` 0.37 with `bundled-sqlcipher-vendored-openssl`. sqlite-vec must be loaded as a compile-time extension via `sqlite3_auto_extension`, which registers the extension globally before any connection is opened.

**Critical constraint:** The `sqlite3_auto_extension` call must happen once, before any `Connection::open()`. The current architecture has a centralized connection factory in blufio-storage. The auto_extension call goes there.

```rust
// In blufio-storage or blufio-memory initialization (once, at startup)
use sqlite_vec::sqlite3_vec_init;
use rusqlite::ffi::sqlite3_auto_extension;

unsafe {
    sqlite3_auto_extension(Some(std::mem::transmute(
        sqlite3_vec_init as *const ()
    )));
}
```

**Compatibility note:** sqlite-vec 0.1.6 depends on `rusqlite ^0.31`. The project uses rusqlite 0.37. The sqlite-vec crate only uses rusqlite's FFI types (`sqlite3_auto_extension`), so the actual API surface is the raw C `sqlite3` pointer, which is stable. This should work but MUST be verified at compile time. If the crate version pins conflict, the `sqlite3_vec_init` function pointer can be obtained directly from the C library compiled alongside SQLCipher -- sqlite-vec is pure C with zero dependencies.

### vec0 Virtual Table Design

```sql
-- V15 migration: Create vec0 virtual table for vector search
CREATE VIRTUAL TABLE IF NOT EXISTS memories_vec USING vec0(
    memory_id TEXT PRIMARY KEY,
    embedding float[384] distance_metric=cosine,
    status TEXT
);
```

**Design decisions:**

1. **Separate table, not replacing memories.embedding BLOB:** The BLOB column stays because (a) MMR reranking in retriever.rs reads embeddings from Memory structs, (b) GDPR erasure/export operates on the memories table directly, (c) backward compatibility for existing data. The vec0 table is an index, not a replacement.

2. **`memory_id TEXT PRIMARY KEY`:** Maps to `memories.id`. Required for joining back to the main table after KNN search.

3. **`status TEXT` as metadata column:** sqlite-vec metadata columns support WHERE clause filtering during KNN. This lets us filter `status = 'active'` directly in the vector search without a post-filter JOIN. This is the key performance win -- the current code loads ALL active embeddings, even if we only need top-k.

4. **`distance_metric=cosine`:** Matches the existing cosine similarity search. sqlite-vec returns distance (1 - similarity), so results need `similarity = 1.0 - distance` conversion.

5. **No classification metadata column:** The `classification != 'restricted'` filter could be a metadata column, but this adds complexity. Instead, post-filter restricted results after the KNN join. With typical memory counts (<10K), the KNN result set is small enough that post-filtering is negligible.

6. **No partition key:** Partition keys are for datasets in the millions. Memory tables at target scale (100-10K entries) don't need partitioning.

### Sync Strategy

The vec0 table must stay in sync with the memories table. Two approaches:

**Option A: Dual-write in MemoryStore methods (RECOMMENDED)**
- `save()`: INSERT into memories + INSERT into memories_vec
- `soft_delete()`: UPDATE memories status + UPDATE memories_vec status
- `supersede()`: UPDATE memories status + UPDATE memories_vec status
- `batch_evict()`: DELETE from memories + DELETE from memories_vec

**Why not triggers:** SQLite triggers cannot INSERT into vec0 virtual tables (vec0 uses a custom virtual table module that intercepts INSERT/UPDATE/DELETE directly). The trigger-based approach that works for FTS5 does not work for vec0.

**Migration for existing data:** The V15 migration creates the table. A one-time backfill query populates it:
```sql
INSERT INTO memories_vec(memory_id, embedding, status)
SELECT id, embedding, status FROM memories
WHERE deleted_at IS NULL;
```
This runs in the migration or as a post-migration step on first startup.

### Modified store.rs API

```rust
impl MemoryStore {
    /// KNN vector search via sqlite-vec vec0 table.
    /// Returns (memory_id, distance) pairs for the k nearest neighbors
    /// among active memories.
    pub async fn knn_search(
        &self,
        query_embedding: &[f32],
        k: usize,
    ) -> Result<Vec<(String, f32)>, BlufioError> {
        let embedding_blob = vec_to_blob(query_embedding);
        let limit = k as i64;
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT memory_id, distance
                     FROM memories_vec
                     WHERE embedding MATCH ?1
                       AND k = ?2
                       AND status = 'active'"
                )?;
                let results = stmt
                    .query_map(rusqlite::params![embedding_blob, limit], |row| {
                        let id: String = row.get(0)?;
                        let distance: f32 = row.get(1)?;
                        Ok((id, distance))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(results)
            })
            .await
            .map_err(storage_err)
    }
}
```

### Modified retriever.rs

```rust
/// Vector search via sqlite-vec KNN (replaces in-memory cosine scan).
/// Returns (id, similarity) pairs, sorted by similarity descending.
async fn vector_search(
    &self,
    query_embedding: &[f32],
) -> Result<Vec<(String, f32)>, BlufioError> {
    let knn_results = self.store.knn_search(
        query_embedding,
        self.config.max_retrieval_results,
    ).await?;

    // Convert distance to similarity: cosine distance = 1 - cosine similarity
    let results: Vec<(String, f32)> = knn_results
        .into_iter()
        .filter_map(|(id, distance)| {
            let similarity = 1.0 - distance;
            if similarity >= self.config.similarity_threshold as f32 {
                Some((id, similarity))
            } else {
                None
            }
        })
        .collect();

    Ok(results)
}
```

**What is removed:** `get_active_embeddings()` is no longer called in the search hot path. It remains available for other uses (validation, export) but the O(n) full-table scan is eliminated from retrieval.

## Integration Detail: Performance Benchmarking Suite

### Existing Infrastructure

The project already has:
- 4 criterion bench files: `bench_memory.rs`, `bench_context.rs`, `bench_compaction.rs`, `bench_pii.rs`
- `bench_results` SQLite table (V11 migration) for storing benchmark results
- `.github/workflows/bench.yml` with >20% regression detection
- `cargo bench -p blufio` as the bench entry point

### New Benchmarks to Add

**1. bench_injection.rs (NEW file)**
```
Benchmark groups:
- injection_classify: RegexSet fast-path + detail extraction at 1KB/5KB/10KB input sizes
- injection_score: calculate_score() with 1/3/5/10 matches
- injection_pipeline: Full pipeline.scan_input() with clean/suspicious/hostile inputs
- injection_credential: OutputScreener.check_credentials() with mixed content
- injection_output_screen: Full screen_tool_args() with various arg sizes
```

**2. bench_memory.rs (MODIFIED -- add vec group)**
```
New benchmark groups:
- memory_knn_search: sqlite-vec KNN at 100/500/1000/5000 memories
  (requires in-memory SQLite with sqlite-vec loaded)
- memory_vec_insert: vec0 INSERT throughput at batch sizes
- memory_vec_sync: dual-write (memories + memories_vec) vs single-write overhead
```

**3. Binary size and RSS tracking (bench.yml enhancement)**
```yaml
# After benchmarks run:
- name: Measure binary size
  run: |
    cargo build --release -p blufio
    ls -la target/release/blufio | awk '{print $5}' > binary-size.txt
    echo "Binary size: $(cat binary-size.txt) bytes"

- name: Measure peak RSS (optional, Linux only)
  run: |
    /usr/bin/time -v target/release/blufio doctor 2>&1 | \
      grep "Maximum resident" | awk '{print $NF}' > peak-rss.txt || true
```

### Benchmark Data Strategy

The `bench_results` table (V11) already exists. The CLI `blufio bench` command can store results there. The new benchmarks should:
1. Run via `cargo bench -p blufio` (criterion, for CI regression detection)
2. Optionally store results via `blufio bench store` (for historical tracking in SQLite)
3. Compare against baselines via `blufio bench compare` (for operator visibility)

## Integration Detail: Injection Defense Hardening

### Current State

The injection defense system has:
- 11 patterns across 3 categories (RoleHijacking, InstructionOverride, DataExfiltration)
- Two-phase detection: RegexSet fast path + individual Regex detail extraction
- Confidence scoring: severity + positional bonus + multi-match bonus
- 5-layer pipeline: L1 classifier, L3 HMAC boundaries, L4 output screening, L5 HITL
- Custom patterns via TOML config

### Expansion Areas

**New injection categories to add to `InjectionCategory` enum:**

```rust
pub enum InjectionCategory {
    RoleHijacking,           // existing
    InstructionOverride,     // existing
    DataExfiltration,        // existing
    EncodingObfuscation,     // NEW: base64, hex, rot13 encoded instructions
    DelimiterManipulation,   // NEW: markdown/HTML delimiters to create false boundaries
    IndirectInjection,       // NEW: patterns found in tool output (MCP/WASM returns)
}
```

**New patterns to add to PATTERNS array (expanding from 11 to ~25-30):**

| Category | Pattern | Severity | Rationale |
|----------|---------|----------|-----------|
| RoleHijacking | `(?i)act\s+as\s+(if\s+)?you'?re?\s+not\s+bound` | 0.4 | DAN-style unbounding |
| RoleHijacking | `(?i)pretend\s+(you\s+are|to\s+be)\s+` | 0.3 | Persona hijacking |
| RoleHijacking | `(?i)(?:from\s+now\s+on|henceforth)\s+you\s+(will|shall|must)\s+` | 0.3 | Future-tense role override |
| InstructionOverride | `(?i)(?:developer|admin|debug)\s+mode` | 0.4 | Mode switching attacks |
| InstructionOverride | `(?i)reveal\s+(your|the)\s+(system\s+)?prompt` | 0.4 | System prompt extraction |
| InstructionOverride | `(?i)<\|(?:system|user|assistant)\|>` | 0.4 | ChatML delimiter injection |
| InstructionOverride | `(?i)<<\s*SYS\s*>>` | 0.4 | Llama-style system delimiter |
| InstructionOverride | `(?i)\[/?SYSTEM\]` | 0.3 | Bracketed system tag |
| DataExfiltration | `(?i)(?:fetch|load|visit|open)\s+(https?://|ftp://)` | 0.3 | URL-triggered exfiltration |
| DataExfiltration | `(?i)(?:include|import|require|fetch)\s+(?:the\s+)?(?:file|url|link)` | 0.2 | Resource inclusion |
| DataExfiltration | `(?i)(?:curl|wget|http\s+get)\s+` | 0.3 | Command-line exfiltration |
| DelimiterManipulation | `(?i)```(?:system|instruction|admin)` | 0.3 | Code block false boundary |
| DelimiterManipulation | `(?i)---\s*(?:new\s+)?(?:system|instruction)` | 0.3 | Markdown HR as delimiter |
| EncodingObfuscation | `(?i)(?:base64|b64)\s*(?:decode|:\s)` | 0.2 | Explicit encoding reference |

**Encoding detection (new capability):**

```rust
/// Check if input contains base64-encoded injection patterns.
/// Scans for base64 segments, decodes them, and re-classifies the decoded content.
fn check_encoded_content(input: &str) -> Vec<InjectionMatch> {
    // Find base64 candidate segments (40+ chars of [A-Za-z0-9+/=])
    // Decode each segment
    // Run classifier on decoded text
    // If decoded text triggers patterns, return matches with EncodingObfuscation category
}
```

**Implementation approach:** Add this as an optional second pass after the RegexSet fast path. Only runs if the fast path finds zero matches AND the input contains suspicious characteristics (long alphanumeric sequences suggesting encoding). This avoids performance overhead on clean inputs.

### Credential Pattern Expansion (output_screen.rs)

Current credential patterns: 6 (Anthropic, OpenAI project, OpenAI, AWS, database URI, Bearer token).

**New patterns to add:**

| Name | Pattern | Rationale |
|------|---------|-----------|
| `google_api_key` | `AIza[0-9A-Za-z_-]{35}` | Google/Gemini API keys |
| `github_token` | `gh[ps]_[A-Za-z0-9]{36,}` | GitHub personal/secret tokens |
| `slack_token` | `xox[baprs]-[0-9a-zA-Z-]{10,}` | Slack bot/app tokens |
| `stripe_key` | `(?:sk\|pk)_(?:test\|live)_[0-9a-zA-Z]{24,}` | Stripe API keys |
| `jwt_token` | `eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+` | JWT tokens (three-part base64) |
| `private_key_block` | `-----BEGIN (?:RSA |EC |DSA )?PRIVATE KEY-----` | PEM private key headers |

### PII Pattern Sharing (Cross-Crate)

The existing architecture already has PII detection shared between `blufio-security::pii` and `blufio-injection::output_screen.rs`. The L4 output screener calls `detect_pii()` from blufio-security. This pattern is already correctly wired (Phase 64 completed this integration). No architectural change needed -- just expand the pattern sets in their respective crates.

## Patterns to Follow

### Pattern 1: Dual-Write Sync (vec0 + memories)
**What:** Every MemoryStore write operation updates both the memories table and the memories_vec virtual table within the same tokio-rusqlite `call()` closure.
**When:** Any mutation to memories that affects embedding or status.
**Why:** SQLite triggers don't work with vec0 virtual tables. Keeping both writes in the same closure ensures atomicity through tokio-rusqlite's single-writer thread.
```rust
self.conn.call(move |conn| {
    let tx = conn.transaction()?;
    tx.execute("INSERT INTO memories ...", params![...])?;
    tx.execute("INSERT INTO memories_vec(memory_id, embedding, status) VALUES (?1, ?2, ?3)",
        params![id, embedding_blob, "active"])?;
    tx.commit()?;
    Ok(())
}).await
```

### Pattern 2: Lazy Extension Loading
**What:** Register sqlite-vec via `sqlite3_auto_extension` once at process startup, before any connection is opened.
**When:** In the storage/memory initialization path.
**Why:** `auto_extension` is global and must precede connection creation. Calling it multiple times is safe (idempotent) but unnecessary.

### Pattern 3: Feature-Gated Benchmarks
**What:** Benchmarks that need sqlite-vec loaded should gate on test infrastructure, not production features.
**When:** bench_memory.rs KNN benchmarks.
**Why:** Criterion benchmarks run in the test profile. The sqlite-vec extension must be loaded in the bench harness setup, not via the production auto_extension path.

### Pattern 4: Pattern Array as Single Source of Truth
**What:** All injection patterns defined in one `PATTERNS` array, with RegexSet and individual Regex compiled from the same source (existing pattern from patterns.rs).
**When:** Adding new injection patterns.
**Why:** Prevents index mismatch between RegexSet (fast path) and individual Regex (detail extraction). This is the same architecture used by `blufio-security::pii`.

## Anti-Patterns to Avoid

### Anti-Pattern 1: Replacing memories.embedding BLOB with vec0-only storage
**What:** Removing the embedding BLOB column from the memories table and storing embeddings only in vec0.
**Why bad:** (a) MMR reranking in retriever.rs reads embeddings from Memory structs to compute pairwise similarity. If embeddings are only in vec0, every MMR step requires a vec0 query. (b) GDPR export needs to include embeddings. (c) Memory validation uses embeddings for duplicate detection. (d) vec0 is an extension -- if it fails to load, the system becomes non-functional instead of degrading to the current in-memory search.
**Instead:** Keep the BLOB column. vec0 is an index, like FTS5 is an index for content.

### Anti-Pattern 2: Loading all embeddings for benchmarking
**What:** Benchmarking KNN by loading all embeddings into Rust and measuring cosine similarity.
**Why bad:** This benchmarks the OLD path, not the new sqlite-vec path. The point of sqlite-vec is to avoid loading all embeddings.
**Instead:** Benchmark the actual `store.knn_search()` path that hits sqlite-vec's internal index.

### Anti-Pattern 3: Encoding detection on every input
**What:** Running base64/hex decode + re-classify on every user message.
**Why bad:** 99%+ of inputs are clean. Decoding is expensive relative to regex. This adds latency to every message.
**Instead:** Only run encoding detection when (a) the primary classifier finds zero matches AND (b) heuristics detect long alphanumeric segments suggesting encoding.

### Anti-Pattern 4: Failing CI on any benchmark regression
**What:** Setting the regression threshold to 0% or 5%.
**Why bad:** Criterion measurements have natural variance, especially in CI (shared runners, variable load). Tight thresholds cause flaky failures that erode trust in the CI signal.
**Instead:** Keep the existing 20% threshold for CI failure. Log all regressions (even small ones) as warnings for human review.

## Scalability Considerations

| Concern | Current (in-memory) | With sqlite-vec (100-1K memories) | At 10K memories | At 100K memories |
|---------|---------------------|----------------------------------|-----------------|------------------|
| Vector search latency | Load all + O(n) scan: ~1-5ms at 500 | KNN query: <1ms | KNN query: ~1-5ms | KNN query: ~5-20ms |
| Memory usage for search | All embeddings in RAM: ~750KB at 500 (384 * 4B * 500) | Near zero (SQLite manages) | Near zero | Near zero |
| Insert overhead | 1 write | 2 writes (memories + vec0) | Same | Same |
| Cold start | No index build | vec0 index in SQLite file | Same | Same |
| BM25 search | FTS5 (unchanged) | FTS5 (unchanged) | FTS5 (unchanged) | FTS5 (unchanged) |
| Injection scan | 11 patterns: <0.1ms | ~25 patterns: <0.2ms | N/A | N/A |

## Build Order and Dependencies

The three features have minimal cross-dependency. Suggested build order:

```
Phase 1: sqlite-vec Integration
  1a. Add sqlite-vec dependency to workspace Cargo.toml
  1b. Register extension in storage/memory initialization
  1c. V15 migration: CREATE vec0 table + backfill existing data
  1d. Add knn_search() to MemoryStore
  1e. Add dual-write to save(), soft_delete(), supersede(), batch_evict()
  1f. Replace vector_search() in retriever.rs to use knn_search()
  1g. Verify: existing tests pass, new KNN tests pass

Phase 2: Injection Defense Hardening
  2a. Add new categories to InjectionCategory enum
  2b. Expand PATTERNS array (~15 new patterns)
  2c. Add encoding detection (base64/hex decode + re-classify)
  2d. Expand credential patterns in output_screen.rs (~6 new patterns)
  2e. Update tests for all new patterns
  2f. Verify: existing tests pass, new pattern tests pass

Phase 3: Performance Benchmarking Suite
  3a. Add bench_injection.rs (criterion benchmarks for injection pipeline)
  3b. Add KNN benchmark group to bench_memory.rs
  3c. Enhance bench.yml with binary size + RSS tracking
  3d. Run full benchmark suite, establish baselines
  3e. Verify: no >20% regressions from sqlite-vec or pattern expansion

Dependencies:
  Phase 3 depends on Phase 1 (KNN benchmarks need sqlite-vec)
  Phase 3 depends on Phase 2 (injection benchmarks need expanded patterns)
  Phase 1 and Phase 2 are independent of each other
```

## Sources

- [sqlite-vec official documentation](https://alexgarcia.xyz/sqlite-vec/) - HIGH confidence
- [sqlite-vec Rust integration guide](https://alexgarcia.xyz/sqlite-vec/rust.html) - HIGH confidence
- [sqlite-vec API reference](https://alexgarcia.xyz/sqlite-vec/api-reference.html) - HIGH confidence
- [sqlite-vec KNN query documentation](https://alexgarcia.xyz/sqlite-vec/features/knn.html) - HIGH confidence
- [sqlite-vec metadata columns blog](https://alexgarcia.xyz/blog/2024/sqlite-vec-metadata-release/index.html) - HIGH confidence
- [sqlite-vec GitHub repository](https://github.com/asg017/sqlite-vec) - HIGH confidence
- [sqlite-vec crate on crates.io](https://crates.io/crates/sqlite-vec) (v0.1.6) - HIGH confidence
- [OWASP LLM Top 10 2025 - Prompt Injection](https://genai.owasp.org/llmrisk/llm01-prompt-injection/) - HIGH confidence
- [OWASP LLM Prompt Injection Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/LLM_Prompt_Injection_Prevention_Cheat_Sheet.html) - HIGH confidence
- [criterion.rs GitHub](https://github.com/bheisler/criterion.rs) - HIGH confidence
- [Bencher - Track Criterion benchmarks in CI](https://bencher.dev/learn/track-in-ci/rust/criterion/) - MEDIUM confidence
- Direct code analysis of blufio-memory (retriever.rs, store.rs, types.rs, embedder.rs, eviction.rs) - HIGH confidence
- Direct code analysis of blufio-injection (classifier.rs, patterns.rs, pipeline.rs, output_screen.rs, config.rs) - HIGH confidence
- Direct analysis of bench_memory.rs, bench_context.rs, bench_compaction.rs, bench_pii.rs - HIGH confidence
- Direct analysis of V3, V11 migrations and bench.yml CI workflow - HIGH confidence
