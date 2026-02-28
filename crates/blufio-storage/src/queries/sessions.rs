// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Session CRUD operations.

use blufio_core::BlufioError;
use rusqlite::params;

use crate::database::Database;
use crate::models::Session;

/// Create a new session.
pub async fn create_session(db: &Database, session: &Session) -> Result<(), BlufioError> {
    let session = session.clone();
    db.connection()
        .call(move |conn| {
            conn.execute(
                "INSERT INTO sessions (id, channel, user_id, state, metadata, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    session.id,
                    session.channel,
                    session.user_id,
                    session.state,
                    session.metadata,
                    session.created_at,
                    session.updated_at,
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(crate::database::map_tr_err)
}

/// Get a session by ID.
pub async fn get_session(db: &Database, id: &str) -> Result<Option<Session>, BlufioError> {
    let id = id.to_string();
    db.connection()
        .call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, channel, user_id, state, metadata, created_at, updated_at
                 FROM sessions WHERE id = ?1",
            )?;
            let result = stmt.query_row(params![id], |row| {
                Ok(Session {
                    id: row.get(0)?,
                    channel: row.get(1)?,
                    user_id: row.get(2)?,
                    state: row.get(3)?,
                    metadata: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            });
            match result {
                Ok(session) => Ok(Some(session)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e),
            }
        })
        .await
        .map_err(crate::database::map_tr_err)
}

/// List sessions, optionally filtered by state.
pub async fn list_sessions(
    db: &Database,
    state: Option<&str>,
) -> Result<Vec<Session>, BlufioError> {
    let state = state.map(|s| s.to_string());
    db.connection()
        .call(move |conn| {
            let mut sessions = Vec::new();
            match &state {
                Some(state_filter) => {
                    let mut stmt = conn.prepare(
                        "SELECT id, channel, user_id, state, metadata, created_at, updated_at
                         FROM sessions WHERE state = ?1 ORDER BY created_at DESC",
                    )?;
                    let rows = stmt.query_map(params![state_filter], |row| {
                        Ok(Session {
                            id: row.get(0)?,
                            channel: row.get(1)?,
                            user_id: row.get(2)?,
                            state: row.get(3)?,
                            metadata: row.get(4)?,
                            created_at: row.get(5)?,
                            updated_at: row.get(6)?,
                        })
                    })?;
                    for row in rows {
                        sessions.push(row?);
                    }
                }
                None => {
                    let mut stmt = conn.prepare(
                        "SELECT id, channel, user_id, state, metadata, created_at, updated_at
                         FROM sessions ORDER BY created_at DESC",
                    )?;
                    let rows = stmt.query_map([], |row| {
                        Ok(Session {
                            id: row.get(0)?,
                            channel: row.get(1)?,
                            user_id: row.get(2)?,
                            state: row.get(3)?,
                            metadata: row.get(4)?,
                            created_at: row.get(5)?,
                            updated_at: row.get(6)?,
                        })
                    })?;
                    for row in rows {
                        sessions.push(row?);
                    }
                }
            }
            Ok(sessions)
        })
        .await
        .map_err(crate::database::map_tr_err)
}

/// Update a session's state and updated_at timestamp.
pub async fn update_session_state(
    db: &Database,
    id: &str,
    state: &str,
) -> Result<(), BlufioError> {
    let id = id.to_string();
    let state = state.to_string();
    db.connection()
        .call(move |conn| {
            conn.execute(
                "UPDATE sessions SET state = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?2",
                params![state, id],
            )?;
            Ok(())
        })
        .await
        .map_err(crate::database::map_tr_err)
}

#[cfg(test)]
mod tests {
    use super::*;
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
        }
    }

    #[tokio::test]
    async fn create_and_get_session_roundtrips() {
        let (db, _dir) = setup_db().await;
        let session = make_session("sess-1");

        create_session(&db, &session).await.unwrap();
        let retrieved = get_session(&db, "sess-1").await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, "sess-1");
        assert_eq!(retrieved.channel, "cli");
        assert_eq!(retrieved.user_id, Some("user-1".to_string()));
        assert_eq!(retrieved.state, "active");

        db.close().await.unwrap();
    }

    #[tokio::test]
    async fn get_nonexistent_session_returns_none() {
        let (db, _dir) = setup_db().await;
        let result = get_session(&db, "no-such-session").await.unwrap();
        assert!(result.is_none());
        db.close().await.unwrap();
    }

    #[tokio::test]
    async fn list_sessions_with_filter() {
        let (db, _dir) = setup_db().await;
        let s1 = make_session("s1");
        let mut s2 = make_session("s2");
        s2.state = "closed".to_string();

        create_session(&db, &s1).await.unwrap();
        create_session(&db, &s2).await.unwrap();

        let all = list_sessions(&db, None).await.unwrap();
        assert_eq!(all.len(), 2);

        let active = list_sessions(&db, Some("active")).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "s1");

        let closed = list_sessions(&db, Some("closed")).await.unwrap();
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].id, "s2");

        db.close().await.unwrap();
    }

    #[tokio::test]
    async fn update_session_state_works() {
        let (db, _dir) = setup_db().await;
        let session = make_session("s-upd");
        create_session(&db, &session).await.unwrap();

        update_session_state(&db, "s-upd", "paused").await.unwrap();

        let retrieved = get_session(&db, "s-upd").await.unwrap().unwrap();
        assert_eq!(retrieved.state, "paused");
        db.close().await.unwrap();
    }
}
