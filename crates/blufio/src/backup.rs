// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio backup` and `blufio restore` command implementation.
//!
//! Uses rusqlite's Backup API for atomic, consistent copies that work
//! even while the database is being written to in WAL mode. Since the
//! vault is stored in the same SQLite file, backup automatically includes
//! encrypted credentials.

use std::io::ErrorKind;
use std::path::Path;
use std::time::Duration;

use blufio_core::BlufioError;
use rusqlite::Connection;

/// Verify the integrity of a SQLite database file using `PRAGMA integrity_check`.
///
/// Opens a read-only connection, runs `PRAGMA integrity_check(1)` (limited to
/// one error row for speed on corrupt databases), and returns `Ok(())` if the
/// database is intact. On failure, returns an error containing the first
/// integrity check issue found.
///
/// The connection is automatically dropped when this function returns,
/// ensuring no file locks are held after verification.
pub fn run_integrity_check(path: &Path) -> Result<(), BlufioError> {
    let conn = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| BlufioError::Storage {
        source: Box::new(e),
    })?;

    let mut stmt = conn
        .prepare("PRAGMA integrity_check(1)")
        .map_err(|e| BlufioError::Storage {
            source: Box::new(e),
        })?;

    let rows: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| BlufioError::Storage {
            source: Box::new(e),
        })?
        .filter_map(|r| r.ok())
        .collect();

    if rows.len() == 1 && rows[0] == "ok" {
        Ok(())
    } else {
        let first_error = rows
            .first()
            .map(|s| s.as_str())
            .unwrap_or("unknown error");
        Err(BlufioError::Storage {
            source: Box::new(std::io::Error::new(
                ErrorKind::InvalidData,
                format!("integrity check failed ({first_error})"),
            )),
        })
    }
}

/// Run a backup of the SQLite database to the specified path.
///
/// Uses rusqlite's Backup API for atomic, consistent copies that work
/// even while the database is being written to in WAL mode.
pub fn run_backup(db_path: &str, backup_path: &str) -> Result<(), BlufioError> {
    let src_path = Path::new(db_path);
    if !src_path.exists() {
        return Err(BlufioError::Storage {
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("database not found: {db_path}"),
            )),
        });
    }

    // Open source in read-only mode to minimize impact on running instance.
    let src = Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| BlufioError::Storage {
        source: Box::new(e),
    })?;

    let mut dst = Connection::open(backup_path).map_err(|e| BlufioError::Storage {
        source: Box::new(e),
    })?;

    let backup =
        rusqlite::backup::Backup::new(&src, &mut dst).map_err(|e| BlufioError::Storage {
            source: Box::new(e),
        })?;

    // Copy 100 pages per step, sleep 10ms between steps.
    // This allows the running instance to continue writing.
    backup
        .run_to_completion(100, Duration::from_millis(10), None)
        .map_err(|e| BlufioError::Storage {
            source: Box::new(e),
        })?;

    // Drop connections before integrity check to release file locks.
    drop(backup);
    drop(src);
    drop(dst);

    // Verify backup integrity.
    if let Err(e) = run_integrity_check(Path::new(backup_path)) {
        let _ = std::fs::remove_file(backup_path);
        eprintln!("Backup FAILED: {e}. Backup file deleted.");
        eprintln!("Run 'blufio doctor' for full database diagnostics.");
        return Err(e);
    }

    // Report file size with integrity status.
    let metadata = std::fs::metadata(backup_path).map_err(|e| BlufioError::Storage {
        source: Box::new(e),
    })?;
    let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
    eprintln!("Backup complete: {size_mb:.1} MB, integrity: ok");

    Ok(())
}

/// Restore the database from a backup file.
///
/// Creates a safety backup of the current DB before overwriting.
/// Validates that the source is a valid SQLite database.
pub fn run_restore(db_path: &str, restore_from: &str) -> Result<(), BlufioError> {
    let src_path = Path::new(restore_from);
    if !src_path.exists() {
        return Err(BlufioError::Storage {
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("backup file not found: {restore_from}"),
            )),
        });
    }

    // Validate source is a valid SQLite DB.
    let test_conn =
        Connection::open_with_flags(restore_from, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| BlufioError::Storage {
                source: Box::new(e),
            })?;

    // Quick validation: can we query it?
    test_conn
        .execute_batch("SELECT 1")
        .map_err(|e| BlufioError::Storage {
            source: Box::new(e),
        })?;
    drop(test_conn);

    // Create safety backup of current DB (if it exists).
    let dst_path = Path::new(db_path);
    if dst_path.exists() {
        let pre_restore_path = format!("{db_path}.pre-restore");
        eprintln!("Creating safety backup: {pre_restore_path}");
        run_backup(db_path, &pre_restore_path)?;
    }

    // Perform restore using backup API (reverse direction).
    let src = Connection::open_with_flags(restore_from, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| BlufioError::Storage {
            source: Box::new(e),
        })?;

    let mut dst = Connection::open(db_path).map_err(|e| BlufioError::Storage {
        source: Box::new(e),
    })?;

    let backup =
        rusqlite::backup::Backup::new(&src, &mut dst).map_err(|e| BlufioError::Storage {
            source: Box::new(e),
        })?;

    backup
        .run_to_completion(100, Duration::from_millis(10), None)
        .map_err(|e| BlufioError::Storage {
            source: Box::new(e),
        })?;

    let metadata = std::fs::metadata(db_path).map_err(|e| BlufioError::Storage {
        source: Box::new(e),
    })?;
    let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
    eprintln!("Restore complete: {size_mb:.1} MB restored from {restore_from}");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_nonexistent_source_fails() {
        let result = run_backup("/tmp/nonexistent-blufio-src.db", "/tmp/blufio-backup.db");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"));
    }

    #[test]
    fn restore_nonexistent_source_fails() {
        let result = run_restore("/tmp/blufio-target.db", "/tmp/nonexistent-blufio-backup.db");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"));
    }

    #[test]
    fn backup_and_restore_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let src_path = dir.path().join("source.db");
        let backup_path = dir.path().join("backup.db");

        // Create a source DB with some data.
        let conn = Connection::open(&src_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT);
             INSERT INTO test VALUES (1, 'hello');
             INSERT INTO test VALUES (2, 'world');",
        )
        .unwrap();
        drop(conn);

        // Backup.
        run_backup(src_path.to_str().unwrap(), backup_path.to_str().unwrap()).unwrap();

        // Verify backup is a valid SQLite DB with the data.
        let backup_conn = Connection::open(&backup_path).unwrap();
        let count: i64 = backup_conn
            .query_row("SELECT COUNT(*) FROM test", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
        drop(backup_conn);
    }

    #[test]
    fn restore_creates_pre_restore_backup() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("current.db");
        let backup_path = dir.path().join("backup.db");

        // Create current DB.
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE current_data (id INTEGER PRIMARY KEY);
             INSERT INTO current_data VALUES (1);",
        )
        .unwrap();
        drop(conn);

        // Create backup DB with different data.
        let conn = Connection::open(&backup_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE restored_data (id INTEGER PRIMARY KEY);
             INSERT INTO restored_data VALUES (42);",
        )
        .unwrap();
        drop(conn);

        // Restore.
        run_restore(db_path.to_str().unwrap(), backup_path.to_str().unwrap()).unwrap();

        // Verify pre-restore backup exists.
        let pre_restore = format!("{}.pre-restore", db_path.to_str().unwrap());
        assert!(Path::new(&pre_restore).exists());

        // Verify pre-restore backup has original data.
        let pre_conn = Connection::open(&pre_restore).unwrap();
        let count: i64 = pre_conn
            .query_row("SELECT COUNT(*) FROM current_data", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        drop(pre_conn);

        // Verify current DB has restored data.
        let restored_conn = Connection::open(&db_path).unwrap();
        let val: i64 = restored_conn
            .query_row("SELECT id FROM restored_data", [], |row| row.get(0))
            .unwrap();
        assert_eq!(val, 42);
        drop(restored_conn);
    }

    #[test]
    fn restore_invalid_source_fails() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("target.db");
        let invalid_path = dir.path().join("invalid.db");

        // Create a non-SQLite file.
        std::fs::write(&invalid_path, b"this is not a sqlite file").unwrap();

        let result = run_restore(db_path.to_str().unwrap(), invalid_path.to_str().unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn backup_empty_db() {
        let dir = tempfile::tempdir().unwrap();
        let src_path = dir.path().join("empty.db");
        let backup_path = dir.path().join("empty_backup.db");

        // Create empty DB.
        let conn = Connection::open(&src_path).unwrap();
        drop(conn);

        // Backup should succeed.
        run_backup(src_path.to_str().unwrap(), backup_path.to_str().unwrap()).unwrap();

        // Backup should be openable.
        let backup_conn = Connection::open(&backup_path).unwrap();
        backup_conn.execute_batch("SELECT 1").unwrap();
        drop(backup_conn);
    }

    #[test]
    fn test_integrity_check_valid_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("valid.db");

        // Create a valid DB with data.
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT);
             INSERT INTO test VALUES (1, 'hello');
             INSERT INTO test VALUES (2, 'world');",
        )
        .unwrap();
        drop(conn);

        // Integrity check should pass.
        assert!(run_integrity_check(&db_path).is_ok());
    }

    #[test]
    fn test_integrity_check_empty_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("empty.db");

        // Create empty DB.
        let conn = Connection::open(&db_path).unwrap();
        drop(conn);

        // Integrity check should pass on empty DB.
        assert!(run_integrity_check(&db_path).is_ok());
    }

    #[test]
    fn test_integrity_check_corrupt_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("corrupt.db");

        // Create a valid DB with enough data to have multiple pages.
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT);",
        )
        .unwrap();
        // Insert enough rows to push data past the first page.
        for i in 0..100 {
            conn.execute(
                "INSERT INTO test VALUES (?1, ?2)",
                rusqlite::params![i, format!("data-{i}-padding-to-make-rows-longer-for-page-fill")],
            )
            .unwrap();
        }
        drop(conn);

        // Corrupt bytes further into the file (past the header and root page
        // metadata) to trigger integrity_check failure without preventing
        // the file from being opened.
        let mut data = std::fs::read(&db_path).unwrap();
        assert!(data.len() > 4096, "DB file too small for multi-page corruption test");
        // Corrupt bytes in the second page area (offset 4096+).
        for i in 4096..4196 {
            data[i] = 0xFF;
        }
        std::fs::write(&db_path, &data).unwrap();

        // Integrity check should fail -- either via PRAGMA integrity_check
        // returning errors, or via rusqlite detecting malformed data.
        let result = run_integrity_check(&db_path);
        assert!(
            result.is_err(),
            "Expected integrity check to fail on corrupt database"
        );
        let err = result.unwrap_err().to_string();
        // Accept either our custom message or rusqlite's malformed error.
        assert!(
            err.contains("integrity check failed") || err.contains("malformed"),
            "Expected corruption-related error, got: {err}"
        );
    }
}
