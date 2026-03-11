// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! MCP resource helpers for exposing Blufio data as MCP resources.
//!
//! Provides URI parsing for `blufio://` resource URIs and async helper
//! functions that read data from [`MemoryStore`] and [`StorageAdapter`].
//!
//! Resource URIs follow this pattern:
//! - `blufio://memory/{id}` -- fetch a single memory by ID
//! - `blufio://memory/search?q={query}&limit={limit}` -- FTS5 keyword search
//! - `blufio://sessions` -- list all sessions
//! - `blufio://sessions/{id}` -- message history for a session

use blufio_core::StorageAdapter;
use blufio_memory::MemoryStore;
use percent_encoding::percent_decode_str;

/// Parsed resource request from a `blufio://` URI.
#[derive(Debug, Clone, PartialEq)]
pub enum ResourceRequest {
    /// Fetch a single memory by its ID.
    MemoryById(String),
    /// Search memories using BM25 full-text search.
    MemorySearch { query: String, limit: usize },
    /// List all sessions.
    SessionList,
    /// Fetch message history for a specific session.
    SessionHistory(String),
}

/// Parse a `blufio://` resource URI into a [`ResourceRequest`].
///
/// # Errors
///
/// Returns an error string if the URI scheme is not `blufio://`,
/// the path is unrecognized, or required query parameters are missing.
pub fn parse_resource_uri(uri: &str) -> Result<ResourceRequest, String> {
    let rest = uri
        .strip_prefix("blufio://")
        .ok_or_else(|| format!("unsupported URI scheme: {uri}"))?;

    // Split on '?' to separate path from query string.
    let (path, query_str) = match rest.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (rest, None),
    };

    match path {
        // blufio://sessions
        "sessions" => Ok(ResourceRequest::SessionList),

        // blufio://memory/search?q=...&limit=...
        "memory/search" => {
            let query_str = query_str.ok_or("memory/search requires query parameters")?;
            let mut q: Option<String> = None;
            let mut limit: usize = 10;

            for pair in query_str.split('&') {
                if let Some((key, value)) = pair.split_once('=') {
                    match key {
                        "q" => {
                            let decoded = percent_decode_str(value)
                                .decode_utf8()
                                .map_err(|e| format!("invalid query encoding: {e}"))?;
                            q = Some(decoded.into_owned());
                        }
                        "limit" => {
                            limit = value
                                .parse()
                                .map_err(|_| format!("invalid limit: {value}"))?;
                        }
                        _ => {} // ignore unknown params
                    }
                }
            }

            let query = q.ok_or("memory/search requires 'q' parameter")?;
            Ok(ResourceRequest::MemorySearch { query, limit })
        }

        // blufio://sessions/{id} or blufio://memory/{id}
        _ => {
            if let Some(session_id) = path.strip_prefix("sessions/") {
                if session_id.is_empty() {
                    return Err("session ID cannot be empty".to_string());
                }
                Ok(ResourceRequest::SessionHistory(session_id.to_string()))
            } else if let Some(memory_id) = path.strip_prefix("memory/") {
                if memory_id.is_empty() {
                    return Err("memory ID cannot be empty".to_string());
                }
                Ok(ResourceRequest::MemoryById(memory_id.to_string()))
            } else {
                Err(format!("unknown resource path: {path}"))
            }
        }
    }
}

/// Read a single memory by ID and return its JSON representation.
///
/// The embedding vector is explicitly excluded from the output.
/// Returns fields: id, content, source, confidence, status, session_id,
/// created_at, updated_at.
pub async fn read_memory_by_id(store: &MemoryStore, id: &str) -> Result<serde_json::Value, String> {
    let memory = store
        .get_by_id(id)
        .await
        .map_err(|e| format!("storage error: {e}"))?
        .ok_or_else(|| format!("Memory not found: {id}"))?;

    Ok(serde_json::json!({
        "id": memory.id,
        "content": memory.content,
        "source": memory.source.as_str(),
        "confidence": memory.confidence,
        "status": memory.status.as_str(),
        "session_id": memory.session_id,
        "created_at": memory.created_at,
        "updated_at": memory.updated_at,
    }))
}

/// Search memories using BM25 full-text search and return results as JSON.
///
/// For each match, fetches the full memory and includes a `relevance_score`.
/// Embedding vectors are excluded.
pub async fn read_memory_search(
    store: &MemoryStore,
    query: &str,
    limit: usize,
) -> Result<serde_json::Value, String> {
    let results = store
        .search_bm25(query, limit)
        .await
        .map_err(|e| format!("search error: {e}"))?;

    let mut memories = Vec::new();
    for (id, score) in results {
        if let Ok(Some(memory)) = store.get_by_id(&id).await {
            memories.push(serde_json::json!({
                "id": memory.id,
                "content": memory.content,
                "source": memory.source.as_str(),
                "confidence": memory.confidence,
                "status": memory.status.as_str(),
                "session_id": memory.session_id,
                "created_at": memory.created_at,
                "updated_at": memory.updated_at,
                "relevance_score": score,
            }));
        }
    }

    Ok(serde_json::Value::Array(memories))
}

/// List all sessions and return summaries as JSON.
///
/// Returns an array of objects with: id, channel, created_at.
pub async fn read_session_list(storage: &dyn StorageAdapter) -> Result<serde_json::Value, String> {
    let sessions = storage
        .list_sessions(None)
        .await
        .map_err(|e| format!("storage error: {e}"))?;

    let summaries: Vec<serde_json::Value> = sessions
        .iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id,
                "channel": s.channel,
                "created_at": s.created_at,
            })
        })
        .collect();

    Ok(serde_json::Value::Array(summaries))
}

/// Read message history for a specific session and return as JSON.
///
/// Returns an array of objects with: id, role, content, created_at.
pub async fn read_session_history(
    storage: &dyn StorageAdapter,
    session_id: &str,
) -> Result<serde_json::Value, String> {
    let messages = storage
        .get_messages(session_id, None)
        .await
        .map_err(|e| format!("storage error: {e}"))?;

    let msgs: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            serde_json::json!({
                "id": m.id,
                "role": m.role,
                "content": m.content,
                "created_at": m.created_at,
            })
        })
        .collect();

    Ok(serde_json::Value::Array(msgs))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── URI parsing tests ──────────────────────────────────────────

    #[test]
    fn parse_memory_by_id() {
        let req = parse_resource_uri("blufio://memory/abc123").unwrap();
        assert_eq!(req, ResourceRequest::MemoryById("abc123".to_string()));
    }

    #[test]
    fn parse_memory_by_id_with_hyphens() {
        let req = parse_resource_uri("blufio://memory/mem-abc-123").unwrap();
        assert_eq!(req, ResourceRequest::MemoryById("mem-abc-123".to_string()));
    }

    #[test]
    fn parse_memory_search_with_query_and_limit() {
        let req = parse_resource_uri("blufio://memory/search?q=test&limit=5").unwrap();
        assert_eq!(
            req,
            ResourceRequest::MemorySearch {
                query: "test".to_string(),
                limit: 5,
            }
        );
    }

    #[test]
    fn parse_memory_search_default_limit() {
        let req = parse_resource_uri("blufio://memory/search?q=hello+world").unwrap();
        assert_eq!(
            req,
            ResourceRequest::MemorySearch {
                query: "hello+world".to_string(),
                limit: 10,
            }
        );
    }

    #[test]
    fn parse_memory_search_encoded_query() {
        let req = parse_resource_uri("blufio://memory/search?q=hello%20world&limit=3").unwrap();
        assert_eq!(
            req,
            ResourceRequest::MemorySearch {
                query: "hello world".to_string(),
                limit: 3,
            }
        );
    }

    #[test]
    fn parse_memory_search_missing_query() {
        let result = parse_resource_uri("blufio://memory/search?limit=5");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("requires 'q' parameter"));
    }

    #[test]
    fn parse_memory_search_missing_query_string() {
        let result = parse_resource_uri("blufio://memory/search");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("requires query parameters"));
    }

    #[test]
    fn parse_session_list() {
        let req = parse_resource_uri("blufio://sessions").unwrap();
        assert_eq!(req, ResourceRequest::SessionList);
    }

    #[test]
    fn parse_session_history() {
        let req = parse_resource_uri("blufio://sessions/sess-1").unwrap();
        assert_eq!(req, ResourceRequest::SessionHistory("sess-1".to_string()));
    }

    #[test]
    fn parse_unknown_scheme_returns_error() {
        let result = parse_resource_uri("https://example.com");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unsupported URI scheme"));
    }

    #[test]
    fn parse_unknown_path_returns_error() {
        let result = parse_resource_uri("blufio://unknown/path");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown resource path"));
    }

    #[test]
    fn parse_empty_memory_id_returns_error() {
        let result = parse_resource_uri("blufio://memory/");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("memory ID cannot be empty"));
    }

    #[test]
    fn parse_empty_session_id_returns_error() {
        let result = parse_resource_uri("blufio://sessions/");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("session ID cannot be empty"));
    }

    // ── Data access tests (require async runtime + SQLite) ────────

    use blufio_memory::types::{Memory, MemorySource, MemoryStatus};

    async fn setup_memory_db() -> tokio_rusqlite::Connection {
        let conn = tokio_rusqlite::Connection::open_in_memory().await.unwrap();
        conn.call(|conn| -> Result<(), tokio_rusqlite::rusqlite::Error> {
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
                END;",
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
            classification: blufio_core::classification::DataClassification::default(),
            created_at: "2026-03-01T00:00:00.000Z".to_string(),
            updated_at: "2026-03-01T00:00:00.000Z".to_string(),
        }
    }

    #[tokio::test]
    async fn read_memory_by_id_returns_json_without_embedding() {
        let conn = setup_memory_db().await;
        let store = MemoryStore::new(conn);
        let memory = make_test_memory("mem-1", "User likes pizza");
        store.save(&memory).await.unwrap();

        let result = read_memory_by_id(&store, "mem-1").await.unwrap();
        assert_eq!(result["id"], "mem-1");
        assert_eq!(result["content"], "User likes pizza");
        assert_eq!(result["source"], "explicit");
        assert_eq!(result["confidence"], 0.9);
        assert_eq!(result["status"], "active");
        // Embedding MUST NOT be in the JSON output.
        assert!(result.get("embedding").is_none());
    }

    #[tokio::test]
    async fn read_memory_by_id_not_found() {
        let conn = setup_memory_db().await;
        let store = MemoryStore::new(conn);

        let result = read_memory_by_id(&store, "nonexistent").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Memory not found"));
    }

    #[tokio::test]
    async fn read_memory_search_returns_results() {
        let conn = setup_memory_db().await;
        let store = MemoryStore::new(conn);
        store
            .save(&make_test_memory(
                "mem-1",
                "The golden retriever is named Max",
            ))
            .await
            .unwrap();
        store
            .save(&make_test_memory("mem-2", "User prefers dark mode"))
            .await
            .unwrap();

        let result = read_memory_search(&store, "golden retriever", 10)
            .await
            .unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "mem-1");
        assert!(arr[0].get("relevance_score").is_some());
        // No embedding in search results.
        assert!(arr[0].get("embedding").is_none());
    }

    #[tokio::test]
    async fn read_memory_search_empty_results() {
        let conn = setup_memory_db().await;
        let store = MemoryStore::new(conn);
        store
            .save(&make_test_memory("mem-1", "User likes pizza"))
            .await
            .unwrap();

        let result = read_memory_search(&store, "quantum physics", 10)
            .await
            .unwrap();
        let arr = result.as_array().unwrap();
        assert!(arr.is_empty());
    }

    // ── StorageAdapter mock for session tests ─────────────────────

    use async_trait::async_trait;
    use blufio_core::error::BlufioError;
    use blufio_core::types::{HealthStatus, Message, QueueEntry, Session};

    struct MockStorage {
        sessions: Vec<Session>,
        messages: Vec<Message>,
    }

    impl MockStorage {
        fn new() -> Self {
            Self {
                sessions: vec![
                    Session {
                        id: "sess-1".to_string(),
                        channel: "telegram".to_string(),
                        user_id: None,
                        state: "active".to_string(),
                        metadata: None,
                        created_at: "2026-03-01T00:00:00Z".to_string(),
                        updated_at: "2026-03-01T00:00:00Z".to_string(),
                        classification: Default::default(),
                    },
                    Session {
                        id: "sess-2".to_string(),
                        channel: "cli".to_string(),
                        user_id: None,
                        state: "closed".to_string(),
                        metadata: None,
                        created_at: "2026-03-02T00:00:00Z".to_string(),
                        updated_at: "2026-03-02T00:00:00Z".to_string(),
                        classification: Default::default(),
                    },
                ],
                messages: vec![
                    Message {
                        id: "msg-1".to_string(),
                        session_id: "sess-1".to_string(),
                        role: "user".to_string(),
                        content: "Hello!".to_string(),
                        token_count: None,
                        metadata: None,
                        created_at: "2026-03-01T00:00:01Z".to_string(),
                        classification: Default::default(),
                    },
                    Message {
                        id: "msg-2".to_string(),
                        session_id: "sess-1".to_string(),
                        role: "assistant".to_string(),
                        content: "Hi there!".to_string(),
                        token_count: Some(5),
                        metadata: None,
                        created_at: "2026-03-01T00:00:02Z".to_string(),
                        classification: Default::default(),
                    },
                ],
            }
        }
    }

    #[async_trait]
    impl blufio_core::traits::adapter::PluginAdapter for MockStorage {
        fn name(&self) -> &str {
            "mock-storage"
        }
        fn version(&self) -> semver::Version {
            semver::Version::new(0, 1, 0)
        }
        fn adapter_type(&self) -> blufio_core::types::AdapterType {
            blufio_core::types::AdapterType::Storage
        }
        async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
            Ok(HealthStatus::Healthy)
        }
        async fn shutdown(&self) -> Result<(), BlufioError> {
            Ok(())
        }
    }

    #[async_trait]
    impl StorageAdapter for MockStorage {
        async fn initialize(&self) -> Result<(), BlufioError> {
            Ok(())
        }
        async fn close(&self) -> Result<(), BlufioError> {
            Ok(())
        }
        async fn create_session(&self, _session: &Session) -> Result<(), BlufioError> {
            Ok(())
        }
        async fn get_session(&self, id: &str) -> Result<Option<Session>, BlufioError> {
            Ok(self.sessions.iter().find(|s| s.id == id).cloned())
        }
        async fn list_sessions(&self, _state: Option<&str>) -> Result<Vec<Session>, BlufioError> {
            Ok(self.sessions.clone())
        }
        async fn update_session_state(&self, _id: &str, _state: &str) -> Result<(), BlufioError> {
            Ok(())
        }
        async fn insert_message(&self, _message: &Message) -> Result<(), BlufioError> {
            Ok(())
        }
        async fn get_messages(
            &self,
            session_id: &str,
            _limit: Option<i64>,
        ) -> Result<Vec<Message>, BlufioError> {
            Ok(self
                .messages
                .iter()
                .filter(|m| m.session_id == session_id)
                .cloned()
                .collect())
        }
        async fn delete_messages_by_ids(
            &self,
            _session_id: &str,
            _message_ids: &[String],
        ) -> Result<usize, BlufioError> {
            Ok(0)
        }
        async fn enqueue(&self, _queue_name: &str, _payload: &str) -> Result<i64, BlufioError> {
            Ok(0)
        }
        async fn dequeue(&self, _queue_name: &str) -> Result<Option<QueueEntry>, BlufioError> {
            Ok(None)
        }
        async fn ack(&self, _id: i64) -> Result<(), BlufioError> {
            Ok(())
        }
        async fn fail(&self, _id: i64) -> Result<(), BlufioError> {
            Ok(())
        }
        async fn get_entity_classification(
            &self,
            _entity_type: &str,
            _entity_id: &str,
        ) -> Result<Option<String>, BlufioError> {
            Ok(None)
        }
        async fn set_entity_classification(
            &self,
            _entity_type: &str,
            _entity_id: &str,
            _level: &str,
        ) -> Result<bool, BlufioError> {
            Ok(false)
        }
        async fn list_entities_by_classification(
            &self,
            _entity_type: &str,
            _level: Option<&str>,
        ) -> Result<Vec<(String, String)>, BlufioError> {
            Ok(vec![])
        }
        async fn bulk_update_classification(
            &self,
            _entity_type: &str,
            _new_level: &str,
            _current_level: Option<&str>,
            _session_id: Option<&str>,
            _from_date: Option<&str>,
            _to_date: Option<&str>,
            _pattern: Option<&str>,
            _dry_run: bool,
        ) -> Result<(usize, usize, usize, Vec<String>), BlufioError> {
            Ok((0, 0, 0, vec![]))
        }
    }

    #[tokio::test]
    async fn read_session_list_returns_summaries() {
        let storage = MockStorage::new();
        let result = read_session_list(&storage).await.unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["id"], "sess-1");
        assert_eq!(arr[0]["channel"], "telegram");
        assert!(arr[0].get("created_at").is_some());
        // Should NOT contain user_id, state, metadata (summary only).
        assert!(arr[0].get("state").is_none());
        assert!(arr[0].get("metadata").is_none());
    }

    #[tokio::test]
    async fn read_session_history_returns_messages() {
        let storage = MockStorage::new();
        let result = read_session_history(&storage, "sess-1").await.unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["id"], "msg-1");
        assert_eq!(arr[0]["role"], "user");
        assert_eq!(arr[0]["content"], "Hello!");
        assert_eq!(arr[1]["role"], "assistant");
        // Should NOT contain token_count or metadata.
        assert!(arr[0].get("token_count").is_none());
    }

    #[tokio::test]
    async fn read_session_history_empty_session() {
        let storage = MockStorage::new();
        let result = read_session_history(&storage, "nonexistent").await.unwrap();
        let arr = result.as_array().unwrap();
        assert!(arr.is_empty());
    }
}
