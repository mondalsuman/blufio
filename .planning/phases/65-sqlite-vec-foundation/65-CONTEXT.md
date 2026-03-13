# Phase 65: sqlite-vec Foundation - Context

**Gathered:** 2026-03-13
**Status:** Ready for planning

<domain>
## Phase Boundary

Register sqlite-vec with SQLCipher, create vec0 virtual table with metadata columns, wire KNN search into the hybrid retriever. Existing in-memory vector search remains available behind a config toggle. Migration and full pipeline switch are Phase 67 concerns.

</domain>

<decisions>
## Implementation Decisions

### Dual-path rollout
- Config toggle controls vec0 vs in-memory vector search (default OFF)
- Toggle is permanent (never removed, even after Phase 67 migration)
- When enabled, vec0 fully replaces in-memory path (no shadow mode)
- Startup only -- toggling via hot reload does NOT trigger population, requires restart
- vec0 virtual table always created (via migration) regardless of toggle -- toggle only affects whether search uses it
- Dual-write: saves write to BOTH memories.embedding BLOB and vec0 table when enabled
- Same-transaction atomicity: memory + vec0 inserts in single transaction, both succeed or both fail
- Eviction (batch_evict) and soft-delete sync to vec0 in Phase 65 (not deferred)
- Eager population at startup: all active embeddings copied to vec0 in batched 500-row chunks
- Population is idempotent (skip existing rows, safe to restart if interrupted)
- Info-level progress logging during population ("Populating vec0: 500/2000 memories...")
- Same rowid as memories table for direct correlation

### Fallback behavior
- sqlite-vec compiled into binary (not loadable extension) -- preserves single-binary model
- If sqlite-vec fails to load/register: graceful fallback to in-memory search with warn! log + Prometheus metric (blufio_memory_vec0_fallback)
- If SQLCipher + sqlite-vec proves incompatible: investigate alternatives before giving up
- Per-query runtime fallback: individual vec0 query failures transparently retry via in-memory
- Fallback log rate-limiting: log first 5, then suppress and log every 60 seconds
- If vec0 write fails during save: entire transaction rolls back (memory not saved either)
- `blufio doctor` reports vec0 health: extension loaded, table row count vs memories count, sync drift
- `blufio memory rebuild-vec0` CLI command for drop-and-repopulate recovery
- EventBus emits vec0 lifecycle events: Vec0Enabled, Vec0FallbackTriggered, Vec0PopulationComplete

### Search tuning
- Cosine distance function in vec0 (matches existing cosine_similarity pattern)
- Distance-to-similarity conversion in Rust after query (1.0 - distance), not in SQL
- Reuse existing max_retrieval_results config for KNN limit (no separate vec0 limit)
- similarity_threshold filter applied in SQL (WHERE distance < 1.0 - threshold) per VEC-03 philosophy
- vec0 KNN results replace in-memory cosine results in existing RRF pipeline (BM25 + KNN + RRF + decay + importance + MMR preserved)
- f32 precision for vec0 embedding column (same as current BLOB storage)
- vector_search() method internals replaced based on toggle -- callers (HybridRetriever) unchanged
- Hardcoded table name: "memories_vec0"
- Optional session_id filter parameter on vec0 search (foundation for Phase 67 partition key)

### vec0 schema
- Metadata columns: status (TEXT), classification (TEXT) -- for in-query filtering per VEC-03
- Partition column: session_id (TEXT) -- added now for Phase 67 readiness
- Auxiliary columns: memory_id (TEXT), content (TEXT), source (TEXT), confidence (REAL), created_at (TEXT) -- for JOIN-free retrieval per VEC-08
- Embedding: 384-dim float32 vector with cosine distance

### Crate organization
- vec0 code lives in blufio-memory crate (single file: vec0.rs)
- sqlite-vec extension registration: Claude's discretion on placement (connection factory vs MemoryStore)
- sqlite-vec dependency at workspace level in root Cargo.toml
- Config fields added to existing MemoryConfig struct (under [memory] TOML section)
- MemoryStore gains vec0_enabled field set once at construction
- Separate Prometheus metrics with vec0 prefix (e.g. blufio_memory_vec0_search_duration)
- OTel span attribute: blufio.memory.backend = "vec0" | "in_memory" on existing retrieve span

### Migration safety
- New numbered SQL migration creates vec0 virtual table (migration only creates, does not populate)
- Population is a separate startup step (runs after migration, based on toggle)

### Testing strategy
- In-memory SQLite for unit tests, temp file DBs for integration tests
- Dedicated SQLCipher + vec0 integration test in crates/blufio/tests/
- Parity test: verifies vec0 results match in-memory results for same query (same IDs, similar scores)
- Synthetic embeddings for unit tests, real ONNX embeddings for one integration test
- Dedicated VEC-03 filter test: mixed status/classification data, assert correct result counts
- Fallback test: simulate vec0 failure, verify fallback returns same results as in-memory
- Basic Criterion benchmark: full hybrid pipeline (vec0 -> BM25 -> RRF) at 100 and 1K entries vs in-memory

### Claude's Discretion
- Config key naming (e.g. memory.vec0_enabled vs memory.backend enum)
- sqlite-vec extension registration placement (connection factory vs MemoryStore init)
- Exact vec0 SQL syntax and CREATE VIRTUAL TABLE statement
- Migration version number
- Exact Prometheus metric names beyond the vec0 prefix convention
- EventBus event variant naming and payload structure

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `MemoryStore` (store.rs): Current BLOB-based memory store with save/get/search_bm25/batch_evict -- vec0 operations extend this
- `HybridRetriever` (retriever.rs): RRF pipeline with temporal_decay, importance_boost, MMR -- vec0 KNN results slot into vector_search() replacing in-memory cosine
- `cosine_similarity()` (types.rs): Current similarity function -- vec0 uses native cosine distance instead
- `vec_to_blob()`/`blob_to_vec()` (types.rs): BLOB serialization -- still used for dual-write
- `open_connection()`/`open_connection_sync()` (database.rs): Centralized connection factory with PRAGMA key -- sqlite-vec registration goes here or in MemoryStore
- `MemoryConfig` (blufio-config): Existing config struct -- vec0 toggle and settings added here
- `EventBus` (blufio-bus): Existing pub/sub with MemoryEvent variants -- extend for vec0 lifecycle events
- FTS5 virtual table pattern (memories_fts): Existing virtual table with sync triggers -- vec0 follows same pattern

### Established Patterns
- tokio-rusqlite single-writer: All DB operations via conn.call() on background thread
- Numbered SQL migrations in blufio-storage/src/migrations.rs
- PRAGMA key as first statement on every connection (SQLCipher)
- Status/classification filtering: WHERE status='active' AND classification!='restricted' AND deleted_at IS NULL
- Prometheus metrics via metrics crate with counter!/gauge!/histogram! macros
- OTel spans via tracing::info_span! with attribute recording
- blufio doctor subcommand for health checks

### Integration Points
- `retriever.rs:vector_search()` -- internals replaced with vec0 KNN when toggle is on
- `store.rs:save()` -- extended with vec0 INSERT in same transaction
- `store.rs:batch_evict()` -- extended with vec0 DELETE in same transaction
- `store.rs:soft_delete()` -- extended with vec0 row update
- `database.rs:open_connection()` -- potential sqlite-vec registration site
- `MemoryConfig` -- new vec0 fields
- CLI main.rs -- new `memory rebuild-vec0` subcommand
- blufio doctor -- new vec0 health check

</code_context>

<specifics>
## Specific Ideas

- Toggle is permanent because operator may need to disable vec0 for debugging or compatibility issues at any time
- Eager population chosen over on-demand to ensure first search after enabling vec0 has complete data
- Same-transaction atomicity critical: never have memories table and vec0 out of sync
- Full hybrid pipeline benchmark (not just isolated KNN) to validate real-world impact in Phase 65
- Schema includes Phase 67 columns (session_id, auxiliary columns) now to avoid ALTER/rebuild later

</specifics>

<deferred>
## Deferred Ideas

None -- discussion stayed within phase scope

</deferred>

---

*Phase: 65-sqlite-vec-foundation*
*Context gathered: 2026-03-13*
