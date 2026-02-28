// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! SQLite implementation of the StorageAdapter trait.

use async_trait::async_trait;
use tokio::sync::OnceCell;
use tracing::debug;

use blufio_config::model::StorageConfig;
use blufio_core::types::{Message, QueueEntry, Session};
use blufio_core::{
    AdapterType, BlufioError, HealthStatus, PluginAdapter, StorageAdapter,
};

use crate::database::Database;
use crate::queries;

/// SQLite-backed storage adapter.
///
/// Wraps a [`Database`] handle and delegates all query operations to the
/// typed query modules. The database is lazily initialized on the first
/// call to [`StorageAdapter::initialize`].
pub struct SqliteStorage {
    config: StorageConfig,
    db: OnceCell<Database>,
}

impl SqliteStorage {
    /// Create a new SqliteStorage with the given configuration.
    ///
    /// The database connection is not opened until [`initialize`] is called.
    pub fn new(config: StorageConfig) -> Self {
        Self {
            config,
            db: OnceCell::new(),
        }
    }

    /// Returns a reference to the underlying Database, or an error if not initialized.
    fn db(&self) -> Result<&Database, BlufioError> {
        self.db.get().ok_or_else(|| BlufioError::Storage {
            source: "storage not initialized -- call initialize() first".into(),
        })
    }
}

#[async_trait]
impl PluginAdapter for SqliteStorage {
    fn name(&self) -> &str {
        "sqlite"
    }

    fn version(&self) -> semver::Version {
        semver::Version::new(0, 1, 0)
    }

    fn adapter_type(&self) -> AdapterType {
        AdapterType::Storage
    }

    async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
        let db = self.db()?;
        db.connection()
            .call(|conn| -> Result<(), rusqlite::Error> {
                conn.execute_batch("SELECT 1;")?;
                Ok(())
            })
            .await
            .map_err(crate::database::map_tr_err)?;
        Ok(HealthStatus::Healthy)
    }

    async fn shutdown(&self) -> Result<(), BlufioError> {
        // Shutdown delegates to close if the DB was initialized.
        if let Some(db) = self.db.get() {
            db.connection()
                .call(|conn| -> Result<(), rusqlite::Error> {
                    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
                    Ok(())
                })
                .await
                .map_err(crate::database::map_tr_err)?;
            debug!("shutdown: WAL checkpoint complete");
        }
        Ok(())
    }
}

#[async_trait]
impl StorageAdapter for SqliteStorage {
    async fn initialize(&self) -> Result<(), BlufioError> {
        let path = self.config.database_path.clone();
        let db = Database::open(&path).await?;
        self.db
            .set(db)
            .map_err(|_| BlufioError::Storage {
                source: "storage already initialized".into(),
            })?;
        debug!(path = %self.config.database_path, "SQLite storage initialized");
        Ok(())
    }

    async fn close(&self) -> Result<(), BlufioError> {
        let db = self.db()?;
        // Checkpoint WAL before close.
        db.connection()
            .call(|conn| -> Result<(), rusqlite::Error> {
                conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
                Ok(())
            })
            .await
            .map_err(crate::database::map_tr_err)?;
        debug!("WAL checkpoint complete");
        Ok(())
    }

    // --- Session operations ---

    async fn create_session(&self, session: &Session) -> Result<(), BlufioError> {
        queries::sessions::create_session(self.db()?, session).await
    }

    async fn get_session(&self, id: &str) -> Result<Option<Session>, BlufioError> {
        queries::sessions::get_session(self.db()?, id).await
    }

    async fn list_sessions(&self, state: Option<&str>) -> Result<Vec<Session>, BlufioError> {
        queries::sessions::list_sessions(self.db()?, state).await
    }

    async fn update_session_state(&self, id: &str, state: &str) -> Result<(), BlufioError> {
        queries::sessions::update_session_state(self.db()?, id, state).await
    }

    // --- Message operations ---

    async fn insert_message(&self, message: &Message) -> Result<(), BlufioError> {
        queries::messages::insert_message(self.db()?, message).await
    }

    async fn get_messages(
        &self,
        session_id: &str,
        limit: Option<i64>,
    ) -> Result<Vec<Message>, BlufioError> {
        queries::messages::get_messages_for_session(self.db()?, session_id, limit).await
    }

    // --- Queue operations ---

    async fn enqueue(&self, queue_name: &str, payload: &str) -> Result<i64, BlufioError> {
        queries::queue::enqueue(self.db()?, queue_name, payload).await
    }

    async fn dequeue(&self, queue_name: &str) -> Result<Option<QueueEntry>, BlufioError> {
        queries::queue::dequeue(self.db()?, queue_name).await
    }

    async fn ack(&self, id: i64) -> Result<(), BlufioError> {
        queries::queue::ack(self.db()?, id).await
    }

    async fn fail(&self, id: i64) -> Result<(), BlufioError> {
        queries::queue::fail(self.db()?, id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_config(path: &str) -> StorageConfig {
        StorageConfig {
            database_path: path.to_string(),
            wal_mode: true,
        }
    }

    #[tokio::test]
    async fn sqlite_storage_implements_plugin_adapter() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let storage = SqliteStorage::new(make_config(db_path.to_str().unwrap()));

        assert_eq!(storage.name(), "sqlite");
        assert_eq!(storage.version(), semver::Version::new(0, 1, 0));
        assert_eq!(storage.adapter_type(), AdapterType::Storage);
    }

    #[tokio::test]
    async fn initialize_opens_database_at_configured_path() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("init_test.db");
        let storage = SqliteStorage::new(make_config(db_path.to_str().unwrap()));

        storage.initialize().await.unwrap();
        assert!(db_path.exists(), "database file should be created");
    }

    #[tokio::test]
    async fn initialize_twice_returns_error() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("double_init.db");
        let storage = SqliteStorage::new(make_config(db_path.to_str().unwrap()));

        storage.initialize().await.unwrap();
        let result = storage.initialize().await;
        assert!(result.is_err(), "second initialize should fail");
    }

    #[tokio::test]
    async fn health_check_returns_healthy_when_initialized() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("health.db");
        let storage = SqliteStorage::new(make_config(db_path.to_str().unwrap()));

        storage.initialize().await.unwrap();
        let status = storage.health_check().await.unwrap();
        assert_eq!(status, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn health_check_fails_when_not_initialized() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("no_init.db");
        let storage = SqliteStorage::new(make_config(db_path.to_str().unwrap()));

        let result = storage.health_check().await;
        assert!(result.is_err(), "health_check should fail before initialize");
    }

    #[tokio::test]
    async fn full_session_lifecycle_through_adapter() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("lifecycle.db");
        let storage = SqliteStorage::new(make_config(db_path.to_str().unwrap()));
        storage.initialize().await.unwrap();

        // Create a session.
        let session = Session {
            id: "sess-adapter-1".to_string(),
            channel: "cli".to_string(),
            user_id: Some("user-1".to_string()),
            state: "active".to_string(),
            metadata: None,
            created_at: "2026-01-01T00:00:00.000Z".to_string(),
            updated_at: "2026-01-01T00:00:00.000Z".to_string(),
        };
        storage.create_session(&session).await.unwrap();

        // Retrieve it.
        let retrieved = storage.get_session("sess-adapter-1").await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, "sess-adapter-1");
        assert_eq!(retrieved.channel, "cli");

        // Insert messages.
        let m1 = Message {
            id: "m1".to_string(),
            session_id: "sess-adapter-1".to_string(),
            role: "user".to_string(),
            content: "hello".to_string(),
            token_count: Some(5),
            metadata: None,
            created_at: "2026-01-01T00:00:01.000Z".to_string(),
        };
        let m2 = Message {
            id: "m2".to_string(),
            session_id: "sess-adapter-1".to_string(),
            role: "assistant".to_string(),
            content: "hi there".to_string(),
            token_count: Some(8),
            metadata: None,
            created_at: "2026-01-01T00:00:02.000Z".to_string(),
        };
        storage.insert_message(&m1).await.unwrap();
        storage.insert_message(&m2).await.unwrap();

        // Get messages.
        let messages = storage.get_messages("sess-adapter-1", None).await.unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].role, "assistant");

        // Update session state.
        storage
            .update_session_state("sess-adapter-1", "closed")
            .await
            .unwrap();
        let updated = storage.get_session("sess-adapter-1").await.unwrap().unwrap();
        assert_eq!(updated.state, "closed");

        // List sessions.
        let all = storage.list_sessions(None).await.unwrap();
        assert_eq!(all.len(), 1);

        storage.close().await.unwrap();
    }

    #[tokio::test]
    async fn queue_operations_through_adapter() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("queue_adapter.db");
        let storage = SqliteStorage::new(make_config(db_path.to_str().unwrap()));
        storage.initialize().await.unwrap();

        let id = storage
            .enqueue("inbound", r#"{"msg":"test"}"#)
            .await
            .unwrap();
        assert!(id > 0);

        let entry = storage.dequeue("inbound").await.unwrap();
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.status, "processing");

        storage.ack(entry.id).await.unwrap();

        storage.close().await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_runs_checkpoint() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("shutdown.db");
        let storage = SqliteStorage::new(make_config(db_path.to_str().unwrap()));
        storage.initialize().await.unwrap();

        // Write some data.
        let session = Session {
            id: "sess-shutdown".to_string(),
            channel: "cli".to_string(),
            user_id: None,
            state: "active".to_string(),
            metadata: None,
            created_at: "2026-01-01T00:00:00.000Z".to_string(),
            updated_at: "2026-01-01T00:00:00.000Z".to_string(),
        };
        storage.create_session(&session).await.unwrap();

        // Shutdown should succeed.
        storage.shutdown().await.unwrap();
    }
}
