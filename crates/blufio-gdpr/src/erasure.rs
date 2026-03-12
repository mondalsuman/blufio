// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Erasure orchestrator for GDPR right-to-erasure (Art. 17).
//!
//! Orchestrates atomic multi-table cascade deletion across all user data:
//! messages, memories, compaction archives, cost records (anonymized), and sessions,
//! within a single SQLite transaction.
//!
//! Audit trail erasure is best-effort via [`erase_audit_trail`] (separate DB).
//! FTS5 consistency is verified post-erasure via [`cleanup_memory_index`].

use crate::models::{ErasureManifest, GdprError};

/// Find all sessions belonging to a user.
///
/// Queries `sessions WHERE user_id = ?1 AND deleted_at IS NULL` and returns
/// parsed session records. Sessions returned here are "fully owned" by the user.
pub async fn find_user_sessions(
    conn: &tokio_rusqlite::Connection,
    user_id: &str,
) -> Result<Vec<UserSession>, GdprError> {
    let user_id = user_id.to_string();
    conn.call(move |conn| -> Result<Vec<UserSession>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT id, channel, user_id, state, metadata, created_at, updated_at, \
             COALESCE(classification, 'internal') \
             FROM sessions WHERE user_id = ?1 AND deleted_at IS NULL",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![user_id], |row| {
                Ok(UserSession {
                    id: row.get(0)?,
                    channel: row.get(1)?,
                    user_id: row.get::<_, Option<String>>(2)?,
                    state: row.get(3)?,
                    metadata: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    classification: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await
    .map_err(|e| GdprError::ErasureFailed(e.to_string()))
}

/// Count how many sessions have state "active".
pub fn check_active_sessions(sessions: &[UserSession]) -> usize {
    sessions.iter().filter(|s| s.state == "active").count()
}

/// Execute the full erasure cascade within a single SQLite transaction.
///
/// Atomically deletes: messages, memories, compaction archives, anonymizes
/// cost records, and deletes sessions. Returns an [`ErasureManifest`] with
/// counts of affected rows.
pub async fn execute_erasure(
    conn: &tokio_rusqlite::Connection,
    session_ids: &[String],
    user_id: &str,
) -> Result<ErasureManifest, GdprError> {
    let session_ids = session_ids.to_vec();
    let user_id = user_id.to_string();

    conn.call(move |conn| -> Result<ErasureManifest, rusqlite::Error> {
        let tx = conn.transaction()?;

        // Helper: generate "?1, ?2, ..." placeholders
        let placeholders: String = (1..=session_ids.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        // Params are passed via params_from_iter directly in each query.

        // a. DELETE messages
        let messages_deleted = if session_ids.is_empty() {
            0
        } else {
            tx.execute(
                &format!(
                    "DELETE FROM messages WHERE session_id IN ({placeholders}) AND deleted_at IS NULL"
                ),
                rusqlite::params_from_iter(session_ids.iter()),
            )? as u64
        };

        // b. DELETE memories (hard delete for GDPR -- FTS5 triggers fire automatically)
        let memories_deleted = if session_ids.is_empty() {
            0
        } else {
            tx.execute(
                &format!("DELETE FROM memories WHERE session_id IN ({placeholders})"),
                rusqlite::params_from_iter(session_ids.iter()),
            )? as u64
        };

        // c. DELETE compaction archives (LIKE-based JSON matching)
        let mut archives_deleted: u64 = 0;
        for sid in &session_ids {
            let pattern = format!("%{sid}%");
            let count = tx.execute(
                "DELETE FROM compaction_archives WHERE session_ids LIKE ?1",
                rusqlite::params![pattern],
            )? as u64;
            archives_deleted += count;
        }

        // d. Anonymize cost records (SET session_id = NULL, preserve aggregates)
        let cost_records_anonymized = if session_ids.is_empty() {
            0
        } else {
            tx.execute(
                &format!(
                    "UPDATE cost_ledger SET session_id = NULL WHERE session_id IN ({placeholders})"
                ),
                rusqlite::params_from_iter(session_ids.iter()),
            )? as u64
        };

        // e. DELETE sessions
        let sessions_deleted = if session_ids.is_empty() {
            0
        } else {
            tx.execute(
                &format!(
                    "DELETE FROM sessions WHERE id IN ({placeholders}) AND deleted_at IS NULL"
                ),
                rusqlite::params_from_iter(session_ids.iter()),
            )? as u64
        };

        tx.commit()?;

        let manifest = ErasureManifest {
            manifest_id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            user_id,
            messages_deleted,
            sessions_deleted,
            memories_deleted,
            archives_deleted,
            cost_records_anonymized,
            audit_entries_redacted: 0, // Set by caller after audit erasure
            session_ids,
        };

        Ok(manifest)
    })
    .await
    .map_err(|e| GdprError::ErasureFailed(e.to_string()))
}

/// Erase audit trail entries for a user (best-effort).
///
/// Wraps [`blufio_audit::chain::erase_audit_entries`]. Returns the count
/// of erased entries on success, or a warning string on failure.
pub async fn erase_audit_trail(
    audit_conn: &tokio_rusqlite::Connection,
    user_id: &str,
) -> Result<u64, String> {
    blufio_audit::chain::erase_audit_entries(audit_conn, user_id)
        .await
        .map(|report| report.entries_erased as u64)
        .map_err(|e| format!("audit erasure warning: {e}"))
}

/// Verify FTS5 consistency after memory erasure and rebuild if needed.
///
/// Compares row counts between `memories` and `memories_fts`. If they
/// differ, triggers an FTS5 rebuild.
pub async fn cleanup_memory_index(
    conn: &tokio_rusqlite::Connection,
    _session_ids: &[String],
) -> Result<(), GdprError> {
    conn.call(|conn| -> Result<(), rusqlite::Error> {
        let mem_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))?;
        let fts_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM memories_fts", [], |row| row.get(0))?;

        if mem_count != fts_count {
            conn.execute_batch("INSERT INTO memories_fts(memories_fts) VALUES('rebuild')")?;
        }
        Ok(())
    })
    .await
    .map_err(|e| GdprError::ErasureFailed(format!("FTS5 cleanup failed: {e}")))
}

/// A lightweight session record used during erasure operations.
///
/// Contains only the fields needed for erasure logic (identifying sessions,
/// checking active state). This avoids depending on `blufio-core::types::Session`.
#[derive(Debug, Clone)]
pub struct UserSession {
    pub id: String,
    pub channel: String,
    pub user_id: Option<String>,
    pub state: String,
    pub metadata: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub classification: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create in-memory SQLite with schema matching real tables.
    async fn setup_test_db() -> tokio_rusqlite::Connection {
        let conn = tokio_rusqlite::Connection::open_in_memory().await.unwrap();
        conn.call(|conn| -> Result<(), rusqlite::Error> {
            conn.execute_batch(
                "CREATE TABLE sessions (
                    id TEXT PRIMARY KEY NOT NULL,
                    channel TEXT NOT NULL,
                    user_id TEXT,
                    state TEXT NOT NULL DEFAULT 'active',
                    metadata TEXT,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    classification TEXT NOT NULL DEFAULT 'internal',
                    deleted_at TEXT
                );

                CREATE TABLE messages (
                    id TEXT PRIMARY KEY NOT NULL,
                    session_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    content TEXT NOT NULL,
                    token_count INTEGER,
                    metadata TEXT,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    classification TEXT NOT NULL DEFAULT 'internal',
                    deleted_at TEXT
                );

                CREATE TABLE memories (
                    id TEXT PRIMARY KEY NOT NULL,
                    content TEXT NOT NULL,
                    embedding BLOB NOT NULL,
                    source TEXT NOT NULL,
                    confidence REAL NOT NULL DEFAULT 0.5,
                    status TEXT NOT NULL DEFAULT 'active',
                    superseded_by TEXT,
                    session_id TEXT,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    classification TEXT NOT NULL DEFAULT 'internal',
                    deleted_at TEXT
                );

                CREATE VIRTUAL TABLE memories_fts USING fts5(
                    content,
                    content='memories',
                    content_rowid='rowid'
                );

                CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
                    INSERT INTO memories_fts(rowid, content) VALUES (new.rowid, new.content);
                END;
                CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN
                    INSERT INTO memories_fts(memories_fts, rowid, content)
                        VALUES('delete', old.rowid, old.content);
                END;

                CREATE TABLE compaction_archives (
                    id TEXT PRIMARY KEY NOT NULL,
                    user_id TEXT NOT NULL,
                    summary TEXT NOT NULL,
                    quality_score REAL,
                    session_ids TEXT NOT NULL DEFAULT '[]',
                    classification TEXT NOT NULL DEFAULT 'internal',
                    created_at TEXT NOT NULL,
                    token_count INTEGER
                );

                CREATE TABLE cost_ledger (
                    id TEXT PRIMARY KEY NOT NULL,
                    session_id TEXT,
                    model TEXT NOT NULL,
                    feature_type TEXT NOT NULL,
                    input_tokens INTEGER NOT NULL DEFAULT 0,
                    output_tokens INTEGER NOT NULL DEFAULT 0,
                    cache_read_tokens INTEGER NOT NULL DEFAULT 0,
                    cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
                    cost_usd REAL NOT NULL DEFAULT 0.0,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    deleted_at TEXT
                );",
            )?;
            Ok(())
        })
        .await
        .unwrap();
        conn
    }

    /// Insert test data for a given user.
    async fn seed_user_data(conn: &tokio_rusqlite::Connection, user_id: &str, session_id: &str) {
        let uid = user_id.to_string();
        let sid = session_id.to_string();
        conn.call(move |conn| -> Result<(), rusqlite::Error> {
            conn.execute(
                "INSERT INTO sessions (id, channel, user_id, state) VALUES (?1, 'cli', ?2, 'closed')",
                rusqlite::params![sid, uid],
            )?;
            conn.execute(
                "INSERT INTO messages (id, session_id, role, content) VALUES (?1, ?2, 'user', 'hello')",
                rusqlite::params![format!("msg-{sid}-1"), sid],
            )?;
            conn.execute(
                "INSERT INTO messages (id, session_id, role, content) VALUES (?1, ?2, 'assistant', 'hi there')",
                rusqlite::params![format!("msg-{sid}-2"), sid],
            )?;
            conn.execute(
                "INSERT INTO memories (id, content, embedding, source, session_id) VALUES (?1, 'user likes rust', X'00', 'extracted', ?2)",
                rusqlite::params![format!("mem-{sid}-1"), sid],
            )?;
            conn.execute(
                "INSERT INTO compaction_archives (id, user_id, summary, session_ids, created_at) VALUES (?1, ?2, 'summary', ?3, '2026-01-01T00:00:00Z')",
                rusqlite::params![format!("arch-{sid}-1"), uid, format!("[\"{sid}\"]")],
            )?;
            conn.execute(
                "INSERT INTO cost_ledger (id, session_id, model, feature_type, input_tokens, output_tokens, cost_usd) VALUES (?1, ?2, 'gpt-4', 'chat', 100, 50, 0.05)",
                rusqlite::params![format!("cost-{sid}-1"), sid],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn find_user_sessions_returns_matching_sessions() {
        let conn = setup_test_db().await;
        seed_user_data(&conn, "user-1", "sess-1").await;
        seed_user_data(&conn, "user-2", "sess-2").await;

        let sessions = find_user_sessions(&conn, "user-1").await.unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "sess-1");
        assert_eq!(sessions[0].user_id.as_deref(), Some("user-1"));
    }

    #[tokio::test]
    async fn find_user_sessions_returns_empty_for_nonexistent_user() {
        let conn = setup_test_db().await;
        let sessions = find_user_sessions(&conn, "no-such-user").await.unwrap();
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn check_active_sessions_counts_active() {
        let sessions = vec![
            UserSession {
                id: "s1".into(),
                channel: "cli".into(),
                user_id: Some("u1".into()),
                state: "active".into(),
                metadata: None,
                created_at: "".into(),
                updated_at: "".into(),
                classification: "internal".into(),
            },
            UserSession {
                id: "s2".into(),
                channel: "cli".into(),
                user_id: Some("u1".into()),
                state: "closed".into(),
                metadata: None,
                created_at: "".into(),
                updated_at: "".into(),
                classification: "internal".into(),
            },
            UserSession {
                id: "s3".into(),
                channel: "cli".into(),
                user_id: Some("u1".into()),
                state: "active".into(),
                metadata: None,
                created_at: "".into(),
                updated_at: "".into(),
                classification: "internal".into(),
            },
        ];
        assert_eq!(check_active_sessions(&sessions), 2);
    }

    #[tokio::test]
    async fn execute_erasure_deletes_all_user_data() {
        let conn = setup_test_db().await;
        seed_user_data(&conn, "user-1", "sess-1").await;
        seed_user_data(&conn, "user-2", "sess-2").await;

        let manifest = execute_erasure(&conn, &["sess-1".to_string()], "user-1")
            .await
            .unwrap();

        assert_eq!(manifest.messages_deleted, 2);
        assert_eq!(manifest.memories_deleted, 1);
        assert_eq!(manifest.archives_deleted, 1);
        assert_eq!(manifest.cost_records_anonymized, 1);
        assert_eq!(manifest.sessions_deleted, 1);
        assert_eq!(manifest.user_id, "user-1");
        assert_eq!(manifest.session_ids, vec!["sess-1".to_string()]);

        // Verify user-2 data is untouched
        let remaining = conn
            .call(|conn| -> Result<i64, rusqlite::Error> {
                conn.query_row(
                    "SELECT COUNT(*) FROM messages WHERE session_id = 'sess-2'",
                    [],
                    |row| row.get(0),
                )
            })
            .await
            .unwrap();
        assert_eq!(remaining, 2, "user-2 messages should be untouched");
    }

    #[tokio::test]
    async fn execute_erasure_preserves_other_users_sessions() {
        let conn = setup_test_db().await;
        seed_user_data(&conn, "user-1", "sess-1").await;
        seed_user_data(&conn, "user-2", "sess-2").await;

        execute_erasure(&conn, &["sess-1".to_string()], "user-1")
            .await
            .unwrap();

        // user-2 session should still exist
        let count: i64 = conn
            .call(|conn| -> Result<i64, rusqlite::Error> {
                conn.query_row(
                    "SELECT COUNT(*) FROM sessions WHERE id = 'sess-2'",
                    [],
                    |row| row.get(0),
                )
            })
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn cost_anonymization_preserves_other_fields() {
        let conn = setup_test_db().await;
        seed_user_data(&conn, "user-1", "sess-1").await;

        execute_erasure(&conn, &["sess-1".to_string()], "user-1")
            .await
            .unwrap();

        // Cost record should still exist with NULL session_id but preserved fields
        let (session_id, model, cost): (Option<String>, String, f64) = conn
            .call(|conn| -> Result<(Option<String>, String, f64), rusqlite::Error> {
                conn.query_row(
                    "SELECT session_id, model, cost_usd FROM cost_ledger WHERE id = 'cost-sess-1-1'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
            })
            .await
            .unwrap();
        assert!(
            session_id.is_none(),
            "session_id should be NULL after anonymization"
        );
        assert_eq!(model, "gpt-4", "model field should be preserved");
        assert!(
            (cost - 0.05).abs() < f64::EPSILON,
            "cost should be preserved"
        );
    }

    #[tokio::test]
    async fn execute_erasure_empty_session_ids() {
        let conn = setup_test_db().await;
        let manifest = execute_erasure(&conn, &[], "no-user").await.unwrap();
        assert_eq!(manifest.messages_deleted, 0);
        assert_eq!(manifest.sessions_deleted, 0);
        assert_eq!(manifest.memories_deleted, 0);
        assert_eq!(manifest.archives_deleted, 0);
        assert_eq!(manifest.cost_records_anonymized, 0);
    }

    #[tokio::test]
    async fn cleanup_memory_index_no_mismatch() {
        let conn = setup_test_db().await;
        seed_user_data(&conn, "user-1", "sess-1").await;

        // After seeding, FTS5 triggers should keep things in sync
        let result = cleanup_memory_index(&conn, &["sess-1".to_string()]).await;
        assert!(result.is_ok());
    }
}
