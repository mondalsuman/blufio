// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! SQLite-backed memory store with vector BLOB storage and FTS5 for BM25.

use blufio_core::error::BlufioError;
use tokio_rusqlite::Connection;

use crate::types::{blob_to_vec, vec_to_blob, Memory, MemorySource, MemoryStatus};

/// Helper to convert tokio_rusqlite errors into BlufioError::Storage.
fn storage_err(e: tokio_rusqlite::Error) -> BlufioError {
    BlufioError::Storage {
        source: Box::new(e),
    }
}

/// Persistent store for memories in SQLite.
///
/// Stores embeddings as BLOBs and maintains an FTS5 virtual table
/// for BM25 keyword search. Sync triggers keep FTS5 up to date.
pub struct MemoryStore {
    conn: Connection,
}

impl MemoryStore {
    /// Creates a new MemoryStore wrapping an existing connection.
    ///
    /// The connection should already have V3 migration applied
    /// (memories table + memories_fts virtual table).
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Save a memory to the store.
    pub async fn save(&self, memory: &Memory) -> Result<(), BlufioError> {
        let id = memory.id.clone();
        let content = memory.content.clone();
        let embedding_blob = vec_to_blob(&memory.embedding);
        let source = memory.source.as_str().to_string();
        let confidence = memory.confidence;
        let status = memory.status.as_str().to_string();
        let superseded_by = memory.superseded_by.clone();
        let session_id = memory.session_id.clone();
        let created_at = memory.created_at.clone();
        let updated_at = memory.updated_at.clone();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO memories (id, content, embedding, source, confidence, status, superseded_by, session_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    rusqlite::params![id, content, embedding_blob, source, confidence, status, superseded_by, session_id, created_at, updated_at],
                )?;
                Ok(())
            })
            .await
            .map_err(storage_err)
    }

    /// Get a memory by ID.
    pub async fn get_by_id(&self, id: &str) -> Result<Option<Memory>, BlufioError> {
        let id = id.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, content, embedding, source, confidence, status, superseded_by, session_id, created_at, updated_at FROM memories WHERE id = ?1",
                )?;
                let memory = stmt
                    .query_row(rusqlite::params![id], |row| {
                        Ok(row_to_memory(row))
                    })
                    .optional()?;
                Ok(memory)
            })
            .await
            .map_err(storage_err)
    }

    /// Get all active memories.
    pub async fn get_active(&self) -> Result<Vec<Memory>, BlufioError> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, content, embedding, source, confidence, status, superseded_by, session_id, created_at, updated_at FROM memories WHERE status = 'active' ORDER BY created_at DESC",
                )?;
                let memories = stmt
                    .query_map([], |row| Ok(row_to_memory(row)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(memories)
            })
            .await
            .map_err(storage_err)
    }

    /// Get all active memory embeddings (lightweight -- no content).
    ///
    /// Returns (id, embedding) pairs for vector search.
    pub async fn get_active_embeddings(&self) -> Result<Vec<(String, Vec<f32>)>, BlufioError> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, embedding FROM memories WHERE status = 'active'",
                )?;
                let results = stmt
                    .query_map([], |row| {
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

    /// Search memories using BM25 via FTS5.
    ///
    /// Returns (memory_id, bm25_score) pairs sorted by relevance.
    /// BM25 scores are negative (more negative = more relevant).
    pub async fn search_bm25(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(String, f64)>, BlufioError> {
        let query = query.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT m.id, bm25(memories_fts) as score FROM memories_fts JOIN memories m ON m.rowid = memories_fts.rowid WHERE memories_fts MATCH ?1 AND m.status = 'active' ORDER BY bm25(memories_fts) LIMIT ?2",
                )?;
                let results = stmt
                    .query_map(rusqlite::params![query, limit as i64], |row| {
                        let id: String = row.get(0)?;
                        let score: f64 = row.get(1)?;
                        Ok((id, score))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(results)
            })
            .await
            .map_err(storage_err)
    }

    /// Soft-delete a memory (set status to 'forgotten').
    pub async fn soft_delete(&self, id: &str) -> Result<(), BlufioError> {
        let id = id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE memories SET status = 'forgotten', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?1",
                    rusqlite::params![id],
                )?;
                Ok(())
            })
            .await
            .map_err(storage_err)
    }

    /// Supersede a memory (mark old as superseded, link to new).
    pub async fn supersede(&self, old_id: &str, new_id: &str) -> Result<(), BlufioError> {
        let old_id = old_id.to_string();
        let new_id = new_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE memories SET status = 'superseded', superseded_by = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?2",
                    rusqlite::params![new_id, old_id],
                )?;
                Ok(())
            })
            .await
            .map_err(storage_err)
    }

    /// Get memories by IDs (batch retrieval after hybrid search).
    pub async fn get_memories_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<Memory>, BlufioError> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let ids = ids.to_vec();
        self.conn
            .call(move |conn| {
                // Build parameterized query for IN clause
                let placeholders: Vec<String> =
                    (1..=ids.len()).map(|i| format!("?{i}")).collect();
                let sql = format!(
                    "SELECT id, content, embedding, source, confidence, status, superseded_by, session_id, created_at, updated_at FROM memories WHERE id IN ({}) AND status = 'active'",
                    placeholders.join(", ")
                );
                let mut stmt = conn.prepare(&sql)?;

                let params: Vec<&dyn rusqlite::types::ToSql> =
                    ids.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect();
                let memories = stmt
                    .query_map(params.as_slice(), |row| Ok(row_to_memory(row)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(memories)
            })
            .await
            .map_err(storage_err)
    }
}

/// Convert a rusqlite Row to a Memory struct.
fn row_to_memory(row: &rusqlite::Row) -> Memory {
    let embedding_blob: Vec<u8> = row.get(2).unwrap_or_default();
    let source_str: String = row.get(3).unwrap_or_default();
    let status_str: String = row.get(5).unwrap_or_default();

    Memory {
        id: row.get(0).unwrap_or_default(),
        content: row.get(1).unwrap_or_default(),
        embedding: blob_to_vec(&embedding_blob),
        source: MemorySource::from_str_value(&source_str),
        confidence: row.get(4).unwrap_or(0.5),
        status: MemoryStatus::from_str_value(&status_str),
        superseded_by: row.get(6).unwrap_or(None),
        session_id: row.get(7).unwrap_or(None),
        created_at: row.get(8).unwrap_or_default(),
        updated_at: row.get(9).unwrap_or_default(),
    }
}

/// Extension trait for optional row queries.
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().await.unwrap();
        conn.call(|conn| -> Result<(), rusqlite::Error> {
            // Run V1 schema (needed for refinery_schema_history compatibility)
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
            // Run V3 memory schema
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
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
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
                    INSERT INTO memories_fts(memories_fts, rowid, content)
                        VALUES('delete', old.rowid, old.content);
                END;

                CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
                    INSERT INTO memories_fts(memories_fts, rowid, content)
                        VALUES('delete', old.rowid, old.content);
                    INSERT INTO memories_fts(rowid, content) VALUES (new.rowid, new.content);
                END;

                CREATE INDEX IF NOT EXISTS idx_memories_status ON memories(status);
                CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at);",
            )?;
            Ok(())
        })
        .await
        .unwrap();
        conn
    }

    fn make_test_memory(id: &str, content: &str) -> Memory {
        Memory {
            id: id.to_string(),
            content: content.to_string(),
            embedding: vec![0.1; 384],
            source: MemorySource::Explicit,
            confidence: 0.9,
            status: MemoryStatus::Active,
            superseded_by: None,
            session_id: Some("test-session".to_string()),
            created_at: "2026-03-01T00:00:00.000Z".to_string(),
            updated_at: "2026-03-01T00:00:00.000Z".to_string(),
        }
    }

    #[tokio::test]
    async fn save_and_get_by_id() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        let memory = make_test_memory("mem-1", "User's dog is named Max");
        store.save(&memory).await.unwrap();

        let retrieved = store.get_by_id("mem-1").await.unwrap().unwrap();
        assert_eq!(retrieved.id, "mem-1");
        assert_eq!(retrieved.content, "User's dog is named Max");
        assert_eq!(retrieved.source, MemorySource::Explicit);
        assert_eq!(retrieved.embedding.len(), 384);
    }

    #[tokio::test]
    async fn get_by_id_nonexistent() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        let result = store.get_by_id("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_active_returns_only_active() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        let active = make_test_memory("mem-1", "Active memory");
        store.save(&active).await.unwrap();

        let mut forgotten = make_test_memory("mem-2", "Forgotten memory");
        forgotten.status = MemoryStatus::Forgotten;
        store.save(&forgotten).await.unwrap();

        let active_memories = store.get_active().await.unwrap();
        assert_eq!(active_memories.len(), 1);
        assert_eq!(active_memories[0].id, "mem-1");
    }

    #[tokio::test]
    async fn embedding_blob_roundtrip() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        let original_embedding: Vec<f32> = (0..384).map(|i| i as f32 / 384.0).collect();
        let mut memory = make_test_memory("mem-1", "Test embedding");
        memory.embedding = original_embedding.clone();
        store.save(&memory).await.unwrap();

        let retrieved = store.get_by_id("mem-1").await.unwrap().unwrap();
        assert_eq!(retrieved.embedding.len(), 384);
        for (a, b) in original_embedding.iter().zip(retrieved.embedding.iter()) {
            assert!((a - b).abs() < f32::EPSILON);
        }
    }

    #[tokio::test]
    async fn soft_delete_sets_forgotten() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        let memory = make_test_memory("mem-1", "Will be forgotten");
        store.save(&memory).await.unwrap();
        store.soft_delete("mem-1").await.unwrap();

        let retrieved = store.get_by_id("mem-1").await.unwrap().unwrap();
        assert_eq!(retrieved.status, MemoryStatus::Forgotten);
    }

    #[tokio::test]
    async fn supersede_links_memories() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        let old = make_test_memory("mem-old", "Dog is named Max");
        let new = make_test_memory("mem-new", "Dog is named Luna");
        store.save(&old).await.unwrap();
        store.save(&new).await.unwrap();

        store.supersede("mem-old", "mem-new").await.unwrap();

        let old_retrieved = store.get_by_id("mem-old").await.unwrap().unwrap();
        assert_eq!(old_retrieved.status, MemoryStatus::Superseded);
        assert_eq!(old_retrieved.superseded_by, Some("mem-new".to_string()));
    }

    #[tokio::test]
    async fn fts5_search_finds_inserted_memory() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        let memory = make_test_memory("mem-1", "The user has a golden retriever named Max");
        store.save(&memory).await.unwrap();

        let results = store.search_bm25("golden retriever", 10).await.unwrap();
        assert!(!results.is_empty(), "FTS5 search should find the memory");
        assert_eq!(results[0].0, "mem-1");
    }

    #[tokio::test]
    async fn fts5_search_no_results() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        let memory = make_test_memory("mem-1", "The user likes pizza");
        store.save(&memory).await.unwrap();

        let results = store.search_bm25("quantum physics", 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn get_active_embeddings() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        let memory = make_test_memory("mem-1", "Test");
        store.save(&memory).await.unwrap();

        let embeddings = store.get_active_embeddings().await.unwrap();
        assert_eq!(embeddings.len(), 1);
        assert_eq!(embeddings[0].0, "mem-1");
        assert_eq!(embeddings[0].1.len(), 384);
    }

    #[tokio::test]
    async fn get_memories_by_ids() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        store.save(&make_test_memory("mem-1", "Fact 1")).await.unwrap();
        store.save(&make_test_memory("mem-2", "Fact 2")).await.unwrap();
        store.save(&make_test_memory("mem-3", "Fact 3")).await.unwrap();

        let ids = vec!["mem-1".to_string(), "mem-3".to_string()];
        let memories = store.get_memories_by_ids(&ids).await.unwrap();
        assert_eq!(memories.len(), 2);
    }

    #[tokio::test]
    async fn get_memories_by_ids_empty() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        let memories = store.get_memories_by_ids(&[]).await.unwrap();
        assert!(memories.is_empty());
    }
}
