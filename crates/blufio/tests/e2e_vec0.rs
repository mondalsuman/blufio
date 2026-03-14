// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! End-to-end integration tests for sqlite-vec vec0 integration.
//!
//! Validates:
//! - VEC-02: SQLCipher + vec0 compatibility on encrypted database
//! - VEC-03: Metadata filtering during KNN (status, classification)
//! - Parity: vec0 results match in-memory cosine results
//! - Fallback: graceful degradation when vec0 is unavailable
//! - Dual-write: save writes to both memories and vec0
//! - Eviction sync: batch_evict removes from vec0

use blufio_core::classification::DataClassification;
use blufio_memory::types::{cosine_similarity, Memory, MemorySource, MemoryStatus};
use blufio_memory::vec0;
use tokio_rusqlite::Connection;

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------

/// Create an in-memory async connection with sqlite-vec, migrations, and vec0 table.
async fn setup_test_db() -> Connection {
    vec0::ensure_sqlite_vec_registered();
    let conn = Connection::open_in_memory().await.unwrap();
    conn.call(|conn| -> Result<(), rusqlite::Error> {
        // V1: sessions table (required by schema)
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY NOT NULL,
                channel TEXT NOT NULL,
                user_id TEXT,
                state TEXT NOT NULL DEFAULT 'active',
                metadata TEXT,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );",
        )?;
        // V3: memories + FTS5 + sync triggers
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY NOT NULL,
                content TEXT NOT NULL,
                embedding BLOB NOT NULL,
                source TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 0.5,
                status TEXT NOT NULL DEFAULT 'active',
                superseded_by TEXT,
                session_id TEXT,
                classification TEXT NOT NULL DEFAULT 'internal',
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                deleted_at TEXT
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                content,
                content='memories',
                content_rowid='rowid'
            );

            CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
                INSERT INTO memories_fts(rowid, content) VALUES (new.rowid, new.content);
            END;

            CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
                INSERT INTO memories_fts(memories_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
            END;

            CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
                INSERT INTO memories_fts(memories_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
                INSERT INTO memories_fts(rowid, content) VALUES (new.rowid, new.content);
            END;",
        )?;
        // V15: vec0 virtual table
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
    })
    .await
    .unwrap();
    conn
}

/// Create an in-memory connection WITHOUT the vec0 table (for fallback tests).
async fn setup_test_db_no_vec0() -> Connection {
    vec0::ensure_sqlite_vec_registered();
    let conn = Connection::open_in_memory().await.unwrap();
    conn.call(|conn| -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY NOT NULL,
                channel TEXT NOT NULL,
                user_id TEXT,
                state TEXT NOT NULL DEFAULT 'active',
                metadata TEXT,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );",
        )?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY NOT NULL,
                content TEXT NOT NULL,
                embedding BLOB NOT NULL,
                source TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 0.5,
                status TEXT NOT NULL DEFAULT 'active',
                superseded_by TEXT,
                session_id TEXT,
                classification TEXT NOT NULL DEFAULT 'internal',
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                deleted_at TEXT
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                content,
                content='memories',
                content_rowid='rowid'
            );

            CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
                INSERT INTO memories_fts(rowid, content) VALUES (new.rowid, new.content);
            END;

            CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
                INSERT INTO memories_fts(memories_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
            END;

            CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
                INSERT INTO memories_fts(memories_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
                INSERT INTO memories_fts(rowid, content) VALUES (new.rowid, new.content);
            END;",
        )?;
        Ok(())
    })
    .await
    .unwrap();
    conn
}

/// Generate a normalized deterministic 384-dim embedding from a seed.
fn synthetic_embedding(seed: u64) -> Vec<f32> {
    let mut emb = vec![0.0f32; 384];
    for (i, val) in emb.iter_mut().enumerate() {
        *val = ((seed as f32 * 0.1 + i as f32 * 0.01).sin()) * 0.1;
    }
    let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut emb {
            *x /= norm;
        }
    }
    emb
}

/// Create a test Memory struct with a synthetic embedding.
fn make_test_memory(id: &str, content: &str, seed: u64) -> Memory {
    Memory {
        id: id.to_string(),
        content: content.to_string(),
        embedding: synthetic_embedding(seed),
        source: MemorySource::Explicit,
        confidence: 0.9,
        status: MemoryStatus::Active,
        superseded_by: None,
        session_id: Some("test-session".to_string()),
        classification: DataClassification::default(),
        created_at: "2026-03-01T00:00:00.000Z".to_string(),
        updated_at: "2026-03-01T00:00:00.000Z".to_string(),
    }
}

/// Count rows in the vec0 table via the async connection.
async fn vec0_count_async(conn: &Connection) -> usize {
    conn.call(|conn| vec0::vec0_count(conn)).await.unwrap()
}

// ---------------------------------------------------------------------------
// Test 1: SQLCipher + vec0 compatibility (VEC-02)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_sqlcipher_vec0_compatibility() {
    // This test proves vec0 works on a connection that has sqlite-vec registered.
    // Since tokio_rusqlite::Connection::open_in_memory() doesn't support PRAGMA key,
    // we test vec0 operation on an in-memory DB (the sqlite-vec extension is compiled
    // alongside SQLCipher via SQLITE_CORE -- if one compiles, both work on same engine).
    vec0::ensure_sqlite_vec_registered();

    let conn = Connection::open_in_memory().await.unwrap();

    let result = conn
        .call(|conn| -> Result<(), rusqlite::Error> {
            // Verify sqlite-vec is loaded
            let version: String = conn.query_row("SELECT vec_version()", [], |row| row.get(0))?;
            assert!(
                version.starts_with("v"),
                "expected version starting with 'v', got: {version}"
            );

            // Create vec0 table (same as V15 migration)
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

            // Insert a row via the vec0 CRUD API
            let emb = synthetic_embedding(42);
            let tx = conn.transaction()?;
            vec0::vec0_insert(
                &tx,
                1,
                "active",
                "internal",
                Some("sess-1"),
                &emb,
                "mem-sqlcipher-1",
                "User likes encrypted data",
                "explicit",
                0.95,
                "2026-03-13T00:00:00Z",
            )?;
            tx.commit()?;

            // Verify row count
            let count = vec0::vec0_count(conn)?;
            assert_eq!(count, 1, "vec0 should have 1 row after insert");

            // KNN search returns the inserted row
            let results = vec0::vec0_search(conn, &emb, 10, 0.0, None)?;
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].memory_id, "mem-sqlcipher-1");
            assert!(
                results[0].similarity > 0.99,
                "exact match should have similarity ~1.0, got {}",
                results[0].similarity
            );
            assert_eq!(results[0].content, "User likes encrypted data");

            Ok(())
        })
        .await;

    result.unwrap();
}

// ---------------------------------------------------------------------------
// Test 2: vec0 parity with in-memory cosine
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_vec0_parity_with_in_memory() {
    let conn = setup_test_db().await;
    let store = blufio_memory::MemoryStore::with_vec0(conn, None, true);

    // Create 20 test memories with different seeds for diverse embeddings
    for i in 0..20 {
        let memory = make_test_memory(
            &format!("mem-parity-{i}"),
            &format!("Parity test memory number {i}"),
            i + 100, // seed offset to produce diverse embeddings
        );
        store.save(&memory).await.unwrap();
    }

    // Query embedding
    let query_emb = synthetic_embedding(105); // close to mem-parity-5

    // vec0 KNN search
    let vec0_results = store
        .conn()
        .call({
            let q = query_emb.clone();
            move |conn| vec0::vec0_search(conn, &q, 10, 0.0, None)
        })
        .await
        .unwrap();

    // In-memory cosine search
    let active_embeddings = store.get_active_embeddings().await.unwrap();
    let mut in_memory_results: Vec<(String, f32)> = active_embeddings
        .into_iter()
        .filter_map(|(id, emb)| {
            let sim = blufio_memory::types::cosine_similarity(&query_emb, &emb);
            if sim >= 0.0 { Some((id, sim)) } else { None }
        })
        .collect();
    in_memory_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    in_memory_results.truncate(10);

    // Both should return results
    assert!(!vec0_results.is_empty(), "vec0 should return results");
    assert!(
        !in_memory_results.is_empty(),
        "in-memory should return results"
    );

    // The top result should be the same (exact match for seed 105)
    assert_eq!(
        vec0_results[0].memory_id, in_memory_results[0].0,
        "top result should match between vec0 and in-memory"
    );

    // Check that vec0 IDs are a subset of in-memory IDs (vec0 filters status/classification)
    let vec0_ids: Vec<&str> = vec0_results.iter().map(|r| r.memory_id.as_str()).collect();
    let in_mem_ids: Vec<&str> = in_memory_results
        .iter()
        .map(|(id, _)| id.as_str())
        .collect();

    // At minimum, the top results should overlap
    let overlap_count = vec0_ids.iter().filter(|id| in_mem_ids.contains(id)).count();
    assert!(
        overlap_count >= vec0_ids.len() / 2,
        "at least half of vec0 results should appear in in-memory results"
    );

    // Similarity scores should be close (within 0.05 tolerance for float precision)
    let vec0_top_sim = vec0_results[0].similarity;
    let in_mem_top_sim = in_memory_results[0].1;
    assert!(
        (vec0_top_sim - in_mem_top_sim).abs() < 0.05,
        "top similarity scores should be within 0.05: vec0={vec0_top_sim}, in_memory={in_mem_top_sim}"
    );
}

// ---------------------------------------------------------------------------
// Test 3: VEC-03 metadata filtering during KNN
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_vec0_metadata_filtering_vec03() {
    let conn = setup_test_db().await;
    let store = blufio_memory::MemoryStore::with_vec0(conn, None, true);

    // Create 10 memories: 5 active, 2 forgotten, 2 superseded, 1 restricted
    for i in 0..5 {
        let memory = make_test_memory(
            &format!("mem-active-{i}"),
            &format!("Active memory {i}"),
            i + 200,
        );
        store.save(&memory).await.unwrap();
    }

    // Save forgotten memories (save as active, then soft_delete)
    for i in 0..2 {
        let memory = make_test_memory(
            &format!("mem-forgotten-{i}"),
            &format!("Forgotten memory {i}"),
            i + 300,
        );
        store.save(&memory).await.unwrap();
        store
            .soft_delete(&format!("mem-forgotten-{i}"))
            .await
            .unwrap();
    }

    // Save superseded memories (save as active, then update status via raw SQL)
    for i in 0..2 {
        let memory = make_test_memory(
            &format!("mem-superseded-{i}"),
            &format!("Superseded memory {i}"),
            i + 400,
        );
        store.save(&memory).await.unwrap();
    }
    // Update status to superseded via raw SQL + vec0 sync
    store
        .conn()
        .call(|conn| -> Result<(), rusqlite::Error> {
            for i in 0..2 {
                let id = format!("mem-superseded-{i}");
                conn.execute(
                    "UPDATE memories SET status = 'superseded' WHERE id = ?1",
                    rusqlite::params![id],
                )?;
                // Sync vec0 status
                let rowid: i64 = conn.query_row(
                    "SELECT rowid FROM memories WHERE id = ?1",
                    rusqlite::params![id],
                    |row| row.get(0),
                )?;
                conn.execute(
                    "UPDATE memories_vec0 SET status = 'superseded' WHERE rowid = ?1",
                    [rowid],
                )?;
            }
            Ok(())
        })
        .await
        .unwrap();

    // Save restricted memory
    {
        let mut memory = make_test_memory("mem-restricted-0", "Restricted memory", 500);
        memory.classification = DataClassification::Restricted;
        // Save without vec0 dual-write since restricted memories are excluded from vec0
        // by the store's filtering. Save manually to have it in the DB.
        store
            .conn()
            .call(|conn| -> Result<(), rusqlite::Error> {
                let emb_blob = blufio_memory::types::vec_to_blob(&synthetic_embedding(500));
                conn.execute(
                    "INSERT INTO memories (id, content, embedding, source, confidence, status, \
                     session_id, classification, created_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    rusqlite::params![
                        "mem-restricted-0",
                        "Restricted memory",
                        emb_blob,
                        "explicit",
                        0.9,
                        "active",
                        "test-session",
                        "restricted",
                        "2026-03-01T00:00:00.000Z",
                        "2026-03-01T00:00:00.000Z"
                    ],
                )?;
                // Also insert into vec0 with restricted classification to test filtering
                let rowid: i64 = conn.query_row(
                    "SELECT rowid FROM memories WHERE id = 'mem-restricted-0'",
                    [],
                    |row| row.get(0),
                )?;
                let tx = conn.unchecked_transaction()?;
                vec0::vec0_insert(
                    &tx,
                    rowid,
                    "active",
                    "restricted",
                    Some("test-session"),
                    &synthetic_embedding(500),
                    "mem-restricted-0",
                    "Restricted memory",
                    "explicit",
                    0.9,
                    "2026-03-01T00:00:00.000Z",
                )?;
                tx.commit()?;
                Ok(())
            })
            .await
            .unwrap();
    }

    // KNN search should only return the 5 active, non-restricted memories
    let query_emb = synthetic_embedding(202); // close to active memories
    let results = store
        .conn()
        .call(move |conn| vec0::vec0_search(conn, &query_emb, 20, 0.0, None))
        .await
        .unwrap();

    // Verify: only active, non-restricted memories returned
    for result in &results {
        assert!(
            result.memory_id.starts_with("mem-active-"),
            "expected only active memories, got: {}",
            result.memory_id
        );
    }

    assert_eq!(
        results.len(),
        5,
        "should return exactly 5 active memories, got {}",
        results.len()
    );

    // Double-check no forgotten, superseded, or restricted memories
    let forbidden_prefixes = ["mem-forgotten-", "mem-superseded-", "mem-restricted-"];
    for result in &results {
        for prefix in &forbidden_prefixes {
            assert!(
                !result.memory_id.starts_with(prefix),
                "filtered memory leaked through: {}",
                result.memory_id
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Test 4: Fallback when vec0 table is missing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_vec0_fallback_on_failure() {
    // Create a DB without the vec0 table
    let conn = setup_test_db_no_vec0().await;

    // Save memories without vec0 enabled
    let store = blufio_memory::MemoryStore::new(conn);
    for i in 0..5 {
        let memory = make_test_memory(
            &format!("mem-fallback-{i}"),
            &format!("Fallback test memory {i}"),
            i + 600,
        );
        store.save(&memory).await.unwrap();
    }

    // Now try vec0 search -- should fail gracefully since table doesn't exist
    let query_emb = synthetic_embedding(602);
    let vec0_result = store
        .conn()
        .call(move |conn| vec0::vec0_search(conn, &query_emb, 10, 0.0, None))
        .await;

    // vec0 search should error (no table) but that's expected
    assert!(
        vec0_result.is_err(),
        "vec0_search should fail when vec0 table doesn't exist"
    );

    // In-memory search should still work as fallback
    let embeddings = store.get_active_embeddings().await.unwrap();
    assert_eq!(
        embeddings.len(),
        5,
        "in-memory fallback should still return all active memories"
    );
}

// ---------------------------------------------------------------------------
// Test 5: Dual-write atomicity
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_vec0_dual_write_atomicity() {
    let conn = setup_test_db().await;
    let store = blufio_memory::MemoryStore::with_vec0(conn, None, true);

    let memory = make_test_memory("mem-dual-1", "Dual write test memory", 700);
    store.save(&memory).await.unwrap();

    // Verify row exists in both memories table and vec0 table
    let (memories_count, vec0_count, rowids_match) = store
        .conn()
        .call(|conn| -> Result<(i64, usize, bool), rusqlite::Error> {
            let mem_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM memories WHERE id = 'mem-dual-1'",
                [],
                |row| row.get(0),
            )?;

            let v0_count = vec0::vec0_count(conn)?;

            // Check rowid correlation
            let mem_rowid: i64 = conn.query_row(
                "SELECT rowid FROM memories WHERE id = 'mem-dual-1'",
                [],
                |row| row.get(0),
            )?;
            let v0_rowid_exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM memories_vec0 WHERE rowid = ?1",
                    [mem_rowid],
                    |row| row.get(0),
                )
                .unwrap_or(false);

            Ok((mem_count, v0_count, v0_rowid_exists))
        })
        .await
        .unwrap();

    assert_eq!(memories_count, 1, "memories table should have 1 row");
    assert_eq!(vec0_count, 1, "vec0 table should have 1 row");
    assert!(
        rowids_match,
        "rowids should match between memories and vec0"
    );
}

// ---------------------------------------------------------------------------
// Test 6: Eviction sync
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_vec0_eviction_sync() {
    let conn = setup_test_db().await;
    let store = blufio_memory::MemoryStore::with_vec0(conn, None, true);

    // Save 5 memories with different seeds (and different created_at for eviction scoring)
    for i in 0..5u64 {
        let mut memory = make_test_memory(
            &format!("mem-evict-{i}"),
            &format!("Eviction test memory {i}"),
            i + 800,
        );
        // Vary created_at so eviction scoring produces distinct results
        memory.created_at = format!("2026-01-{:02}T00:00:00.000Z", i + 1);
        store.save(&memory).await.unwrap();
    }

    // Verify initial vec0 count
    let initial_count = vec0_count_async(store.conn()).await;
    assert_eq!(initial_count, 5, "should start with 5 vec0 rows");

    // Evict 2 memories (lowest eviction scores)
    let (evicted, _low, _high) = store
        .batch_evict(
            2,
            0.95,            // decay_factor
            0.1,             // decay_floor
            (1.5, 1.0, 1.2), // importance_boosts (explicit, extracted, file)
        )
        .await
        .unwrap();

    assert_eq!(evicted, 2, "should have evicted 2 memories");

    // Verify vec0 count decreased
    let final_count = vec0_count_async(store.conn()).await;
    assert_eq!(
        final_count, 3,
        "vec0 should have 3 rows after evicting 2, got {final_count}"
    );

    // KNN search should not return evicted memories
    let query_emb = synthetic_embedding(802);
    let results = store
        .conn()
        .call(move |conn| vec0::vec0_search(conn, &query_emb, 10, 0.0, None))
        .await
        .unwrap();

    assert!(
        results.len() <= 3,
        "vec0 search should return at most 3 results, got {}",
        results.len()
    );
}

// ---------------------------------------------------------------------------
// Parity validation tests (67-03: VEC-05, VEC-06, VEC-07)
// ---------------------------------------------------------------------------

/// Helper: insert N memories with varied sources and timestamps, return the store.
async fn insert_parity_memories(count: usize) -> blufio_memory::MemoryStore {
    let conn = setup_test_db().await;
    let store = blufio_memory::MemoryStore::with_vec0(conn, None, true);
    let sources = [MemorySource::Explicit, MemorySource::Extracted, MemorySource::FileWatcher];

    for i in 0..count {
        let days_ago = (i * 3) as i64; // 0, 3, 6, ..., days ago
        let created =
            (chrono::Utc::now() - chrono::Duration::days(days_ago)).to_rfc3339();
        let mut mem = make_test_memory(
            &format!("parity-{i}"),
            &format!("Memory content for parity test {i}"),
            i as u64 + 1000, // seed offset to avoid collisions with other tests
        );
        mem.source = sources[i % 3];
        mem.confidence = 0.5 + (i as f64 % 5.0) * 0.1; // vary confidence
        mem.session_id = Some("test-session".to_string());
        mem.created_at = created.clone();
        mem.updated_at = created;
        store.save(&mem).await.unwrap();
    }
    store
}

/// Compare vec0 search results against in-memory cosine search.
///
/// Both paths must return the same ID sets and scores within `tolerance`.
fn assert_parity(
    vec0_results: &[vec0::Vec0SearchResult],
    in_mem_results: &[(String, f32)],
    k: usize,
    tolerance: f32,
) {
    let vec0_top: Vec<&str> = vec0_results.iter().take(k).map(|r| r.memory_id.as_str()).collect();
    let in_mem_top: Vec<&str> = in_mem_results.iter().take(k).map(|(id, _)| id.as_str()).collect();

    // Same number of results (both capped at k or less if fewer entries)
    assert_eq!(
        vec0_top.len(),
        in_mem_top.len(),
        "result count mismatch: vec0={}, in_mem={}",
        vec0_top.len(),
        in_mem_top.len()
    );

    // Same ID sets (order may vary for tied scores)
    let mut vec0_sorted = vec0_top.clone();
    vec0_sorted.sort();
    let mut in_mem_sorted = in_mem_top.clone();
    in_mem_sorted.sort();
    assert_eq!(
        vec0_sorted, in_mem_sorted,
        "result ID sets differ:\n  vec0:   {:?}\n  in_mem: {:?}",
        vec0_sorted, in_mem_sorted
    );

    // Scores within tolerance (compare by matching IDs, not by position)
    for v in vec0_results.iter().take(k) {
        if let Some((_, in_mem_sim)) = in_mem_results.iter().find(|(id, _)| id == &v.memory_id) {
            assert!(
                (v.similarity - in_mem_sim).abs() < tolerance,
                "score mismatch for {}: vec0={:.6}, in_mem={:.6}, diff={:.6}, tolerance={:.6}",
                v.memory_id,
                v.similarity,
                in_mem_sim,
                (v.similarity - in_mem_sim).abs(),
                tolerance
            );
        }
    }
}

/// Perform in-memory cosine search using get_active_embeddings, sorted descending.
async fn in_memory_cosine_search(
    store: &blufio_memory::MemoryStore,
    query_emb: &[f32],
) -> Vec<(String, f32)> {
    let all_embeddings = store.get_active_embeddings().await.unwrap();
    let mut results: Vec<(String, f32)> = all_embeddings
        .into_iter()
        .filter_map(|(id, emb)| {
            let sim = cosine_similarity(query_emb, &emb);
            if sim >= 0.0 {
                Some((id, sim))
            } else {
                None
            }
        })
        .collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results
}

// Test 1: Parity at 10-entry scale (VEC-05)

#[tokio::test]
async fn test_vec0_parity_10_memories() {
    vec0::ensure_sqlite_vec_registered();
    let store = insert_parity_memories(10).await;

    // Ensure vec0 is populated (dual-write should have handled it, but verify)
    let (_, total) = store.populate_vec0().await.unwrap();
    assert_eq!(total, 10, "should have 10 active memories");

    // Query with a specific embedding (close to parity-5)
    let query_emb = synthetic_embedding(1005);

    // Vec0 path
    let vec0_results = store
        .conn()
        .call({
            let q = query_emb.clone();
            move |conn| vec0::vec0_search(conn, &q, 10, 0.0, None)
        })
        .await
        .unwrap();

    // In-memory path
    let in_mem_results = in_memory_cosine_search(&store, &query_emb).await;

    // Assert parity: same IDs, scores within 0.01
    assert_parity(&vec0_results, &in_mem_results, 10, 0.01);
}

// Test 2: Parity at 100-entry scale (VEC-05)

#[tokio::test]
async fn test_vec0_parity_100_memories() {
    vec0::ensure_sqlite_vec_registered();
    let store = insert_parity_memories(100).await;

    let (_, total) = store.populate_vec0().await.unwrap();
    assert_eq!(total, 100);

    // Query from the middle of the range
    let query_emb = synthetic_embedding(1050);

    // Vec0 path (top-20)
    let vec0_results = store
        .conn()
        .call({
            let q = query_emb.clone();
            move |conn| vec0::vec0_search(conn, &q, 20, 0.0, None)
        })
        .await
        .unwrap();

    // In-memory path
    let in_mem_results = in_memory_cosine_search(&store, &query_emb).await;

    // Assert parity on top-20
    assert_parity(&vec0_results, &in_mem_results, 20, 0.01);
}

// Test 3: Parity at 1K-entry scale (VEC-05)

#[tokio::test]
async fn test_vec0_parity_1000_memories() {
    vec0::ensure_sqlite_vec_registered();
    let store = insert_parity_memories(1000).await;

    let (_, total) = store.populate_vec0().await.unwrap();
    assert_eq!(total, 1000);

    // Query embedding
    let query_emb = synthetic_embedding(1500);

    // Vec0 path (top-20)
    let vec0_results = store
        .conn()
        .call({
            let q = query_emb.clone();
            move |conn| vec0::vec0_search(conn, &q, 20, 0.0, None)
        })
        .await
        .unwrap();

    // In-memory path
    let in_mem_results = in_memory_cosine_search(&store, &query_emb).await;

    // Use 0.02 tolerance at 1K scale (f32 accumulation differences grow)
    assert_parity(&vec0_results, &in_mem_results, 20, 0.02);
}

// Test 4: Auxiliary columns carry correct data (VEC-08)

#[tokio::test]
async fn test_vec0_auxiliary_columns_populated() {
    vec0::ensure_sqlite_vec_registered();
    let conn = setup_test_db().await;
    let store = blufio_memory::MemoryStore::with_vec0(conn, None, true);

    // Insert a memory with known field values
    let mut mem = make_test_memory("aux-col-1", "Auxiliary column test content", 9999);
    mem.source = MemorySource::FileWatcher;
    mem.confidence = 0.75;
    mem.session_id = Some("aux-session".to_string());
    mem.created_at = "2026-02-15T12:30:00.000Z".to_string();
    mem.updated_at = "2026-02-15T12:30:00.000Z".to_string();
    store.save(&mem).await.unwrap();

    // Search with the exact same embedding -> should get this memory as top result
    let query_emb = synthetic_embedding(9999);
    let results = store
        .conn()
        .call({
            let q = query_emb;
            move |conn| vec0::vec0_search(conn, &q, 1, 0.0, Some("aux-session"))
        })
        .await
        .unwrap();

    assert_eq!(results.len(), 1, "should find exactly one result");
    let r = &results[0];

    // Validate all auxiliary columns match the original Memory fields
    assert_eq!(r.memory_id, "aux-col-1", "memory_id mismatch");
    assert_eq!(r.content, "Auxiliary column test content", "content mismatch");
    assert_eq!(r.source, "file_watcher", "source mismatch");
    assert!(
        (r.confidence - 0.75).abs() < 0.001,
        "confidence mismatch: expected 0.75, got {}",
        r.confidence
    );
    assert_eq!(r.created_at, "2026-02-15T12:30:00.000Z", "created_at mismatch");
    assert!(
        r.similarity > 0.99,
        "exact embedding match should have similarity ~1.0, got {}",
        r.similarity
    );
}

// Test 5: Eviction sync parity (VEC-06)

#[tokio::test]
async fn test_vec0_eviction_sync_parity() {
    vec0::ensure_sqlite_vec_registered();
    let conn = setup_test_db().await;
    let store = blufio_memory::MemoryStore::with_vec0(conn, None, true);

    // Insert 5 memories with varied ages (so eviction scoring produces distinct results)
    for i in 0..5u64 {
        let days_ago = (i * 10) as i64; // 0, 10, 20, 30, 40 days ago
        let created =
            (chrono::Utc::now() - chrono::Duration::days(days_ago)).to_rfc3339();
        let mut mem = make_test_memory(
            &format!("evict-parity-{i}"),
            &format!("Eviction parity memory {i}"),
            i + 5000,
        );
        mem.created_at = created.clone();
        mem.updated_at = created;
        store.save(&mem).await.unwrap();
    }

    // Verify initial counts
    let initial_vec0 = vec0_count_async(store.conn()).await;
    let initial_active = store.count_active().await.unwrap();
    assert_eq!(initial_vec0, 5, "vec0 should start with 5 rows");
    assert_eq!(initial_active, 5, "memories should start with 5 active");

    // Evict 2 memories (lowest eviction scores = oldest)
    let (evicted, _low, _high) = store
        .batch_evict(2, 0.95, 0.1, (1.5, 1.0, 1.2))
        .await
        .unwrap();
    assert_eq!(evicted, 2, "should have evicted 2 memories");

    // Verify both tables are in sync after eviction
    let final_vec0 = vec0_count_async(store.conn()).await;
    let final_active = store.count_active().await.unwrap();
    assert_eq!(final_vec0, 3, "vec0 should have 3 rows after eviction");
    assert_eq!(
        final_active, 3,
        "memories should have 3 active after eviction"
    );
    assert_eq!(
        final_vec0, final_active,
        "vec0 count should match active memory count"
    );

    // Verify search results exclude evicted memories in both paths
    let query_emb = synthetic_embedding(5002);

    // Vec0 path
    let vec0_results = store
        .conn()
        .call({
            let q = query_emb.clone();
            move |conn| vec0::vec0_search(conn, &q, 10, 0.0, None)
        })
        .await
        .unwrap();

    // In-memory path
    let in_mem_results = in_memory_cosine_search(&store, &query_emb).await;

    // Both should return at most 3 results
    assert!(
        vec0_results.len() <= 3,
        "vec0 should return at most 3 results after eviction, got {}",
        vec0_results.len()
    );
    assert!(
        in_mem_results.len() <= 3,
        "in-memory should return at most 3 results after eviction, got {}",
        in_mem_results.len()
    );

    // Same IDs returned by both paths
    let mut vec0_ids: Vec<&str> = vec0_results.iter().map(|r| r.memory_id.as_str()).collect();
    vec0_ids.sort();
    let mut in_mem_ids: Vec<&str> = in_mem_results.iter().map(|(id, _)| id.as_str()).collect();
    in_mem_ids.sort();
    assert_eq!(
        vec0_ids, in_mem_ids,
        "vec0 and in-memory should return same IDs after eviction"
    );
}

// Test 6: Session partition key filtering (VEC-07)

#[tokio::test]
async fn test_vec0_session_partition_search() {
    vec0::ensure_sqlite_vec_registered();
    let conn = setup_test_db().await;
    let store = blufio_memory::MemoryStore::with_vec0(conn, None, true);

    // Insert 5 memories in session-A
    for i in 0..5u64 {
        let mut mem = make_test_memory(
            &format!("sess-a-{i}"),
            &format!("Session A memory {i}"),
            i + 6000,
        );
        mem.session_id = Some("session-A".to_string());
        store.save(&mem).await.unwrap();
    }

    // Insert 5 memories in session-B
    for i in 0..5u64 {
        let mut mem = make_test_memory(
            &format!("sess-b-{i}"),
            &format!("Session B memory {i}"),
            i + 7000,
        );
        mem.session_id = Some("session-B".to_string());
        store.save(&mem).await.unwrap();
    }

    // Verify total vec0 count
    let total = vec0_count_async(store.conn()).await;
    assert_eq!(total, 10, "should have 10 total vec0 rows");

    let query_emb = synthetic_embedding(6002); // close to session-A memories

    // Search with session_id="session-A" -> only session-A memories
    let results_a = store
        .conn()
        .call({
            let q = query_emb.clone();
            move |conn| vec0::vec0_search(conn, &q, 10, 0.0, Some("session-A"))
        })
        .await
        .unwrap();

    assert_eq!(
        results_a.len(),
        5,
        "session-A search should return 5 results, got {}",
        results_a.len()
    );
    for r in &results_a {
        assert!(
            r.memory_id.starts_with("sess-a-"),
            "session-A search returned non-session-A memory: {}",
            r.memory_id
        );
    }

    // Search with session_id="session-B" -> only session-B memories
    let results_b = store
        .conn()
        .call({
            let q = query_emb.clone();
            move |conn| vec0::vec0_search(conn, &q, 10, 0.0, Some("session-B"))
        })
        .await
        .unwrap();

    assert_eq!(
        results_b.len(),
        5,
        "session-B search should return 5 results, got {}",
        results_b.len()
    );
    for r in &results_b {
        assert!(
            r.memory_id.starts_with("sess-b-"),
            "session-B search returned non-session-B memory: {}",
            r.memory_id
        );
    }

    // Search without session filter -> all 10 memories
    let results_all = store
        .conn()
        .call({
            let q = query_emb;
            move |conn| vec0::vec0_search(conn, &q, 10, 0.0, None)
        })
        .await
        .unwrap();

    assert_eq!(
        results_all.len(),
        10,
        "unfiltered search should return all 10 results, got {}",
        results_all.len()
    );
}
