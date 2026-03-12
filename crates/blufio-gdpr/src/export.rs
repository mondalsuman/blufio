// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Data export logic for GDPR data portability (Art. 20).
//!
//! Supports JSON and CSV export formats with filtering by session, date range,
//! and data type. PII redaction is opt-in via `apply_redaction()`.
//! Restricted data is always excluded from exports.

use std::path::{Path, PathBuf};

use blufio_core::classification::DataClassification;
use blufio_security::classification_guard::ClassificationGuard;

use crate::models::{
    ExportData, ExportEnvelope, ExportMetadata, FilterCriteria, GdprError,
};

/// Collected user data ready for export.
#[derive(Debug, Clone)]
pub struct CollectedData {
    /// Messages as JSON values.
    pub messages: Vec<serde_json::Value>,
    /// Sessions as JSON values.
    pub sessions: Vec<serde_json::Value>,
    /// Memories as JSON values.
    pub memories: Vec<serde_json::Value>,
    /// Cost records as JSON values.
    pub cost_records: Vec<serde_json::Value>,
    /// Count of items excluded due to Restricted classification.
    pub restricted_excluded: usize,
}

/// Collect all user data for the given session IDs, applying filters.
///
/// Queries messages, sessions, memories, and cost records. Excludes any record
/// with `Restricted` classification. Applies optional filters from `FilterCriteria`.
pub async fn collect_user_data(
    conn: &tokio_rusqlite::Connection,
    session_ids: &[String],
    filters: &FilterCriteria,
) -> Result<CollectedData, GdprError> {
    let session_ids = session_ids.to_vec();
    let filters = filters.clone();

    conn.call(move |conn| -> Result<CollectedData, rusqlite::Error> {
        let guard = ClassificationGuard::instance();
        let mut restricted_excluded = 0usize;

        let should_include_type = |t: &str| -> bool {
            match &filters.data_types {
                Some(types) => types.iter().any(|dt| dt == t),
                None => true,
            }
        };

        // Filter by session if specified
        let effective_session_ids: Vec<&String> = match &filters.session_id {
            Some(sid) => session_ids.iter().filter(|s| s.as_str() == sid).collect(),
            None => session_ids.iter().collect(),
        };

        if effective_session_ids.is_empty() {
            return Ok(CollectedData {
                messages: vec![],
                sessions: vec![],
                memories: vec![],
                cost_records: vec![],
                restricted_excluded: 0,
            });
        }

        let eff_placeholders: String = (1..=effective_session_ids.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");

        // --- Messages ---
        let messages = if should_include_type("messages") {
            let mut sql = format!(
                "SELECT id, session_id, role, content, token_count, metadata, created_at, \
                 COALESCE(classification, 'internal') \
                 FROM messages WHERE session_id IN ({eff_placeholders}) AND deleted_at IS NULL"
            );
            if let Some(ref since) = filters.since {
                sql.push_str(&format!(" AND created_at >= '{since}'"));
            }
            if let Some(ref until) = filters.until {
                sql.push_str(&format!(" AND created_at <= '{until}'"));
            }

            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(
                rusqlite::params_from_iter(effective_session_ids.iter()),
                |row| {
                    let classification_str: String = row.get(7)?;
                    Ok((
                        serde_json::json!({
                            "id": row.get::<_, String>(0)?,
                            "session_id": row.get::<_, String>(1)?,
                            "role": row.get::<_, String>(2)?,
                            "content": row.get::<_, String>(3)?,
                            "token_count": row.get::<_, Option<i64>>(4)?,
                            "metadata": row.get::<_, Option<String>>(5)?,
                            "created_at": row.get::<_, String>(6)?,
                            "classification": &classification_str,
                        }),
                        classification_str,
                    ))
                },
            )?;

            let mut result = Vec::new();
            for row in rows {
                let (value, cls_str) = row?;
                let cls = DataClassification::from_str_value(&cls_str)
                    .unwrap_or_default();
                if guard.can_export(cls) {
                    result.push(value);
                } else {
                    restricted_excluded += 1;
                }
            }
            result
        } else {
            vec![]
        };

        // --- Sessions ---
        let sessions = if should_include_type("sessions") {
            let mut sql = format!(
                "SELECT id, channel, user_id, state, metadata, created_at, updated_at, \
                 COALESCE(classification, 'internal') \
                 FROM sessions WHERE id IN ({eff_placeholders}) AND deleted_at IS NULL"
            );
            if let Some(ref since) = filters.since {
                sql.push_str(&format!(" AND created_at >= '{since}'"));
            }
            if let Some(ref until) = filters.until {
                sql.push_str(&format!(" AND created_at <= '{until}'"));
            }

            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(
                rusqlite::params_from_iter(effective_session_ids.iter()),
                |row| {
                    let classification_str: String = row.get(7)?;
                    Ok((
                        serde_json::json!({
                            "id": row.get::<_, String>(0)?,
                            "channel": row.get::<_, String>(1)?,
                            "user_id": row.get::<_, Option<String>>(2)?,
                            "state": row.get::<_, String>(3)?,
                            "metadata": row.get::<_, Option<String>>(4)?,
                            "created_at": row.get::<_, String>(5)?,
                            "updated_at": row.get::<_, String>(6)?,
                            "classification": &classification_str,
                        }),
                        classification_str,
                    ))
                },
            )?;

            let mut result = Vec::new();
            for row in rows {
                let (value, cls_str) = row?;
                let cls = DataClassification::from_str_value(&cls_str)
                    .unwrap_or_default();
                if guard.can_export(cls) {
                    result.push(value);
                } else {
                    restricted_excluded += 1;
                }
            }
            result
        } else {
            vec![]
        };

        // --- Memories (skip embedding) ---
        let memories = if should_include_type("memories") {
            let mut sql = format!(
                "SELECT id, content, source, confidence, status, session_id, \
                 COALESCE(classification, 'internal'), created_at, updated_at \
                 FROM memories WHERE session_id IN ({eff_placeholders})"
            );
            if let Some(ref since) = filters.since {
                sql.push_str(&format!(" AND created_at >= '{since}'"));
            }
            if let Some(ref until) = filters.until {
                sql.push_str(&format!(" AND created_at <= '{until}'"));
            }

            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(
                rusqlite::params_from_iter(effective_session_ids.iter()),
                |row| {
                    let classification_str: String = row.get(6)?;
                    Ok((
                        serde_json::json!({
                            "id": row.get::<_, String>(0)?,
                            "content": row.get::<_, String>(1)?,
                            "source": row.get::<_, String>(2)?,
                            "confidence": row.get::<_, f64>(3)?,
                            "status": row.get::<_, String>(4)?,
                            "session_id": row.get::<_, Option<String>>(5)?,
                            "classification": &classification_str,
                            "created_at": row.get::<_, String>(7)?,
                            "updated_at": row.get::<_, String>(8)?,
                        }),
                        classification_str,
                    ))
                },
            )?;

            let mut result = Vec::new();
            for row in rows {
                let (value, cls_str) = row?;
                let cls = DataClassification::from_str_value(&cls_str)
                    .unwrap_or_default();
                if guard.can_export(cls) {
                    result.push(value);
                } else {
                    restricted_excluded += 1;
                }
            }
            result
        } else {
            vec![]
        };

        // --- Cost records ---
        let cost_records = if should_include_type("cost_records") {
            let sql = format!(
                "SELECT id, session_id, model, feature_type, input_tokens, output_tokens, \
                 cache_read_tokens, cache_creation_tokens, cost_usd, created_at \
                 FROM cost_ledger WHERE session_id IN ({eff_placeholders})"
            );

            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(
                rusqlite::params_from_iter(effective_session_ids.iter()),
                |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, String>(0)?,
                        "session_id": row.get::<_, Option<String>>(1)?,
                        "model": row.get::<_, String>(2)?,
                        "feature_type": row.get::<_, String>(3)?,
                        "input_tokens": row.get::<_, i64>(4)?,
                        "output_tokens": row.get::<_, i64>(5)?,
                        "cache_read_tokens": row.get::<_, i64>(6)?,
                        "cache_creation_tokens": row.get::<_, i64>(7)?,
                        "cost_usd": row.get::<_, f64>(8)?,
                        "created_at": row.get::<_, String>(9)?,
                    }))
                },
            )?;

            rows.collect::<Result<Vec<_>, _>>()?
        } else {
            vec![]
        };

        Ok(CollectedData {
            messages,
            sessions,
            memories,
            cost_records,
            restricted_excluded,
        })
    })
    .await
    .map_err(|e| GdprError::ExportFailed(e.to_string()))
}

/// Apply PII redaction to collected data in place.
///
/// Uses [`ClassificationGuard::redact_for_export`] on content fields of
/// messages and memories. Restricted items should already be filtered out.
pub fn apply_redaction(data: &mut CollectedData) {
    let guard = ClassificationGuard::instance();

    // Redact message content
    for msg in &mut data.messages {
        if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
            let cls_str = msg
                .get("classification")
                .and_then(|v| v.as_str())
                .unwrap_or("internal");
            let cls = DataClassification::from_str_value(cls_str).unwrap_or_default();
            if let Some(redacted) = guard.redact_for_export(content, cls) {
                msg["content"] = serde_json::Value::String(redacted);
            }
        }
    }

    // Redact memory content
    for mem in &mut data.memories {
        if let Some(content) = mem.get("content").and_then(|v| v.as_str()) {
            let cls_str = mem
                .get("classification")
                .and_then(|v| v.as_str())
                .unwrap_or("internal");
            let cls = DataClassification::from_str_value(cls_str).unwrap_or_default();
            if let Some(redacted) = guard.redact_for_export(content, cls) {
                mem["content"] = serde_json::Value::String(redacted);
            }
        }
    }

    // Redact session metadata
    for sess in &mut data.sessions {
        if let Some(metadata) = sess.get("metadata").and_then(|v| v.as_str()) {
            let cls_str = sess
                .get("classification")
                .and_then(|v| v.as_str())
                .unwrap_or("internal");
            let cls = DataClassification::from_str_value(cls_str).unwrap_or_default();
            if let Some(redacted) = guard.redact_for_export(metadata, cls) {
                sess["metadata"] = serde_json::Value::String(redacted);
            }
        }
    }
}

/// Write collected data as a JSON export file.
///
/// Produces an [`ExportEnvelope`] with metadata header and data sections.
/// Returns the file size in bytes. Flushes and syncs the file for data safety.
pub fn write_json_export(
    data: &CollectedData,
    metadata: &ExportMetadata,
    output_path: &Path,
) -> Result<u64, GdprError> {
    use std::io::Write;

    // Create parent directory if needed
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            GdprError::ExportDirNotWritable(format!("{}: {e}", parent.display()))
        })?;
    }

    let envelope = ExportEnvelope {
        export_metadata: metadata.clone(),
        data: ExportData {
            messages: data.messages.clone(),
            sessions: data.sessions.clone(),
            memories: data.memories.clone(),
            cost_records: data.cost_records.clone(),
        },
    };

    let json = serde_json::to_string_pretty(&envelope)
        .map_err(|e| GdprError::ExportFailed(format!("JSON serialization failed: {e}")))?;

    let mut file = std::fs::File::create(output_path)
        .map_err(|e| GdprError::ExportFailed(format!("cannot create file: {e}")))?;
    file.write_all(json.as_bytes())
        .map_err(|e| GdprError::ExportFailed(format!("write failed: {e}")))?;
    file.flush()
        .map_err(|e| GdprError::ExportFailed(format!("flush failed: {e}")))?;
    file.sync_all()
        .map_err(|e| GdprError::ExportFailed(format!("sync failed: {e}")))?;

    let size = json.len() as u64;
    Ok(size)
}

/// Write collected data as a CSV export file.
///
/// Produces a single CSV with a `data_type` column to distinguish rows.
/// Columns: data_type, id, session_id, content, role, created_at, updated_at,
/// classification, metadata_json.
/// Returns the file size in bytes.
pub fn write_csv_export(
    data: &CollectedData,
    _metadata: &ExportMetadata,
    output_path: &Path,
) -> Result<u64, GdprError> {
    // Create parent directory if needed
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            GdprError::ExportDirNotWritable(format!("{}: {e}", parent.display()))
        })?;
    }

    let mut wtr = csv::Writer::from_path(output_path)
        .map_err(|e| GdprError::ExportFailed(format!("cannot create CSV: {e}")))?;

    // Write header
    wtr.write_record([
        "data_type",
        "id",
        "session_id",
        "content",
        "role",
        "created_at",
        "updated_at",
        "classification",
        "metadata_json",
    ])
    .map_err(|e| GdprError::ExportFailed(format!("CSV header write failed: {e}")))?;

    // Messages
    for msg in &data.messages {
        wtr.write_record([
            "message",
            msg.get("id").and_then(|v| v.as_str()).unwrap_or(""),
            msg.get("session_id").and_then(|v| v.as_str()).unwrap_or(""),
            msg.get("content").and_then(|v| v.as_str()).unwrap_or(""),
            msg.get("role").and_then(|v| v.as_str()).unwrap_or(""),
            msg.get("created_at").and_then(|v| v.as_str()).unwrap_or(""),
            "", // messages don't have updated_at
            msg.get("classification").and_then(|v| v.as_str()).unwrap_or(""),
            &msg.get("metadata")
                .map(|v| v.to_string())
                .unwrap_or_default(),
        ])
        .map_err(|e| GdprError::ExportFailed(format!("CSV write failed: {e}")))?;
    }

    // Sessions
    for sess in &data.sessions {
        wtr.write_record([
            "session",
            sess.get("id").and_then(|v| v.as_str()).unwrap_or(""),
            "", // session_id is the id itself
            "", // no content field
            "", // no role
            sess.get("created_at").and_then(|v| v.as_str()).unwrap_or(""),
            sess.get("updated_at").and_then(|v| v.as_str()).unwrap_or(""),
            sess.get("classification").and_then(|v| v.as_str()).unwrap_or(""),
            &sess
                .get("metadata")
                .map(|v| v.to_string())
                .unwrap_or_default(),
        ])
        .map_err(|e| GdprError::ExportFailed(format!("CSV write failed: {e}")))?;
    }

    // Memories
    for mem in &data.memories {
        wtr.write_record([
            "memory",
            mem.get("id").and_then(|v| v.as_str()).unwrap_or(""),
            mem.get("session_id").and_then(|v| v.as_str()).unwrap_or(""),
            mem.get("content").and_then(|v| v.as_str()).unwrap_or(""),
            "", // no role
            mem.get("created_at").and_then(|v| v.as_str()).unwrap_or(""),
            mem.get("updated_at").and_then(|v| v.as_str()).unwrap_or(""),
            mem.get("classification").and_then(|v| v.as_str()).unwrap_or(""),
            "", // no extra metadata
        ])
        .map_err(|e| GdprError::ExportFailed(format!("CSV write failed: {e}")))?;
    }

    // Cost records
    for cost in &data.cost_records {
        // Flatten cost-specific fields into metadata_json
        let meta = serde_json::json!({
            "model": cost.get("model"),
            "feature_type": cost.get("feature_type"),
            "input_tokens": cost.get("input_tokens"),
            "output_tokens": cost.get("output_tokens"),
            "cost_usd": cost.get("cost_usd"),
        });
        wtr.write_record([
            "cost_record",
            cost.get("id").and_then(|v| v.as_str()).unwrap_or(""),
            cost.get("session_id").and_then(|v| v.as_str()).unwrap_or(""),
            "", // no content
            "", // no role
            cost.get("created_at").and_then(|v| v.as_str()).unwrap_or(""),
            "", // no updated_at
            "", // no classification on cost records
            &meta.to_string(),
        ])
        .map_err(|e| GdprError::ExportFailed(format!("CSV write failed: {e}")))?;
    }

    wtr.flush()
        .map_err(|e| GdprError::ExportFailed(format!("CSV flush failed: {e}")))?;

    let size = std::fs::metadata(output_path)
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(size)
}

/// Resolve the output path for an export file.
///
/// If `custom_output` is provided, uses that directly. Otherwise constructs
/// a path from the GDPR config export directory or the default data directory.
pub fn resolve_export_path(
    config: &crate::config::GdprConfig,
    user_id: &str,
    format: &str,
    custom_output: Option<&str>,
    data_dir: &str,
) -> PathBuf {
    if let Some(custom) = custom_output {
        return PathBuf::from(custom);
    }

    let base_dir = config
        .export_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(data_dir).join("exports"));

    let ts = chrono::Utc::now()
        .format("%Y%m%dT%H%M%SZ")
        .to_string();

    base_dir.join(format!("gdpr-export-{user_id}-{ts}.{format}"))
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
                "INSERT INTO sessions (id, channel, user_id, state, created_at) \
                 VALUES ('s1', 'cli', 'u1', 'closed', '2026-01-15T10:00:00Z')",
                [],
            )?;
            conn.execute(
                "INSERT INTO messages (id, session_id, role, content, created_at) \
                 VALUES ('m1', 's1', 'user', 'Hello world', '2026-01-15T10:00:00Z')",
                [],
            )?;
            conn.execute(
                "INSERT INTO messages (id, session_id, role, content, created_at) \
                 VALUES ('m2', 's1', 'assistant', 'Hi there!', '2026-01-15T11:00:00Z')",
                [],
            )?;
            conn.execute(
                "INSERT INTO memories (id, content, embedding, source, session_id) \
                 VALUES ('mem1', 'user likes rust', X'00', 'extracted', 's1')",
                [],
            )?;
            conn.execute(
                "INSERT INTO cost_ledger (id, session_id, model, feature_type, input_tokens, output_tokens, cost_usd) \
                 VALUES ('c1', 's1', 'gpt-4', 'chat', 100, 50, 0.05)",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }

    fn no_filters() -> FilterCriteria {
        FilterCriteria {
            session_id: None,
            since: None,
            until: None,
            data_types: None,
            redacted: false,
        }
    }

    #[tokio::test]
    async fn collect_user_data_returns_all_types() {
        let conn = setup_test_db().await;
        seed_data(&conn).await;

        let data = collect_user_data(&conn, &["s1".into()], &no_filters())
            .await
            .unwrap();

        assert_eq!(data.messages.len(), 2);
        assert_eq!(data.sessions.len(), 1);
        assert_eq!(data.memories.len(), 1);
        assert_eq!(data.cost_records.len(), 1);
        assert_eq!(data.restricted_excluded, 0);
    }

    #[tokio::test]
    async fn collect_with_since_until_filters() {
        let conn = setup_test_db().await;
        seed_data(&conn).await;

        let filters = FilterCriteria {
            since: Some("2026-01-15T10:30:00Z".into()),
            until: None,
            session_id: None,
            data_types: None,
            redacted: false,
        };
        let data = collect_user_data(&conn, &["s1".into()], &filters)
            .await
            .unwrap();

        // Only the assistant message at 11:00 should match
        assert_eq!(data.messages.len(), 1);
        assert_eq!(
            data.messages[0].get("role").and_then(|v| v.as_str()),
            Some("assistant")
        );
    }

    #[tokio::test]
    async fn collect_with_session_filter() {
        let conn = setup_test_db().await;
        seed_data(&conn).await;

        // Seed another session
        conn.call(|conn| -> Result<(), rusqlite::Error> {
            conn.execute(
                "INSERT INTO sessions (id, channel, user_id, state) VALUES ('s2', 'cli', 'u1', 'closed')",
                [],
            )?;
            conn.execute(
                "INSERT INTO messages (id, session_id, role, content) VALUES ('m3', 's2', 'user', 'second session')",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let filters = FilterCriteria {
            session_id: Some("s1".into()),
            since: None,
            until: None,
            data_types: None,
            redacted: false,
        };
        let data = collect_user_data(&conn, &["s1".into(), "s2".into()], &filters)
            .await
            .unwrap();

        // Only s1 data
        assert_eq!(data.messages.len(), 2);
        assert_eq!(data.sessions.len(), 1);
    }

    #[tokio::test]
    async fn collect_with_type_filter() {
        let conn = setup_test_db().await;
        seed_data(&conn).await;

        let filters = FilterCriteria {
            data_types: Some(vec!["messages".into()]),
            session_id: None,
            since: None,
            until: None,
            redacted: false,
        };
        let data = collect_user_data(&conn, &["s1".into()], &filters)
            .await
            .unwrap();

        assert_eq!(data.messages.len(), 2);
        assert!(data.sessions.is_empty());
        assert!(data.memories.is_empty());
        assert!(data.cost_records.is_empty());
    }

    #[tokio::test]
    async fn collect_excludes_restricted_data() {
        let conn = setup_test_db().await;
        seed_data(&conn).await;

        // Mark one message as restricted
        conn.call(|conn| -> Result<(), rusqlite::Error> {
            conn.execute(
                "UPDATE messages SET classification = 'restricted' WHERE id = 'm1'",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let data = collect_user_data(&conn, &["s1".into()], &no_filters())
            .await
            .unwrap();

        assert_eq!(data.messages.len(), 1, "restricted message should be excluded");
        assert_eq!(data.restricted_excluded, 1);
    }

    #[tokio::test]
    async fn write_json_export_produces_valid_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("export.json");

        let data = CollectedData {
            messages: vec![serde_json::json!({"id": "m1", "content": "hello"})],
            sessions: vec![],
            memories: vec![],
            cost_records: vec![],
            restricted_excluded: 0,
        };
        let metadata = ExportMetadata {
            timestamp: "2026-01-15T10:00:00Z".into(),
            user_id: "test-user".into(),
            blufio_version: "0.1.0".into(),
            filter_criteria: no_filters(),
        };

        let size = write_json_export(&data, &metadata, &path).unwrap();
        assert!(size > 0);
        assert!(path.exists());

        // Verify valid JSON with envelope structure
        let contents = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert!(parsed.get("export_metadata").is_some());
        assert!(parsed.get("data").is_some());
        assert_eq!(
            parsed["export_metadata"]["user_id"].as_str(),
            Some("test-user")
        );
        assert_eq!(parsed["data"]["messages"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn write_json_export_with_redaction() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("export-redacted.json");

        let mut data = CollectedData {
            messages: vec![serde_json::json!({
                "id": "m1",
                "content": "Contact user@example.com for details",
                "classification": "internal"
            })],
            sessions: vec![],
            memories: vec![],
            cost_records: vec![],
            restricted_excluded: 0,
        };

        apply_redaction(&mut data);

        let metadata = ExportMetadata {
            timestamp: "2026-01-15T10:00:00Z".into(),
            user_id: "test-user".into(),
            blufio_version: "0.1.0".into(),
            filter_criteria: FilterCriteria {
                redacted: true,
                ..no_filters()
            },
        };

        write_json_export(&data, &metadata, &path).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("[EMAIL]"), "PII should be redacted");
        assert!(!contents.contains("user@example.com"), "original email should be removed");
    }

    #[tokio::test]
    async fn write_csv_export_produces_valid_csv() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("export.csv");

        let data = CollectedData {
            messages: vec![serde_json::json!({
                "id": "m1",
                "session_id": "s1",
                "role": "user",
                "content": "Hello",
                "created_at": "2026-01-15T10:00:00Z",
                "classification": "internal"
            })],
            sessions: vec![serde_json::json!({
                "id": "s1",
                "channel": "cli",
                "created_at": "2026-01-15T10:00:00Z",
                "updated_at": "2026-01-15T10:00:00Z",
                "classification": "internal"
            })],
            memories: vec![],
            cost_records: vec![],
            restricted_excluded: 0,
        };
        let metadata = ExportMetadata {
            timestamp: "2026-01-15T10:00:00Z".into(),
            user_id: "test-user".into(),
            blufio_version: "0.1.0".into(),
            filter_criteria: no_filters(),
        };

        let size = write_csv_export(&data, &metadata, &path).unwrap();
        assert!(size > 0);

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("data_type,id,session_id"));
        assert!(contents.contains("message,m1,s1"));
        assert!(contents.contains("session,s1"));
    }

    #[test]
    fn csv_handles_special_characters() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("special.csv");

        let data = CollectedData {
            messages: vec![serde_json::json!({
                "id": "m1",
                "session_id": "s1",
                "role": "user",
                "content": "Hello, world\nNew line here\nAnd a \"quote\"",
                "created_at": "2026-01-15T10:00:00Z",
                "classification": "internal"
            })],
            sessions: vec![],
            memories: vec![],
            cost_records: vec![],
            restricted_excluded: 0,
        };
        let metadata = ExportMetadata {
            timestamp: "2026-01-15T10:00:00Z".into(),
            user_id: "test-user".into(),
            blufio_version: "0.1.0".into(),
            filter_criteria: no_filters(),
        };

        write_csv_export(&data, &metadata, &path).unwrap();

        // Parse back with csv reader to verify correctness
        let mut rdr = csv::Reader::from_path(&path).unwrap();
        let records: Vec<csv::StringRecord> = rdr.records().map(|r| r.unwrap()).collect();
        assert_eq!(records.len(), 1);
        // The content field should contain the original text with commas and newlines
        let content = &records[0][3]; // content is 4th column (0-indexed: 3)
        assert!(content.contains(','));
        assert!(content.contains('\n'));
        assert!(content.contains('"'));
    }

    #[test]
    fn resolve_export_path_uses_custom_output() {
        let config = crate::config::GdprConfig::default();
        let path = resolve_export_path(&config, "u1", "json", Some("/tmp/custom.json"), "/data");
        assert_eq!(path, PathBuf::from("/tmp/custom.json"));
    }

    #[test]
    fn resolve_export_path_uses_default_dir() {
        let config = crate::config::GdprConfig::default();
        let path = resolve_export_path(&config, "u1", "json", None, "/data");
        let path_str = path.to_str().unwrap();
        assert!(path_str.starts_with("/data/exports/gdpr-export-u1-"));
        assert!(path_str.ends_with(".json"));
    }

    #[test]
    fn resolve_export_path_uses_config_export_dir() {
        let mut config = crate::config::GdprConfig::default();
        config.export_dir = Some("/custom/exports".into());
        let path = resolve_export_path(&config, "u1", "csv", None, "/data");
        let path_str = path.to_str().unwrap();
        assert!(path_str.starts_with("/custom/exports/gdpr-export-u1-"));
        assert!(path_str.ends_with(".csv"));
    }
}
