// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Transparency report generation for GDPR Art. 15 (right of access).
//!
//! Provides count queries across all data types for a given user, including
//! audit trail entry counts with a note about retention policy.

use crate::models::{GdprError, ReportData};

/// Count all data held for a user across all tables.
///
/// Returns a [`ReportData`] with per-type counts. If `audit_conn` is provided,
/// also counts audit entries referencing the user.
pub async fn count_user_data(
    conn: &tokio_rusqlite::Connection,
    audit_conn: Option<&tokio_rusqlite::Connection>,
    session_ids: &[String],
    user_id: &str,
) -> Result<ReportData, GdprError> {
    let session_ids_owned = session_ids.to_vec();

    let (messages, sessions_count, memories, archives, cost_records) = conn
        .call(move |conn| -> Result<(u64, u64, u64, u64, u64), rusqlite::Error> {
            let sessions_count = session_ids_owned.len() as u64;

            if session_ids_owned.is_empty() {
                return Ok((0, 0, 0, 0, 0));
            }

            let placeholders: String = (1..=session_ids_owned.len())
                .map(|i| format!("?{i}"))
                .collect::<Vec<_>>()
                .join(", ");

            // Count messages
            let messages: i64 = {
                let sql = format!(
                    "SELECT COUNT(*) FROM messages WHERE session_id IN ({placeholders}) AND deleted_at IS NULL"
                );
                let mut stmt = conn.prepare(&sql)?;
                stmt.query_row(
                    rusqlite::params_from_iter(session_ids_owned.iter()),
                    |row| row.get(0),
                )?
            };

            // Count memories
            let memories: i64 = {
                let sql = format!(
                    "SELECT COUNT(*) FROM memories WHERE session_id IN ({placeholders})"
                );
                let mut stmt = conn.prepare(&sql)?;
                stmt.query_row(
                    rusqlite::params_from_iter(session_ids_owned.iter()),
                    |row| row.get(0),
                )?
            };

            // Count archives (LIKE-based matching)
            let mut archives: i64 = 0;
            for sid in &session_ids_owned {
                let pattern = format!("%{sid}%");
                let count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM compaction_archives WHERE session_ids LIKE ?1",
                    rusqlite::params![pattern],
                    |row| row.get(0),
                )?;
                archives += count;
            }

            // Count cost records
            let cost_records: i64 = {
                let sql = format!(
                    "SELECT COUNT(*) FROM cost_ledger WHERE session_id IN ({placeholders})"
                );
                let mut stmt = conn.prepare(&sql)?;
                stmt.query_row(
                    rusqlite::params_from_iter(session_ids_owned.iter()),
                    |row| row.get(0),
                )?
            };

            Ok((
                messages as u64,
                sessions_count,
                memories as u64,
                archives as u64,
                cost_records as u64,
            ))
        })
        .await
        .map_err(|e| GdprError::ReportFailed(e.to_string()))?;

    // Count audit entries if audit connection is provided
    let audit_entries = if let Some(audit) = audit_conn {
        let uid = user_id.to_string();
        audit
            .call(move |conn| -> Result<u64, rusqlite::Error> {
                // Check if the audit_entries table exists
                let table_exists: bool = conn
                    .query_row(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='audit_entries'",
                        [],
                        |row| row.get::<_, i64>(0),
                    )
                    .map(|c| c > 0)
                    .unwrap_or(false);

                if !table_exists {
                    return Ok(0);
                }

                let count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM audit_entries WHERE \
                     (actor LIKE ?1 OR session_id LIKE ?1 OR details_json LIKE ?1) \
                     AND pii_marker = 0",
                    rusqlite::params![format!("%{uid}%")],
                    |row| row.get(0),
                )?;
                Ok(count as u64)
            })
            .await
            .unwrap_or(0)
    } else {
        0
    };

    let audit_note = if audit_entries > 0 {
        format!(
            "{audit_entries} audit entries referencing this user (not deletable per retention policy)"
        )
    } else {
        "No audit entries found for this user".to_string()
    };

    Ok(ReportData {
        user_id: user_id.to_string(),
        messages,
        sessions: sessions_count,
        memories,
        archives,
        cost_records,
        audit_entries,
        audit_note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    created_at TEXT NOT NULL DEFAULT '2026-01-15T10:00:00Z',
                    updated_at TEXT NOT NULL DEFAULT '2026-01-15T10:00:00Z',
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
                    created_at TEXT NOT NULL DEFAULT '2026-01-15T10:00:00Z',
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
                    created_at TEXT NOT NULL DEFAULT '2026-01-15T10:00:00Z',
                    updated_at TEXT NOT NULL DEFAULT '2026-01-15T10:00:00Z',
                    classification TEXT NOT NULL DEFAULT 'internal',
                    deleted_at TEXT
                );

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
                    created_at TEXT NOT NULL DEFAULT '2026-01-15T10:00:00Z',
                    deleted_at TEXT
                );",
            )?;
            Ok(())
        })
        .await
        .unwrap();
        conn
    }

    async fn seed_data(conn: &tokio_rusqlite::Connection) {
        conn.call(|conn| -> Result<(), rusqlite::Error> {
            conn.execute(
                "INSERT INTO sessions (id, channel, user_id, state) VALUES ('s1', 'cli', 'u1', 'closed')",
                [],
            )?;
            conn.execute(
                "INSERT INTO messages (id, session_id, role, content) VALUES ('m1', 's1', 'user', 'hello')",
                [],
            )?;
            conn.execute(
                "INSERT INTO messages (id, session_id, role, content) VALUES ('m2', 's1', 'assistant', 'hi')",
                [],
            )?;
            conn.execute(
                "INSERT INTO memories (id, content, embedding, source, session_id) VALUES ('mem1', 'fact', X'00', 'extracted', 's1')",
                [],
            )?;
            conn.execute(
                "INSERT INTO compaction_archives (id, user_id, summary, session_ids, created_at) VALUES ('a1', 'u1', 'summary', '[\"s1\"]', '2026-01-01T00:00:00Z')",
                [],
            )?;
            conn.execute(
                "INSERT INTO cost_ledger (id, session_id, model, feature_type, cost_usd) VALUES ('c1', 's1', 'gpt-4', 'chat', 0.05)",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn count_user_data_returns_correct_counts() {
        let conn = setup_test_db().await;
        seed_data(&conn).await;

        let report = count_user_data(&conn, None, &["s1".into()], "u1")
            .await
            .unwrap();

        assert_eq!(report.user_id, "u1");
        assert_eq!(report.messages, 2);
        assert_eq!(report.sessions, 1);
        assert_eq!(report.memories, 1);
        assert_eq!(report.archives, 1);
        assert_eq!(report.cost_records, 1);
        assert_eq!(report.audit_entries, 0);
    }

    #[tokio::test]
    async fn count_user_data_with_audit_entries() {
        let conn = setup_test_db().await;
        seed_data(&conn).await;

        // Set up audit DB
        let audit_conn = tokio_rusqlite::Connection::open_in_memory().await.unwrap();
        audit_conn
            .call(|conn| -> Result<(), rusqlite::Error> {
                conn.execute_batch(
                    "CREATE TABLE audit_entries (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        entry_hash TEXT NOT NULL,
                        prev_hash TEXT NOT NULL,
                        timestamp TEXT NOT NULL,
                        event_type TEXT NOT NULL,
                        action TEXT NOT NULL,
                        resource_type TEXT NOT NULL,
                        resource_id TEXT NOT NULL,
                        actor TEXT NOT NULL,
                        session_id TEXT NOT NULL,
                        details_json TEXT NOT NULL,
                        pii_marker INTEGER NOT NULL DEFAULT 0
                    );
                    INSERT INTO audit_entries (entry_hash, prev_hash, timestamp, event_type, action, resource_type, resource_id, actor, session_id, details_json)
                    VALUES ('abc', '000', '2026-01-15T10:00:00Z', 'session.created', 'create', 'session', 's1', 'user:u1', 's1', '{}');
                    INSERT INTO audit_entries (entry_hash, prev_hash, timestamp, event_type, action, resource_type, resource_id, actor, session_id, details_json)
                    VALUES ('def', 'abc', '2026-01-15T10:01:00Z', 'message.created', 'create', 'message', 'm1', 'user:u1', 's1', '{\"user_id\":\"u1\"}');",
                )?;
                Ok(())
            })
            .await
            .unwrap();

        let report = count_user_data(&conn, Some(&audit_conn), &["s1".into()], "u1")
            .await
            .unwrap();

        assert_eq!(report.audit_entries, 2);
        assert!(report.audit_note.contains("2 audit entries"));
        assert!(report.audit_note.contains("not deletable"));
    }

    #[tokio::test]
    async fn count_user_data_empty_sessions() {
        let conn = setup_test_db().await;

        let report = count_user_data(&conn, None, &[], "no-user").await.unwrap();

        assert_eq!(report.messages, 0);
        assert_eq!(report.sessions, 0);
        assert_eq!(report.memories, 0);
        assert_eq!(report.archives, 0);
        assert_eq!(report.cost_records, 0);
    }
}
