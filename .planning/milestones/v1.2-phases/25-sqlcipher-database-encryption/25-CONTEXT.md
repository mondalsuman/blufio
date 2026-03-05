# Phase 25: SQLCipher Database Encryption - Context

**Gathered:** 2026-03-03
**Status:** Ready for planning

<domain>
## Phase Boundary

Encrypt the database at rest using SQLCipher so that a stolen disk or backup file reveals nothing without the encryption key. Includes centralized key management across all database consumers, a migration CLI for existing plaintext databases, and encryption status reporting in doctor. Key rotation and decrypt commands are out of scope.

</domain>

<decisions>
## Implementation Decisions

### Key Provisioning UX
- Hard error when BLUFIO_DB_KEY is not set but an encrypted database exists — refuse to start with clear message: "Database is encrypted but BLUFIO_DB_KEY is not set"
- Accept both raw passphrase (any string) and hex-encoded 256-bit key with auto-detect — if value is exactly 64 hex chars, treat as raw key bytes; otherwise treat as passphrase
- Include `blufio db keygen` command that prints a cryptographically random 256-bit hex key to stdout
- Wrong key error uses generic + actionable message: "Cannot open database: file is encrypted or not a database. Verify BLUFIO_DB_KEY is correct." — avoids leaking whether it's wrong-key vs corrupt

### Encrypt Migration CLI
- `blufio db encrypt` requires interactive confirmation before migrating — shows DB path and size, with `--yes` flag to skip for automation/CI
- Step-by-step status line output matching existing backup/restore style: "Exporting to temp file... done (5.2 MB)\nEncrypting... done\nVerifying integrity... ok\nSwapping files... done\nEncryption complete."
- On interrupted/incomplete previous run: auto-detect leftover temp files, clean them up, and re-run from scratch — three-file safety strategy means original is always untouched
- No `blufio db decrypt` command — encrypt only; operators can use sqlcipher CLI directly for the rare reverse operation

### Doctor Encryption Display
- Show full details when encrypted: status, cipher version (e.g., "SQLCipher 4.6.1"), page size (e.g., 4096), and whether BLUFIO_DB_KEY is set
- Neutral info status when DB is not encrypted — not a warning (encryption is optional, not all deployments need it)
- Warn (yellow) when BLUFIO_DB_KEY is set but DB is still plaintext — this is a likely operator mistake, suggest running `blufio db encrypt`
- Encryption check is a quick check (always visible), not gated behind --deep

### Backup/Restore Behavior
- Backups always encrypted with same key when source is encrypted — no plaintext export option, no accidental leaks
- Include encryption status in backup/restore summary output: "Backup complete: 5.2 MB, integrity: ok, encryption: enabled"
- Restore uses same BLUFIO_DB_KEY for both source (backup file) and destination — no --source-key flag, same key only
- When BLUFIO_DB_KEY is not set and DB is plaintext, backup/restore behavior is unchanged from today — encryption is opt-in, no surprises

### Claude's Discretion
- Three-file safety strategy implementation details (temp file naming, swap order)
- Connection centralization approach (how to unify ~15+ connection open sites into open_connection() factory)
- SQLCipher PRAGMA ordering and configuration (cipher_page_size, kdf_iter, etc.)
- Key validation approach (SELECT after PRAGMA key)
- Error handling and retry patterns during migration

</decisions>

<specifics>
## Specific Ideas

- Encrypt CLI output style should match existing backup/restore output — step-by-step status lines, not progress bars
- `blufio db keygen` should be a simple one-liner operators can pipe to a secrets manager
- Error messages should never distinguish between "wrong key" and "corrupt database" — same generic message for both

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `Database::open()` in `blufio-storage/src/database.rs` — centralized connection point where PRAGMA key should be injected, already has PRAGMA setup pattern
- `run_integrity_check()` in `blufio/src/backup.rs` — PRAGMA integrity_check pattern reusable for migration verification
- Doctor check framework in `blufio/src/doctor.rs` — `CheckResult` struct with Pass/Warn/Fail status, ready for new encryption check
- `blufio-vault/src/crypto.rs` — existing AES-256-GCM crypto patterns (ring crate), though SQLCipher handles its own crypto

### Established Patterns
- `tokio_rusqlite::Connection` for async DB access, sync `rusqlite::Connection` in backup.rs for Backup API
- PRAGMAs applied via `conn.call()` closures on background thread
- Error mapping: `map_tokio_rusqlite_err()` / `map_tr_err()` pattern across crates
- WAL mode with checkpoint on close
- Workspace Cargo.toml defines `rusqlite = { version = "0.37", features = ["bundled"] }` — needs feature flag change to `bundled-sqlcipher-vendored-openssl`

### Integration Points
- ~15+ direct `Connection::open()` calls across crates that need centralization: main.rs (3), mcp_server.rs (1), serve.rs (2), shell.rs (1), doctor.rs (2), backup.rs (5+), ledger.rs (1), pin_store.rs (1)
- Config system for BLUFIO_DB_KEY environment variable reading
- CLI subcommand structure for adding `blufio db encrypt` and `blufio db keygen`
- Backup/restore functions in backup.rs need key parameter threading

</code_context>

<deferred>
## Deferred Ideas

- `blufio db decrypt` command (reverse operation) — rare need, operators can use sqlcipher CLI
- Key rotation command (re-encrypt with new key) — separate phase if needed
- --source-key flag for restoring backups encrypted with a different key — cross-environment restore scenario
- --plaintext flag for exporting unencrypted backups — debugging use case

</deferred>

---

*Phase: 25-sqlcipher-database-encryption*
*Context gathered: 2026-03-03*
