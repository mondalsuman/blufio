// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! SQLite-backed memory store with vector BLOB storage and FTS5 for BM25.

use std::sync::Arc;

use blufio_bus::EventBus;
use blufio_bus::events::{BusEvent, MemoryEvent, new_event_id, now_timestamp};
use blufio_core::classification::DataClassification;
use blufio_core::error::BlufioError;
use tokio_rusqlite::Connection;

use crate::types::{Memory, MemorySource, MemoryStatus, blob_to_vec, vec_to_blob};

/// Helper to convert tokio_rusqlite errors into BlufioError::Storage.
fn storage_err(e: tokio_rusqlite::Error) -> BlufioError {
    BlufioError::storage_connection_failed(e)
}

/// Persistent store for memories in SQLite.
///
/// Stores embeddings as BLOBs and maintains an FTS5 virtual table
/// for BM25 keyword search. Sync triggers keep FTS5 up to date.
pub struct MemoryStore {
    conn: Connection,
    /// Optional event bus for emitting memory CRUD events.
    /// Set to `None` in tests and CLI contexts.
    event_bus: Option<Arc<EventBus>>,
}

impl MemoryStore {
    /// Creates a new MemoryStore wrapping an existing connection.
    ///
    /// The connection should already have V3 migration applied
    /// (memories table + memories_fts virtual table).
    pub fn new(conn: Connection) -> Self {
        Self {
            conn,
            event_bus: None,
        }
    }

    /// Creates a new MemoryStore with an event bus for audit event emission.
    pub fn with_event_bus(conn: Connection, event_bus: Arc<EventBus>) -> Self {
        Self {
            conn,
            event_bus: Some(event_bus),
        }
    }

    /// Access the underlying connection (for advanced operations like hard-delete).
    pub fn conn(&self) -> &Connection {
        &self.conn
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
        let classification = memory.classification.as_str().to_string();
        let created_at = memory.created_at.clone();
        let updated_at = memory.updated_at.clone();

        let mem_id = memory.id.clone();
        let mem_source = memory.source.as_str().to_string();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO memories (id, content, embedding, source, confidence, status, superseded_by, session_id, classification, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    rusqlite::params![id, content, embedding_blob, source, confidence, status, superseded_by, session_id, classification, created_at, updated_at],
                )?;
                Ok(())
            })
            .await
            .map_err(storage_err)?;

        if let Some(ref bus) = self.event_bus {
            bus.publish(BusEvent::Memory(MemoryEvent::Created {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                memory_id: mem_id,
                source: mem_source,
            }))
            .await;
        }

        Ok(())
    }

    /// Get a memory by ID.
    pub async fn get_by_id(&self, id: &str) -> Result<Option<Memory>, BlufioError> {
        let mem_id = id.to_string();
        let id = id.to_string();
        let result = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, content, embedding, source, confidence, status, superseded_by, session_id, classification, created_at, updated_at FROM memories WHERE id = ?1 AND deleted_at IS NULL",
                )?;
                let memory = stmt
                    .query_row(rusqlite::params![id], |row| {
                        Ok(row_to_memory(row))
                    })
                    .optional()?;
                Ok(memory)
            })
            .await
            .map_err(storage_err)?;

        if result.is_some()
            && let Some(ref bus) = self.event_bus
        {
            let _ = bus
                .publish(BusEvent::Memory(MemoryEvent::Retrieved {
                    event_id: new_event_id(),
                    timestamp: now_timestamp(),
                    memory_id: mem_id,
                    query: String::new(),
                }))
                .await;
        }

        Ok(result)
    }

    /// Get all active memories, excluding Restricted data.
    pub async fn get_active(&self) -> Result<Vec<Memory>, BlufioError> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, content, embedding, source, confidence, status, superseded_by, session_id, classification, created_at, updated_at FROM memories WHERE status = 'active' AND classification != 'restricted' AND deleted_at IS NULL ORDER BY created_at DESC",
                )?;
                let memories = stmt
                    .query_map([], |row| Ok(row_to_memory(row)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(memories)
            })
            .await
            .map_err(storage_err)
    }

    /// Get all active memory embeddings (lightweight -- no content), excluding Restricted.
    ///
    /// Returns (id, embedding) pairs for vector search.
    pub async fn get_active_embeddings(&self) -> Result<Vec<(String, Vec<f32>)>, BlufioError> {
        self.conn
            .call(move |conn| {
                let mut stmt =
                    conn.prepare("SELECT id, embedding FROM memories WHERE status = 'active' AND classification != 'restricted' AND deleted_at IS NULL")?;
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

    /// Search memories using BM25 via FTS5, excluding Restricted data.
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
                    "SELECT m.id, bm25(memories_fts) as score FROM memories_fts JOIN memories m ON m.rowid = memories_fts.rowid WHERE memories_fts MATCH ?1 AND m.status = 'active' AND m.classification != 'restricted' AND m.deleted_at IS NULL ORDER BY bm25(memories_fts) LIMIT ?2",
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
        let mem_id = id.to_string();
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
            .map_err(storage_err)?;

        if let Some(ref bus) = self.event_bus {
            bus.publish(BusEvent::Memory(MemoryEvent::Deleted {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                memory_id: mem_id,
            }))
            .await;
        }

        Ok(())
    }

    /// Supersede a memory (mark old as superseded, link to new).
    pub async fn supersede(&self, old_id: &str, new_id: &str) -> Result<(), BlufioError> {
        let mem_id = old_id.to_string();
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
            .map_err(storage_err)?;

        if let Some(ref bus) = self.event_bus {
            bus.publish(BusEvent::Memory(MemoryEvent::Updated {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                memory_id: mem_id,
            }))
            .await;
        }

        Ok(())
    }

    /// Count all active non-restricted memories.
    pub async fn count_active(&self) -> Result<usize, BlufioError> {
        self.conn
            .call(move |conn| {
                let count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM memories WHERE status = 'active' AND classification != 'restricted' AND deleted_at IS NULL",
                    [],
                    |row| row.get(0),
                )?;
                Ok(count as usize)
            })
            .await
            .map_err(storage_err)
    }

    /// Get all active memories with embeddings for validation (pairwise comparison).
    ///
    /// Delegates to `get_active()` which already loads full Memory structs including embeddings.
    pub async fn get_all_active_with_embeddings(&self) -> Result<Vec<Memory>, BlufioError> {
        self.get_active().await
    }

    /// Hard-delete the lowest-scored active memories by eviction score.
    ///
    /// Computes eviction score in Rust: `importance_boost * max(decay_factor^days, decay_floor)`.
    /// Returns `(count_deleted, lowest_score_of_deleted, highest_score_of_deleted)`.
    ///
    /// The delete is wrapped in a single transaction so FTS5 triggers fire consistently.
    pub async fn batch_evict(
        &self,
        count: usize,
        decay_factor: f64,
        decay_floor: f64,
        importance_boosts: (f64, f64, f64),
    ) -> Result<(usize, f64, f64), BlufioError> {
        let (boost_explicit, boost_extracted, boost_file) = importance_boosts;

        self.conn
            .call(move |conn| {
                // Step 1: Load all active non-restricted memories with metadata for scoring
                let rows: Vec<(String, String, String)> = {
                    let mut stmt = conn.prepare(
                        "SELECT id, source, created_at FROM memories WHERE status = 'active' AND classification != 'restricted' AND deleted_at IS NULL",
                    )?;
                    stmt.query_map([], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?
                };

                // Step 2: Compute eviction score for each memory in Rust
                let now = chrono::Utc::now();
                let mut scored: Vec<(String, f64)> = rows
                    .into_iter()
                    .map(|(id, source, created_at)| {
                        let boost = match source.as_str() {
                            "explicit" => boost_explicit,
                            "file_watcher" => boost_file,
                            _ => boost_extracted,
                        };
                        let days = chrono::DateTime::parse_from_rfc3339(&created_at)
                            .or_else(|_| {
                                // Handle format like "2026-03-01T00:00:00.000Z"
                                chrono::DateTime::parse_from_str(&created_at, "%Y-%m-%dT%H:%M:%S%.fZ")
                            })
                            .map(|dt| (now - dt.with_timezone(&chrono::Utc)).num_days().max(0) as f64)
                            .unwrap_or(0.0);
                        let decay = decay_factor.powf(days).max(decay_floor);
                        let score = boost * decay;
                        (id, score)
                    })
                    .collect();

                // Step 3: Sort by score ascending (lowest first) and take `count` for eviction
                scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                let to_evict: Vec<(String, f64)> = scored.into_iter().take(count).collect();

                if to_evict.is_empty() {
                    return Ok((0, 0.0, 0.0));
                }

                let lowest_score = to_evict.first().map(|t| t.1).unwrap_or(0.0);
                let highest_score = to_evict.last().map(|t| t.1).unwrap_or(0.0);
                let ids: Vec<String> = to_evict.into_iter().map(|(id, _)| id).collect();

                // Step 4: Delete in a single transaction (FTS5 triggers fire per row)
                let tx = conn.transaction()?;
                let placeholders: Vec<String> =
                    (1..=ids.len()).map(|i| format!("?{i}")).collect();
                let sql = format!(
                    "DELETE FROM memories WHERE id IN ({})",
                    placeholders.join(", ")
                );
                let params: Vec<&dyn rusqlite::types::ToSql> =
                    ids.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect();
                let deleted = tx.execute(&sql, params.as_slice())?;
                tx.commit()?;

                Ok((deleted, lowest_score, highest_score))
            })
            .await
            .map_err(storage_err)
    }

    /// Get memories by IDs (batch retrieval after hybrid search), excluding Restricted.
    pub async fn get_memories_by_ids(&self, ids: &[String]) -> Result<Vec<Memory>, BlufioError> {
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
                    "SELECT id, content, embedding, source, confidence, status, superseded_by, session_id, classification, created_at, updated_at FROM memories WHERE id IN ({}) AND status = 'active' AND classification != 'restricted' AND deleted_at IS NULL",
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
///
/// Column order: id(0), content(1), embedding(2), source(3), confidence(4),
/// status(5), superseded_by(6), session_id(7), classification(8),
/// created_at(9), updated_at(10).
fn row_to_memory(row: &rusqlite::Row) -> Memory {
    let embedding_blob: Vec<u8> = row.get(2).unwrap_or_default();
    let source_str: String = row.get(3).unwrap_or_default();
    let status_str: String = row.get(5).unwrap_or_default();
    let classification_str: String = row.get(8).unwrap_or_default();

    Memory {
        id: row.get(0).unwrap_or_default(),
        content: row.get(1).unwrap_or_default(),
        embedding: blob_to_vec(&embedding_blob),
        source: MemorySource::from_str_value(&source_str),
        confidence: row.get(4).unwrap_or(0.5),
        status: MemoryStatus::from_str_value(&status_str),
        superseded_by: row.get(6).unwrap_or(None),
        session_id: row.get(7).unwrap_or(None),
        classification: DataClassification::from_str_value(&classification_str).unwrap_or_default(),
        created_at: row.get(9).unwrap_or_default(),
        updated_at: row.get(10).unwrap_or_default(),
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
            // Run V3 memory schema + V12 classification column
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
            classification: DataClassification::default(),
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

        store
            .save(&make_test_memory("mem-1", "Fact 1"))
            .await
            .unwrap();
        store
            .save(&make_test_memory("mem-2", "Fact 2"))
            .await
            .unwrap();
        store
            .save(&make_test_memory("mem-3", "Fact 3"))
            .await
            .unwrap();

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

    #[tokio::test]
    async fn save_includes_classification_column() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        let mut memory = make_test_memory("mem-cls", "Classified memory");
        memory.classification = DataClassification::Confidential;
        store.save(&memory).await.unwrap();

        let retrieved = store.get_by_id("mem-cls").await.unwrap().unwrap();
        assert_eq!(retrieved.classification, DataClassification::Confidential);
    }

    #[tokio::test]
    async fn get_active_excludes_restricted() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        // Save an internal memory
        let internal = make_test_memory("mem-int", "Internal memory");
        store.save(&internal).await.unwrap();

        // Save a restricted memory
        let mut restricted = make_test_memory("mem-res", "Restricted memory");
        restricted.classification = DataClassification::Restricted;
        store.save(&restricted).await.unwrap();

        // Save a confidential memory
        let mut confidential = make_test_memory("mem-conf", "Confidential memory");
        confidential.classification = DataClassification::Confidential;
        store.save(&confidential).await.unwrap();

        let active = store.get_active().await.unwrap();
        assert_eq!(active.len(), 2, "restricted memory should be excluded");
        let ids: Vec<&str> = active.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"mem-int"));
        assert!(ids.contains(&"mem-conf"));
        assert!(!ids.contains(&"mem-res"));
    }

    #[tokio::test]
    async fn get_active_embeddings_excludes_restricted() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        let internal = make_test_memory("mem-int", "Internal");
        store.save(&internal).await.unwrap();

        let mut restricted = make_test_memory("mem-res", "Restricted");
        restricted.classification = DataClassification::Restricted;
        store.save(&restricted).await.unwrap();

        let embeddings = store.get_active_embeddings().await.unwrap();
        assert_eq!(embeddings.len(), 1);
        assert_eq!(embeddings[0].0, "mem-int");
    }

    #[tokio::test]
    async fn get_memories_by_ids_excludes_restricted() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        store
            .save(&make_test_memory("mem-1", "Normal"))
            .await
            .unwrap();

        let mut restricted = make_test_memory("mem-2", "Restricted");
        restricted.classification = DataClassification::Restricted;
        store.save(&restricted).await.unwrap();

        let ids = vec!["mem-1".to_string(), "mem-2".to_string()];
        let memories = store.get_memories_by_ids(&ids).await.unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].id, "mem-1");
    }

    #[tokio::test]
    async fn search_bm25_excludes_restricted() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        let internal = make_test_memory("mem-int", "The user has a golden retriever");
        store.save(&internal).await.unwrap();

        let mut restricted = make_test_memory("mem-res", "The user has a golden labrador");
        restricted.classification = DataClassification::Restricted;
        store.save(&restricted).await.unwrap();

        let results = store.search_bm25("golden", 10).await.unwrap();
        assert_eq!(
            results.len(),
            1,
            "restricted should be excluded from search"
        );
        assert_eq!(results[0].0, "mem-int");
    }

    #[tokio::test]
    async fn count_active_returns_correct_count() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        store
            .save(&make_test_memory("mem-1", "Active one"))
            .await
            .unwrap();
        store
            .save(&make_test_memory("mem-2", "Active two"))
            .await
            .unwrap();

        let count = store.count_active().await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn count_active_excludes_non_active_and_restricted() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        // Active internal
        store
            .save(&make_test_memory("mem-active", "Active"))
            .await
            .unwrap();

        // Superseded
        let mut sup = make_test_memory("mem-sup", "Superseded");
        sup.status = MemoryStatus::Superseded;
        store.save(&sup).await.unwrap();

        // Forgotten
        let mut forg = make_test_memory("mem-forg", "Forgotten");
        forg.status = MemoryStatus::Forgotten;
        store.save(&forg).await.unwrap();

        // Active but restricted
        let mut restricted = make_test_memory("mem-restricted", "Restricted");
        restricted.classification = blufio_core::classification::DataClassification::Restricted;
        store.save(&restricted).await.unwrap();

        let count = store.count_active().await.unwrap();
        assert_eq!(count, 1, "only active non-restricted should be counted");
    }

    #[tokio::test]
    async fn batch_evict_deletes_lowest_scored() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        // Insert 15 memories with varying ages (older = lower eviction score with decay)
        for i in 0..15 {
            let created = chrono::Utc::now() - chrono::Duration::days((i + 1) as i64);
            let mut mem = make_test_memory(&format!("mem-{i:02}"), &format!("Memory {i}"));
            mem.source = crate::types::MemorySource::Extracted;
            mem.confidence = 0.6;
            mem.created_at = created.to_rfc3339();
            store.save(&mem).await.unwrap();
        }

        let count_before = store.count_active().await.unwrap();
        assert_eq!(count_before, 15);

        // Evict 6 (down to 9 from 15 with max_entries=10, target=9)
        let (deleted, lowest, highest) = store
            .batch_evict(6, 0.95, 0.1, (1.0, 0.6, 0.8))
            .await
            .unwrap();

        assert_eq!(deleted, 6);
        assert!(lowest > 0.0, "lowest score should be positive");
        assert!(highest >= lowest, "highest should be >= lowest");

        let count_after = store.count_active().await.unwrap();
        assert_eq!(count_after, 9);
    }

    #[tokio::test]
    async fn batch_evict_only_deletes_active() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        // Active memories
        for i in 0..5 {
            let created = chrono::Utc::now() - chrono::Duration::days((i + 1) as i64);
            let mut mem = make_test_memory(&format!("mem-active-{i}"), &format!("Active {i}"));
            mem.created_at = created.to_rfc3339();
            store.save(&mem).await.unwrap();
        }

        // Superseded memories (should not be evicted)
        for i in 0..3 {
            let mut mem = make_test_memory(&format!("mem-sup-{i}"), &format!("Superseded {i}"));
            mem.status = MemoryStatus::Superseded;
            store.save(&mem).await.unwrap();
        }

        let (deleted, _, _) = store
            .batch_evict(3, 0.95, 0.1, (1.0, 0.6, 0.8))
            .await
            .unwrap();
        assert_eq!(deleted, 3);

        // Only active should have been affected
        let remaining = store.count_active().await.unwrap();
        assert_eq!(remaining, 2, "only 2 active memories should remain");
    }

    #[tokio::test]
    async fn batch_evict_fts5_consistent() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        // Insert memories with searchable content
        for i in 0..5 {
            let created = chrono::Utc::now() - chrono::Duration::days((i + 1) as i64);
            let mut mem = make_test_memory(
                &format!("mem-search-{i}"),
                &format!("golden retriever fact number {i}"),
            );
            mem.created_at = created.to_rfc3339();
            store.save(&mem).await.unwrap();
        }

        // Evict 3
        let (deleted, _, _) = store
            .batch_evict(3, 0.95, 0.1, (1.0, 0.6, 0.8))
            .await
            .unwrap();
        assert_eq!(deleted, 3);

        // FTS5 search should still work and only find remaining 2
        let results = store.search_bm25("golden retriever", 10).await.unwrap();
        assert_eq!(
            results.len(),
            2,
            "FTS5 should only find 2 remaining memories after eviction"
        );
    }

    #[tokio::test]
    async fn get_all_active_with_embeddings_returns_full_structs() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        store
            .save(&make_test_memory("mem-1", "First memory"))
            .await
            .unwrap();
        store
            .save(&make_test_memory("mem-2", "Second memory"))
            .await
            .unwrap();

        let mut restricted = make_test_memory("mem-restricted", "Restricted");
        restricted.classification = blufio_core::classification::DataClassification::Restricted;
        store.save(&restricted).await.unwrap();

        let memories = store.get_all_active_with_embeddings().await.unwrap();
        assert_eq!(memories.len(), 2);
        for mem in &memories {
            assert_eq!(mem.embedding.len(), 384, "embedding should be loaded");
            assert_ne!(mem.content, "Restricted");
        }
    }

    #[tokio::test]
    async fn classification_round_trips_through_database() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        for level in [
            DataClassification::Public,
            DataClassification::Internal,
            DataClassification::Confidential,
        ] {
            let id = format!("mem-{}", level.as_str());
            let mut memory = make_test_memory(&id, &format!("{} memory", level.as_str()));
            memory.classification = level;
            store.save(&memory).await.unwrap();

            let retrieved = store.get_by_id(&id).await.unwrap().unwrap();
            assert_eq!(
                retrieved.classification, level,
                "round-trip failed for {level}"
            );
        }
    }
}
