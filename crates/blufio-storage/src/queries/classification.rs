// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Classification CRUD operations for entities (messages, sessions, memories).

use blufio_core::BlufioError;
use rusqlite::params;

use crate::database::Database;

/// Result of a bulk classification update operation.
#[derive(Debug, Clone, Default)]
pub struct BulkClassificationResult {
    /// Total number of entities matching the filter.
    pub total: usize,
    /// Number of entities successfully updated.
    pub succeeded: usize,
    /// Number of entities that failed to update.
    pub failed: usize,
    /// Error messages for failed entities.
    pub errors: Vec<String>,
}

/// Maps an entity type string to its corresponding SQL table name.
///
/// Returns `None` for unrecognized entity types. The returned table name is
/// safe for direct interpolation into SQL because it comes from a fixed set.
fn table_for_entity(entity_type: &str) -> Option<&'static str> {
    match entity_type {
        "memory" => Some("memories"),
        "message" => Some("messages"),
        "session" => Some("sessions"),
        _ => None,
    }
}

/// Get the classification level of an entity.
///
/// Returns `None` if the entity does not exist.
pub async fn get_entity_classification(
    db: &Database,
    entity_type: &str,
    entity_id: &str,
) -> Result<Option<String>, BlufioError> {
    let table = table_for_entity(entity_type)
        .ok_or_else(|| BlufioError::Internal(format!("unknown entity type: {entity_type}")))?;
    let entity_id = entity_id.to_string();

    db.connection()
        .call(move |conn| {
            let sql = format!("SELECT classification FROM {table} WHERE id = ?1 AND deleted_at IS NULL");
            let mut stmt = conn.prepare(&sql)?;
            let result = stmt.query_row(params![entity_id], |row| row.get::<_, String>(0));
            match result {
                Ok(classification) => Ok(Some(classification)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e),
            }
        })
        .await
        .map_err(crate::database::map_tr_err)
}

/// Set the classification level on an entity.
///
/// Returns `true` if a row was updated, `false` if the entity was not found.
pub async fn set_entity_classification(
    db: &Database,
    entity_type: &str,
    entity_id: &str,
    level: &str,
) -> Result<bool, BlufioError> {
    let table = table_for_entity(entity_type)
        .ok_or_else(|| BlufioError::Internal(format!("unknown entity type: {entity_type}")))?;
    let entity_id = entity_id.to_string();
    let level = level.to_string();

    db.connection()
        .call(move |conn| {
            let sql = format!("UPDATE {table} SET classification = ?1 WHERE id = ?2 AND deleted_at IS NULL");
            let rows_affected = conn.execute(&sql, params![level, entity_id])?;
            Ok(rows_affected > 0)
        })
        .await
        .map_err(crate::database::map_tr_err)
}

/// List entities with their classification levels, optionally filtered by level.
///
/// Returns `Vec<(id, classification)>` pairs.
pub async fn list_entities_by_classification(
    db: &Database,
    entity_type: &str,
    level: Option<&str>,
) -> Result<Vec<(String, String)>, BlufioError> {
    let table = table_for_entity(entity_type)
        .ok_or_else(|| BlufioError::Internal(format!("unknown entity type: {entity_type}")))?;
    let level = level.map(|s| s.to_string());

    db.connection()
        .call(move |conn| {
            let mut results = Vec::new();
            match &level {
                Some(level_filter) => {
                    let sql = format!(
                        "SELECT id, classification FROM {table} WHERE classification = ?1 AND deleted_at IS NULL ORDER BY id"
                    );
                    let mut stmt = conn.prepare(&sql)?;
                    let rows = stmt.query_map(params![level_filter], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })?;
                    for row in rows {
                        results.push(row?);
                    }
                }
                None => {
                    let sql = format!(
                        "SELECT id, classification FROM {table} WHERE deleted_at IS NULL ORDER BY classification, id"
                    );
                    let mut stmt = conn.prepare(&sql)?;
                    let rows = stmt.query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })?;
                    for row in rows {
                        results.push(row?);
                    }
                }
            }
            Ok(results)
        })
        .await
        .map_err(crate::database::map_tr_err)
}

/// Bulk update classification levels with optional filters.
///
/// Supports filtering by current_level, session_id, date range, and content pattern.
/// When `dry_run` is true, returns the count of matching entities without modifying data.
#[allow(clippy::too_many_arguments)]
pub async fn bulk_update_classification(
    db: &Database,
    entity_type: &str,
    new_level: &str,
    current_level: Option<&str>,
    session_id: Option<&str>,
    from_date: Option<&str>,
    to_date: Option<&str>,
    pattern: Option<&str>,
    dry_run: bool,
) -> Result<BulkClassificationResult, BlufioError> {
    let table = table_for_entity(entity_type)
        .ok_or_else(|| BlufioError::Internal(format!("unknown entity type: {entity_type}")))?;
    let is_session_table = entity_type == "session";

    let new_level = new_level.to_string();
    let current_level = current_level.map(|s| s.to_string());
    let session_id = session_id.map(|s| s.to_string());
    let from_date = from_date.map(|s| s.to_string());
    let to_date = to_date.map(|s| s.to_string());
    let pattern = pattern.map(|s| s.to_string());

    db.connection()
        .call(move |conn| {
            // Helper: build condition list and params from filters.
            let build_conditions =
                |start_idx: usize| -> (Vec<String>, Vec<Box<dyn rusqlite::types::ToSql>>) {
                    let mut conds: Vec<String> = Vec::new();
                    let mut vals: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
                    let mut idx = start_idx;

                    // Always exclude soft-deleted records.
                    conds.push("deleted_at IS NULL".to_string());

                    if let Some(ref cl) = current_level {
                        conds.push(format!("classification = ?{idx}"));
                        vals.push(Box::new(cl.clone()));
                        idx += 1;
                    }

                    if !is_session_table && let Some(ref sid) = session_id {
                        conds.push(format!("session_id = ?{idx}"));
                        vals.push(Box::new(sid.clone()));
                        idx += 1;
                    }

                    if let Some(ref fd) = from_date {
                        conds.push(format!("created_at >= ?{idx}"));
                        vals.push(Box::new(fd.clone()));
                        idx += 1;
                    }

                    if let Some(ref td) = to_date {
                        conds.push(format!("created_at <= ?{idx}"));
                        vals.push(Box::new(td.clone()));
                        idx += 1;
                    }

                    if !is_session_table && let Some(ref p) = pattern {
                        conds.push(format!("content LIKE ?{idx}"));
                        vals.push(Box::new(p.clone()));
                    }

                    (conds, vals)
                };

            let format_where = |conds: &[String]| -> String {
                if conds.is_empty() {
                    String::new()
                } else {
                    format!(" WHERE {}", conds.join(" AND "))
                }
            };

            if dry_run {
                let (conds, vals) = build_conditions(1);
                let where_clause = format_where(&conds);
                let params_slice: Vec<&dyn rusqlite::types::ToSql> =
                    vals.iter().map(|b| b.as_ref()).collect();

                let sql = format!("SELECT COUNT(*) FROM {table}{where_clause}");
                let count: usize =
                    conn.query_row(&sql, params_slice.as_slice(), |row| row.get(0))?;
                Ok(BulkClassificationResult {
                    total: count,
                    succeeded: 0,
                    failed: 0,
                    errors: Vec::new(),
                })
            } else {
                // For UPDATE, ?1 is new_level; conditions start at ?2.
                let (conds, cond_vals) = build_conditions(2);
                let where_clause = format_where(&conds);

                let mut update_params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
                update_params.push(Box::new(new_level.clone()));
                update_params.extend(cond_vals);

                let sql = format!("UPDATE {table} SET classification = ?1{where_clause}");
                let update_slice: Vec<&dyn rusqlite::types::ToSql> =
                    update_params.iter().map(|b| b.as_ref()).collect();
                let rows_affected = conn.execute(&sql, update_slice.as_slice())?;

                Ok(BulkClassificationResult {
                    total: rows_affected,
                    succeeded: rows_affected,
                    failed: 0,
                    errors: Vec::new(),
                })
            }
        })
        .await
        .map_err(crate::database::map_tr_err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Message, Session};
    use crate::queries::{messages, sessions};
    use blufio_core::classification::DataClassification;
    use tempfile::tempdir;

    async fn setup_db() -> (Database, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::open(db_path.to_str().unwrap()).await.unwrap();
        (db, dir)
    }

    fn make_session(id: &str) -> Session {
        Session {
            id: id.to_string(),
            channel: "cli".to_string(),
            user_id: Some("user-1".to_string()),
            state: "active".to_string(),
            metadata: None,
            created_at: "2026-01-01T00:00:00.000Z".to_string(),
            updated_at: "2026-01-01T00:00:00.000Z".to_string(),
            classification: DataClassification::default(),
        }
    }

    fn make_msg(id: &str, session_id: &str, classification: DataClassification) -> Message {
        Message {
            id: id.to_string(),
            session_id: session_id.to_string(),
            role: "user".to_string(),
            content: format!("content for {id}"),
            token_count: Some(10),
            metadata: None,
            created_at: "2026-01-01T00:00:01.000Z".to_string(),
            classification,
        }
    }

    #[tokio::test]
    async fn get_classification_returns_correct_level_for_message() {
        let (db, _dir) = setup_db().await;
        let session = make_session("sess-1");
        sessions::create_session(&db, &session).await.unwrap();

        let msg = make_msg("msg-1", "sess-1", DataClassification::Confidential);
        messages::insert_message(&db, &msg).await.unwrap();

        let result = get_entity_classification(&db, "message", "msg-1")
            .await
            .unwrap();
        assert_eq!(result, Some("confidential".to_string()));
    }

    #[tokio::test]
    async fn get_classification_returns_correct_level_for_session() {
        let (db, _dir) = setup_db().await;
        let mut session = make_session("sess-cls");
        session.classification = DataClassification::Public;
        sessions::create_session(&db, &session).await.unwrap();

        let result = get_entity_classification(&db, "session", "sess-cls")
            .await
            .unwrap();
        assert_eq!(result, Some("public".to_string()));
    }

    #[tokio::test]
    async fn get_classification_returns_none_for_nonexistent() {
        let (db, _dir) = setup_db().await;
        let result = get_entity_classification(&db, "message", "no-such-msg")
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn set_classification_updates_and_returns_true() {
        let (db, _dir) = setup_db().await;
        let session = make_session("sess-1");
        sessions::create_session(&db, &session).await.unwrap();

        let msg = make_msg("msg-1", "sess-1", DataClassification::Internal);
        messages::insert_message(&db, &msg).await.unwrap();

        let updated = set_entity_classification(&db, "message", "msg-1", "restricted")
            .await
            .unwrap();
        assert!(updated);

        // Verify the change.
        let result = get_entity_classification(&db, "message", "msg-1")
            .await
            .unwrap();
        assert_eq!(result, Some("restricted".to_string()));
    }

    #[tokio::test]
    async fn set_classification_returns_false_for_nonexistent() {
        let (db, _dir) = setup_db().await;
        let updated = set_entity_classification(&db, "message", "no-such", "public")
            .await
            .unwrap();
        assert!(!updated);
    }

    #[tokio::test]
    async fn list_by_classification_with_filter() {
        let (db, _dir) = setup_db().await;
        let session = make_session("sess-1");
        sessions::create_session(&db, &session).await.unwrap();

        let msg1 = make_msg("msg-1", "sess-1", DataClassification::Internal);
        let msg2 = make_msg("msg-2", "sess-1", DataClassification::Confidential);
        let msg3 = make_msg("msg-3", "sess-1", DataClassification::Internal);
        messages::insert_message(&db, &msg1).await.unwrap();
        messages::insert_message(&db, &msg2).await.unwrap();
        messages::insert_message(&db, &msg3).await.unwrap();

        let internal = list_entities_by_classification(&db, "message", Some("internal"))
            .await
            .unwrap();
        assert_eq!(internal.len(), 2);
        assert!(internal.iter().all(|(_, c)| c == "internal"));

        let confidential = list_entities_by_classification(&db, "message", Some("confidential"))
            .await
            .unwrap();
        assert_eq!(confidential.len(), 1);
        assert_eq!(confidential[0].0, "msg-2");
    }

    #[tokio::test]
    async fn list_by_classification_without_filter() {
        let (db, _dir) = setup_db().await;
        let session = make_session("sess-1");
        sessions::create_session(&db, &session).await.unwrap();

        let msg1 = make_msg("msg-1", "sess-1", DataClassification::Public);
        let msg2 = make_msg("msg-2", "sess-1", DataClassification::Internal);
        messages::insert_message(&db, &msg1).await.unwrap();
        messages::insert_message(&db, &msg2).await.unwrap();

        let all = list_entities_by_classification(&db, "message", None)
            .await
            .unwrap();
        assert_eq!(all.len(), 2);
        // Should be ordered by classification then id.
        assert_eq!(all[0].1, "internal");
        assert_eq!(all[1].1, "public");
    }

    #[tokio::test]
    async fn bulk_update_dry_run_counts_without_modifying() {
        let (db, _dir) = setup_db().await;
        let session = make_session("sess-1");
        sessions::create_session(&db, &session).await.unwrap();

        let msg1 = make_msg("msg-1", "sess-1", DataClassification::Internal);
        let msg2 = make_msg("msg-2", "sess-1", DataClassification::Internal);
        messages::insert_message(&db, &msg1).await.unwrap();
        messages::insert_message(&db, &msg2).await.unwrap();

        let result = bulk_update_classification(
            &db,
            "message",
            "confidential",
            Some("internal"),
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .unwrap();
        assert_eq!(result.total, 2);
        assert_eq!(result.succeeded, 0);

        // Verify nothing was modified.
        let check = get_entity_classification(&db, "message", "msg-1")
            .await
            .unwrap();
        assert_eq!(check, Some("internal".to_string()));
    }

    #[tokio::test]
    async fn bulk_update_applies_update_and_returns_count() {
        let (db, _dir) = setup_db().await;
        let session = make_session("sess-1");
        sessions::create_session(&db, &session).await.unwrap();

        let msg1 = make_msg("msg-1", "sess-1", DataClassification::Internal);
        let msg2 = make_msg("msg-2", "sess-1", DataClassification::Internal);
        let msg3 = make_msg("msg-3", "sess-1", DataClassification::Confidential);
        messages::insert_message(&db, &msg1).await.unwrap();
        messages::insert_message(&db, &msg2).await.unwrap();
        messages::insert_message(&db, &msg3).await.unwrap();

        let result = bulk_update_classification(
            &db,
            "message",
            "restricted",
            None,
            None,
            None,
            None,
            None,
            false,
        )
        .await
        .unwrap();
        assert_eq!(result.total, 3);
        assert_eq!(result.succeeded, 3);

        // Verify all were updated.
        let all = list_entities_by_classification(&db, "message", Some("restricted"))
            .await
            .unwrap();
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn bulk_update_with_current_level_filter() {
        let (db, _dir) = setup_db().await;
        let session = make_session("sess-1");
        sessions::create_session(&db, &session).await.unwrap();

        let msg1 = make_msg("msg-1", "sess-1", DataClassification::Internal);
        let msg2 = make_msg("msg-2", "sess-1", DataClassification::Confidential);
        messages::insert_message(&db, &msg1).await.unwrap();
        messages::insert_message(&db, &msg2).await.unwrap();

        let result = bulk_update_classification(
            &db,
            "message",
            "restricted",
            Some("internal"),
            None,
            None,
            None,
            None,
            false,
        )
        .await
        .unwrap();
        assert_eq!(result.succeeded, 1);

        // Only msg-1 should be restricted now.
        let check1 = get_entity_classification(&db, "message", "msg-1")
            .await
            .unwrap();
        assert_eq!(check1, Some("restricted".to_string()));

        let check2 = get_entity_classification(&db, "message", "msg-2")
            .await
            .unwrap();
        assert_eq!(check2, Some("confidential".to_string()));
    }
}
