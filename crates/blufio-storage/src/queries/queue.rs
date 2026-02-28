// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Queue operations for crash-safe message processing.

use blufio_core::BlufioError;
use rusqlite::params;

use crate::database::Database;
use crate::models::QueueEntry;

/// Enqueue a new item. Returns the auto-generated queue entry ID.
pub async fn enqueue(
    db: &Database,
    queue_name: &str,
    payload: &str,
) -> Result<i64, BlufioError> {
    let queue_name = queue_name.to_string();
    let payload = payload.to_string();
    db.connection()
        .call(move |conn| {
            conn.execute(
                "INSERT INTO queue (queue_name, payload) VALUES (?1, ?2)",
                params![queue_name, payload],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .await
        .map_err(crate::database::map_tr_err)
}

/// Dequeue the next pending entry from the named queue.
///
/// Atomically selects the oldest pending entry and marks it as "processing"
/// with a 5-minute lock timeout. Returns `None` if the queue is empty.
pub async fn dequeue(db: &Database, queue_name: &str) -> Result<Option<QueueEntry>, BlufioError> {
    let queue_name = queue_name.to_string();
    db.connection()
        .call(move |conn| {
            // Use a transaction to atomically find + update the next pending entry.
            let tx = conn.transaction()?;

            let result = {
                let mut stmt = tx.prepare(
                    "SELECT id, queue_name, payload, status, attempts, max_attempts,
                            created_at, updated_at, locked_until
                     FROM queue
                     WHERE queue_name = ?1 AND status = 'pending'
                     ORDER BY id ASC
                     LIMIT 1",
                )?;
                stmt.query_row(params![queue_name], |row| {
                    Ok(QueueEntry {
                        id: row.get(0)?,
                        queue_name: row.get(1)?,
                        payload: row.get(2)?,
                        status: row.get(3)?,
                        attempts: row.get(4)?,
                        max_attempts: row.get(5)?,
                        created_at: row.get(6)?,
                        updated_at: row.get(7)?,
                        locked_until: row.get(8)?,
                    })
                })
            };

            match result {
                Ok(entry) => {
                    tx.execute(
                        "UPDATE queue SET status = 'processing',
                         locked_until = strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '+5 minutes'),
                         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                         WHERE id = ?1",
                        params![entry.id],
                    )?;
                    tx.commit()?;

                    // Return the entry with updated status.
                    Ok(Some(QueueEntry {
                        status: "processing".to_string(),
                        ..entry
                    }))
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    tx.commit()?;
                    Ok(None)
                }
                Err(e) => Err(e),
            }
        })
        .await
        .map_err(crate::database::map_tr_err)
}

/// Acknowledge successful processing of a queue entry.
///
/// Marks the entry as "completed".
pub async fn ack(db: &Database, id: i64) -> Result<(), BlufioError> {
    db.connection()
        .call(move |conn| {
            conn.execute(
                "UPDATE queue SET status = 'completed',
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                params![id],
            )?;
            Ok(())
        })
        .await
        .map_err(crate::database::map_tr_err)
}

/// Mark a queue entry as failed.
///
/// Increments attempts. If attempts >= max_attempts, sets status to "failed".
/// Otherwise resets to "pending" for retry and clears the lock.
pub async fn fail(db: &Database, id: i64) -> Result<(), BlufioError> {
    db.connection()
        .call(move |conn| {
            // First get current attempts and max_attempts.
            let (attempts, max_attempts): (i32, i32) = conn.query_row(
                "SELECT attempts, max_attempts FROM queue WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;

            let new_attempts = attempts + 1;
            if new_attempts >= max_attempts {
                conn.execute(
                    "UPDATE queue SET status = 'failed', attempts = ?1,
                     locked_until = NULL,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                     WHERE id = ?2",
                    params![new_attempts, id],
                )?;
            } else {
                conn.execute(
                    "UPDATE queue SET status = 'pending', attempts = ?1,
                     locked_until = NULL,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                     WHERE id = ?2",
                    params![new_attempts, id],
                )?;
            }
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

    #[tokio::test]
    async fn enqueue_and_dequeue_lifecycle() {
        let (db, _dir) = setup_db().await;

        let id = enqueue(&db, "inbound", r#"{"msg":"hello"}"#).await.unwrap();
        assert!(id > 0);

        let entry = dequeue(&db, "inbound").await.unwrap();
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.id, id);
        assert_eq!(entry.status, "processing");
        assert_eq!(entry.queue_name, "inbound");
        assert_eq!(entry.payload, r#"{"msg":"hello"}"#);

        // Queue should be empty now (no more pending).
        let next = dequeue(&db, "inbound").await.unwrap();
        assert!(next.is_none());

        db.close().await.unwrap();
    }

    #[tokio::test]
    async fn ack_marks_completed() {
        let (db, _dir) = setup_db().await;

        let id = enqueue(&db, "test", "payload").await.unwrap();
        let _entry = dequeue(&db, "test").await.unwrap().unwrap();

        ack(&db, id).await.unwrap();

        // Verify status is completed.
        let status: String = db
            .connection()
            .call(move |conn| -> Result<String, rusqlite::Error> {
                conn.query_row(
                    "SELECT status FROM queue WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
            })
            .await
            .unwrap();
        assert_eq!(status, "completed");

        db.close().await.unwrap();
    }

    #[tokio::test]
    async fn fail_increments_attempts_and_retries() {
        let (db, _dir) = setup_db().await;

        let id = enqueue(&db, "test", "payload").await.unwrap();
        let _entry = dequeue(&db, "test").await.unwrap().unwrap();

        // Default max_attempts is 3. First fail: attempts=1, back to pending.
        fail(&db, id).await.unwrap();

        let (status, attempts): (String, i32) = db
            .connection()
            .call(move |conn| -> Result<(String, i32), rusqlite::Error> {
                conn.query_row(
                    "SELECT status, attempts FROM queue WHERE id = ?1",
                    params![id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
            })
            .await
            .unwrap();
        assert_eq!(status, "pending");
        assert_eq!(attempts, 1);

        db.close().await.unwrap();
    }

    #[tokio::test]
    async fn fail_marks_permanently_failed_at_max_attempts() {
        let (db, _dir) = setup_db().await;

        let id = enqueue(&db, "test", "payload").await.unwrap();

        // Fail 3 times (max_attempts = 3).
        for _ in 0..3 {
            let _entry = dequeue(&db, "test").await.unwrap().unwrap();
            fail(&db, id).await.unwrap();
        }

        let status: String = db
            .connection()
            .call(move |conn| -> Result<String, rusqlite::Error> {
                conn.query_row(
                    "SELECT status FROM queue WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
            })
            .await
            .unwrap();
        assert_eq!(status, "failed");

        db.close().await.unwrap();
    }

    #[tokio::test]
    async fn dequeue_empty_queue_returns_none() {
        let (db, _dir) = setup_db().await;
        let result = dequeue(&db, "nonexistent").await.unwrap();
        assert!(result.is_none());
        db.close().await.unwrap();
    }

    #[tokio::test]
    async fn concurrent_writers_no_sqlite_busy() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("concurrent_test.db");
        let db = Database::open(db_path.to_str().unwrap()).await.unwrap();

        // Spawn 10 concurrent tasks all writing through the same Database.
        let mut handles = Vec::new();
        for i in 0..10 {
            let conn = db.connection().clone();
            let handle = tokio::spawn(async move {
                conn.call(move |conn| -> Result<(), rusqlite::Error> {
                    conn.execute(
                        "INSERT INTO queue (queue_name, payload) VALUES (?1, ?2)",
                        params![format!("q-{i}"), format!(r#"{{"n":{i}}}"#)],
                    )?;
                    Ok(())
                })
                .await
            });
            handles.push(handle);
        }

        // All should complete without SQLITE_BUSY.
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok(), "concurrent write failed: {result:?}");
        }

        // Verify all 10 entries are present.
        let count: i64 = db
            .connection()
            .call(|conn| -> Result<i64, rusqlite::Error> {
                conn.query_row("SELECT COUNT(*) FROM queue", [], |row| row.get(0))
            })
            .await
            .unwrap();
        assert_eq!(count, 10);

        db.close().await.unwrap();
    }
}
