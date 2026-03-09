// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! SQLite-backed storage for batches and batch items.

use blufio_core::BlufioError;

use super::{BatchItemResult, BatchResponse};

/// Storage operations for batch processing.
pub struct BatchStore {
    conn: tokio_rusqlite::Connection,
}

impl BatchStore {
    /// Create a new batch store backed by the given SQLite connection.
    pub fn new(conn: tokio_rusqlite::Connection) -> Self {
        Self { conn }
    }

    /// Create a new batch with its items, returning the batch ID.
    pub async fn create_batch(
        &self,
        items: &[serde_json::Value],
        api_key_id: Option<&str>,
    ) -> Result<String, BlufioError> {
        let batch_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let total_items = items.len();
        let api_key_id = api_key_id.map(|s| s.to_string());

        // Serialize each item for storage.
        let serialized_items: Vec<String> = items
            .iter()
            .map(|item| serde_json::to_string(item).unwrap_or_else(|_| "{}".into()))
            .collect();

        let batch_id_c = batch_id.clone();
        let now_c = now.clone();

        self.conn
            .call(move |conn| {
                let tx = conn.transaction()?;

                tx.execute(
                    "INSERT INTO batches (id, status, total_items, created_at, api_key_id)
                     VALUES (?1, 'processing', ?2, ?3, ?4)",
                    rusqlite::params![batch_id_c, total_items, now_c, api_key_id],
                )?;

                for (index, request_json) in serialized_items.iter().enumerate() {
                    tx.execute(
                        "INSERT INTO batch_items (batch_id, item_index, status, request, created_at)
                         VALUES (?1, ?2, 'pending', ?3, ?4)",
                        rusqlite::params![batch_id_c, index, request_json, now_c],
                    )?;
                }

                tx.commit()?;
                Ok(())
            })
            .await
            .map_err(map_err)?;

        Ok(batch_id)
    }

    /// Get batch status with all item results.
    pub async fn get_batch(&self, batch_id: &str) -> Result<Option<BatchResponse>, BlufioError> {
        let batch_id = batch_id.to_string();
        self.conn
            .call(move |conn| {
                // Get the batch record.
                let mut stmt = conn.prepare(
                    "SELECT id, status, total_items, completed_items, failed_items, created_at, completed_at, api_key_id
                     FROM batches WHERE id = ?1",
                )?;
                let batch = stmt
                    .query_row(rusqlite::params![batch_id], |row| {
                        Ok(BatchResponse {
                            id: row.get(0)?,
                            status: row.get(1)?,
                            total_items: row.get::<_, i64>(2)? as usize,
                            completed_items: row.get::<_, i64>(3)? as usize,
                            failed_items: row.get::<_, i64>(4)? as usize,
                            created_at: row.get(5)?,
                            completed_at: row.get(6)?,
                            api_key_id: row.get(7)?,
                            items: None, // Populated below
                        })
                    })
                    .optional()?;

                let Some(mut batch) = batch else {
                    return Ok(None);
                };

                // Get item results if batch is complete.
                if batch.status == "completed" || batch.status == "failed" {
                    let mut item_stmt = conn.prepare(
                        "SELECT item_index, status, response, completed_at
                         FROM batch_items WHERE batch_id = ?1 ORDER BY item_index",
                    )?;
                    let items = item_stmt
                        .query_map(rusqlite::params![batch_id], |row| {
                            let response_json: Option<String> = row.get(2)?;
                            let response = response_json.and_then(|s| serde_json::from_str(&s).ok());
                            let status: String = row.get(1)?;
                            let error = if status == "failed" {
                                response_json_to_error(row.get::<_, Option<String>>(2)?)
                            } else {
                                None
                            };
                            Ok(BatchItemResult {
                                index: row.get::<_, i64>(0)? as usize,
                                status,
                                response,
                                error,
                            })
                        })?
                        .collect::<Result<Vec<_>, _>>()?;
                    batch.items = Some(items);
                }

                Ok(Some(batch))
            })
            .await
            .map_err(map_err)
    }

    /// Update a batch item's status and result.
    pub async fn update_item(
        &self,
        batch_id: &str,
        item_index: usize,
        status: &str,
        response: Option<&str>,
        error: Option<&str>,
    ) -> Result<(), BlufioError> {
        let batch_id = batch_id.to_string();
        let status = status.to_string();
        let now = chrono::Utc::now().to_rfc3339();

        // Store either the response or the error as the response column.
        let response_data = if let Some(resp) = response {
            Some(resp.to_string())
        } else {
            error.map(|err| serde_json::json!({"error": err}).to_string())
        };

        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE batch_items SET status = ?1, response = ?2, completed_at = ?3
                     WHERE batch_id = ?4 AND item_index = ?5",
                    rusqlite::params![status, response_data, now, batch_id, item_index],
                )?;
                Ok(())
            })
            .await
            .map_err(map_err)
    }

    /// Finalize a batch by counting results and updating status.
    ///
    /// Returns `(success_count, error_count)`.
    pub async fn finalize_batch(&self, batch_id: &str) -> Result<(usize, usize), BlufioError> {
        let batch_id = batch_id.to_string();
        let now = chrono::Utc::now().to_rfc3339();

        self.conn
            .call(move |conn| {
                let completed: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM batch_items WHERE batch_id = ?1 AND status = 'completed'",
                    rusqlite::params![batch_id],
                    |row| row.get(0),
                )?;
                let failed: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM batch_items WHERE batch_id = ?1 AND status = 'failed'",
                    rusqlite::params![batch_id],
                    |row| row.get(0),
                )?;

                let status = if failed > 0 && completed == 0 {
                    "failed"
                } else {
                    "completed"
                };

                conn.execute(
                    "UPDATE batches SET status = ?1, completed_items = ?2, failed_items = ?3, completed_at = ?4
                     WHERE id = ?5",
                    rusqlite::params![status, completed, failed, now, batch_id],
                )?;

                Ok((completed as usize, failed as usize))
            })
            .await
            .map_err(map_err)
    }

    /// Get the request JSON for a specific batch item.
    pub async fn get_item_request(
        &self,
        batch_id: &str,
        item_index: usize,
    ) -> Result<Option<String>, BlufioError> {
        let batch_id = batch_id.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT request FROM batch_items WHERE batch_id = ?1 AND item_index = ?2",
                )?;
                let request = stmt
                    .query_row(rusqlite::params![batch_id, item_index], |row| row.get(0))
                    .optional()?;
                Ok(request)
            })
            .await
            .map_err(map_err)
    }
}

/// Extract error message from a response JSON string for failed items.
fn response_json_to_error(response: Option<String>) -> Option<String> {
    response.and_then(|s| {
        serde_json::from_str::<serde_json::Value>(&s)
            .ok()
            .and_then(|v| {
                v.get("error")
                    .and_then(|e| e.as_str())
                    .map(|s| s.to_string())
            })
    })
}

fn map_err(e: tokio_rusqlite::Error<rusqlite::Error>) -> BlufioError {
    BlufioError::storage_connection_failed(e)
}

/// Extension trait for rusqlite to add `.optional()` to query results.
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_store() -> BatchStore {
        let conn = tokio_rusqlite::Connection::open_in_memory().await.unwrap();
        conn.call(|conn| {
            conn.execute_batch("PRAGMA foreign_keys = ON;")?;
            conn.execute_batch(include_str!(
                "../../../blufio-storage/migrations/V7__api_keys_webhooks_batch.sql"
            ))?;
            Ok::<(), rusqlite::Error>(())
        })
        .await
        .unwrap();
        BatchStore::new(conn)
    }

    #[tokio::test]
    async fn create_and_get_batch() {
        let store = setup_store().await;
        let items = vec![
            serde_json::json!({"model": "gpt-4o", "messages": [{"role": "user", "content": "hello"}]}),
            serde_json::json!({"model": "gpt-4o", "messages": [{"role": "user", "content": "world"}]}),
        ];

        let batch_id = store.create_batch(&items, None).await.unwrap();
        let batch = store.get_batch(&batch_id).await.unwrap().unwrap();

        assert_eq!(batch.status, "processing");
        assert_eq!(batch.total_items, 2);
        assert_eq!(batch.completed_items, 0);
        assert_eq!(batch.failed_items, 0);
        assert!(batch.items.is_none()); // Not populated while processing
    }

    #[tokio::test]
    async fn update_item_and_finalize() {
        let store = setup_store().await;
        let items = vec![
            serde_json::json!({"model": "gpt-4o"}),
            serde_json::json!({"model": "gpt-4o"}),
        ];

        let batch_id = store.create_batch(&items, Some("key-1")).await.unwrap();

        // Update items.
        store
            .update_item(&batch_id, 0, "completed", Some(r#"{"id":"cmpl-1"}"#), None)
            .await
            .unwrap();
        store
            .update_item(&batch_id, 1, "failed", None, Some("model not found"))
            .await
            .unwrap();

        // Finalize.
        let (success, failed) = store.finalize_batch(&batch_id).await.unwrap();
        assert_eq!(success, 1);
        assert_eq!(failed, 1);

        // Get batch with results.
        let batch = store.get_batch(&batch_id).await.unwrap().unwrap();
        assert_eq!(batch.status, "completed"); // At least one succeeded
        assert_eq!(batch.completed_items, 1);
        assert_eq!(batch.failed_items, 1);
        assert!(batch.items.is_some());
        assert_eq!(batch.api_key_id, Some("key-1".to_string()));

        let items = batch.items.unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].status, "completed");
        assert_eq!(items[1].status, "failed");
    }

    #[tokio::test]
    async fn all_failed_batch_status() {
        let store = setup_store().await;
        let items = vec![serde_json::json!({"model": "bad-model"})];

        let batch_id = store.create_batch(&items, None).await.unwrap();

        store
            .update_item(&batch_id, 0, "failed", None, Some("model not found"))
            .await
            .unwrap();

        let (success, failed) = store.finalize_batch(&batch_id).await.unwrap();
        assert_eq!(success, 0);
        assert_eq!(failed, 1);

        let batch = store.get_batch(&batch_id).await.unwrap().unwrap();
        assert_eq!(batch.status, "failed");
    }

    #[tokio::test]
    async fn get_nonexistent_batch() {
        let store = setup_store().await;
        let batch = store.get_batch("nonexistent").await.unwrap();
        assert!(batch.is_none());
    }

    #[tokio::test]
    async fn get_item_request() {
        let store = setup_store().await;
        let items = vec![serde_json::json!({"model": "gpt-4o", "messages": []})];

        let batch_id = store.create_batch(&items, None).await.unwrap();

        let request = store.get_item_request(&batch_id, 0).await.unwrap();
        assert!(request.is_some());
        let request_json: serde_json::Value = serde_json::from_str(&request.unwrap()).unwrap();
        assert_eq!(request_json["model"], "gpt-4o");
    }
}
