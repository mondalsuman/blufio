// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Message CRUD operations.

use blufio_core::BlufioError;
use rusqlite::params;

use crate::database::Database;
use crate::models::Message;

/// Insert a new message.
pub async fn insert_message(db: &Database, msg: &Message) -> Result<(), BlufioError> {
    let msg = msg.clone();
    db.connection()
        .call(move |conn| {
            conn.execute(
                "INSERT INTO messages (id, session_id, role, content, token_count, metadata, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    msg.id,
                    msg.session_id,
                    msg.role,
                    msg.content,
                    msg.token_count,
                    msg.metadata,
                    msg.created_at,
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
                        "SELECT id, session_id, role, content, token_count, metadata, created_at
                         FROM messages WHERE session_id = ?1
                         ORDER BY created_at ASC LIMIT ?2",
                    )?;
                    let rows = stmt.query_map(params![session_id, lim], |row| {
                        Ok(Message {
                            id: row.get(0)?,
                            session_id: row.get(1)?,
                            role: row.get(2)?,
                            content: row.get(3)?,
                            token_count: row.get(4)?,
                            metadata: row.get(5)?,
                            created_at: row.get(6)?,
                        })
                    })?;
                    for row in rows {
                        messages.push(row?);
                    }
                }
                None => {
                    let mut stmt = conn.prepare(
                        "SELECT id, session_id, role, content, token_count, metadata, created_at
                         FROM messages WHERE session_id = ?1
                         ORDER BY created_at ASC",
                    )?;
                    let rows = stmt.query_map(params![session_id], |row| {
                        Ok(Message {
                            id: row.get(0)?,
                            session_id: row.get(1)?,
                            role: row.get(2)?,
                            content: row.get(3)?,
                            token_count: row.get(4)?,
                            metadata: row.get(5)?,
                            created_at: row.get(6)?,
                        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Session;
    use crate::queries::sessions::create_session;
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

        let messages = get_messages_for_session(&db, "sess-1", None)
            .await
            .unwrap();
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
        let messages = get_messages_for_session(&db, "sess-1", None)
            .await
            .unwrap();
        assert!(messages.is_empty());
        db.close().await.unwrap();
    }
}
