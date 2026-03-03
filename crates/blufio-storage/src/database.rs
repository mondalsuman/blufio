// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Database connection management with PRAGMA setup, WAL mode, encryption, and lifecycle.
//!
//! All writes are serialized through tokio-rusqlite's single background thread.
//! Do NOT create additional Connection instances for writes.
//!
//! When `BLUFIO_DB_KEY` is set, SQLCipher encryption is applied transparently.
//! Use [`open_connection`] or [`open_connection_sync`] for all database access --
//! these ensure `PRAGMA key` is always the first statement on every connection.

use blufio_core::BlufioError;
use tracing::{debug, info};

/// Convert a tokio-rusqlite error (wrapping rusqlite::Error) into BlufioError::Storage.
fn map_tokio_rusqlite_err(e: tokio_rusqlite::Error<rusqlite::Error>) -> BlufioError {
    BlufioError::Storage {
        source: Box::new(e),
    }
}

// ---------------------------------------------------------------------------
// Encryption helpers
// ---------------------------------------------------------------------------

/// Apply the SQLCipher encryption key as the first PRAGMA on a connection.
///
/// Auto-detects key format:
/// - 64 hex characters: raw hex key via `PRAGMA key = "x'...'";`
/// - Otherwise: passphrase via `PRAGMA key = '...';`
fn apply_encryption_key(conn: &rusqlite::Connection, key: &str) -> Result<(), rusqlite::Error> {
    let is_hex_key = key.len() == 64 && key.chars().all(|c| c.is_ascii_hexdigit());

    if is_hex_key {
        conn.execute_batch(&format!("PRAGMA key = \"x'{key}'\";"))
    } else {
        let escaped = key.replace('\'', "''");
        conn.execute_batch(&format!("PRAGMA key = '{escaped}';"))
    }
}

/// Verify that the encryption key is correct by querying `sqlite_master`.
///
/// A wrong key produces "file is encrypted or is not a database" on the first
/// real query, so we test immediately after `PRAGMA key`.
fn verify_key(conn: &rusqlite::Connection) -> Result<(), BlufioError> {
    conn.query_row("SELECT count(*) FROM sqlite_master;", [], |_| Ok(()))
        .map_err(|_| BlufioError::Storage {
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Cannot open database: file is encrypted or not a database. \
                 Verify BLUFIO_DB_KEY is correct.",
            )),
        })
}

/// Check whether a file begins with the standard SQLite header (`SQLite format 3\0`).
///
/// Returns `true` for plaintext SQLite files **and** for files that are too small
/// to contain a header (including empty/zero-byte files created by opening a
/// connection without writing). Returns `false` only for files that have at
/// least 16 bytes but do **not** match the plaintext SQLite header -- these are
/// assumed to be encrypted.
pub fn is_plaintext_sqlite(path: &std::path::Path) -> std::io::Result<bool> {
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let mut header = [0u8; 16];
    let bytes_read = file.read(&mut header)?;
    if bytes_read < 16 {
        // File is too small to have any header -- treat as empty/new (plaintext).
        return Ok(true);
    }
    Ok(&header == b"SQLite format 3\0")
}

// ---------------------------------------------------------------------------
// Connection factories
// ---------------------------------------------------------------------------

/// Ensure parent directories exist for a database path.
fn ensure_parent_dirs(path: &str) -> Result<(), BlufioError> {
    if let Some(parent) = std::path::Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| BlufioError::Storage {
            source: Box::new(e),
        })?;
    }
    Ok(())
}

/// Open an async tokio-rusqlite connection with optional SQLCipher encryption.
///
/// When `BLUFIO_DB_KEY` is set, `PRAGMA key` is applied as the **first** statement
/// and verified immediately. When the env var is absent, the connection opens in
/// plain-text mode -- but if the file already exists and is encrypted, a hard
/// error is returned.
///
/// All production code should use this function instead of calling
/// `tokio_rusqlite::Connection::open()` directly.
pub async fn open_connection(path: &str) -> Result<tokio_rusqlite::Connection, BlufioError> {
    ensure_parent_dirs(path)?;

    let key = std::env::var("BLUFIO_DB_KEY").ok();
    let file_path = std::path::Path::new(path);

    // Pre-flight: detect encrypted file without a key.
    if key.is_none() && file_path.exists() {
        let is_plain = is_plaintext_sqlite(file_path).unwrap_or(true);
        if !is_plain {
            return Err(BlufioError::Storage {
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Database is encrypted but BLUFIO_DB_KEY is not set",
                )),
            });
        }
    }

    let conn = tokio_rusqlite::Connection::open(path)
        .await
        .map_err(|e| BlufioError::Storage {
            source: Box::new(e),
        })?;

    if let Some(key) = key {
        conn.call(move |conn| {
            apply_encryption_key(conn, &key)?;
            Ok(())
        })
        .await
        .map_err(map_tokio_rusqlite_err)?;

        // Verify key correctness (must be a separate call because verify_key
        // returns BlufioError, not rusqlite::Error).
        let verify_result = conn
            .call(|conn| conn.query_row("SELECT count(*) FROM sqlite_master;", [], |_| Ok(())))
            .await;

        if verify_result.is_err() {
            return Err(BlufioError::Storage {
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Cannot open database: file is encrypted or not a database. \
                     Verify BLUFIO_DB_KEY is correct.",
                )),
            });
        }
    }

    Ok(conn)
}

/// Open a synchronous rusqlite connection with optional SQLCipher encryption.
///
/// Same semantics as [`open_connection`] but returns a sync `rusqlite::Connection`.
/// Used by `backup.rs` which requires the sync Backup API.
pub fn open_connection_sync(
    path: &str,
    flags: rusqlite::OpenFlags,
) -> Result<rusqlite::Connection, BlufioError> {
    ensure_parent_dirs(path)?;

    let key = std::env::var("BLUFIO_DB_KEY").ok();
    let file_path = std::path::Path::new(path);

    // Pre-flight: detect encrypted file without a key.
    if key.is_none() && file_path.exists() {
        let is_plain = is_plaintext_sqlite(file_path).unwrap_or(true);
        if !is_plain {
            return Err(BlufioError::Storage {
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Database is encrypted but BLUFIO_DB_KEY is not set",
                )),
            });
        }
    }

    let conn =
        rusqlite::Connection::open_with_flags(path, flags).map_err(|e| BlufioError::Storage {
            source: Box::new(e),
        })?;

    if let Some(key) = key {
        apply_encryption_key(&conn, &key).map_err(|e| BlufioError::Storage {
            source: Box::new(e),
        })?;
        verify_key(&conn)?;
    }

    Ok(conn)
}

// ---------------------------------------------------------------------------
// Database struct
// ---------------------------------------------------------------------------

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
    /// 1. Opens the connection via the centralized factory (handles encryption).
    /// 2. Applies WAL mode and performance PRAGMAs.
    /// 3. Runs embedded migrations.
    pub async fn open(path: &str) -> Result<Self, BlufioError> {
        info!(path = %path, "opening database");
        let conn = open_connection(path).await?;

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
    use serial_test::serial;
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
            .call(
                |conn| -> Result<(i64, i64, i64, i64, i64), rusqlite::Error> {
                    let sync: i64 = conn.query_row("PRAGMA synchronous;", [], |row| row.get(0))?;
                    let fk: i64 = conn.query_row("PRAGMA foreign_keys;", [], |row| row.get(0))?;
                    let timeout: i64 =
                        conn.query_row("PRAGMA busy_timeout;", [], |row| row.get(0))?;
                    let cache: i64 = conn.query_row("PRAGMA cache_size;", [], |row| row.get(0))?;
                    let temp: i64 = conn.query_row("PRAGMA temp_store;", [], |row| row.get(0))?;
                    Ok((sync, fk, timeout, cache, temp))
                },
            )
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
                let mut stmt = conn
                    .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name;")?;
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
            assert_eq!(
                wal_size, 0,
                "WAL file should be empty after TRUNCATE checkpoint"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Connection factory tests
    // -----------------------------------------------------------------------

    /// Safety: env var mutations are guarded by #[serial] -- only one test
    /// touches env vars at a time.
    unsafe fn set_key(val: &str) {
        std::env::set_var("BLUFIO_DB_KEY", val);
    }

    /// Safety: see `set_key`.
    unsafe fn remove_key() {
        std::env::remove_var("BLUFIO_DB_KEY");
    }

    #[tokio::test]
    #[serial]
    async fn test_open_connection_no_key() {
        // Ensure no key is set.
        unsafe { remove_key() };

        let dir = tempdir().unwrap();
        let db_path = dir.path().join("plain.db");
        let conn = open_connection(db_path.to_str().unwrap()).await.unwrap();

        // Basic query should work.
        let result: i64 = conn
            .call(|conn| -> Result<i64, rusqlite::Error> {
                conn.execute_batch("CREATE TABLE t(x INTEGER); INSERT INTO t VALUES(42);")?;
                conn.query_row("SELECT x FROM t;", [], |row| row.get(0))
            })
            .await
            .unwrap();
        assert_eq!(result, 42);
    }

    #[tokio::test]
    #[serial]
    async fn test_open_connection_with_key() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("encrypted.db");

        // Set a passphrase key.
        unsafe { set_key("test-passphrase-key") };

        let conn = open_connection(db_path.to_str().unwrap()).await.unwrap();
        conn.call(|conn| -> Result<(), rusqlite::Error> {
            conn.execute_batch("CREATE TABLE t(x TEXT); INSERT INTO t VALUES('secret');")?;
            Ok(())
        })
        .await
        .unwrap();
        drop(conn);

        // File should NOT be plaintext.
        assert!(!is_plaintext_sqlite(&db_path).unwrap());

        // Re-open with same key should work.
        let conn2 = open_connection(db_path.to_str().unwrap()).await.unwrap();
        let val: String = conn2
            .call(|conn| -> Result<String, rusqlite::Error> {
                conn.query_row("SELECT x FROM t;", [], |row| row.get(0))
            })
            .await
            .unwrap();
        assert_eq!(val, "secret");

        unsafe { remove_key() };
    }

    #[tokio::test]
    #[serial]
    async fn test_open_connection_hex_key() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("hex_key.db");

        // 64-char hex key (256 bits).
        let hex_key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        unsafe { set_key(hex_key) };

        let conn = open_connection(db_path.to_str().unwrap()).await.unwrap();
        conn.call(|conn| -> Result<(), rusqlite::Error> {
            conn.execute_batch("CREATE TABLE t(x INTEGER); INSERT INTO t VALUES(99);")?;
            Ok(())
        })
        .await
        .unwrap();
        drop(conn);

        // Re-open with same hex key.
        let conn2 = open_connection(db_path.to_str().unwrap()).await.unwrap();
        let val: i64 = conn2
            .call(|conn| -> Result<i64, rusqlite::Error> {
                conn.query_row("SELECT x FROM t;", [], |row| row.get(0))
            })
            .await
            .unwrap();
        assert_eq!(val, 99);

        unsafe { remove_key() };
    }

    #[tokio::test]
    #[serial]
    async fn test_open_connection_wrong_key() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("wrong_key.db");

        // Create with one key.
        unsafe { set_key("correct-key") };
        let conn = open_connection(db_path.to_str().unwrap()).await.unwrap();
        conn.call(|conn| -> Result<(), rusqlite::Error> {
            conn.execute_batch("CREATE TABLE t(x INTEGER);")?;
            Ok(())
        })
        .await
        .unwrap();
        drop(conn);

        // Try to open with wrong key.
        unsafe { set_key("wrong-key") };
        let result = open_connection(db_path.to_str().unwrap()).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("encrypted or not a database"),
            "Expected generic error, got: {err_msg}"
        );

        unsafe { remove_key() };
    }

    #[tokio::test]
    #[serial]
    async fn test_encrypted_db_without_key_errors() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("no_key.db");

        // Create encrypted DB.
        unsafe { set_key("my-key") };
        let conn = open_connection(db_path.to_str().unwrap()).await.unwrap();
        conn.call(|conn| -> Result<(), rusqlite::Error> {
            conn.execute_batch("CREATE TABLE t(x INTEGER);")?;
            Ok(())
        })
        .await
        .unwrap();
        drop(conn);

        // Remove key and try to open.
        unsafe { remove_key() };
        let result = open_connection(db_path.to_str().unwrap()).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("BLUFIO_DB_KEY is not set"),
            "Expected missing key error, got: {err_msg}"
        );
    }

    #[test]
    fn test_is_plaintext_sqlite_true() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("plain_check.db");

        // Create a plain SQLite DB.
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE t(x INTEGER);").unwrap();
        drop(conn);

        assert!(is_plaintext_sqlite(&db_path).unwrap());
    }

    #[test]
    #[serial]
    fn test_is_plaintext_sqlite_false_encrypted() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("enc_check.db");

        // Create an encrypted DB via sync factory.
        unsafe { set_key("detect-test-key") };
        let conn = open_connection_sync(db_path.to_str().unwrap(), rusqlite::OpenFlags::default())
            .unwrap();
        conn.execute_batch("CREATE TABLE t(x INTEGER);").unwrap();
        drop(conn);

        assert!(!is_plaintext_sqlite(&db_path).unwrap());
        unsafe { remove_key() };
    }

    #[test]
    #[serial]
    fn test_open_connection_sync_with_key() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("sync_enc.db");

        unsafe { set_key("sync-test-key") };
        let conn = open_connection_sync(db_path.to_str().unwrap(), rusqlite::OpenFlags::default())
            .unwrap();
        conn.execute_batch("CREATE TABLE t(x TEXT); INSERT INTO t VALUES('sync-secret');")
            .unwrap();
        drop(conn);

        // Re-open and verify.
        let conn2 = open_connection_sync(
            db_path.to_str().unwrap(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .unwrap();
        let val: String = conn2
            .query_row("SELECT x FROM t;", [], |row| row.get(0))
            .unwrap();
        assert_eq!(val, "sync-secret");

        unsafe { remove_key() };
    }
}
