---
phase: 25-sqlcipher-database-encryption
plan: 01
subsystem: storage
tags: [sqlcipher, encryption, database, connection-factory]

requires:
  - phase: 24-sd-notify-integration
    provides: stable storage crate for encryption additions
provides:
  - bundled-sqlcipher-vendored-openssl feature flag replacing bundled
  - centralized open_connection() and open_connection_sync() factories with PRAGMA key
  - is_plaintext_sqlite() header detection function
  - apply_encryption_key() and verify_key() internal helpers
affects: [25-02, 25-03, all database consumers]

tech-stack:
  changed: [rusqlite bundled -> bundled-sqlcipher-vendored-openssl]
  patterns: [centralized connection factory, env-based key injection]

key-files:
  modified:
    - Cargo.toml
    - crates/blufio-storage/src/database.rs
    - crates/blufio-storage/src/lib.rs
    - crates/blufio-storage/Cargo.toml

key-decisions:
  - "BLUFIO_DB_KEY env var for encryption key -- consistent with BLUFIO_VAULT_KEY pattern"
  - "Auto-detect hex vs passphrase keys: 64 hex chars = raw hex (x'' syntax), else passphrase"
  - "Pre-flight encrypted file detection: hard error if encrypted file opened without key"
  - "Empty/small files treated as plaintext (not encrypted) to handle zero-byte SQLite files"

patterns-established:
  - "All database connections must go through open_connection() or open_connection_sync()"
  - "PRAGMA key is always the first statement on every new connection"
  - "Key verification via SELECT count(*) FROM sqlite_master immediately after PRAGMA key"

requirements-completed: [CIPH-01, CIPH-02, CIPH-03]

completed: 2026-03-03
---

# Plan 25-01: Feature Flag + Centralized Connection Factory Summary

**Switch rusqlite to bundled-sqlcipher-vendored-openssl and create centralized connection factory with transparent encryption support**

## Performance

- **Files modified:** 4
- **Tests added:** 7

## Accomplishments
- Changed workspace rusqlite feature from `bundled` to `bundled-sqlcipher-vendored-openssl`
- Created `open_connection()` async factory and `open_connection_sync()` sync factory
- Both factories auto-apply PRAGMA key when BLUFIO_DB_KEY is set
- Added `is_plaintext_sqlite()` for header-based encryption detection
- Added `apply_encryption_key()` with hex/passphrase auto-detection
- Added `verify_key()` for immediate key correctness verification
- Refactored `Database::open()` to use `open_connection()` internally
- All 7 new tests pass with `#[serial]` for env var safety

## Files Modified
- `Cargo.toml` - Changed rusqlite feature to `bundled-sqlcipher-vendored-openssl`
- `crates/blufio-storage/src/database.rs` - Added factory functions, helpers, and tests
- `crates/blufio-storage/src/lib.rs` - Exported new public functions
- `crates/blufio-storage/Cargo.toml` - Added serial_test and hex dev-dependencies

## Issues Encountered
- Rust 2024 edition requires `unsafe {}` blocks for `std::env::set_var/remove_var` in tests
- tokio-rusqlite `conn.call()` closures need explicit return type annotations for type inference

---
*Phase: 25-sqlcipher-database-encryption*
*Completed: 2026-03-03*
