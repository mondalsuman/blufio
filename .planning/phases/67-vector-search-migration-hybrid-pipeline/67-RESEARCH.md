# Phase 67: Vector Search Migration & Hybrid Pipeline - Research

**Researched:** 2026-03-14
**Domain:** SQLite vec0 migration, hybrid retrieval parity, partial JOIN elimination
**Confidence:** HIGH

## Summary

Phase 67 is primarily a **validation and integration completion** phase, not a greenfield implementation. The heavy lifting (vec0 schema, dual-write, batch populate, search, fallback) was completed in Phase 65. Phase 67's core work is:

1. **Wiring vec0 into the startup path** -- the `initialize_memory()` function currently creates `MemoryStore::new(conn)` without vec0 enablement or population. This must be changed to use `MemoryStore::with_vec0()` and call `populate_vec0()` at startup when `config.memory.vec0_enabled` is true.
2. **Partial JOIN elimination** -- vec0 search already returns auxiliary data (content, source, confidence, created_at); the retriever pipeline currently discards this and re-fetches via `get_memories_by_ids()`. The optimization is to carry vec0 auxiliary data through the pipeline for scoring (importance boost, temporal decay), only falling back to `get_memories_by_ids()` for MMR embeddings.
3. **Parity validation** -- comprehensive tests proving vec0 and in-memory paths produce functionally identical hybrid retrieval results.
4. **Default flip** -- changing `vec0_enabled` serde default from `false` to `true`.

**Primary recommendation:** Structure work as: (1) startup wiring + migration validation, (2) retriever optimization with partial JOIN elimination, (3) parity test suite, (4) config default flip + doctor validation.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions
- Migration trigger: Automatic at startup when vec0_enabled=true (reuses existing `vec0_populate_batch()`)
- Migration blocks startup -- queries wait until complete, guarantees first query has complete vec0 data
- Per-batch info logging: "Migrating vec0: 500/2000 memories..." (already implemented in Phase 65)
- No dedicated CLI migration command -- operator flips `vec0_enabled=true` in TOML and restarts
- Retry from where it left off -- migration is idempotent (LEFT JOIN skips existing rows)
- Indefinite retry on restart -- no max retry counter, operator can disable vec0 toggle if stuck
- No rollback needed -- partial state is valid, interrupted migration resumes on next startup
- Keep memories.embedding BLOB column after migration -- dual-write continues
- Enables fallback to in-memory path if needed (toggle is permanent per Phase 65)
- Storage cost accepted (~1.5KB/row for 384-dim f32 vectors)
- Reuse existing `Vec0PopulationComplete` event (already emits count + duration_ms from Phase 65)
- No new event variant needed for migration -- semantically identical to population
- `blufio doctor` verifies migration completeness: sync drift check between memories count and vec0 count
- Vec0 auxiliary columns provide content, source, confidence, created_at for scoring steps
- Still fetch embeddings from memories table for MMR diversity reranking (384-dim vectors)
- Optimization: only fetch embedding column from memories (not full row) when vec0 provides other fields
- "Functionally identical" = same memory IDs in top-K results, with similarity scores within 0.01 tolerance
- Order may vary within tied scores (f32 precision differences between vec0 cosine distance and Rust cosine_similarity)
- Parity test: run same query through both vec0 and in-memory paths, compare result ID sets and score ranges
- Test at multiple scales: 10, 100, 1K entries
- After Phase 67 validation, vec0_enabled defaults to true for new installs
- Existing installs keep their current setting (no forced migration)
- Toggle remains permanent -- operators can always disable vec0 and fall back to in-memory

### Claude's Discretion
- Whether to read embedding back from vec0 (if sqlite-vec supports it) vs always fetching from memories
- Exact parity test implementation and fixture design
- Integration test structure and module organization
- Whether partial JOIN elimination is worth a separate retriever code path or just optimizing get_memories_by_ids
- Config default value change mechanism (serde default vs migration)

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope

</user_constraints>

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| VEC-04 | Existing BLOB embeddings migrate to vec0 virtual table via batched migration (500-row chunks) with rollback strategy | Startup wiring of `populate_vec0()` -- migration infrastructure already exists from Phase 65; this phase wires it into the `initialize_memory()` startup path |
| VEC-05 | Hybrid retrieval (BM25 + vec0 KNN + RRF fusion + temporal decay + importance boost + MMR diversity) preserved and functionally identical to pre-migration | Parity test suite comparing full pipeline output between vec0 and in-memory paths at 10/100/1K scales |
| VEC-06 | Eviction (batch_evict) and soft-delete operations sync across both memories and vec0 tables in single transaction | Already implemented in Phase 65 (`store.rs` dual-write transactions); Phase 67 validates with parity tests |
| VEC-07 | vec0 partition key by session_id enables faster within-session vector search | Already implemented in Phase 65 schema (`session_id text partition key`); Phase 67 validates with session-filtered search tests |
| VEC-08 | vec0 auxiliary columns eliminate JOIN to memories table for search result retrieval (single-query path) | Partial JOIN elimination: auxiliary columns provide metadata for scoring; embeddings still fetched from memories for MMR |

</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| sqlite-vec | 0.1.6 | vec0 virtual table for KNN search | Already integrated in Phase 65 |
| rusqlite | existing | SQLite binding with transaction support | Project standard |
| tokio-rusqlite | existing | Async SQLite via single-writer thread | Project pattern |
| chrono | existing | Timestamp parsing for temporal decay | Project standard |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tracing | existing | Logging migration progress | Startup migration logging |
| metrics | existing | Prometheus gauges for vec0 row count | Post-migration metrics |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Separate migration tool | Startup-embedded migration | Startup migration is simpler, no new binary/command needed (user decision: locked) |
| Full JOIN elimination | Partial elimination | MMR needs raw embeddings which vec0 cannot efficiently return for pairwise similarity |

## Architecture Patterns

### Recommended Code Changes

```
crates/blufio-memory/src/
  retriever.rs     # Modify vec0_vector_search to carry auxiliary data through pipeline
  store.rs         # Add optimized get_embeddings_by_ids (embedding-only fetch)
  vec0.rs          # No changes needed (already complete)

crates/blufio/src/
  serve/storage.rs # Wire vec0_enabled into initialize_memory, call populate_vec0 at startup

crates/blufio-config/src/
  model.rs         # Change vec0_enabled default from false to true
```

### Pattern 1: Startup Migration Wiring

**What:** Connect vec0 population to the `initialize_memory()` function
**When to use:** When `config.memory.vec0_enabled` is true
**Key insight:** The `initialize_memory()` function currently creates `MemoryStore::new(conn)` which always sets `vec0_enabled: false`. It must switch to `MemoryStore::with_vec0()`.

```rust
// In serve/storage.rs::initialize_memory()
// BEFORE (current):
let memory_store = Arc::new(MemoryStore::new(memory_conn));

// AFTER (Phase 67):
let vec0_enabled = config.memory.vec0_enabled;
if vec0_enabled {
    blufio_memory::vec0::ensure_sqlite_vec_registered();
}
let memory_store = Arc::new(MemoryStore::with_vec0(
    memory_conn,
    None,  // event_bus wired later
    vec0_enabled,
));

// After store creation, before retriever creation:
if vec0_enabled {
    info!("starting vec0 migration/population...");
    match memory_store.populate_vec0().await {
        Ok((populated, total)) => {
            info!(populated, total, "vec0 population complete");
        }
        Err(e) => {
            warn!(error = %e, "vec0 population failed, falling back to in-memory search");
            // Note: retriever fallback handles this at query time
        }
    }
}
```

### Pattern 2: Partial JOIN Elimination via Vec0 Auxiliary Data

**What:** Carry vec0 search result metadata through the retriever pipeline to avoid re-fetching from memories table
**When to use:** When vec0 is enabled and provides content, source, confidence, created_at
**Key insight:** The current flow discards `Vec0SearchResult` rich data (content, source, confidence, created_at) and only keeps `(memory_id, similarity)`, then re-fetches everything via `get_memories_by_ids()`. The optimization carries auxiliary data forward for scoring, only fetching embeddings for MMR.

```rust
// New struct to carry vec0 metadata through the pipeline
struct Vec0ScoringData {
    memory_id: String,
    similarity: f32,
    content: String,
    source: String,
    confidence: f64,
    created_at: String,
}

// In retriever.rs, when vec0_enabled:
// 1. vec0_vector_search returns Vec0ScoringData instead of (String, f32)
// 2. After RRF fusion, build ScoredMemory from vec0 data + importance/decay
// 3. Only call get_embeddings_by_ids() for the final MMR step (embedding-only)
```

### Pattern 3: Embedding-Only Fetch for MMR

**What:** New store method that fetches only id + embedding for a batch of IDs
**When to use:** When vec0 provides all other fields, and MMR needs pairwise cosine similarity

```rust
// New method in store.rs
pub async fn get_embeddings_by_ids(&self, ids: &[String]) -> Result<Vec<(String, Vec<f32>)>, BlufioError> {
    let ids = ids.to_vec();
    self.conn
        .call(move |conn| {
            let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{i}")).collect();
            let sql = format!(
                "SELECT id, embedding FROM memories WHERE id IN ({})",
                placeholders.join(", ")
            );
            let mut stmt = conn.prepare(&sql)?;
            let params: Vec<&dyn rusqlite::types::ToSql> =
                ids.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect();
            let results = stmt
                .query_map(params.as_slice(), |row| {
                    let id: String = row.get(0)?;
                    let blob: Vec<u8> = row.get(1)?;
                    Ok((id, blob_to_vec(&blob)))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(results)
        })
        .await
        .map_err(storage_err)
}
```

### Anti-Patterns to Avoid
- **Reading embeddings from vec0**: sqlite-vec stores vectors for MATCH queries, not for retrieval. Reading them back is not the intended API and may not return raw f32 vectors.
- **Forcing full JOIN elimination**: MMR requires pairwise cosine similarity on raw embeddings. Trying to avoid the memories table entirely would require storing duplicate embeddings in the code path, which is worse than a targeted SELECT id, embedding query.
- **Non-blocking startup migration**: The user locked this decision -- migration MUST block startup to guarantee the first query has complete data.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Batched migration | Custom migration loop | `vec0_populate_batch()` | Already idempotent, handles LEFT JOIN skip, batch commits |
| Sync drift detection | Custom count comparison | `blufio doctor check_vec0()` | Already implemented in Phase 65 |
| Cosine similarity | Custom SIMD | `cosine_similarity()` from types.rs | Already unit-tested, handles edge cases |
| RRF fusion | Custom merge | `reciprocal_rank_fusion()` | Already implemented and tested with k=60 |

**Key insight:** Almost all infrastructure already exists from Phase 65. Phase 67's value is wiring, optimization, and validation -- not new low-level components.

## Common Pitfalls

### Pitfall 1: Forgetting to Register sqlite-vec Before Opening Connections
**What goes wrong:** `vec0` virtual table operations fail with "no such module: vec0"
**Why it happens:** `ensure_sqlite_vec_registered()` uses `sqlite3_auto_extension` which only applies to connections opened AFTER registration
**How to avoid:** Call `ensure_sqlite_vec_registered()` early in `initialize_memory()`, before `open_connection()` for the memory store
**Warning signs:** Tests pass (they call it) but production fails (startup doesn't call it)

### Pitfall 2: vec0 Distance vs Similarity Confusion
**What goes wrong:** Score comparisons are inverted or out of expected range
**Why it happens:** vec0 returns cosine DISTANCE (0-2 range), not similarity (-1 to 1). Conversion is `similarity = 1.0 - distance`.
**How to avoid:** The existing `map_search_row()` already handles this. Do NOT double-convert.
**Warning signs:** Parity test shows scores differ by constant offset

### Pitfall 3: f32 Precision Differences Between vec0 and Rust
**What goes wrong:** Parity test asserts exact equality and fails
**Why it happens:** vec0 uses C-level float operations; Rust cosine_similarity uses different float accumulation order
**How to avoid:** User-locked tolerance of 0.01 for similarity scores. Compare ID sets first, then check score ranges are within tolerance.
**Warning signs:** Same IDs returned but similarity scores differ by ~0.001-0.01

### Pitfall 4: MemoryStore Event Bus Not Wired in initialize_memory
**What goes wrong:** `Vec0PopulationComplete` event never emitted, subscribers don't trigger
**Why it happens:** `initialize_memory()` creates MemoryStore with `event_bus: None` because event bus is created later in the startup sequence
**How to avoid:** Either (a) create event bus earlier and pass to `MemoryStore::with_vec0()`, or (b) accept that startup population doesn't emit events (background re-population would emit them). Per the CONTEXT.md, the existing `Vec0PopulationComplete` event is reused.
**Warning signs:** Population completes silently, no event bus subscriber activity

### Pitfall 5: Parity Tests Require Real Embeddings
**What goes wrong:** Parity test uses uniform `vec![0.1; 384]` embeddings, all memories have identical vectors, no meaningful ranking
**Why it happens:** Synthetic embeddings are all identical, so KNN returns arbitrary order
**How to avoid:** Use `synthetic_embedding(seed)` from vec0.rs tests which generates unique normalized vectors via seeded sin function
**Warning signs:** Parity test passes trivially because all scores are identical

### Pitfall 6: Config Default Change Breaks Existing Installs
**What goes wrong:** Existing users who never set `vec0_enabled` suddenly get startup migration on upgrade
**Why it happens:** Changing serde default from `false` to `true` means TOML files without explicit `vec0_enabled = false` will default to `true`
**How to avoid:** This IS the intended behavior per user decision. New installs get `true`, existing installs that explicitly set it keep their value. Document the change clearly.
**Warning signs:** None -- this is intentional

## Code Examples

### Current initialize_memory (BEFORE -- needs modification)
```rust
// Source: crates/blufio/src/serve/storage.rs:164-166
let memory_conn = blufio_storage::open_connection(&config.storage.database_path).await?;
let memory_store = Arc::new(MemoryStore::new(memory_conn));
// NOTE: vec0 is never enabled, populate_vec0 never called
```

### Vec0 Auxiliary Column Data (already available)
```rust
// Source: crates/blufio-memory/src/vec0.rs:20-34
pub struct Vec0SearchResult {
    pub memory_id: String,
    pub similarity: f32,
    pub content: String,
    pub source: String,
    pub confidence: f64,
    pub created_at: String,
}
// These fields are ALREADY returned by vec0_search but DISCARDED in vec0_vector_search
```

### Current Retriever Discards Vec0 Auxiliary Data
```rust
// Source: crates/blufio-memory/src/retriever.rs:282-300
async fn vec0_vector_search(&self, query_embedding: &[f32]) -> Result<Vec<(String, f32)>, BlufioError> {
    // ... calls vec0_search which returns Vec0SearchResult with all auxiliary data
    Ok(results
        .into_iter()
        .map(|r| (r.memory_id, r.similarity))  // <-- DISCARDS content, source, confidence, created_at
        .collect())
}
```

### Existing Batch Population (reuse as-is)
```rust
// Source: crates/blufio-memory/src/store.rs:511-541
pub async fn populate_vec0(&self) -> Result<(usize, usize), BlufioError> {
    let start = std::time::Instant::now();
    let (populated, total) = self.conn
        .call(move |conn| vec0::vec0_populate_batch(conn, 500))
        .await.map_err(storage_err)?;
    // Updates Prometheus gauge, emits Vec0PopulationComplete event
    Ok((populated, total))
}
```

### Parity Test Pattern
```rust
// Recommended approach for VEC-05 parity validation
#[tokio::test]
async fn hybrid_retrieval_parity_vec0_vs_in_memory() {
    // Setup: insert N memories with synthetic_embedding(seed) for unique vectors
    // 1. Run full pipeline with vec0 enabled -> collect (id, score) results
    // 2. Run full pipeline with in-memory path -> collect (id, score) results
    // 3. Assert: same set of memory IDs in top-K
    // 4. Assert: scores within 0.01 tolerance
    // 5. Assert: order matches within tie-break tolerance
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| In-memory brute-force cosine | vec0 KNN with metadata filtering | Phase 65 (v1.6) | O(n) -> O(k*log(n)) search |
| Full Memory struct fetch after search | Partial: scoring from vec0, embeddings from memories | Phase 67 (this phase) | Reduces data transfer by ~90% for scoring path |
| vec0_enabled defaults false | vec0_enabled defaults true | Phase 67 (this phase) | New installs get vec0 by default |

**Deprecated/outdated:**
- Nothing deprecated. Both paths (vec0 and in-memory) remain available via toggle.

## Open Questions

1. **Embedding Readback from vec0**
   - What we know: sqlite-vec stores vectors as optimized internal format for MATCH queries. The `distance` column returns the computed distance but raw vector data readback is not well-documented.
   - What's unclear: Whether `SELECT embedding FROM memories_vec0 WHERE rowid = ?` returns usable f32 bytes.
   - Recommendation: Do NOT rely on reading embeddings from vec0. The user decision explicitly says "still fetch embeddings from memories table for MMR." Use the new `get_embeddings_by_ids()` approach instead.

2. **Event Bus Timing at Startup**
   - What we know: `event_bus` is created in `subsystems::create_event_bus()` AFTER `init_memory_system()` runs. `MemoryStore` accepts `Option<Arc<EventBus>>`.
   - What's unclear: Whether reordering startup to create event bus first causes issues.
   - Recommendation: Either (a) create event bus earlier, or (b) accept that startup population doesn't emit bus events (the `info!` log is sufficient for operator visibility). Lean toward (b) since the migration is a one-time startup event visible in logs.

3. **Separate Retriever Code Path vs Optimized Fetch**
   - What we know: The current retriever has a single `retrieve()` method that works for both paths.
   - What's unclear: Whether branching within `retrieve()` to use vec0 auxiliary data adds too much complexity.
   - Recommendation: Create a single optimized path that works for both: when vec0 is enabled, `vec0_vector_search()` returns richer data that `retrieve()` uses directly for scoring. The only conditional is whether to call `get_embeddings_by_ids()` (vec0 path) or `get_memories_by_ids()` (in-memory path) before MMR.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (Rust built-in) |
| Config file | Cargo.toml per crate |
| Quick run command | `cargo test -p blufio-memory -- --test-threads=1` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| VEC-04 | Startup migration populates vec0 from BLOB embeddings | integration | `cargo test -p blufio-memory store::tests::vec0_populate_copies_active_memories -- -x` | Exists (basic version) |
| VEC-04 | Migration is idempotent (resume on interrupt) | integration | `cargo test -p blufio-memory store::tests::vec0_populate_is_idempotent -- -x` | Exists |
| VEC-04 | Migration blocks startup | integration | `cargo test -p blufio-memory -- vec0_startup_migration -x` | Wave 0 |
| VEC-05 | Hybrid retrieval parity vec0 vs in-memory | integration | `cargo test -p blufio-memory -- parity -x` | Wave 0 |
| VEC-05 | Parity at 10/100/1K scale | integration | `cargo test -p blufio-memory -- parity_scale -x` | Wave 0 |
| VEC-06 | batch_evict syncs vec0 atomically | integration | `cargo test -p blufio-memory store::tests::vec0_batch_evict_deletes_from_vec0 -- -x` | Exists |
| VEC-06 | soft_delete syncs vec0 status | integration | `cargo test -p blufio-memory store::tests::vec0_soft_delete_updates_status_in_vec0 -- -x` | Exists |
| VEC-07 | session_id partition key filters | unit | `cargo test -p blufio-memory vec0::tests::vec0_knn_search_with_session_id_filter -- -x` | Exists |
| VEC-08 | vec0 auxiliary columns used for scoring | integration | `cargo test -p blufio-memory -- vec0_auxiliary_scoring -x` | Wave 0 |
| VEC-08 | Embedding-only fetch for MMR | integration | `cargo test -p blufio-memory -- get_embeddings_by_ids -x` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p blufio-memory -- --test-threads=1`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] Parity test comparing vec0 vs in-memory full pipeline results at 10/100/1K scale
- [ ] Integration test for startup migration wiring (vec0 population triggered at init)
- [ ] Integration test for vec0 auxiliary data used in scoring pipeline
- [ ] Unit test for `get_embeddings_by_ids()` store method
- [ ] Test for config default value change (`vec0_enabled: true`)

*(Existing test infrastructure covers VEC-04 basic migration, VEC-06 atomic sync, VEC-07 session partitioning. Wave 0 gaps are parity validation and retriever optimization tests.)*

## Sources

### Primary (HIGH confidence)
- **Codebase analysis** -- direct reading of `vec0.rs`, `retriever.rs`, `store.rs`, `serve/storage.rs`, `serve/mod.rs`, `model.rs`
- **Phase 65 implementation** -- all vec0 infrastructure verified in codebase (dual-write, batch populate, search, fallback)
- **CONTEXT.md decisions** -- locked decisions from user discussion session

### Secondary (MEDIUM confidence)
- **sqlite-vec documentation** -- vec0 auxiliary columns (`+column_name type`) are for output-only fields returned with search results
- **State.md decisions** -- vec0 returns cosine distance (0-2) not similarity (0-1), confirmed in codebase

### Tertiary (LOW confidence)
- **vec0 embedding readback** -- unclear whether raw embedding bytes can be read back from vec0 virtual table; recommendation is to NOT attempt this (user decision aligns)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all libraries already integrated, no new dependencies
- Architecture: HIGH -- all code paths read and understood, modifications are surgical
- Pitfalls: HIGH -- verified against codebase, Phase 65 lessons incorporated
- Parity testing: MEDIUM -- test design is clear but f32 precision behavior at 1K scale needs empirical validation

**Research date:** 2026-03-14
**Valid until:** 2026-04-14 (stable domain, no external dependency changes expected)
