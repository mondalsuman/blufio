# Phase 67: Vector Search Migration & Hybrid Pipeline - Context

**Gathered:** 2026-03-14
**Status:** Ready for planning

<domain>
## Phase Boundary

Migrate existing BLOB embeddings to vec0 virtual table, validate hybrid retrieval parity (BM25 + vec0 KNN + RRF + temporal decay + importance + MMR), partially eliminate JOIN to memories table using auxiliary columns, and make vec0 the default for new installs. Phase 65 laid the foundation (schema, dual-write, populate); this phase completes the migration path and validates production readiness.

</domain>

<decisions>
## Implementation Decisions

### Migration trigger & process
- Automatic at startup when vec0_enabled=true (reuses existing `vec0_populate_batch()`)
- Migration blocks startup — queries wait until complete, guarantees first query has complete vec0 data
- Per-batch info logging: "Migrating vec0: 500/2000 memories..." (already implemented in Phase 65)
- No dedicated CLI migration command — operator flips `vec0_enabled=true` in TOML and restarts

### Rollback & retry strategy
- Retry from where it left off — migration is idempotent (LEFT JOIN skips existing rows)
- Indefinite retry on restart — no max retry counter, operator can disable vec0 toggle if stuck
- No rollback needed — partial state is valid, interrupted migration resumes on next startup

### BLOB preservation
- Keep memories.embedding BLOB column after migration — dual-write continues
- Enables fallback to in-memory path if needed (toggle is permanent per Phase 65)
- Storage cost accepted (~1.5KB/row for 384-dim f32 vectors)

### EventBus integration
- Reuse existing `Vec0PopulationComplete` event (already emits count + duration_ms from Phase 65)
- No new event variant needed for migration — semantically identical to population

### Doctor health check
- `blufio doctor` verifies migration completeness: sync drift check between memories count and vec0 count
- Already decided in Phase 65, Phase 67 validates this works post-migration

### JOIN elimination (partial)
- Vec0 auxiliary columns provide content, source, confidence, created_at for scoring steps
- Still fetch embeddings from memories table for MMR diversity reranking (384-dim vectors)
- Rationale: MMR needs pairwise cosine similarity on embeddings; sqlite-vec embedding column is for MATCH queries, not direct retrieval
- Net effect: scoring (importance_boost * decay) can use vec0 auxiliary data, but get_memories_by_ids() still needed for MMR embeddings
- Optimization: only fetch embedding column from memories (not full row) when vec0 provides other fields

### Parity validation
- "Functionally identical" = same memory IDs in top-K results, with similarity scores within 0.01 tolerance
- Order may vary within tied scores (f32 precision differences between vec0 cosine distance and Rust cosine_similarity)
- Parity test: run same query through both vec0 and in-memory paths, compare result ID sets and score ranges
- Test at multiple scales: 10, 100, 1K entries

### Default state transition
- After Phase 67 validation, vec0_enabled defaults to true for new installs
- Existing installs keep their current setting (no forced migration)
- Toggle remains permanent — operators can always disable vec0 and fall back to in-memory

### Claude's Discretion
- Whether to read embedding back from vec0 (if sqlite-vec supports it) vs always fetching from memories
- Exact parity test implementation and fixture design
- Integration test structure and module organization
- Whether partial JOIN elimination is worth a separate retriever code path or just optimizing get_memories_by_ids
- Config default value change mechanism (serde default vs migration)

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `vec0_populate_batch()` (vec0.rs): Already does batched 500-row migration with idempotent LEFT JOIN skip — Phase 67 reuses directly
- `populate_vec0()` (store.rs): Async wrapper with Prometheus gauge update and Vec0PopulationComplete event — migration entry point
- `rebuild_vec0()` (store.rs): Drop-and-recreate recovery path — unchanged
- `vec0_search()` (vec0.rs): Returns Vec0SearchResult with memory_id, similarity, content, source, confidence, created_at — auxiliary columns ready for partial JOIN elimination
- `HybridRetriever::vector_search()` (retriever.rs): Dispatch to vec0 or in-memory with fallback — unchanged
- `get_memories_by_ids()` (store.rs): Fetches full Memory structs including embeddings — still needed for MMR

### Established Patterns
- tokio-rusqlite single-writer thread for all DB operations
- Same-transaction atomicity for dual-write (save, batch_evict, soft_delete) — all implemented in Phase 65
- Config toggle pattern: `vec0_enabled` set once at construction, startup-only
- Per-query fallback with rate-limited logging (AtomicU64 counters)

### Integration Points
- `retriever.rs:vec0_vector_search()` — Currently maps to (memory_id, similarity); could be extended to carry auxiliary data for scoring
- `retriever.rs:retrieve()` Step 5 — get_memories_by_ids() is the JOIN to optimize; could skip non-embedding fields when vec0 provides them
- `MemoryConfig` default value — change `vec0_enabled` default from false to true
- `serve.rs` startup — populate_vec0() already called when vec0_enabled; this IS the migration

</code_context>

<specifics>
## Specific Ideas

- Migration is essentially "enable the toggle and restart" — the existing populate_vec0() IS the migration
- Phase 67's primary value is validation (parity tests) and the partial JOIN elimination, not new migration machinery
- Partial JOIN elimination: vec0_search returns enough metadata for importance_boost and temporal_decay calculation; only embeddings need memories table
- Parity test should compare end-to-end HybridRetriever results, not just vec0 vs in-memory vector search, since the full pipeline includes BM25 + RRF + decay + importance + MMR

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 67-vector-search-migration-hybrid-pipeline*
*Context gathered: 2026-03-14*
