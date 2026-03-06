// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! SQLite-backed storage for webhooks and dead letter queue.

use blufio_core::BlufioError;

use super::{CreateWebhookRequest, CreateWebhookResponse, Webhook, WebhookListItem};

/// Storage operations for webhooks.
pub struct WebhookStore {
    conn: tokio_rusqlite::Connection,
}

impl WebhookStore {
    /// Create a new webhook store backed by the given SQLite connection.
    pub fn new(conn: tokio_rusqlite::Connection) -> Self {
        Self { conn }
    }

    /// Register a new webhook, returning the response with the secret (shown once).
    pub async fn create(
        &self,
        req: &CreateWebhookRequest,
    ) -> Result<CreateWebhookResponse, BlufioError> {
        let id = uuid::Uuid::new_v4().to_string();
        let secret = generate_secret();
        let now = chrono::Utc::now().to_rfc3339();
        let events_json = serde_json::to_string(&req.events).unwrap_or_else(|_| "[]".into());

        let id_c = id.clone();
        let url_c = req.url.clone();
        let secret_c = secret.clone();
        let events_c = events_json.clone();
        let now_c = now.clone();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO webhooks (id, url, secret, events, active, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)",
                    rusqlite::params![id_c, url_c, secret_c, events_c, now_c],
                )?;
                Ok(())
            })
            .await
            .map_err(map_err)?;

        Ok(CreateWebhookResponse {
            id,
            url: req.url.clone(),
            secret,
            events: req.events.clone(),
            created_at: now,
        })
    }

    /// List all active webhooks (never exposes secrets).
    pub async fn list(&self) -> Result<Vec<WebhookListItem>, BlufioError> {
        self.conn
            .call(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, url, events, active, created_at
                     FROM webhooks ORDER BY created_at DESC",
                )?;
                let items = stmt
                    .query_map([], |row| {
                        let events_json: String = row.get(2)?;
                        let events: Vec<String> =
                            serde_json::from_str(&events_json).unwrap_or_default();
                        Ok(WebhookListItem {
                            id: row.get(0)?,
                            url: row.get(1)?,
                            events,
                            active: row.get::<_, i64>(3)? == 1,
                            created_at: row.get(4)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(items)
            })
            .await
            .map_err(map_err)
    }

    /// Get a webhook by ID with its secret (internal use for delivery engine).
    pub async fn get(&self, id: &str) -> Result<Option<Webhook>, BlufioError> {
        let id = id.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, url, secret, events, active, created_at, updated_at
                     FROM webhooks WHERE id = ?1",
                )?;
                let webhook = stmt
                    .query_row(rusqlite::params![id], |row| {
                        let events_json: String = row.get(3)?;
                        let events: Vec<String> =
                            serde_json::from_str(&events_json).unwrap_or_default();
                        Ok(Webhook {
                            id: row.get(0)?,
                            url: row.get(1)?,
                            secret: row.get(2)?,
                            events,
                            active: row.get::<_, i64>(4)? == 1,
                            created_at: row.get(5)?,
                            updated_at: row.get(6)?,
                        })
                    })
                    .optional()?;
                Ok(webhook)
            })
            .await
            .map_err(map_err)
    }

    /// Delete a webhook by ID.
    pub async fn delete(&self, id: &str) -> Result<(), BlufioError> {
        let id = id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute("DELETE FROM webhooks WHERE id = ?1", rusqlite::params![id])?;
                Ok(())
            })
            .await
            .map_err(map_err)
    }

    /// List all active webhooks whose event filter includes the given event type.
    pub async fn list_for_event(&self, event_type: &str) -> Result<Vec<Webhook>, BlufioError> {
        let event_type = event_type.to_string();
        self.conn
            .call(move |conn| {
                // Use LIKE for JSON array containment check.
                // The events column stores a JSON array like ["chat.completed","tool.invoked"].
                let pattern = format!("%\"{event_type}\"%");
                let mut stmt = conn.prepare(
                    "SELECT id, url, secret, events, active, created_at, updated_at
                     FROM webhooks WHERE active = 1 AND events LIKE ?1",
                )?;
                let webhooks = stmt
                    .query_map(rusqlite::params![pattern], |row| {
                        let events_json: String = row.get(3)?;
                        let events: Vec<String> =
                            serde_json::from_str(&events_json).unwrap_or_default();
                        Ok(Webhook {
                            id: row.get(0)?,
                            url: row.get(1)?,
                            secret: row.get(2)?,
                            events,
                            active: row.get::<_, i64>(4)? == 1,
                            created_at: row.get(5)?,
                            updated_at: row.get(6)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(webhooks)
            })
            .await
            .map_err(map_err)
    }

    /// Insert a failed delivery into the dead letter queue.
    pub async fn insert_dead_letter(
        &self,
        webhook_id: &str,
        event_type: &str,
        payload: &str,
        error: &str,
        attempt_count: i64,
    ) -> Result<(), BlufioError> {
        let webhook_id = webhook_id.to_string();
        let event_type = event_type.to_string();
        let payload = payload.to_string();
        let error = error.to_string();
        let now = chrono::Utc::now().to_rfc3339();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO webhook_dead_letter (webhook_id, event_type, payload, last_attempt_at, attempt_count, last_error, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?4)",
                    rusqlite::params![webhook_id, event_type, payload, now, attempt_count, error],
                )?;
                Ok(())
            })
            .await
            .map_err(map_err)
    }
}

/// Generate a 32-byte random hex secret for HMAC signing.
fn generate_secret() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn map_err(e: tokio_rusqlite::Error<rusqlite::Error>) -> BlufioError {
    BlufioError::Storage {
        source: Box::new(e),
    }
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

    async fn setup_store() -> WebhookStore {
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
        WebhookStore::new(conn)
    }

    #[tokio::test]
    async fn create_and_get() {
        let store = setup_store().await;
        let req = CreateWebhookRequest {
            url: "https://example.com/hook".into(),
            events: vec!["chat.completed".into()],
        };

        let resp = store.create(&req).await.unwrap();
        assert!(!resp.secret.is_empty());
        assert_eq!(resp.secret.len(), 64); // 32 bytes = 64 hex chars
        assert_eq!(resp.url, "https://example.com/hook");

        let webhook = store.get(&resp.id).await.unwrap().unwrap();
        assert_eq!(webhook.url, "https://example.com/hook");
        assert_eq!(webhook.secret, resp.secret);
        assert!(webhook.active);
    }

    #[tokio::test]
    async fn list_hides_secrets() {
        let store = setup_store().await;
        let req = CreateWebhookRequest {
            url: "https://example.com/hook".into(),
            events: vec!["chat.completed".into()],
        };
        store.create(&req).await.unwrap();

        let items = store.list().await.unwrap();
        assert_eq!(items.len(), 1);
        // WebhookListItem has no secret field -- verified by type system.
        assert_eq!(items[0].url, "https://example.com/hook");
    }

    #[tokio::test]
    async fn delete_webhook() {
        let store = setup_store().await;
        let req = CreateWebhookRequest {
            url: "https://example.com/hook".into(),
            events: vec!["chat.completed".into()],
        };
        let resp = store.create(&req).await.unwrap();
        store.delete(&resp.id).await.unwrap();

        let found = store.get(&resp.id).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn list_for_event_filters_correctly() {
        let store = setup_store().await;

        // Webhook 1: subscribes to chat.completed
        let req1 = CreateWebhookRequest {
            url: "https://example.com/chat".into(),
            events: vec!["chat.completed".into()],
        };
        store.create(&req1).await.unwrap();

        // Webhook 2: subscribes to tool.invoked
        let req2 = CreateWebhookRequest {
            url: "https://example.com/tool".into(),
            events: vec!["tool.invoked".into()],
        };
        store.create(&req2).await.unwrap();

        // Webhook 3: subscribes to both
        let req3 = CreateWebhookRequest {
            url: "https://example.com/both".into(),
            events: vec!["chat.completed".into(), "tool.invoked".into()],
        };
        store.create(&req3).await.unwrap();

        let chat_hooks = store.list_for_event("chat.completed").await.unwrap();
        assert_eq!(chat_hooks.len(), 2); // Webhook 1 and 3

        let tool_hooks = store.list_for_event("tool.invoked").await.unwrap();
        assert_eq!(tool_hooks.len(), 2); // Webhook 2 and 3

        let batch_hooks = store.list_for_event("batch.completed").await.unwrap();
        assert_eq!(batch_hooks.len(), 0);
    }

    #[tokio::test]
    async fn insert_dead_letter() {
        let store = setup_store().await;

        // Create a webhook first (foreign key constraint).
        let req = CreateWebhookRequest {
            url: "https://example.com/hook".into(),
            events: vec!["chat.completed".into()],
        };
        let resp = store.create(&req).await.unwrap();

        store
            .insert_dead_letter(
                &resp.id,
                "chat.completed",
                r#"{"event_type":"chat.completed"}"#,
                "connection refused",
                5,
            )
            .await
            .unwrap();
    }

    #[test]
    fn secret_format() {
        let secret = generate_secret();
        assert_eq!(secret.len(), 64); // 32 bytes = 64 hex chars
        assert!(secret.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn secret_is_random() {
        let s1 = generate_secret();
        let s2 = generate_secret();
        assert_ne!(s1, s2);
    }
}
