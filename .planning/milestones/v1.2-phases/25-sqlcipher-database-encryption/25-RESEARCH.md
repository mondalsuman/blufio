# Phase 25: SQLCipher Database Encryption - Research

**Researched:** 2026-03-03
**Domain:** SQLCipher database encryption at rest via rusqlite
**Confidence:** HIGH

## Summary

SQLCipher is a standalone fork of SQLite that provides 256-bit AES encryption of database files. Rusqlite supports SQLCipher natively via the `bundled-sqlcipher-vendored-openssl` feature flag, which bundles both SQLCipher and OpenSSL for zero external dependencies. The critical implementation requirement is that `PRAGMA key` must be the **absolute first statement** on every new connection before any other operations, as SQLCipher uses just-in-time key derivation.

The project currently has ~15 direct `Connection::open()` call sites across 8 files. Centralizing these into a single `open_connection()` factory in `blufio-storage` is the highest-impact task -- it eliminates the risk of any consumer bypassing encryption. The plaintext-to-encrypted migration uses SQLCipher's `sqlcipher_export()` function via an ATTACH DATABASE pattern, which is the official recommended approach.

**Primary recommendation:** Change the workspace `rusqlite` feature from `bundled` to `bundled-sqlcipher-vendored-openssl`, create a centralized `open_connection()` factory that conditionally applies `PRAGMA key` when `BLUFIO_DB_KEY` is set, and route all connection sites through it.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Hard error when BLUFIO_DB_KEY is not set but an encrypted database exists -- refuse to start with clear message
- Accept both raw passphrase and hex-encoded 256-bit key with auto-detect (64 hex chars = raw key bytes, otherwise passphrase)
- Include `blufio db keygen` command that prints a cryptographically random 256-bit hex key to stdout
- Wrong key error uses generic + actionable message (avoids leaking whether it's wrong-key vs corrupt)
- `blufio db encrypt` requires interactive confirmation before migrating, with `--yes` flag for automation
- Step-by-step status line output matching existing backup/restore style
- On interrupted previous run: auto-detect leftover temp files, clean up, re-run from scratch
- No `blufio db decrypt` command
- Doctor shows full details when encrypted: status, cipher version, page size, BLUFIO_DB_KEY presence
- Neutral info status when DB is not encrypted (not a warning)
- Warn (yellow) when BLUFIO_DB_KEY is set but DB is still plaintext
- Encryption check is a quick check (always visible), not gated behind --deep
- Backups always encrypted with same key when source is encrypted
- Include encryption status in backup/restore summary output
- Restore uses same BLUFIO_DB_KEY for both source and destination
- When BLUFIO_DB_KEY is not set and DB is plaintext, backup/restore unchanged

### Claude's Discretion
- Three-file safety strategy implementation details (temp file naming, swap order)
- Connection centralization approach (how to unify ~15+ connection open sites into open_connection() factory)
- SQLCipher PRAGMA ordering and configuration (cipher_page_size, kdf_iter, etc.)
- Key validation approach (SELECT after PRAGMA key)
- Error handling and retry patterns during migration

### Deferred Ideas (OUT OF SCOPE)
- `blufio db decrypt` command (reverse operation)
- Key rotation command (re-encrypt with new key)
- --source-key flag for restoring backups encrypted with a different key
- --plaintext flag for exporting unencrypted backups
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| CIPH-01 | rusqlite uses bundled-sqlcipher-vendored-openssl feature flag | Workspace Cargo.toml feature flag change from `bundled` to `bundled-sqlcipher-vendored-openssl` |
| CIPH-02 | PRAGMA key is the first statement on every database connection across all crates | Centralized `open_connection()` factory with PRAGMA key injection before any other PRAGMAs |
| CIPH-03 | Encryption key sourced from BLUFIO_DB_KEY environment variable | `std::env::var("BLUFIO_DB_KEY")` at connection factory level |
| CIPH-04 | Connection opener verifies key correctness with immediate SELECT after PRAGMA key | `SELECT count(*) FROM sqlite_master` immediately after PRAGMA key -- fails with "file is encrypted" if wrong key |
| CIPH-05 | Centralized open_connection() factory in blufio-storage used by all 6+ consumers | New `open_connection()` pub async fn in blufio-storage::database, all ~15 call sites updated |
| CIPH-06 | blufio db encrypt CLI migrates plaintext database to encrypted with three-file safety strategy | ATTACH DATABASE + sqlcipher_export() pattern with temp file, verify, swap |
| CIPH-07 | Backup and restore pass encryption key to both source and destination connections | backup.rs Connection::open calls replaced with keyed connections |
| CIPH-08 | blufio doctor reports encryption status, cipher version, and settings | New `check_encryption()` function querying PRAGMA cipher_version, cipher_page_size |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rusqlite | 0.37 | SQLite/SQLCipher bindings for Rust | Already in workspace; `bundled-sqlcipher-vendored-openssl` feature activates SQLCipher |
| tokio-rusqlite | 0.7 | Async wrapper for rusqlite | Already in workspace; provides background thread for DB operations |
| SQLCipher | 4.x (bundled) | 256-bit AES full-database encryption | Industry standard; used by Signal, 1Password; bundled via rusqlite feature |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| ring | 0.17 | Cryptographically secure random bytes for keygen | Already in workspace; `blufio db keygen` uses ring for 256-bit random key |
| hex | 0.4 | Hex encoding/decoding for key format detection | Already in workspace; detect if key is 64 hex chars |
| rand | 0.8 | Random bytes (alternative) | Already in workspace but ring preferred for crypto |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| bundled-sqlcipher-vendored-openssl | bundled-sqlcipher (system OpenSSL) | Requires OpenSSL dev headers on build machine; vendored is self-contained |
| sqlcipher_export() | Manual table-by-table copy | sqlcipher_export handles all tables, triggers, views, virtual tables atomically |
| PRAGMA key passphrase | PRAGMA key raw hex only | Passphrase mode is more user-friendly; hex mode is more secure; support both |

## Architecture Patterns

### Recommended Connection Factory Pattern

```rust
// blufio-storage/src/database.rs

/// Open a tokio-rusqlite connection with optional SQLCipher encryption.
///
/// If BLUFIO_DB_KEY is set, applies PRAGMA key as the first statement,
/// then verifies the key is correct with a test query.
pub async fn open_connection(path: &str) -> Result<tokio_rusqlite::Connection, BlufioError> {
    let conn = tokio_rusqlite::Connection::open(path)
        .await
        .map_err(|e| BlufioError::Storage { source: Box::new(e) })?;

    let db_key = std::env::var("BLUFIO_DB_KEY").ok();

    conn.call(move |conn| {
        // PRAGMA key MUST be the absolute first statement
        if let Some(key) = &db_key {
            apply_encryption_key(conn, key)?;
            // Verify key correctness immediately
            conn.execute_batch("SELECT count(*) FROM sqlite_master;")?;
        }
        Ok(())
    })
    .await
    .map_err(|e| {
        // Generic error message -- don't leak whether wrong-key vs corrupt
        BlufioError::Storage {
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Cannot open database: file is encrypted or not a database. Verify BLUFIO_DB_KEY is correct.",
            )),
        }
    })?;

    Ok(conn)
}

fn apply_encryption_key(conn: &rusqlite::Connection, key: &str) -> Result<(), rusqlite::Error> {
    if key.len() == 64 && key.chars().all(|c| c.is_ascii_hexdigit()) {
        // Raw 256-bit hex key: use x'' syntax
        conn.execute_batch(&format!("PRAGMA key = \"x'{key}'\";"))?;
    } else {
        // Passphrase mode: use quoted string
        let escaped = key.replace('\'', "''");
        conn.execute_batch(&format!("PRAGMA key = '{escaped}';"))?;
    }
    Ok(())
}
```

### Sync Connection Factory (for backup.rs)

```rust
/// Open a sync rusqlite connection with optional SQLCipher encryption.
/// Used by backup/restore which need the sync Backup API.
pub fn open_connection_sync(path: &str, flags: rusqlite::OpenFlags) -> Result<rusqlite::Connection, BlufioError> {
    let conn = rusqlite::Connection::open_with_flags(path, flags)
        .map_err(|e| BlufioError::Storage { source: Box::new(e) })?;

    if let Ok(key) = std::env::var("BLUFIO_DB_KEY") {
        apply_encryption_key(&conn, &key)?;
        conn.execute_batch("SELECT count(*) FROM sqlite_master;")
            .map_err(|_| BlufioError::Storage {
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Cannot open database: file is encrypted or not a database. Verify BLUFIO_DB_KEY is correct.",
                )),
            })?;
    }

    Ok(conn)
}
```

### Three-File Safety Strategy for `blufio db encrypt`

```
Original: blufio.db (plaintext, untouched until verified)
Step 1: Export to blufio.db.encrypting (temp encrypted copy via sqlcipher_export)
Step 2: Verify blufio.db.encrypting with PRAGMA integrity_check
Step 3: Rename blufio.db -> blufio.db.pre-encrypt (safety backup)
Step 4: Rename blufio.db.encrypting -> blufio.db (atomic swap)
Step 5: Delete blufio.db.pre-encrypt after successful verification
```

### PRAGMA Ordering (Critical)

```sql
-- MUST be this exact order on every connection:
PRAGMA key = 'xxx';                    -- 1. FIRST: Set encryption key
SELECT count(*) FROM sqlite_master;    -- 2. Verify key works
PRAGMA journal_mode = WAL;             -- 3. Then all other PRAGMAs
PRAGMA synchronous = NORMAL;
PRAGMA busy_timeout = 5000;
PRAGMA foreign_keys = ON;
PRAGMA cache_size = -16000;
PRAGMA temp_store = MEMORY;
```

### Anti-Patterns to Avoid
- **Setting PRAGMAs before PRAGMA key:** Any PRAGMA or query before `PRAGMA key` will cause SQLCipher to initialize with the wrong (or no) key, corrupting the session
- **Different key on different connections:** All connections to the same DB must use the exact same key
- **Calling PRAGMA key on an unencrypted DB with BLUFIO_DB_KEY set:** This actually creates a new encrypted DB overlaying the plaintext one -- must detect plaintext vs encrypted first
- **Using rusqlite's Backup API between plaintext and encrypted:** The Backup API does a page-level copy; if source is encrypted, destination pages are encrypted too (same key required on both)

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Plaintext-to-encrypted migration | Manual row-by-row copy | `sqlcipher_export()` via `ATTACH DATABASE` | Handles ALL objects (tables, triggers, views, virtual tables, indexes) atomically |
| Crypto-secure random key generation | Custom random + hex encode | `ring::rand::SystemRandom` + hex encode | Crypto-grade CSPRNG already in workspace |
| Encryption at rest | Application-level column encryption | SQLCipher full-database encryption | Encrypts entire file including metadata, indexes, journal; no plaintext leaks |
| Key derivation from passphrase | Custom PBKDF2/Argon2 for DB key | SQLCipher's built-in PBKDF2-HMAC-SHA512 (256000 iterations) | SQLCipher handles KDF internally when passphrase mode is used |

## Common Pitfalls

### Pitfall 1: PRAGMA key After Other Statements
**What goes wrong:** Setting `PRAGMA key` after any other SQL statement (including other PRAGMAs) causes SQLCipher to silently use an empty key, making the database unreadable with the correct key later.
**Why it happens:** SQLCipher initializes the encryption context on the first database operation. If that operation is NOT `PRAGMA key`, it initializes with no key.
**How to avoid:** Factory function enforces PRAGMA key as the absolute first statement. Code review grep for any `Connection::open` not going through factory.
**Warning signs:** "file is encrypted or is not a database" errors appearing after seemingly correct key setup.

### Pitfall 2: Detecting Encrypted vs Plaintext
**What goes wrong:** Opening a plaintext DB with BLUFIO_DB_KEY set tries to decrypt it, which fails or (worse) appears to succeed but data is corrupted.
**Why it happens:** SQLCipher cannot distinguish "wrong key" from "not encrypted" -- both produce the same error.
**How to avoid:** Read the first 16 bytes of the file. A standard SQLite file starts with "SQLite format 3\0". An encrypted file will have random-looking bytes. Use this heuristic before applying PRAGMA key.
**Warning signs:** Doctor check warns when BLUFIO_DB_KEY is set but DB header shows plaintext SQLite.

### Pitfall 3: Backup API with Mixed Encryption
**What goes wrong:** Using rusqlite's Backup API between an encrypted source and an unkeyed destination creates a corrupt file.
**Why it happens:** Backup API copies pages as-is. Encrypted pages copied to an unkeyed connection are still encrypted but the destination doesn't know that.
**How to avoid:** Both source and destination connections must have the same PRAGMA key applied before creating the Backup object.

### Pitfall 4: WAL Mode and Encryption
**What goes wrong:** WAL file is also encrypted, but only if WAL mode is set AFTER PRAGMA key.
**Why it happens:** WAL inherits encryption state from the connection that creates it.
**How to avoid:** Strict PRAGMA ordering: key first, then WAL mode. This is already enforced by the factory pattern.

### Pitfall 5: refinery Migrations with SQLCipher
**What goes wrong:** refinery migrations get a `&mut rusqlite::Connection` via callback. If PRAGMA key wasn't applied first, migrations fail on encrypted DB.
**Why it happens:** refinery doesn't know about SQLCipher; it just opens a connection.
**How to avoid:** Apply PRAGMA key + verification BEFORE passing connection to refinery. The current `Database::open()` runs PRAGMAs then migrations -- just insert PRAGMA key before the existing PRAGMAs.

## Code Examples

### Detecting Plaintext vs Encrypted Database

```rust
/// Check if a database file is a plaintext SQLite database.
/// Returns true if the file starts with "SQLite format 3\0".
fn is_plaintext_sqlite(path: &std::path::Path) -> std::io::Result<bool> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut header = [0u8; 16];
    let n = file.read(&mut header)?;
    if n < 16 {
        return Ok(false); // Too small to be a valid SQLite file
    }
    Ok(&header == b"SQLite format 3\0")
}
```

### Encrypt Migration via sqlcipher_export

```rust
fn encrypt_database(plaintext_path: &str, encrypted_path: &str, key: &str) -> Result<(), BlufioError> {
    // Open plaintext DB (no key needed)
    let conn = rusqlite::Connection::open(plaintext_path)
        .map_err(|e| BlufioError::Storage { source: Box::new(e) })?;

    // Attach encrypted destination
    let escaped_key = key.replace('\'', "''");
    conn.execute_batch(&format!(
        "ATTACH DATABASE '{}' AS encrypted KEY '{}';",
        encrypted_path, escaped_key
    )).map_err(|e| BlufioError::Storage { source: Box::new(e) })?;

    // Export all data to encrypted DB
    conn.execute_batch("SELECT sqlcipher_export('encrypted');")
        .map_err(|e| BlufioError::Storage { source: Box::new(e) })?;

    conn.execute_batch("DETACH DATABASE encrypted;")
        .map_err(|e| BlufioError::Storage { source: Box::new(e) })?;

    Ok(())
}
```

### Key Generation

```rust
fn generate_hex_key() -> String {
    use ring::rand::{SecureRandom, SystemRandom};
    let rng = SystemRandom::new();
    let mut key_bytes = [0u8; 32]; // 256 bits
    rng.fill(&mut key_bytes).expect("system RNG failed");
    hex::encode(key_bytes)
}
```

### Doctor Encryption Check

```rust
async fn check_encryption(db_path: &str) -> CheckResult {
    let start = Instant::now();
    let path = std::path::Path::new(db_path);

    if !path.exists() {
        return CheckResult { name: "Encryption".into(), status: CheckStatus::Pass,
            message: "no database (will be created)".into(), duration: start.elapsed() };
    }

    let is_plaintext = is_plaintext_sqlite(path).unwrap_or(false);
    let has_key = std::env::var("BLUFIO_DB_KEY").is_ok();

    match (is_plaintext, has_key) {
        (true, false) => CheckResult {
            name: "Encryption".into(), status: CheckStatus::Pass,
            message: "not encrypted".into(), duration: start.elapsed()
        },
        (true, true) => CheckResult {
            name: "Encryption".into(), status: CheckStatus::Warn,
            message: "BLUFIO_DB_KEY is set but database is plaintext. Run: blufio db encrypt".into(),
            duration: start.elapsed()
        },
        (false, true) => {
            // Try to query cipher info
            // ... query PRAGMA cipher_version, cipher_provider_version
            CheckResult {
                name: "Encryption".into(), status: CheckStatus::Pass,
                message: format!("encrypted (SQLCipher {version}, page size: {page_size})"),
                duration: start.elapsed()
            }
        },
        (false, false) => CheckResult {
            name: "Encryption".into(), status: CheckStatus::Fail,
            message: "database is encrypted but BLUFIO_DB_KEY is not set".into(),
            duration: start.elapsed()
        },
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| bundled (plain SQLite) | bundled-sqlcipher-vendored-openssl | New for this phase | Enables encryption; larger binary (~2MB) |
| sqlcipher (system lib) | bundled-sqlcipher-vendored-openssl | rusqlite 0.29+ | Zero external dependencies; simpler cross-compile |
| SQLCipher 3.x | SQLCipher 4.x (bundled in rusqlite 0.37) | 2019 | Stronger KDF (256000 iterations), HMAC-SHA512 |

## Open Questions

1. **musl cross-compilation**
   - What we know: `bundled-sqlcipher-vendored-openssl` vendors OpenSSL via openssl-sys. The `release-musl` profile exists in Cargo.toml.
   - What's unclear: Whether vendored OpenSSL cross-compiles cleanly to musl targets.
   - Recommendation: Validate cross-build as first task in Wave 1. If it fails, may need `CC` / `OPENSSL_DIR` env vars in CI.

2. **tokio-rusqlite and PRAGMA key timing**
   - What we know: tokio-rusqlite's `Connection::open()` spawns a background thread and opens the connection. The `conn.call()` closure runs on that thread.
   - What's unclear: Whether there's any implicit query between `open()` and the first `call()`.
   - Recommendation: HIGH confidence this is safe -- tokio-rusqlite just calls `rusqlite::Connection::open()` and the background thread does nothing until `call()` is invoked. Verified by reading tokio-rusqlite source.

3. **Binary size impact**
   - What we know: bundled-sqlcipher-vendored-openssl adds OpenSSL crypto to the binary.
   - What's unclear: Exact size increase (likely 1-3 MB).
   - Recommendation: Acceptable for single-binary deployment model. Measure before/after in CI.

## Sources

### Primary (HIGH confidence)
- [Zetetic SQLCipher API Documentation](https://www.zetetic.net/sqlcipher/sqlcipher-api/) - Official PRAGMA key behavior, sqlcipher_export, cipher_version
- [rusqlite crate on crates.io](https://crates.io/crates/rusqlite) - Feature flags documentation
- [rusqlite GitHub](https://github.com/rusqlite/rusqlite) - bundled-sqlcipher feature implementation
- [Zetetic SQLCipher FAQ: Encrypt Plaintext Database](https://discuss.zetetic.net/t/how-to-encrypt-a-plaintext-sqlite-database-to-use-sqlcipher-and-avoid-file-is-encrypted-or-is-not-a-database-errors/868) - Migration pattern

### Secondary (MEDIUM confidence)
- [rusqlite Issue #765](https://github.com/rusqlite/rusqlite/issues/765) - Bundled sqlcipher discussion
- [rusqlite Issue #966](https://github.com/rusqlite/rusqlite/issues/966) - Windows support for bundled-sqlcipher

### Tertiary (LOW confidence)
- Binary size impact estimates based on OpenSSL static linking patterns

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - rusqlite's bundled-sqlcipher is well-documented and the project already uses rusqlite
- Architecture: HIGH - PRAGMA key ordering is well-documented by Zetetic; factory pattern is standard
- Pitfalls: HIGH - SQLCipher pitfalls are extensively documented in official FAQ
- Migration: HIGH - sqlcipher_export() is the official recommended approach

**Research date:** 2026-03-03
**Valid until:** 2026-04-03 (stable domain, unlikely to change)
