// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! sqlite-vec vec0 virtual table operations.
//!
//! Provides registration of the sqlite-vec extension via `sqlite3_auto_extension`,
//! CRUD operations against the `memories_vec0` virtual table, KNN search with
//! metadata filtering (VEC-03), and batch population for startup eager loading.
//!
//! All functions operate on synchronous `rusqlite::Connection` or `rusqlite::Transaction`
//! references, intended to be called inside `tokio_rusqlite::Connection::call()` closures.

use rusqlite::ffi::sqlite3_auto_extension;
use sqlite_vec::sqlite3_vec_init;
use tracing::info;

use crate::types::vec_to_blob;

/// Result of a vec0 KNN search.
#[derive(Debug, Clone)]
pub struct Vec0SearchResult {
    /// The memory's unique identifier (auxiliary column).
    pub memory_id: String,
    /// Cosine similarity score (1.0 - distance), range [−1.0, 1.0].
    pub similarity: f32,
    /// Memory content (auxiliary column).
    pub content: String,
    /// Memory source as string (auxiliary column).
    pub source: String,
    /// Confidence score (auxiliary column).
    pub confidence: f64,
    /// ISO 8601 creation timestamp (auxiliary column).
    pub created_at: String,
}

/// Register the sqlite-vec extension globally via `sqlite3_auto_extension`.
///
/// This is idempotent and process-global. Must be called before any database
/// connections are opened so that the `vec0` virtual table module is available
/// on every connection (including tokio-rusqlite's background thread).
///
/// Safe to call multiple times -- `sqlite3_auto_extension` ignores duplicates.
#[allow(clippy::missing_transmute_annotations)]
pub fn ensure_sqlite_vec_registered() {
    unsafe {
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    }
}

/// Check if the vec0 module is available on a connection.
///
/// Returns `Some(version_string)` (e.g., `"v0.1.6"`) if sqlite-vec is loaded,
/// or `None` if the extension is not registered.
pub fn check_vec0_available(conn: &rusqlite::Connection) -> Option<String> {
    conn.query_row("SELECT vec_version()", [], |row| row.get::<_, String>(0))
        .ok()
}

/// Insert a row into the `memories_vec0` virtual table.
///
/// The `rowid` should match the rowid of the corresponding row in the `memories`
/// table for direct correlation. The `embedding` is serialized to bytes via
/// `vec_to_blob()`.
#[allow(clippy::too_many_arguments)]
pub fn vec0_insert(
    tx: &rusqlite::Transaction,
    rowid: i64,
    status: &str,
    classification: &str,
    session_id: Option<&str>,
    embedding: &[f32],
    memory_id: &str,
    content: &str,
    source: &str,
    confidence: f64,
    created_at: &str,
) -> Result<(), rusqlite::Error> {
    let embedding_bytes = vec_to_blob(embedding);
    tx.execute(
        "INSERT INTO memories_vec0(rowid, status, classification, session_id, \
         embedding, memory_id, content, source, confidence, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            rowid,
            status,
            classification,
            session_id,
            embedding_bytes,
            memory_id,
            content,
            source,
            confidence,
            created_at,
        ],
    )?;
    Ok(())
}

/// Delete a row from the `memories_vec0` virtual table by rowid.
pub fn vec0_delete(tx: &rusqlite::Transaction, rowid: i64) -> Result<(), rusqlite::Error> {
    tx.execute("DELETE FROM memories_vec0 WHERE rowid = ?1", [rowid])?;
    Ok(())
}

/// Update the status metadata column on a vec0 row.
///
/// vec0 supports UPDATE on metadata columns. If this ever changes in a future
/// sqlite-vec version, the fallback would be DELETE + re-INSERT.
///
/// Returns `true` if a row was updated, `false` if no row matched.
pub fn vec0_update_status(
    tx: &rusqlite::Transaction,
    rowid: i64,
    new_status: &str,
) -> Result<bool, rusqlite::Error> {
    let affected = tx.execute(
        "UPDATE memories_vec0 SET status = ?1 WHERE rowid = ?2",
        rusqlite::params![new_status, rowid],
    )?;
    Ok(affected > 0)
}

/// Perform a KNN search on the `memories_vec0` virtual table.
///
/// Metadata columns filter `status = 'active'` and `classification != 'restricted'`
/// **during** KNN (VEC-03), not post-query. The distance is converted to similarity
/// in Rust: `similarity = 1.0 - distance`. Results below `similarity_threshold` are
/// excluded.
///
/// Optionally filters by `session_id` (partition key).
pub fn vec0_search(
    conn: &rusqlite::Connection,
    query_embedding: &[f32],
    k: usize,
    similarity_threshold: f64,
    session_id: Option<&str>,
) -> Result<Vec<Vec0SearchResult>, rusqlite::Error> {
    let embedding_bytes = vec_to_blob(query_embedding);

    let sql = match session_id {
        Some(_) => {
            "SELECT rowid, distance, memory_id, content, source, confidence, created_at \
             FROM memories_vec0 \
             WHERE embedding MATCH ?1 \
               AND k = ?2 \
               AND status = 'active' \
               AND classification != 'restricted' \
               AND session_id = ?3"
        }
        None => {
            "SELECT rowid, distance, memory_id, content, source, confidence, created_at \
             FROM memories_vec0 \
             WHERE embedding MATCH ?1 \
               AND k = ?2 \
               AND status = 'active' \
               AND classification != 'restricted'"
        }
    };

    let mut stmt = conn.prepare(sql)?;

    let results: Vec<Vec0SearchResult> = match session_id {
        Some(sid) => {
            let rows = stmt.query_map(
                rusqlite::params![embedding_bytes, k as i64, sid],
                map_search_row,
            )?;
            rows.filter_map(|r| r.ok())
                .filter(|r| r.similarity as f64 >= similarity_threshold)
                .collect()
        }
        None => {
            let rows =
                stmt.query_map(rusqlite::params![embedding_bytes, k as i64], map_search_row)?;
            rows.filter_map(|r| r.ok())
                .filter(|r| r.similarity as f64 >= similarity_threshold)
                .collect()
        }
    };

    Ok(results)
}

/// Map a result row from a vec0 KNN query to a [`Vec0SearchResult`].
fn map_search_row(row: &rusqlite::Row) -> Result<Vec0SearchResult, rusqlite::Error> {
    let distance: f64 = row.get(1)?;
    let similarity = 1.0 - distance as f32;
    Ok(Vec0SearchResult {
        memory_id: row.get(2)?,
        similarity,
        content: row.get(3)?,
        source: row.get(4)?,
        confidence: row.get(5)?,
        created_at: row.get(6)?,
    })
}

/// Populate the `memories_vec0` table from existing `memories` rows.
///
/// Performs an idempotent batch insert: only rows in `memories` that do NOT
/// already exist in `memories_vec0` are inserted. Processes in batches of
/// `batch_size` rows. Logs progress at info level.
///
/// Returns `(populated_count, total_active)`.
pub fn vec0_populate_batch(
    conn: &rusqlite::Connection,
    batch_size: usize,
) -> Result<(usize, usize), rusqlite::Error> {
    let total_active: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories \
         WHERE status = 'active' AND classification != 'restricted' AND deleted_at IS NULL",
        [],
        |row| row.get(0),
    )?;

    let mut populated: usize = 0;
    let mut offset: i64 = 0;

    loop {
        let mut stmt = conn.prepare(
            "SELECT m.rowid, m.id, m.content, m.embedding, m.source, m.confidence, \
             m.status, m.classification, m.session_id, m.created_at \
             FROM memories m \
             LEFT JOIN memories_vec0 v ON v.rowid = m.rowid \
             WHERE m.status = 'active' AND m.classification != 'restricted' \
             AND m.deleted_at IS NULL AND v.rowid IS NULL \
             ORDER BY m.rowid LIMIT ?1 OFFSET ?2",
        )?;

        #[allow(clippy::type_complexity)]
        let rows: Vec<(
            i64,
            String,
            String,
            Vec<u8>,
            String,
            f64,
            String,
            String,
            Option<String>,
            String,
        )> = stmt
            .query_map(rusqlite::params![batch_size as i64, offset], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        if rows.is_empty() {
            break;
        }

        let batch_count = rows.len();
        let tx = conn.unchecked_transaction()?;
        for (
            rowid,
            memory_id,
            content,
            embedding_blob,
            source,
            confidence,
            status,
            classification,
            session_id,
            created_at,
        ) in &rows
        {
            tx.execute(
                "INSERT INTO memories_vec0(rowid, status, classification, session_id, \
                 embedding, memory_id, content, source, confidence, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    rowid,
                    status,
                    classification,
                    session_id,
                    embedding_blob,
                    memory_id,
                    content,
                    source,
                    confidence,
                    created_at,
                ],
            )?;
        }
        tx.commit()?;

        populated += batch_count;
        info!(
            "Populating vec0: {}/{} memories...",
            populated, total_active
        );
        offset += batch_count as i64;
    }

    Ok((populated, total_active as usize))
}

/// Count the number of rows in the `memories_vec0` table.
pub fn vec0_count(conn: &rusqlite::Connection) -> Result<usize, rusqlite::Error> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM memories_vec0", [], |row| row.get(0))?;
    Ok(count as usize)
}

/// Drop and recreate the `memories_vec0` virtual table.
///
/// Used by the `blufio memory rebuild-vec0` CLI command for recovery.
pub fn vec0_drop_and_recreate(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch("DROP TABLE IF EXISTS memories_vec0;")?;
    conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS memories_vec0 USING vec0(\
            status text, \
            classification text, \
            session_id text partition key, \
            embedding float[384] distance_metric=cosine, \
            +memory_id text, \
            +content text, \
            +source text, \
            +confidence float, \
            +created_at text\
        );",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// Create an in-memory connection with sqlite-vec registered and vec0 table created.
    fn setup_test_db() -> Connection {
        ensure_sqlite_vec_registered();
        let conn = Connection::open_in_memory().unwrap();

        // Create the memories table (simplified schema matching what vec0 needs)
        conn.execute_batch(
            "CREATE TABLE memories (
                id TEXT NOT NULL PRIMARY KEY,
                content TEXT NOT NULL,
                embedding BLOB NOT NULL,
                source TEXT NOT NULL DEFAULT 'extracted',
                confidence REAL NOT NULL DEFAULT 0.5,
                status TEXT NOT NULL DEFAULT 'active',
                superseded_by TEXT,
                session_id TEXT,
                classification TEXT NOT NULL DEFAULT 'internal',
                created_at TEXT NOT NULL DEFAULT '',
                updated_at TEXT NOT NULL DEFAULT '',
                deleted_at TEXT
            );",
        )
        .unwrap();

        // Create the vec0 virtual table (same as V15 migration).
        // Note: vec0 auxiliary columns accept float/double but NOT "real".
        conn.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS memories_vec0 USING vec0(\
            status text, \
            classification text, \
            session_id text partition key, \
            embedding float[384] distance_metric=cosine, \
            +memory_id text, \
            +content text, \
            +source text, \
            +confidence float, \
            +created_at text\
            );",
        )
        .unwrap();

        conn
    }

    /// Generate a normalized synthetic 384-dim embedding.
    /// Uses a simple pattern: set element at `index` to `value`, rest to small noise.
    fn synthetic_embedding(seed: u32) -> Vec<f32> {
        let mut emb = vec![0.0f32; 384];
        // Create a somewhat unique vector based on seed
        for (i, val) in emb.iter_mut().enumerate() {
            *val = ((seed as f32 * 0.1 + i as f32 * 0.01).sin()) * 0.1;
        }
        // Normalize to unit length
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut emb {
                *x /= norm;
            }
        }
        emb
    }

    /// Generate a very similar embedding (small perturbation of the base).
    fn similar_embedding(base: &[f32], perturbation: f32) -> Vec<f32> {
        let mut emb: Vec<f32> = base.iter().map(|&x| x + perturbation * 0.01).collect();
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut emb {
                *x /= norm;
            }
        }
        emb
    }

    #[test]
    fn ensure_sqlite_vec_registered_and_version() {
        ensure_sqlite_vec_registered();
        let conn = Connection::open_in_memory().unwrap();
        let version: String = conn
            .query_row("SELECT vec_version()", [], |row| row.get(0))
            .unwrap();
        assert!(
            version.starts_with("v"),
            "expected version string starting with 'v', got: {version}"
        );
    }

    #[test]
    fn check_vec0_available_returns_version() {
        ensure_sqlite_vec_registered();
        let conn = Connection::open_in_memory().unwrap();
        let version = check_vec0_available(&conn);
        assert!(version.is_some());
        assert!(version.unwrap().starts_with("v"));
    }

    #[test]
    fn vec0_insert_succeeds() {
        let mut conn = setup_test_db();
        let emb = synthetic_embedding(1);

        let tx = conn.transaction().unwrap();
        vec0_insert(
            &tx,
            1,
            "active",
            "internal",
            Some("sess-1"),
            &emb,
            "mem-1",
            "User likes coffee",
            "explicit",
            0.9,
            "2026-03-13T00:00:00Z",
        )
        .unwrap();
        tx.commit().unwrap();

        // Verify row exists
        let count = vec0_count(&conn).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn vec0_knn_search_returns_ordered_results() {
        let mut conn = setup_test_db();

        let base_emb = synthetic_embedding(1);
        let close_emb = similar_embedding(&base_emb, 0.1);
        let far_emb = synthetic_embedding(100); // very different

        // Insert three memories
        let tx = conn.transaction().unwrap();
        vec0_insert(
            &tx,
            1,
            "active",
            "internal",
            None,
            &base_emb,
            "mem-base",
            "Base memory",
            "explicit",
            0.9,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        vec0_insert(
            &tx,
            2,
            "active",
            "internal",
            None,
            &close_emb,
            "mem-close",
            "Close memory",
            "explicit",
            0.8,
            "2026-01-02T00:00:00Z",
        )
        .unwrap();
        vec0_insert(
            &tx,
            3,
            "active",
            "internal",
            None,
            &far_emb,
            "mem-far",
            "Far memory",
            "explicit",
            0.7,
            "2026-01-03T00:00:00Z",
        )
        .unwrap();
        tx.commit().unwrap();

        // Search with the base embedding -- should find closest first
        let results = vec0_search(&conn, &base_emb, 10, 0.0, None).unwrap();

        assert!(!results.is_empty(), "search should return results");
        // The first result should be the exact match (mem-base)
        assert_eq!(results[0].memory_id, "mem-base");
        assert!(
            results[0].similarity > 0.99,
            "exact match should have similarity ~1.0, got {}",
            results[0].similarity
        );

        // Second result should be the close embedding
        if results.len() > 1 {
            assert_eq!(results[1].memory_id, "mem-close");
            assert!(
                results[1].similarity > results.last().unwrap().similarity || results.len() == 2
            );
        }
    }

    #[test]
    fn vec0_knn_search_distance_to_similarity_conversion() {
        let mut conn = setup_test_db();
        let emb = synthetic_embedding(1);

        let tx = conn.transaction().unwrap();
        vec0_insert(
            &tx,
            1,
            "active",
            "internal",
            None,
            &emb,
            "mem-1",
            "Test",
            "explicit",
            0.9,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        tx.commit().unwrap();

        // Searching with the exact same embedding should yield similarity ~1.0
        let results = vec0_search(&conn, &emb, 10, 0.0, None).unwrap();
        assert_eq!(results.len(), 1);
        assert!(
            results[0].similarity > 0.99,
            "similarity should be ~1.0 for exact match, got {}",
            results[0].similarity
        );
    }

    #[test]
    fn vec0_knn_search_with_session_id_filter() {
        let mut conn = setup_test_db();
        let emb1 = synthetic_embedding(1);
        let emb2 = synthetic_embedding(2);

        let tx = conn.transaction().unwrap();
        vec0_insert(
            &tx,
            1,
            "active",
            "internal",
            Some("sess-A"),
            &emb1,
            "mem-1",
            "Session A memory",
            "explicit",
            0.9,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        vec0_insert(
            &tx,
            2,
            "active",
            "internal",
            Some("sess-B"),
            &emb2,
            "mem-2",
            "Session B memory",
            "explicit",
            0.8,
            "2026-01-02T00:00:00Z",
        )
        .unwrap();
        tx.commit().unwrap();

        // Search with session_id filter -- should only return sess-A
        let results = vec0_search(&conn, &emb1, 10, 0.0, Some("sess-A")).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].memory_id, "mem-1");
    }

    #[test]
    fn vec0_delete_removes_row() {
        let mut conn = setup_test_db();
        let emb = synthetic_embedding(1);

        let tx = conn.transaction().unwrap();
        vec0_insert(
            &tx,
            1,
            "active",
            "internal",
            None,
            &emb,
            "mem-1",
            "Test",
            "explicit",
            0.9,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        tx.commit().unwrap();

        assert_eq!(vec0_count(&conn).unwrap(), 1);

        // Delete the row
        let tx = conn.transaction().unwrap();
        vec0_delete(&tx, 1).unwrap();
        tx.commit().unwrap();

        assert_eq!(vec0_count(&conn).unwrap(), 0);

        // Search should return no results
        let results = vec0_search(&conn, &emb, 10, 0.0, None).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn vec0_update_status_changes_metadata() {
        let mut conn = setup_test_db();
        let emb = synthetic_embedding(1);

        let tx = conn.transaction().unwrap();
        vec0_insert(
            &tx,
            1,
            "active",
            "internal",
            None,
            &emb,
            "mem-1",
            "Test",
            "explicit",
            0.9,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        tx.commit().unwrap();

        // Should be findable with status='active' filter
        let results = vec0_search(&conn, &emb, 10, 0.0, None).unwrap();
        assert_eq!(results.len(), 1);

        // Update status to 'forgotten'
        let tx = conn.transaction().unwrap();
        let updated = vec0_update_status(&tx, 1, "forgotten").unwrap();
        tx.commit().unwrap();
        assert!(updated, "update should affect one row");

        // Now search with status='active' filter should return empty
        let results = vec0_search(&conn, &emb, 10, 0.0, None).unwrap();
        assert!(
            results.is_empty(),
            "forgotten memory should not appear in active search"
        );
    }

    #[test]
    fn vec0_search_mixed_status_returns_only_active() {
        let mut conn = setup_test_db();
        let emb1 = synthetic_embedding(1);
        let emb2 = synthetic_embedding(2);
        let emb3 = synthetic_embedding(3);

        let tx = conn.transaction().unwrap();
        vec0_insert(
            &tx,
            1,
            "active",
            "internal",
            None,
            &emb1,
            "mem-active",
            "Active memory",
            "explicit",
            0.9,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        vec0_insert(
            &tx,
            2,
            "forgotten",
            "internal",
            None,
            &emb2,
            "mem-forgotten",
            "Forgotten memory",
            "explicit",
            0.8,
            "2026-01-02T00:00:00Z",
        )
        .unwrap();
        vec0_insert(
            &tx,
            3,
            "superseded",
            "internal",
            None,
            &emb3,
            "mem-superseded",
            "Superseded memory",
            "explicit",
            0.7,
            "2026-01-03T00:00:00Z",
        )
        .unwrap();
        tx.commit().unwrap();

        // Search should only return the active memory
        let results = vec0_search(&conn, &emb1, 10, 0.0, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].memory_id, "mem-active");
    }

    #[test]
    fn vec0_search_restricted_classification_excluded() {
        let mut conn = setup_test_db();
        let emb1 = synthetic_embedding(1);
        let emb2 = synthetic_embedding(2);

        let tx = conn.transaction().unwrap();
        vec0_insert(
            &tx,
            1,
            "active",
            "internal",
            None,
            &emb1,
            "mem-internal",
            "Internal memory",
            "explicit",
            0.9,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        vec0_insert(
            &tx,
            2,
            "active",
            "restricted",
            None,
            &emb2,
            "mem-restricted",
            "Restricted memory",
            "explicit",
            0.8,
            "2026-01-02T00:00:00Z",
        )
        .unwrap();
        tx.commit().unwrap();

        // Search should exclude restricted
        let results = vec0_search(&conn, &emb1, 10, 0.0, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].memory_id, "mem-internal");
    }

    #[test]
    fn vec0_search_similarity_threshold_filters() {
        let mut conn = setup_test_db();
        let emb1 = synthetic_embedding(1);
        let emb2 = synthetic_embedding(100); // very different

        let tx = conn.transaction().unwrap();
        vec0_insert(
            &tx,
            1,
            "active",
            "internal",
            None,
            &emb1,
            "mem-close",
            "Close",
            "explicit",
            0.9,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        vec0_insert(
            &tx,
            2,
            "active",
            "internal",
            None,
            &emb2,
            "mem-far",
            "Far",
            "explicit",
            0.8,
            "2026-01-02T00:00:00Z",
        )
        .unwrap();
        tx.commit().unwrap();

        // Search with high threshold -- only exact/near match should pass
        let results = vec0_search(&conn, &emb1, 10, 0.99, None).unwrap();
        assert_eq!(
            results.len(),
            1,
            "only exact match should pass 0.99 threshold"
        );
        assert_eq!(results[0].memory_id, "mem-close");
    }

    #[test]
    fn vec0_count_works() {
        let mut conn = setup_test_db();
        assert_eq!(vec0_count(&conn).unwrap(), 0);

        let emb = synthetic_embedding(1);
        let tx = conn.transaction().unwrap();
        vec0_insert(
            &tx,
            1,
            "active",
            "internal",
            None,
            &emb,
            "mem-1",
            "Test",
            "explicit",
            0.9,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        tx.commit().unwrap();

        assert_eq!(vec0_count(&conn).unwrap(), 1);
    }

    #[test]
    fn vec0_drop_and_recreate_works() {
        let mut conn = setup_test_db();
        let emb = synthetic_embedding(1);

        let tx = conn.transaction().unwrap();
        vec0_insert(
            &tx,
            1,
            "active",
            "internal",
            None,
            &emb,
            "mem-1",
            "Test",
            "explicit",
            0.9,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        tx.commit().unwrap();

        assert_eq!(vec0_count(&conn).unwrap(), 1);

        // Drop and recreate
        vec0_drop_and_recreate(&conn).unwrap();

        assert_eq!(vec0_count(&conn).unwrap(), 0);

        // Should still be usable after recreate
        let tx = conn.transaction().unwrap();
        vec0_insert(
            &tx,
            1,
            "active",
            "internal",
            None,
            &emb,
            "mem-1",
            "Test",
            "explicit",
            0.9,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        tx.commit().unwrap();
        assert_eq!(vec0_count(&conn).unwrap(), 1);
    }

    #[test]
    fn vec0_populate_batch_idempotent() {
        let conn = setup_test_db();

        // Insert test data into the memories table
        let emb1 = synthetic_embedding(1);
        let emb2 = synthetic_embedding(2);
        let emb3 = synthetic_embedding(3);
        let blob1 = vec_to_blob(&emb1);
        let blob2 = vec_to_blob(&emb2);
        let blob3 = vec_to_blob(&emb3);

        conn.execute(
            "INSERT INTO memories (id, content, embedding, source, confidence, status, classification, created_at, updated_at) \
             VALUES ('mem-1', 'Memory 1', ?1, 'explicit', 0.9, 'active', 'internal', '2026-01-01', '2026-01-01')",
            [&blob1],
        ).unwrap();
        conn.execute(
            "INSERT INTO memories (id, content, embedding, source, confidence, status, classification, created_at, updated_at) \
             VALUES ('mem-2', 'Memory 2', ?1, 'extracted', 0.8, 'active', 'internal', '2026-01-02', '2026-01-02')",
            [&blob2],
        ).unwrap();
        // This one is 'forgotten' -- should NOT be populated
        conn.execute(
            "INSERT INTO memories (id, content, embedding, source, confidence, status, classification, created_at, updated_at) \
             VALUES ('mem-3', 'Memory 3', ?1, 'explicit', 0.7, 'forgotten', 'internal', '2026-01-03', '2026-01-03')",
            [&blob3],
        ).unwrap();

        // First population
        let (populated, total) = vec0_populate_batch(&conn, 10).unwrap();
        assert_eq!(populated, 2, "should populate 2 active memories");
        assert_eq!(total, 2, "total active should be 2");
        assert_eq!(vec0_count(&conn).unwrap(), 2);

        // Second population should be idempotent (no new rows)
        let (populated2, _) = vec0_populate_batch(&conn, 10).unwrap();
        assert_eq!(populated2, 0, "second run should populate 0 (idempotent)");
        assert_eq!(vec0_count(&conn).unwrap(), 2);
    }
}
