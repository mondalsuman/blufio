// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Message CRUD operations.

use blufio_core::BlufioError;
use blufio_core::classification::DataClassification;
use rusqlite::params;

use crate::database::Database;
use crate::models::Message;

/// Insert a new message.
pub async fn insert_message(db: &Database, msg: &Message) -> Result<(), BlufioError> {
    let msg = msg.clone();
    db.connection()
        .call(move |conn| {
            conn.execute(
                "INSERT INTO messages (id, session_id, role, content, token_count, metadata, created_at, classification)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    msg.id,
                    msg.session_id,
                    msg.role,
                    msg.content,
                    msg.token_count,
                    msg.metadata,
                    msg.created_at,
                    msg.classification.as_str(),
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(crate::database::map_tr_err)
}

/// Get messages for a session in chronological order.
pub async fn get_messages_for_session(
    db: &Database,
    session_id: &str,
    limit: Option<i64>,
) -> Result<Vec<Message>, BlufioError> {
    let session_id = session_id.to_string();
    db.connection()
        .call(move |conn| {
            let mut messages = Vec::new();
            match limit {
                Some(lim) => {
                    let mut stmt = conn.prepare(
                        "SELECT id, session_id, role, content, token_count, metadata, created_at, classification
                         FROM messages WHERE session_id = ?1 AND classification != 'restricted'
                         ORDER BY created_at ASC LIMIT ?2",
                    )?;
                    let rows = stmt.query_map(params![session_id, lim], |row| {
                        Ok(row_to_message(row))
                    })?;
                    for row in rows {
                        messages.push(row?);
                    }
                }
                None => {
                    let mut stmt = conn.prepare(
                        "SELECT id, session_id, role, content, token_count, metadata, created_at, classification
                         FROM messages WHERE session_id = ?1 AND classification != 'restricted'
                         ORDER BY created_at ASC",
                    )?;
                    let rows = stmt.query_map(params![session_id], |row| {
                        Ok(row_to_message(row))
                    })?;
                    for row in rows {
                        messages.push(row?);
                    }
                }
            }
            Ok(messages)
        })
        .await
        .map_err(crate::database::map_tr_err)
}

/// Delete specific messages by their IDs within a session.
///
/// Returns the number of rows deleted.
pub async fn delete_messages_by_ids(
    db: &Database,
    session_id: &str,
    message_ids: &[String],
) -> Result<usize, BlufioError> {
    if message_ids.is_empty() {
        return Ok(0);
    }
    let session_id = session_id.to_string();
    let ids = message_ids.to_vec();
    db.connection()
        .call(move |conn| {
            // Build placeholders: (?2, ?3, ?4, ...)
            let placeholders: Vec<String> = (0..ids.len()).map(|i| format!("?{}", i + 2)).collect();
            let sql = format!(
                "DELETE FROM messages WHERE session_id = ?1 AND id IN ({})",
                placeholders.join(", ")
            );
            let mut stmt = conn.prepare(&sql)?;

            // Bind session_id as param 1, then each message id
            let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> =
                Vec::with_capacity(1 + ids.len());
            params_vec.push(Box::new(session_id));
            for id in &ids {
                params_vec.push(Box::new(id.clone()));
            }
            let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                params_vec.iter().map(|p| p.as_ref()).collect();

            let deleted = stmt.execute(param_refs.as_slice())?;
            Ok(deleted)
        })
        .await
        .map_err(crate::database::map_tr_err)
}

/// Convert a rusqlite Row to a Message struct.
///
/// Column order: id(0), session_id(1), role(2), content(3), token_count(4),
/// metadata(5), created_at(6), classification(7).
fn row_to_message(row: &rusqlite::Row) -> Message {
    let classification_str: String = row.get(7).unwrap_or_default();
    Message {
        id: row.get(0).unwrap_or_default(),
        session_id: row.get(1).unwrap_or_default(),
        role: row.get(2).unwrap_or_default(),
        content: row.get(3).unwrap_or_default(),
        token_count: row.get(4).unwrap_or_default(),
        metadata: row.get(5).unwrap_or_default(),
        created_at: row.get(6).unwrap_or_default(),
        classification: DataClassification::from_str_value(&classification_str).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Session;
    use crate::queries::sessions::create_session;
    use blufio_core::classification::DataClassification;
    use tempfile::tempdir;

    async fn setup_db_with_session() -> (Database, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::open(db_path.to_str().unwrap()).await.unwrap();

        let session = Session {
            id: "sess-1".to_string(),
            channel: "cli".to_string(),
            user_id: None,
            state: "active".to_string(),
            metadata: None,
            created_at: "2026-01-01T00:00:00.000Z".to_string(),
            updated_at: "2026-01-01T00:00:00.000Z".to_string(),
            classification: DataClassification::default(),
        };
        create_session(&db, &session).await.unwrap();
        (db, dir)
    }

    fn make_msg(id: &str, role: &str, content: &str, timestamp: &str) -> Message {
        Message {
            id: id.to_string(),
            session_id: "sess-1".to_string(),
            role: role.to_string(),
            content: content.to_string(),
            token_count: Some(10),
            metadata: None,
            created_at: timestamp.to_string(),
            classification: DataClassification::default(),
        }
    }

    #[tokio::test]
    async fn insert_and_get_messages_in_order() {
        let (db, _dir) = setup_db_with_session().await;

        let m1 = make_msg("m1", "user", "hello", "2026-01-01T00:00:01.000Z");
        let m2 = make_msg("m2", "assistant", "hi there", "2026-01-01T00:00:02.000Z");
        let m3 = make_msg("m3", "user", "how are you?", "2026-01-01T00:00:03.000Z");

        insert_message(&db, &m1).await.unwrap();
        insert_message(&db, &m2).await.unwrap();
        insert_message(&db, &m3).await.unwrap();

        let messages = get_messages_for_session(&db, "sess-1", None).await.unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].id, "m1");
        assert_eq!(messages[1].id, "m2");
        assert_eq!(messages[2].id, "m3");
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].role, "assistant");

        db.close().await.unwrap();
    }

    #[tokio::test]
    async fn get_messages_with_limit() {
        let (db, _dir) = setup_db_with_session().await;

        for i in 0..5 {
            let msg = make_msg(
                &format!("m{i}"),
                "user",
                &format!("msg {i}"),
                &format!("2026-01-01T00:00:0{i}.000Z"),
            );
            insert_message(&db, &msg).await.unwrap();
        }

        let messages = get_messages_for_session(&db, "sess-1", Some(3))
            .await
            .unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].id, "m0");
        assert_eq!(messages[2].id, "m2");

        db.close().await.unwrap();
    }

    #[tokio::test]
    async fn get_messages_empty_session() {
        let (db, _dir) = setup_db_with_session().await;
        let messages = get_messages_for_session(&db, "sess-1", None).await.unwrap();
        assert!(messages.is_empty());
        db.close().await.unwrap();
    }
}
