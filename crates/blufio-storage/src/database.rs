// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Database connection management with PRAGMA setup, WAL mode, and lifecycle.
//!
//! All writes are serialized through tokio-rusqlite's single background thread.
//! Do NOT create additional Connection instances for writes.

use blufio_core::BlufioError;
use tracing::{debug, info};

/// Convert a tokio-rusqlite error (wrapping rusqlite::Error) into BlufioError::Storage.
fn map_tokio_rusqlite_err(e: tokio_rusqlite::Error<rusqlite::Error>) -> BlufioError {
    BlufioError::Storage {
        source: Box::new(e),
    }
}

/// The main database handle wrapping a tokio-rusqlite connection.
///
/// `Database` enforces the single-writer pattern: all reads and writes go
/// through the single background thread managed by `tokio_rusqlite::Connection`.
/// This eliminates SQLITE_BUSY errors under concurrent access.
pub struct Database {
    conn: tokio_rusqlite::Connection,
}

impl Database {
    /// Open (or create) a SQLite database at the given path.
    ///
    /// This function:
    /// 1. Creates parent directories if they don't exist.
    /// 2. Opens the connection via tokio-rusqlite.
    /// 3. Applies WAL mode and performance PRAGMAs.
    /// 4. Runs embedded migrations.
    pub async fn open(path: &str) -> Result<Self, BlufioError> {
        // Create parent directories if needed.
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| BlufioError::Storage {
                    source: Box::new(e),
                })?;
            }
        }

        info!(path = %path, "opening database");
        let conn = tokio_rusqlite::Connection::open(path)
            .await
            .map_err(|e| BlufioError::Storage {
                source: Box::new(e),
            })?;

        // Apply PRAGMAs on the background thread.
        conn.call(|conn| {
            // WAL mode must be set outside any transaction and before other PRAGMAs.
            conn.execute_batch("PRAGMA journal_mode = WAL;")?;
            conn.execute_batch(
                "PRAGMA synchronous = NORMAL;
                 PRAGMA busy_timeout = 5000;
                 PRAGMA foreign_keys = ON;
                 PRAGMA cache_size = -16000;
                 PRAGMA temp_store = MEMORY;",
            )?;
            debug!("applied database PRAGMAs");
            Ok(())
        })
        .await
        .map_err(map_tokio_rusqlite_err)?;

        // Run migrations on the background thread.
        conn.call(|conn| {
            crate::migrations::run_migrations(conn).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::other(e.to_string())),
                )
            })?;
            debug!("migrations applied");
            Ok(())
        })
        .await
        .map_err(map_tokio_rusqlite_err)?;

        Ok(Self { conn })
    }

    /// Returns a reference to the underlying tokio-rusqlite connection.
    ///
    /// Query modules use this to execute SQL on the background thread via `conn.call()`.
    pub fn connection(&self) -> &tokio_rusqlite::Connection {
        &self.conn
    }

    /// Checkpoint WAL and close the database.
    ///
    /// After this call, the database file is self-contained (no `-wal` file)
    /// and safe for `cp` backup (PERS-04).
    pub async fn close(self) -> Result<(), BlufioError> {
        // Checkpoint WAL to merge it into the main database file.
        self.conn
            .call(|conn| {
                conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
                debug!("WAL checkpoint complete");
                Ok(())
            })
            .await
            .map_err(map_tokio_rusqlite_err)?;

        // Close the connection.
        self.conn.close().await.map_err(|e| BlufioError::Storage {
            source: Box::new(e),
        })?;

        info!("database closed");
        Ok(())
    }
}

/// Helper for converting tokio-rusqlite errors in query modules.
pub(crate) fn map_tr_err(e: tokio_rusqlite::Error<rusqlite::Error>) -> BlufioError {
    map_tokio_rusqlite_err(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn open_creates_file_and_parent_dirs() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("sub").join("dir").join("test.db");
        let db = Database::open(db_path.to_str().unwrap()).await.unwrap();
        assert!(db_path.exists(), "database file should be created");
        db.close().await.unwrap();
    }

    #[tokio::test]
    async fn open_applies_wal_mode() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("wal_test.db");
        let db = Database::open(db_path.to_str().unwrap()).await.unwrap();

        let mode: String = db
            .connection()
            .call(|conn| -> Result<String, rusqlite::Error> {
                let mut stmt = conn.prepare("PRAGMA journal_mode;")?;
                let mode: String = stmt.query_row([], |row| row.get(0))?;
                Ok(mode)
            })
            .await
            .unwrap();
        assert_eq!(mode, "wal");

        db.close().await.unwrap();
    }

    #[tokio::test]
    async fn open_applies_all_pragmas() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("pragma_test.db");
        let db = Database::open(db_path.to_str().unwrap()).await.unwrap();

        let (sync, fk, timeout, cache_size, temp_store): (i64, i64, i64, i64, i64) = db
            .connection()
            .call(|conn| -> Result<(i64, i64, i64, i64, i64), rusqlite::Error> {
                let sync: i64 =
                    conn.query_row("PRAGMA synchronous;", [], |row| row.get(0))?;
                let fk: i64 = conn.query_row("PRAGMA foreign_keys;", [], |row| row.get(0))?;
                let timeout: i64 =
                    conn.query_row("PRAGMA busy_timeout;", [], |row| row.get(0))?;
                let cache: i64 =
                    conn.query_row("PRAGMA cache_size;", [], |row| row.get(0))?;
                let temp: i64 =
                    conn.query_row("PRAGMA temp_store;", [], |row| row.get(0))?;
                Ok((sync, fk, timeout, cache, temp))
            })
            .await
            .unwrap();

        // NORMAL synchronous is reported as 1
        assert_eq!(sync, 1);
        assert_eq!(fk, 1);
        assert_eq!(timeout, 5000);
        assert_eq!(cache_size, -16000);
        assert_eq!(temp_store, 2); // MEMORY = 2
        db.close().await.unwrap();
    }

    #[tokio::test]
    async fn migrations_create_all_tables() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("migration_test.db");
        let db = Database::open(db_path.to_str().unwrap()).await.unwrap();

        let tables: Vec<String> = db
            .connection()
            .call(|conn| -> Result<Vec<String>, rusqlite::Error> {
                let mut stmt = conn.prepare(
                    "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name;",
                )?;
                let rows = stmt.query_map([], |row| row.get(0))?;
                let mut names = Vec::new();
                for row in rows {
                    names.push(row?);
                }
                Ok(names)
            })
            .await
            .unwrap();

        assert!(tables.contains(&"sessions".to_string()));
        assert!(tables.contains(&"messages".to_string()));
        assert!(tables.contains(&"queue".to_string()));
        assert!(tables.contains(&"vault_entries".to_string()));
        assert!(tables.contains(&"vault_meta".to_string()));

        db.close().await.unwrap();
    }

    #[tokio::test]
    async fn close_checkpoints_wal() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("checkpoint_test.db");
        let wal_path = dir.path().join("checkpoint_test.db-wal");

        let db = Database::open(db_path.to_str().unwrap()).await.unwrap();

        // Insert some data to ensure WAL has content.
        db.connection()
            .call(|conn| -> Result<(), rusqlite::Error> {
                conn.execute(
                    "INSERT INTO sessions (id, channel) VALUES (?1, ?2)",
                    rusqlite::params!["test-session", "cli"],
                )?;
                Ok(())
            })
            .await
            .unwrap();

        db.close().await.unwrap();

        // After close with TRUNCATE checkpoint, WAL file should not exist
        // or be empty.
        if wal_path.exists() {
            let wal_size = std::fs::metadata(&wal_path).unwrap().len();
            assert_eq!(wal_size, 0, "WAL file should be empty after TRUNCATE checkpoint");
        }
    }
}
