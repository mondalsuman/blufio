// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio db encrypt` and `blufio db keygen` command implementation.
//!
//! Provides a safe, one-command migration path from plaintext SQLite
//! databases to SQLCipher encrypted databases.
//!
//! The encrypt command uses a three-file safety strategy:
//! 1. Export plaintext data to an encrypted temp file (`.encrypting`)
//! 2. Verify the encrypted copy with integrity check
//! 3. Swap files (original -> `.pre-encrypt`, encrypted -> original)
//!
//! The original database is never modified until the encrypted copy
//! is fully verified.

use blufio_core::BlufioError;

/// Generate a random 256-bit encryption key and print it as hex to stdout.
///
/// Operators can pipe this to a secrets manager:
/// ```bash
/// blufio db keygen | vault kv put secret/blufio db_key=-
/// ```
pub fn run_keygen() {
    use ring::rand::{SecureRandom, SystemRandom};
    let rng = SystemRandom::new();
    let mut key_bytes = [0u8; 32]; // 256 bits
    rng.fill(&mut key_bytes).expect("system RNG failed");
    println!("{}", hex::encode(key_bytes));
}

/// Encrypt an existing plaintext database with SQLCipher.
///
/// Requires `BLUFIO_DB_KEY` to be set. Uses the `sqlcipher_export()` function
/// via ATTACH DATABASE to copy all data from the plaintext DB to an encrypted
/// destination, then swaps files after verification.
pub fn run_encrypt(db_path: &str, skip_confirm: bool) -> Result<(), BlufioError> {
    // Pre-checks.
    let key = std::env::var("BLUFIO_DB_KEY").map_err(|_| BlufioError::Storage {
        source: Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "BLUFIO_DB_KEY environment variable must be set before encrypting",
        )),
    })?;

    let path = std::path::Path::new(db_path);
    if !path.exists() {
        return Err(BlufioError::Storage {
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("database not found: {db_path}"),
            )),
        });
    }

    let is_plain = blufio_storage::is_plaintext_sqlite(path).map_err(|e| BlufioError::Storage {
        source: Box::new(e),
    })?;
    if !is_plain {
        return Err(BlufioError::Storage {
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "database is already encrypted",
            )),
        });
    }

    let encrypting_path = format!("{db_path}.encrypting");
    let pre_encrypt_path = format!("{db_path}.pre-encrypt");

    // Clean up leftover temp files from interrupted runs.
    if std::path::Path::new(&encrypting_path).exists() {
        eprintln!("Cleaning up incomplete previous encryption attempt...");
        let _ = std::fs::remove_file(&encrypting_path);
    }

    // Interactive confirmation.
    if !skip_confirm {
        let metadata = std::fs::metadata(db_path).map_err(|e| BlufioError::Storage {
            source: Box::new(e),
        })?;
        let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
        eprintln!("Database: {db_path}");
        eprintln!("Size: {size_mb:.1} MB");
        eprintln!();
        eprintln!("This will encrypt the database in place. A backup of the");
        eprintln!("original will be kept as {pre_encrypt_path}");
        eprintln!();
        eprint!("Continue? [y/N] ");

        use std::io::BufRead;
        let stdin = std::io::stdin();
        let mut line = String::new();
        stdin
            .lock()
            .read_line(&mut line)
            .map_err(|e| BlufioError::Storage {
                source: Box::new(e),
            })?;
        if !line.trim().eq_ignore_ascii_case("y") {
            eprintln!("Aborted.");
            return Ok(());
        }
    }

    // Step 1: Export plaintext to encrypted temp file.
    eprint!("Exporting to temp file... ");
    {
        // Open plaintext DB with no key (direct open, bypassing factory).
        let conn = rusqlite::Connection::open(db_path).map_err(|e| BlufioError::Storage {
            source: Box::new(e),
        })?;

        // Escape key for SQL.
        let escaped_key = key.replace('\'', "''");

        // Determine key syntax.
        let is_hex = key.len() == 64 && key.chars().all(|c| c.is_ascii_hexdigit());
        let key_expr = if is_hex {
            format!("\"x'{key}'\"")
        } else {
            format!("'{escaped_key}'")
        };

        // Escape encrypting_path for SQL.
        let escaped_enc_path = encrypting_path.replace('\'', "''");

        // Attach encrypted destination.
        conn.execute_batch(&format!(
            "ATTACH DATABASE '{escaped_enc_path}' AS encrypted KEY {key_expr};"
        ))
        .map_err(|e| BlufioError::Storage {
            source: Box::new(e),
        })?;

        // Export all data.
        conn.execute_batch("SELECT sqlcipher_export('encrypted');")
            .map_err(|e| BlufioError::Storage {
                source: Box::new(e),
            })?;

        conn.execute_batch("DETACH DATABASE encrypted;")
            .map_err(|e| BlufioError::Storage {
                source: Box::new(e),
            })?;
    }
    let enc_meta = std::fs::metadata(&encrypting_path).map_err(|e| BlufioError::Storage {
        source: Box::new(e),
    })?;
    let size_mb = enc_meta.len() as f64 / (1024.0 * 1024.0);
    eprintln!("done ({size_mb:.1} MB)");

    // Step 2: Verify encrypted copy.
    eprint!("Verifying integrity... ");
    {
        let verify_conn = blufio_storage::open_connection_sync(
            &encrypting_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let result: String = verify_conn
            .query_row("PRAGMA integrity_check(1);", [], |row| row.get(0))
            .map_err(|e| BlufioError::Storage {
                source: Box::new(e),
            })?;
        if result != "ok" {
            // Clean up failed temp file.
            let _ = std::fs::remove_file(&encrypting_path);
            return Err(BlufioError::Storage {
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("encrypted copy failed integrity check: {result}"),
                )),
            });
        }
    }
    eprintln!("ok");

    // Step 3: Swap files.
    eprint!("Swapping files... ");
    // Move original to .pre-encrypt (safety backup).
    std::fs::rename(db_path, &pre_encrypt_path).map_err(|e| BlufioError::Storage {
        source: Box::new(e),
    })?;
    // Move encrypted to original path.
    std::fs::rename(&encrypting_path, db_path).map_err(|e| BlufioError::Storage {
        source: Box::new(e),
    })?;
    // Also clean up WAL and SHM files from plaintext DB.
    let _ = std::fs::remove_file(format!("{db_path}-wal"));
    let _ = std::fs::remove_file(format!("{db_path}-shm"));
    eprintln!("done");

    eprintln!("Encryption complete.");
    eprintln!("Original database saved as: {pre_encrypt_path}");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::tempdir;

    /// Safety: env var mutations are guarded by #[serial].
    unsafe fn set_key(val: &str) {
        unsafe { std::env::set_var("BLUFIO_DB_KEY", val) };
    }

    /// Safety: see `set_key`.
    unsafe fn remove_key() {
        unsafe { std::env::remove_var("BLUFIO_DB_KEY") };
    }

    #[test]
    fn test_keygen_produces_64_hex_chars() {
        // We can't easily capture stdout in a unit test, so test the
        // underlying logic directly.
        use ring::rand::{SecureRandom, SystemRandom};
        let rng = SystemRandom::new();
        let mut key_bytes = [0u8; 32];
        rng.fill(&mut key_bytes).expect("RNG failed");
        let hex_str = hex::encode(key_bytes);
        assert_eq!(hex_str.len(), 64);
        assert!(hex_str.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    #[serial]
    fn test_encrypt_no_key_errors() {
        unsafe { remove_key() };
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create a plaintext DB.
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE t(x INTEGER);").unwrap();
        drop(conn);

        let result = run_encrypt(db_path.to_str().unwrap(), true);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("BLUFIO_DB_KEY"),
            "Expected BLUFIO_DB_KEY error, got: {msg}"
        );
    }

    #[test]
    #[serial]
    fn test_encrypt_no_db_errors() {
        unsafe { set_key("test-key") };
        let result = run_encrypt("/tmp/nonexistent-blufio-encrypt-test.db", true);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("not found"),
            "Expected not found error, got: {msg}"
        );
        unsafe { remove_key() };
    }

    #[test]
    #[serial]
    fn test_encrypt_already_encrypted_errors() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("already_enc.db");

        // Create an encrypted DB.
        unsafe { set_key("already-enc-key") };
        let conn = blufio_storage::open_connection_sync(
            db_path.to_str().unwrap(),
            rusqlite::OpenFlags::default(),
        )
        .unwrap();
        conn.execute_batch("CREATE TABLE t(x INTEGER);").unwrap();
        drop(conn);

        // Try to encrypt again.
        let result = run_encrypt(db_path.to_str().unwrap(), true);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("already encrypted"),
            "Expected already encrypted error, got: {msg}"
        );
        unsafe { remove_key() };
    }

    #[test]
    #[serial]
    fn test_encrypt_roundtrip() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("roundtrip.db");

        // Create a plaintext DB with data.
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE test_data (id INTEGER PRIMARY KEY, value TEXT);
             INSERT INTO test_data VALUES (1, 'hello');
             INSERT INTO test_data VALUES (2, 'world');",
        )
        .unwrap();
        drop(conn);

        // Encrypt.
        unsafe { set_key("roundtrip-key") };
        run_encrypt(db_path.to_str().unwrap(), true).unwrap();

        // File should no longer be plaintext.
        assert!(!blufio_storage::is_plaintext_sqlite(&db_path).unwrap());

        // .pre-encrypt file should exist.
        let pre_encrypt = dir.path().join("roundtrip.db.pre-encrypt");
        assert!(pre_encrypt.exists());

        // Original data should be accessible through encrypted connection.
        let enc_conn = blufio_storage::open_connection_sync(
            db_path.to_str().unwrap(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let count: i64 = enc_conn
            .query_row("SELECT COUNT(*) FROM test_data;", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);

        let val: String = enc_conn
            .query_row("SELECT value FROM test_data WHERE id = 1;", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(val, "hello");

        unsafe { remove_key() };
    }

    #[test]
    #[serial]
    fn test_encrypt_cleanup_leftover() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("cleanup.db");
        let encrypting_path = dir.path().join("cleanup.db.encrypting");

        // Create a plaintext DB.
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE t(x INTEGER); INSERT INTO t VALUES(1);")
            .unwrap();
        drop(conn);

        // Create a leftover .encrypting file.
        std::fs::write(&encrypting_path, b"leftover data").unwrap();
        assert!(encrypting_path.exists());

        // Run encrypt -- should clean up the leftover.
        unsafe { set_key("cleanup-key") };
        run_encrypt(db_path.to_str().unwrap(), true).unwrap();

        // The .encrypting file should not exist after successful encrypt.
        assert!(!encrypting_path.exists());

        // The encrypted DB should work.
        let enc_conn = blufio_storage::open_connection_sync(
            db_path.to_str().unwrap(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let val: i64 = enc_conn
            .query_row("SELECT x FROM t;", [], |row| row.get(0))
            .unwrap();
        assert_eq!(val, 1);

        unsafe { remove_key() };
    }
}
