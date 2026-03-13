# Phase 65: sqlite-vec Foundation - Research

**Researched:** 2026-03-13
**Domain:** sqlite-vec vector search extension, SQLCipher encryption, Rust/rusqlite integration
**Confidence:** MEDIUM (sqlite-vec is alpha, SQLCipher+vec0 interop is unverified in practice)

## Summary

sqlite-vec is a pure-C SQLite extension by Alex Garcia that provides vec0 virtual tables for disk-backed KNN vector search. The Rust crate (`sqlite-vec 0.1.6`) compiles the C source with `SQLITE_CORE` defined and exposes a single FFI entry point (`sqlite3_vec_init`) that gets registered via `sqlite3_auto_extension`. The crate has **zero runtime dependencies** -- rusqlite is only a dev-dependency -- so there is no version conflict with the project's rusqlite 0.37 + `bundled-sqlcipher-vendored-openssl` configuration.

The vec0 virtual table supports metadata columns (filterable during KNN), auxiliary columns (stored separately, retrieved at SELECT time), and partition key columns. Cosine distance is specified per-column at CREATE time. Vectors are passed as raw `f32` BLOB bytes -- the project's existing `vec_to_blob()` already produces this exact format (little-endian f32 bytes), so no new serialization is needed.

The critical risk is SQLCipher + sqlite-vec compatibility. Both compile against SQLite internals -- SQLCipher as a fork, sqlite-vec via `SQLITE_CORE`. Since rusqlite's `bundled-sqlcipher-vendored-openssl` bundles SQLCipher (not stock SQLite), the sqlite-vec C code will compile against SQLCipher's headers and link against its symbols. This should work because sqlite-vec uses standard SQLite virtual table APIs, but it has never been explicitly tested by the sqlite-vec author. Validation must be the very first task.

**Primary recommendation:** Register sqlite-vec via `sqlite3_auto_extension` at process startup (before any connections open), validate SQLCipher compatibility in the first task, then build vec0 operations as methods on MemoryStore with same-transaction dual-write.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
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
- sqlite-vec compiled into binary (not loadable extension) -- preserves single-binary model
- If sqlite-vec fails to load/register: graceful fallback to in-memory search with warn! log + Prometheus metric (blufio_memory_vec0_fallback)
- If SQLCipher + sqlite-vec proves incompatible: investigate alternatives before giving up
- Per-query runtime fallback: individual vec0 query failures transparently retry via in-memory
- Fallback log rate-limiting: log first 5, then suppress and log every 60 seconds
- If vec0 write fails during save: entire transaction rolls back (memory not saved either)
- `blufio doctor` reports vec0 health: extension loaded, table row count vs memories count, sync drift
- `blufio memory rebuild-vec0` CLI command for drop-and-repopulate recovery
- EventBus emits vec0 lifecycle events: Vec0Enabled, Vec0FallbackTriggered, Vec0PopulationComplete
- Cosine distance function in vec0 (matches existing cosine_similarity pattern)
- Distance-to-similarity conversion in Rust after query (1.0 - distance), not in SQL
- Reuse existing max_retrieval_results config for KNN limit (no separate vec0 limit)
- similarity_threshold filter applied in SQL (WHERE distance < 1.0 - threshold) per VEC-03 philosophy
- vec0 KNN results replace in-memory cosine results in existing RRF pipeline (BM25 + KNN + RRF + decay + importance + MMR preserved)
- f32 precision for vec0 embedding column (same as current BLOB storage)
- vector_search() method internals replaced based on toggle -- callers (HybridRetriever) unchanged
- Hardcoded table name: "memories_vec0"
- Optional session_id filter parameter on vec0 search (foundation for Phase 67 partition key)
- Metadata columns: status (TEXT), classification (TEXT) -- for in-query filtering per VEC-03
- Partition column: session_id (TEXT) -- added now for Phase 67 readiness
- Auxiliary columns: memory_id (TEXT), content (TEXT), source (TEXT), confidence (REAL), created_at (TEXT) -- for JOIN-free retrieval per VEC-08
- Embedding: 384-dim float32 vector with cosine distance
- vec0 code lives in blufio-memory crate (single file: vec0.rs)
- sqlite-vec dependency at workspace level in root Cargo.toml
- Config fields added to existing MemoryConfig struct (under [memory] TOML section)
- MemoryStore gains vec0_enabled field set once at construction
- Separate Prometheus metrics with vec0 prefix (e.g. blufio_memory_vec0_search_duration)
- OTel span attribute: blufio.memory.backend = "vec0" | "in_memory" on existing retrieve span
- New numbered SQL migration creates vec0 virtual table (migration only creates, does not populate)
- Population is a separate startup step (runs after migration, based on toggle)
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

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| VEC-01 | Memory store uses sqlite-vec vec0 virtual table for disk-backed KNN vector search instead of in-memory brute-force cosine similarity | sqlite-vec crate provides vec0 virtual tables with MATCH-based KNN; replaces get_active_embeddings() + in-memory cosine loop |
| VEC-02 | sqlite-vec integrates with SQLCipher -- vec0 data encrypted at rest alongside existing memories table | sqlite-vec compiles with SQLITE_CORE against SQLCipher headers; auto_extension registers after PRAGMA key; vec0 shadow tables stored in same encrypted DB file |
| VEC-03 | vec0 metadata columns filter status='active' and classification!='restricted' during KNN search, not post-query | vec0 metadata columns support TEXT type with =, != operators in WHERE clause during KNN MATCH query |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| sqlite-vec | 0.1.6 | vec0 virtual table for KNN vector search | Only pure-C SQLite vector extension; compiles into binary with SQLITE_CORE |
| rusqlite | 0.37 (existing) | SQLite bindings with SQLCipher | Already in project; bundled-sqlcipher-vendored-openssl feature |
| tokio-rusqlite | 0.7 (existing) | Async wrapper for rusqlite | Already in project; single-writer model |
| zerocopy | 0.7.x | Zero-copy f32 vec to bytes for vec0 params | Recommended by sqlite-vec author for Rust; avoids manual byte serialization |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| metrics | 0.24 (existing) | Prometheus metrics | vec0 search duration, fallback counter |
| tracing | 0.1 (existing) | OTel spans and logging | Backend attribute on retrieve span, population progress |
| blufio-bus | (existing) | EventBus for lifecycle events | Vec0Enabled, Vec0FallbackTriggered, Vec0PopulationComplete |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| sqlite-vec | hnswlib/faiss via FFI | Adds native dependency, breaks single-binary; sqlite-vec is pure C |
| zerocopy | bytemuck | Both work; zerocopy is what sqlite-vec docs recommend |
| vec_to_blob() (existing) | zerocopy AsBytes | Both produce identical f32 LE bytes; existing function already works |

**Installation:**
```toml
# In workspace Cargo.toml [workspace.dependencies]
sqlite-vec = "0.1.6"
zerocopy = { version = "0.7", features = ["derive"] }

# In crates/blufio-memory/Cargo.toml [dependencies]
sqlite-vec.workspace = true
zerocopy.workspace = true
```

**Note on version compatibility:** The sqlite-vec crate has rusqlite only as a *dev-dependency* (^0.31), not a runtime dependency. The crate compiles sqlite-vec.c via `cc` with `SQLITE_CORE` defined and exports a single `extern "C" fn sqlite3_vec_init()`. It links against whatever SQLite is already in the process (in our case, SQLCipher via rusqlite's bundled-sqlcipher). There is no version conflict.

## Architecture Patterns

### Recommended Project Structure
```
crates/blufio-memory/src/
├── lib.rs           # Add `pub mod vec0;`
├── vec0.rs          # NEW: All vec0 operations (registration, create, populate, search, sync, rebuild)
├── store.rs         # MODIFIED: Dual-write in save(), sync in batch_evict()/soft_delete()
├── retriever.rs     # MODIFIED: vector_search() conditionally uses vec0
├── types.rs         # Existing (no changes needed)
└── ...

crates/blufio-storage/migrations/
├── V15__vec0_virtual_table.sql  # NEW: CREATE VIRTUAL TABLE memories_vec0

crates/blufio-config/src/model.rs
└── MemoryConfig     # MODIFIED: Add vec0_enabled field

crates/blufio-bus/src/events.rs
└── MemoryEvent      # MODIFIED: Add Vec0 lifecycle variants

crates/blufio/src/
├── cli/memory_cmd.rs  # MODIFIED: Add rebuild-vec0 subcommand
└── doctor.rs          # MODIFIED: Add vec0 health check
```

### Pattern 1: Extension Registration via sqlite3_auto_extension
**What:** Register sqlite-vec globally at process startup so every connection (including tokio-rusqlite's background thread) gets vec0 support automatically.
**When to use:** Always -- must happen before any Database::open() call.
**Why not per-connection:** sqlite-vec author warns per-connection registration causes segfaults in some bindings. `sqlite3_auto_extension` is called once, is process-global, and is a no-op if called multiple times. Since rusqlite calls it through FFI, it works regardless of whether the underlying SQLite is stock or SQLCipher.

**Example:**
```rust
// In blufio-memory/src/vec0.rs or a startup initialization function
use rusqlite::ffi::sqlite3_auto_extension;
use sqlite_vec::sqlite3_vec_init;

/// Register sqlite-vec extension globally. Must be called before any DB connections open.
/// Safe to call multiple times (idempotent).
///
/// Returns Ok(true) if registered, Ok(false) if already registered or unavailable.
pub fn register_sqlite_vec() -> Result<bool, String> {
    unsafe {
        sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite3_vec_init as *const ()
        )));
    }
    Ok(true)
}

/// Verify sqlite-vec is working on a given connection.
pub fn verify_vec0_available(conn: &rusqlite::Connection) -> Result<String, rusqlite::Error> {
    conn.query_row("SELECT vec_version()", [], |row| row.get::<_, String>(0))
}
```

### Pattern 2: vec0 CREATE VIRTUAL TABLE with All Column Types
**What:** The vec0 table schema uses metadata columns for in-query filtering, auxiliary columns for JOIN-free retrieval, and a partition key for Phase 67 session scoping.
**When to use:** In the V15 migration SQL.

**Example:**
```sql
-- V15__vec0_virtual_table.sql
-- vec0 virtual table for KNN vector search with metadata filtering.
-- Note: vec0 requires sqlite-vec extension to be registered on the connection.
-- The extension is registered via sqlite3_auto_extension at process startup.
CREATE VIRTUAL TABLE IF NOT EXISTS memories_vec0 USING vec0(
    -- Metadata columns (filterable during KNN query)
    status text,
    classification text,
    -- Partition key (for Phase 67 session-scoped search)
    session_id text partition key,
    -- Vector column: 384-dim float32 with cosine distance
    embedding float[384] distance_metric=cosine,
    -- Auxiliary columns (stored separately, returned at SELECT time)
    +memory_id text,
    +content text,
    +source text,
    +confidence real,
    +created_at text
);
```

### Pattern 3: KNN Query with Metadata Filtering (VEC-03)
**What:** vec0 metadata columns filter status and classification *during* KNN search, not post-query. This means only matching rows are considered for distance computation.
**When to use:** In the vec0 search method.

**Example:**
```rust
// In vec0.rs - vec0 KNN search
pub fn vec0_knn_search(
    conn: &rusqlite::Connection,
    query_embedding: &[f32],
    limit: usize,
    similarity_threshold: f64,
    session_id: Option<&str>,
) -> Result<Vec<Vec0SearchResult>, rusqlite::Error> {
    // Convert similarity threshold to distance threshold for cosine distance
    // Cosine distance = 1.0 - cosine_similarity
    // If we want similarity >= threshold, we want distance <= 1.0 - threshold
    let distance_threshold = 1.0 - similarity_threshold;

    let sql = if session_id.is_some() {
        "SELECT rowid, distance, memory_id, content, source, confidence, created_at
         FROM memories_vec0
         WHERE embedding MATCH ?1
           AND k = ?2
           AND status = 'active'
           AND classification != 'restricted'
           AND session_id = ?3"
    } else {
        "SELECT rowid, distance, memory_id, content, source, confidence, created_at
         FROM memories_vec0
         WHERE embedding MATCH ?1
           AND k = ?2
           AND status = 'active'
           AND classification != 'restricted'"
    };

    let embedding_bytes = vec_to_blob(query_embedding);
    let mut stmt = conn.prepare(sql)?;

    // ... bind params and collect results
    // Post-filter: distance <= distance_threshold (similarity_threshold in distance space)
    // Convert distance to similarity: similarity = 1.0 - distance
}
```

**Important:** The `k = ?2` clause in vec0 sets the KNN candidate count. The `distance < threshold` filter should be applied post-KNN in Rust (as WHERE clause on distance column), because vec0's metadata filtering happens during KNN but distance thresholding is a separate concern. The metadata columns (status, classification) ARE filtered during KNN -- this is the VEC-03 guarantee.

### Pattern 4: Same-Transaction Dual-Write
**What:** Memory save and vec0 insert happen in the same SQLite transaction.
**When to use:** In store.rs save() when vec0 is enabled.

**Example:**
```rust
// In store.rs save() -- modified version
self.conn
    .call(move |conn| {
        let tx = conn.transaction()?;

        // 1. Insert into memories table (existing)
        tx.execute(
            "INSERT INTO memories (id, content, embedding, source, confidence, status, \
             superseded_by, session_id, classification, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![id, content, embedding_blob, source, confidence,
                              status, superseded_by, session_id, classification,
                              created_at, updated_at],
        )?;

        // 2. Get the rowid just inserted (for vec0 correlation)
        let rowid: i64 = tx.query_row(
            "SELECT rowid FROM memories WHERE id = ?1",
            [&id],
            |row| row.get(0),
        )?;

        // 3. Insert into vec0 with same rowid (when vec0_enabled)
        if vec0_enabled {
            tx.execute(
                "INSERT INTO memories_vec0(rowid, status, classification, session_id, \
                 embedding, memory_id, content, source, confidence, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![rowid, status, classification, session_id,
                                  embedding_blob, id, content, source, confidence,
                                  created_at],
            )?;
        }

        tx.commit()?;
        Ok(())
    })
    .await
    .map_err(storage_err)?;
```

### Pattern 5: Idempotent Eager Population
**What:** At startup, when vec0 is enabled, populate vec0 from all active memories in 500-row batches.
**When to use:** After migration, before first search.

**Example:**
```rust
pub async fn populate_vec0(conn: &Connection, batch_size: usize) -> Result<usize, BlufioError> {
    conn.call(move |conn| {
        // Count total active memories for progress logging
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memories WHERE status = 'active' \
             AND classification != 'restricted' AND deleted_at IS NULL",
            [], |row| row.get(0),
        )?;

        let mut offset = 0i64;
        let mut populated = 0usize;

        loop {
            let tx = conn.transaction()?;
            let mut stmt = tx.prepare(
                "SELECT m.rowid, m.id, m.content, m.embedding, m.source, m.confidence, \
                 m.status, m.classification, m.session_id, m.created_at \
                 FROM memories m \
                 LEFT JOIN memories_vec0 v ON v.rowid = m.rowid \
                 WHERE m.status = 'active' AND m.classification != 'restricted' \
                 AND m.deleted_at IS NULL AND v.rowid IS NULL \
                 ORDER BY m.rowid LIMIT ?1 OFFSET ?2",
            )?;
            // ... batch INSERT into memories_vec0, skip existing (LEFT JOIN IS NULL)
            // Log progress at info level
        }
        Ok(populated)
    })
    .await
    .map_err(storage_err)
}
```

### Anti-Patterns to Avoid
- **Trigger-based sync:** SQLite virtual tables (including vec0) cannot be targets of triggers on regular tables. All sync must be manual in application code within the same transaction.
- **Loading sqlite-vec as a dynamic extension:** Would require `load_extension` and breaks the single-binary model. Use `sqlite3_auto_extension` with the statically compiled C code instead.
- **Per-connection extension registration:** sqlite-vec author reports segfaults with per-connection init in some bindings. Use process-global `sqlite3_auto_extension` instead.
- **Storing distance as similarity in vec0:** vec0 returns cosine *distance* (0.0 to 2.0 range). Convert to similarity (1.0 - distance) in Rust, never in SQL.
- **Using `LIMIT` instead of `k =`:** While SQLite 3.41+ supports LIMIT in vec0 queries, the `k = N` syntax is more portable and explicit. However, since SQLCipher bundles a recent SQLite, LIMIT should also work. Prefer `k = N` for clarity.
- **Post-query filtering:** The entire point of VEC-03 is that metadata columns (status, classification) are filtered DURING KNN, reducing the candidate set before distance computation. Never filter these after the query.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Vector distance computation | Custom cosine similarity loop over all embeddings | vec0 MATCH + distance_metric=cosine | O(n) brute force vs disk-backed indexed search; vec0 handles chunked storage |
| Metadata filtering during search | Post-query WHERE clause on results | vec0 metadata columns in KNN WHERE | VEC-03 requires pre-query filtering; vec0 does this natively |
| Extension registration | Manual dlopen/load_extension | sqlite3_auto_extension + SQLITE_CORE compile | Static linking preserves single-binary; auto_extension is idempotent |
| f32 serialization for vec0 | Custom byte conversion | Existing vec_to_blob() or zerocopy AsBytes | Both produce identical little-endian f32 bytes that vec0 accepts |

**Key insight:** The existing `vec_to_blob()` function in `types.rs` already produces the exact byte format vec0 expects (little-endian f32 array). No new serialization code is needed -- just pass the BLOB directly as the embedding parameter in vec0 INSERT and MATCH queries.

## Common Pitfalls

### Pitfall 1: SQLCipher + sqlite-vec Symbol Conflicts
**What goes wrong:** sqlite-vec compiles with `SQLITE_CORE` which expects standard SQLite symbols. SQLCipher is a fork with the same API but different internals.
**Why it happens:** The sqlite-vec crate's build.rs compiles sqlite-vec.c and links it as a static library. When rusqlite uses `bundled-sqlcipher-vendored-openssl`, it bundles SQLCipher's amalgamation. Both produce `libsqlite3` symbols.
**How to avoid:** The `SQLITE_CORE` flag makes sqlite-vec use the same SQLite symbols already in the process (in this case, SQLCipher's). Since sqlite-vec only uses standard virtual table APIs (sqlite3_create_module, sqlite3_malloc, etc.), it should link correctly against SQLCipher. **Validate this in the first task before building anything else.**
**Warning signs:** Link-time duplicate symbol errors, or runtime "no such module: vec0" even after registration.

### Pitfall 2: PRAGMA key Must Precede vec0 Operations
**What goes wrong:** vec0 shadow tables (where the actual data lives) are stored in the same encrypted database file. If PRAGMA key hasn't been applied, any vec0 query will fail with "file is not a database."
**Why it happens:** sqlite-vec stores vec0 data in internal shadow tables (e.g., `memories_vec0_chunks`, `memories_vec0_rowids`). These are regular SQLite tables in the same database, subject to encryption.
**How to avoid:** The project's existing `open_connection()` already ensures PRAGMA key is the first statement. `sqlite3_auto_extension` registers vec0 support but doesn't create tables -- the migration does. Since migrations run after PRAGMA key (via Database::open), this ordering is naturally correct.
**Warning signs:** "file is encrypted or is not a database" errors during vec0 operations only.

### Pitfall 3: vec0 Rowid Correlation Drift
**What goes wrong:** memories table rowid and vec0 rowid get out of sync, causing incorrect search results or missing data.
**Why it happens:** Manual sync (no triggers on virtual tables), incomplete error handling in dual-write, or interrupted population.
**How to avoid:** Same-transaction atomicity (both writes in one tx, both succeed or both fail). Idempotent population (LEFT JOIN IS NULL to skip existing). `blufio doctor` health check to detect drift. `blufio memory rebuild-vec0` for recovery.
**Warning signs:** Search results returning wrong content, count mismatch between memories and vec0 tables.

### Pitfall 4: Cosine Distance vs Similarity Confusion
**What goes wrong:** Code treats vec0 distance values as similarity scores, producing inverted rankings.
**Why it happens:** vec0 with `distance_metric=cosine` returns cosine *distance* (range 0.0 to 2.0), not cosine *similarity* (range -1.0 to 1.0). For normalized vectors, distance = 1.0 - similarity.
**How to avoid:** Always convert in Rust: `let similarity = 1.0 - distance;`. The similarity_threshold config comparison must also be converted: filter where `distance <= 1.0 - threshold`.
**Warning signs:** High-relevance results ranked last, threshold filtering returning unexpected result counts.

### Pitfall 5: vec0 Migration Without Extension Registration
**What goes wrong:** Migration V15 tries to CREATE VIRTUAL TABLE using vec0 module, but sqlite-vec hasn't been registered yet, causing migration failure.
**Why it happens:** `sqlite3_auto_extension` must be called before `Database::open()` which runs migrations. If registration is deferred or conditional, the migration crashes.
**How to avoid:** Always register sqlite-vec at process startup, unconditionally, before any database opens. The migration always creates the table (per locked decision). Only the search toggle is conditional.
**Warning signs:** "no such module: vec0" during migration.

### Pitfall 6: vec0 Query Parameter Binding
**What goes wrong:** Embedding bytes not accepted by vec0 MATCH clause.
**Why it happens:** vec0 expects raw f32 bytes (1536 bytes for 384 dims) or JSON array. Passing wrong format causes silent wrong results or errors.
**How to avoid:** Use `vec_to_blob()` (produces little-endian f32 bytes) and pass as `&[u8]` parameter. Or use `zerocopy::AsBytes` on `&[f32]` for zero-copy.
**Warning signs:** "vec0 query error" or zero results from KNN when data exists.

## Code Examples

### Complete vec0 Registration and Verification
```rust
// Source: sqlite-vec official Rust example + project patterns
use rusqlite::ffi::sqlite3_auto_extension;
use sqlite_vec::sqlite3_vec_init;

/// Register sqlite-vec as an auto-extension. Idempotent and process-global.
/// Must be called before any Database::open() or open_connection() call.
pub fn ensure_sqlite_vec_registered() {
    unsafe {
        sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite3_vec_init as *const ()
        )));
    }
}

/// Check if vec0 module is available on a connection.
/// Returns the sqlite-vec version string if available.
pub fn check_vec0_available(conn: &rusqlite::Connection) -> Option<String> {
    conn.query_row("SELECT vec_version()", [], |row| row.get::<_, String>(0))
        .ok()
}
```

### vec0 INSERT with Existing vec_to_blob
```rust
// Source: project types.rs + sqlite-vec docs
use crate::types::vec_to_blob;

fn insert_into_vec0(
    tx: &rusqlite::Transaction,
    rowid: i64,
    memory: &Memory,
) -> Result<(), rusqlite::Error> {
    let embedding_bytes = vec_to_blob(&memory.embedding);
    tx.execute(
        "INSERT INTO memories_vec0(rowid, status, classification, session_id, \
         embedding, memory_id, content, source, confidence, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            rowid,
            memory.status.as_str(),
            memory.classification.as_str(),
            memory.session_id,
            embedding_bytes,
            memory.id,
            memory.content,
            memory.source.as_str(),
            memory.confidence,
            memory.created_at,
        ],
    )?;
    Ok(())
}
```

### vec0 KNN Search with VEC-03 Metadata Filtering
```rust
// Source: sqlite-vec KNN docs + project patterns
fn vec0_search(
    conn: &rusqlite::Connection,
    query_embedding: &[f32],
    k: usize,
    similarity_threshold: f64,
    session_id: Option<&str>,
) -> Result<Vec<(String, f32)>, rusqlite::Error> {
    let embedding_bytes = vec_to_blob(query_embedding);
    let distance_threshold = 1.0 - similarity_threshold;

    let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match session_id {
        Some(sid) => (
            "SELECT memory_id, distance FROM memories_vec0 \
             WHERE embedding MATCH ?1 AND k = ?2 \
             AND status = 'active' AND classification != 'restricted' \
             AND session_id = ?3"
                .to_string(),
            vec![
                Box::new(embedding_bytes) as Box<dyn rusqlite::types::ToSql>,
                Box::new(k as i64),
                Box::new(sid.to_string()),
            ],
        ),
        None => (
            "SELECT memory_id, distance FROM memories_vec0 \
             WHERE embedding MATCH ?1 AND k = ?2 \
             AND status = 'active' AND classification != 'restricted'"
                .to_string(),
            vec![
                Box::new(embedding_bytes) as Box<dyn rusqlite::types::ToSql>,
                Box::new(k as i64),
            ],
        ),
    };

    let mut stmt = conn.prepare(&sql)?;
    let results: Vec<(String, f32)> = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |row| {
            let id: String = row.get(0)?;
            let distance: f64 = row.get(1)?;
            let similarity = 1.0 - distance as f32;
            Ok((id, similarity))
        })?
        .filter_map(|r| r.ok())
        .filter(|(_, sim)| *sim >= similarity_threshold as f32)
        .collect();

    Ok(results)
}
```

### vec0 DELETE for Eviction/Soft-Delete Sync
```rust
// Source: sqlite-vec docs + project batch_evict pattern
fn delete_from_vec0(tx: &rusqlite::Transaction, rowid: i64) -> Result<(), rusqlite::Error> {
    tx.execute("DELETE FROM memories_vec0 WHERE rowid = ?1", [rowid])?;
    Ok(())
}

fn update_vec0_status(
    tx: &rusqlite::Transaction,
    rowid: i64,
    new_status: &str,
) -> Result<(), rusqlite::Error> {
    // For soft-delete: update status metadata column so KNN filtering excludes it
    // Note: vec0 supports UPDATE on metadata columns
    tx.execute(
        "UPDATE memories_vec0 SET status = ?1 WHERE rowid = ?2",
        rusqlite::params![new_status, rowid],
    )?;
    Ok(())
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| In-memory brute-force cosine over all embeddings | vec0 KNN with disk-backed search | Phase 65 | O(n) scan eliminated; scales beyond memory |
| Post-query status/classification filtering | vec0 metadata columns filter during KNN | Phase 65 (VEC-03) | Correct result counts; fewer candidates processed |
| Separate BLOB + manual similarity | Single vec0 table with distance_metric=cosine | Phase 65 | SQLite-native distance computation |

**Deprecated/outdated:**
- `get_active_embeddings()` + in-memory cosine loop: Replaced by vec0 MATCH query when toggle is on. Method retained for fallback path.
- sqlite-vss (predecessor to sqlite-vec): Deprecated by author in favor of sqlite-vec. Do not use.

## Open Questions

1. **SQLCipher + sqlite-vec runtime compatibility**
   - What we know: sqlite-vec compiles with SQLITE_CORE against standard virtual table APIs. SQLCipher provides these APIs.
   - What's unclear: No documented real-world usage of sqlite-vec with SQLCipher. Could have subtle runtime issues with shadow table creation or encryption of vec0 internal storage.
   - Recommendation: First task must validate this: create encrypted DB, register vec0, create table, insert, query. If it fails, investigate before proceeding.

2. **vec0 UPDATE on metadata columns**
   - What we know: vec0 supports DELETE and INSERT. Metadata columns exist for filtering.
   - What's unclear: Whether UPDATE on metadata columns (status, classification) is supported, or if soft-delete requires DELETE + re-INSERT. sqlite-vec docs do not explicitly document UPDATE behavior.
   - Recommendation: Test UPDATE on metadata columns in the compatibility validation task. If unsupported, use DELETE + INSERT pattern for soft-delete sync.

3. **sqlite-vec crate alpha status**
   - What we know: Latest stable release is 0.1.6. Alpha versions 0.1.7-alpha.x exist but are not recommended for production.
   - What's unclear: When 0.1.7 stable ships. Whether 0.1.6 has any known bugs affecting our use case.
   - Recommendation: Pin to 0.1.6. The crate's API surface is tiny (one function). Risk is manageable.

4. **zerocopy vs vec_to_blob for embedding parameter binding**
   - What we know: Both produce identical little-endian f32 byte sequences. `vec_to_blob()` allocates a new Vec<u8>. `zerocopy::AsBytes` allows zero-copy from `&[f32]`.
   - What's unclear: Whether rusqlite params accept `&[u8]` from AsBytes without intermediate allocation.
   - Recommendation: Start with existing `vec_to_blob()`. If benchmarks show serialization overhead, switch to zerocopy. The allocation is 1536 bytes (384 * 4) -- negligible.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test + criterion 0.5 |
| Config file | crates/blufio/Cargo.toml (existing bench targets) |
| Quick run command | `cargo test -p blufio-memory --lib` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| VEC-01 | vec0 KNN returns ranked results from disk-backed table | unit | `cargo test -p blufio-memory -- vec0` | No -- Wave 0 |
| VEC-01 | vec0 search replaces in-memory path when toggle on | integration | `cargo test -p blufio --test e2e_vec0` | No -- Wave 0 |
| VEC-02 | vec0 works on SQLCipher-encrypted database | integration | `cargo test -p blufio --test e2e_vec0 -- sqlcipher` | No -- Wave 0 |
| VEC-03 | metadata columns filter status/classification during KNN | unit | `cargo test -p blufio-memory -- vec0_filter` | No -- Wave 0 |
| VEC-03 | result count matches filter expectations | unit | `cargo test -p blufio-memory -- vec0_filter_count` | No -- Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p blufio-memory --lib`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/blufio-memory/src/vec0.rs` -- vec0 module (registration, operations, search)
- [ ] `crates/blufio-storage/migrations/V15__vec0_virtual_table.sql` -- vec0 table creation
- [ ] `crates/blufio/tests/e2e_vec0.rs` -- SQLCipher + vec0 integration tests
- [ ] `crates/blufio/benches/bench_vec0.rs` -- vec0 vs in-memory benchmark
- [ ] sqlite-vec dependency: `cargo add sqlite-vec@0.1.6` at workspace level

## Sources

### Primary (HIGH confidence)
- [sqlite-vec GitHub](https://github.com/asg017/sqlite-vec) - Source code, Cargo.toml.tmpl, build.rs, lib.rs, demo.rs
- [sqlite-vec Rust docs](https://alexgarcia.xyz/sqlite-vec/rust.html) - Official Rust usage guide
- [sqlite-vec vec0 docs](https://alexgarcia.xyz/sqlite-vec/features/vec0.html) - Column types, CREATE syntax, metadata filtering
- [sqlite-vec KNN docs](https://alexgarcia.xyz/sqlite-vec/features/knn.html) - MATCH syntax, distance metrics, k parameter
- [sqlite-vec compiling docs](https://alexgarcia.xyz/sqlite-vec/compiling.html) - SQLITE_CORE, SQLITE_VEC_STATIC flags
- [Project source code] - store.rs, retriever.rs, types.rs, database.rs, migrations.rs (direct reads)

### Secondary (MEDIUM confidence)
- [sqlite-vec crates.io](https://crates.io/crates/sqlite-vec) - Version 0.1.6 confirmed, rusqlite ^0.31 dev-dependency
- [docs.rs/sqlite-vec](https://docs.rs/sqlite-vec/0.1.6/sqlite_vec/) - API surface: single exported fn sqlite3_vec_init
- [sqlite-vec metadata blog](https://alexgarcia.xyz/blog/2024/sqlite-vec-metadata-release/index.html) - Metadata column feature announcement
- [rusqlite auto_extension module](https://docs.rs/rusqlite/latest/rusqlite/auto_extension/index.html) - register_auto_extension API

### Tertiary (LOW confidence)
- [sqlite-vec + SQLCipher compatibility] - No direct documentation found. Inference only from SQLITE_CORE compile flag behavior and SQLite extension API compatibility. **Needs validation.**
- [vec0 UPDATE on metadata columns] - Not explicitly documented. **Needs testing.**

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - sqlite-vec crate API verified from source, project dependencies confirmed from Cargo.toml
- Architecture: HIGH - Patterns derived from existing project code (store.rs, retriever.rs, database.rs) and official sqlite-vec examples
- Pitfalls: MEDIUM - SQLCipher compatibility is the key risk; inferred from architecture but not validated
- vec0 SQL syntax: MEDIUM - Documented on sqlite-vec site but vec0 is alpha software
- Integration patterns: HIGH - Based on direct reading of existing codebase

**Research date:** 2026-03-13
**Valid until:** 2026-04-13 (sqlite-vec is alpha; check for breaking changes if delayed)
