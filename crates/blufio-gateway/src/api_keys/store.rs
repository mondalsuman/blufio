// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! SQLite-backed storage for API keys and rate limit counters.

use blufio_core::BlufioError;
use sha2::{Digest, Sha256};

use super::{ApiKey, CreateKeyRequest, CreateKeyResponse};

/// Storage operations for API keys.
pub struct ApiKeyStore {
    conn: tokio_rusqlite::Connection,
}

impl ApiKeyStore {
    /// Create a new API key store backed by the given SQLite connection.
    pub fn new(conn: tokio_rusqlite::Connection) -> Self {
        Self { conn }
    }

    /// Create a new API key, returning the response with the raw key (shown once).
    pub async fn create(&self, req: &CreateKeyRequest) -> Result<CreateKeyResponse, BlufioError> {
        let id = uuid::Uuid::new_v4().to_string();
        let raw_key = generate_raw_key();
        let key_hash = hash_key(&raw_key);
        let now = chrono::Utc::now().to_rfc3339();
        let rate_limit = req.rate_limit.unwrap_or(60);
        let expires_at = req
            .expires_in_hours
            .map(|hours| (chrono::Utc::now() + chrono::Duration::hours(hours)).to_rfc3339());
        let scopes_json = serde_json::to_string(&req.scopes).unwrap_or_else(|_| "[]".into());

        let id_c = id.clone();
        let hash_c = key_hash.clone();
        let name_c = req.name.clone();
        let scopes_c = scopes_json.clone();
        let now_c = now.clone();
        let expires_c = expires_at.clone();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO api_keys (id, key_hash, name, scopes, rate_limit, created_at, expires_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![id_c, hash_c, name_c, scopes_c, rate_limit, now_c, expires_c],
                )?;
                Ok(())
            })
            .await
            .map_err(map_err)?;

        Ok(CreateKeyResponse {
            id,
            name: req.name.clone(),
            key: raw_key,
            scopes: req.scopes.clone(),
            rate_limit,
            created_at: now,
            expires_at,
        })
    }

    /// Look up an API key by its SHA-256 hash.
    pub async fn lookup(&self, key_hash: &str) -> Result<Option<ApiKey>, BlufioError> {
        let hash = key_hash.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, key_hash, name, scopes, rate_limit, created_at, expires_at, revoked_at
                     FROM api_keys WHERE key_hash = ?1",
                )?;
                let key = stmt
                    .query_row(rusqlite::params![hash], |row| {
                        let scopes_json: String = row.get(3)?;
                        let scopes: Vec<String> =
                            serde_json::from_str(&scopes_json).unwrap_or_default();
                        Ok(ApiKey {
                            id: row.get(0)?,
                            key_hash: row.get(1)?,
                            name: row.get(2)?,
                            scopes,
                            rate_limit: row.get(4)?,
                            created_at: row.get(5)?,
                            expires_at: row.get(6)?,
                            revoked_at: row.get(7)?,
                        })
                    })
                    .optional()?;
                Ok(key)
            })
            .await
            .map_err(map_err)
    }

    /// List all API keys (never returns key hashes).
    pub async fn list(&self) -> Result<Vec<ApiKey>, BlufioError> {
        self.conn
            .call(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, name, scopes, rate_limit, created_at, expires_at, revoked_at
                     FROM api_keys ORDER BY created_at DESC",
                )?;
                let keys = stmt
                    .query_map([], |row| {
                        let scopes_json: String = row.get(2)?;
                        let scopes: Vec<String> =
                            serde_json::from_str(&scopes_json).unwrap_or_default();
                        Ok(ApiKey {
                            id: row.get(0)?,
                            key_hash: String::new(), // Never expose hash
                            name: row.get(1)?,
                            scopes,
                            rate_limit: row.get(3)?,
                            created_at: row.get(4)?,
                            expires_at: row.get(5)?,
                            revoked_at: row.get(6)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(keys)
            })
            .await
            .map_err(map_err)
    }

    /// Revoke an API key by setting its revoked_at timestamp.
    pub async fn revoke(&self, id: &str) -> Result<(), BlufioError> {
        let id = id.to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE api_keys SET revoked_at = ?1 WHERE id = ?2",
                    rusqlite::params![now, id],
                )?;
                Ok(())
            })
            .await
            .map_err(map_err)
    }

    /// Delete an API key permanently.
    pub async fn delete(&self, id: &str) -> Result<(), BlufioError> {
        let id = id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute("DELETE FROM api_keys WHERE id = ?1", rusqlite::params![id])?;
                Ok(())
            })
            .await
            .map_err(map_err)
    }

    /// Atomically increment the rate limit counter for a key in a time window.
    ///
    /// Returns the new count after increment.
    pub async fn increment_rate_count(
        &self,
        key_id: &str,
        window_start: &str,
    ) -> Result<i64, BlufioError> {
        let key_id = key_id.to_string();
        let window = window_start.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO rate_limit_counters (key_id, window_start, count)
                     VALUES (?1, ?2, 1)
                     ON CONFLICT(key_id, window_start) DO UPDATE SET count = count + 1",
                    rusqlite::params![key_id, window],
                )?;
                let count: i64 = conn.query_row(
                    "SELECT count FROM rate_limit_counters WHERE key_id = ?1 AND window_start = ?2",
                    rusqlite::params![key_id, window],
                    |row| row.get(0),
                )?;
                Ok(count)
            })
            .await
            .map_err(map_err)
    }

    /// Clean up rate limit counter entries older than the given timestamp.
    pub async fn cleanup_old_windows(&self, before: &str) -> Result<(), BlufioError> {
        let before = before.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM rate_limit_counters WHERE window_start < ?1",
                    rusqlite::params![before],
                )?;
                Ok(())
            })
            .await
            .map_err(map_err)
    }
}

/// Generate a raw API key with the `blf_sk_` prefix.
fn generate_raw_key() -> String {
    use rand::RngCore;
    let mut random_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut random_bytes);
    format!("blf_sk_{}", hex::encode(random_bytes))
}

/// Compute SHA-256 hash of a raw API key, returned as hex string.
pub fn hash_key(raw_key: &str) -> String {
    hex::encode(Sha256::digest(raw_key.as_bytes()))
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

    async fn setup_store() -> ApiKeyStore {
        let conn = tokio_rusqlite::Connection::open_in_memory().await.unwrap();
        // Apply migrations manually for testing.
        conn.call(|conn| {
            conn.execute_batch("PRAGMA foreign_keys = ON;")?;
            conn.execute_batch(include_str!(
                "../../../blufio-storage/migrations/V7__api_keys_webhooks_batch.sql"
            ))?;
            Ok::<(), rusqlite::Error>(())
        })
        .await
        .unwrap();
        ApiKeyStore::new(conn)
    }

    #[tokio::test]
    async fn create_and_lookup() {
        let store = setup_store().await;
        let req = CreateKeyRequest {
            name: "test-key".into(),
            scopes: vec!["chat.completions".into()],
            rate_limit: Some(100),
            expires_in_hours: None,
        };

        let resp = store.create(&req).await.unwrap();
        assert!(resp.key.starts_with("blf_sk_"));
        assert_eq!(resp.key.len(), 7 + 64); // "blf_sk_" + 64 hex chars
        assert_eq!(resp.name, "test-key");
        assert_eq!(resp.rate_limit, 100);

        // Lookup by hash.
        let hash = hash_key(&resp.key);
        let found = store.lookup(&hash).await.unwrap();
        assert!(found.is_some());
        let key = found.unwrap();
        assert_eq!(key.id, resp.id);
        assert_eq!(key.name, "test-key");
        assert!(key.is_valid());
    }

    #[tokio::test]
    async fn lookup_unknown_hash() {
        let store = setup_store().await;
        let found = store.lookup("nonexistent-hash").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn revoke_key() {
        let store = setup_store().await;
        let req = CreateKeyRequest {
            name: "revoke-me".into(),
            scopes: vec!["admin".into()],
            rate_limit: None,
            expires_in_hours: None,
        };

        let resp = store.create(&req).await.unwrap();
        store.revoke(&resp.id).await.unwrap();

        let hash = hash_key(&resp.key);
        let found = store.lookup(&hash).await.unwrap().unwrap();
        assert!(!found.is_valid());
        assert!(found.revoked_at.is_some());
    }

    #[tokio::test]
    async fn list_keys() {
        let store = setup_store().await;
        let req1 = CreateKeyRequest {
            name: "key-1".into(),
            scopes: vec!["chat.completions".into()],
            rate_limit: None,
            expires_in_hours: None,
        };
        let req2 = CreateKeyRequest {
            name: "key-2".into(),
            scopes: vec!["admin".into()],
            rate_limit: Some(200),
            expires_in_hours: None,
        };

        store.create(&req1).await.unwrap();
        store.create(&req2).await.unwrap();

        let keys = store.list().await.unwrap();
        assert_eq!(keys.len(), 2);
        // Hashes should be empty (not exposed).
        assert!(keys.iter().all(|k| k.key_hash.is_empty()));
    }

    #[tokio::test]
    async fn delete_key() {
        let store = setup_store().await;
        let req = CreateKeyRequest {
            name: "delete-me".into(),
            scopes: vec![],
            rate_limit: None,
            expires_in_hours: None,
        };

        let resp = store.create(&req).await.unwrap();
        store.delete(&resp.id).await.unwrap();

        let hash = hash_key(&resp.key);
        let found = store.lookup(&hash).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn rate_limit_counter_atomic() {
        let store = setup_store().await;
        let key_id = "test-key-id";
        let window = "2026-03-06T12:00:00";

        let count1 = store.increment_rate_count(key_id, window).await.unwrap();
        assert_eq!(count1, 1);

        let count2 = store.increment_rate_count(key_id, window).await.unwrap();
        assert_eq!(count2, 2);

        let count3 = store.increment_rate_count(key_id, window).await.unwrap();
        assert_eq!(count3, 3);
    }

    #[tokio::test]
    async fn rate_limit_separate_windows() {
        let store = setup_store().await;
        let key_id = "test-key-id";

        let count_a = store
            .increment_rate_count(key_id, "2026-03-06T12:00:00")
            .await
            .unwrap();
        assert_eq!(count_a, 1);

        let count_b = store
            .increment_rate_count(key_id, "2026-03-06T12:01:00")
            .await
            .unwrap();
        assert_eq!(count_b, 1); // Different window.
    }

    #[tokio::test]
    async fn cleanup_old_windows() {
        let store = setup_store().await;
        let key_id = "test-key-id";

        store
            .increment_rate_count(key_id, "2026-03-06T11:00:00")
            .await
            .unwrap();
        store
            .increment_rate_count(key_id, "2026-03-06T12:00:00")
            .await
            .unwrap();

        store
            .cleanup_old_windows("2026-03-06T11:30:00")
            .await
            .unwrap();

        // Old window should be cleaned up, new one remains.
        let count = store
            .increment_rate_count(key_id, "2026-03-06T11:00:00")
            .await
            .unwrap();
        assert_eq!(count, 1); // Was cleaned up, so starts fresh.

        let count = store
            .increment_rate_count(key_id, "2026-03-06T12:00:00")
            .await
            .unwrap();
        assert_eq!(count, 2); // Was not cleaned up.
    }

    #[test]
    fn key_format() {
        let key = generate_raw_key();
        assert!(key.starts_with("blf_sk_"));
        assert_eq!(key.len(), 7 + 64);
    }

    #[test]
    fn hash_key_deterministic() {
        let key = "blf_sk_test123";
        let hash1 = hash_key(key);
        let hash2 = hash_key(key);
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // SHA-256 = 32 bytes = 64 hex chars
    }
}
