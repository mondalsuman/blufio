# Architecture Patterns: v1.2 Production Hardening

**Domain:** Production hardening features for existing Rust AI agent platform
**Researched:** 2026-03-03
**Overall confidence:** HIGH (direct codebase analysis + verified library docs)

## Executive Summary

Five features need to integrate into the existing 16-crate Rust workspace: sd_notify (systemd readiness), SQLCipher (database encryption at rest), Minisign (binary signature verification), self-update (binary replacement with rollback), and backup integrity verification. Each feature has a different integration surface. The critical architectural decision is SQLCipher key management -- the encryption key must be applied as the absolute first statement on every database connection, which changes the connection path used by 6+ crates. All other features are isolated to the binary crate with minimal cross-cutting impact.

No new crates are needed. All features integrate into existing crates, primarily the binary crate (`crates/blufio/`), with SQLCipher requiring modifications to `blufio-storage` and `blufio-config`.

## Feature Integration Map

### 1. sd_notify -- systemd Type=notify + Watchdog

**Integration surface:** Binary crate only (`crates/blufio/src/serve.rs`)
**New crate needed:** No
**Existing modifications:** `serve.rs`, `deploy/blufio.service`

#### Where It Fits

The current `serve.rs::run_serve()` has a clear integration point. The startup sequence is:

```
1. init_tracing
2. plugin registry
3. vault startup check
4. storage initialization
5. cost ledger, budget tracker
6. context engine, memory, tools, MCP
7. provider initialization
8. channels, mux.connect()
9. signal handler install
10. agent loop run
```

`sd_notify::notify(true, &[NotifyState::Ready])` goes **after step 8** (mux.connect()) and **before step 10** (agent loop). This is the moment the service is truly ready to accept messages. Specifically, after line 503 in serve.rs where the log reads `"channel multiplexer connected"`.

#### Watchdog Ping Location

The existing `memory_monitor` loop in `serve.rs` (line 756) runs every 5 seconds with a `cancel` token. The watchdog ping belongs in this same loop -- it represents "the service is alive and processing." Adding `sd_notify::notify(false, &[NotifyState::Watchdog])` inside the memory_monitor tick is the minimal-diff approach.

The systemd service file has `WatchdogSec=300` (5 minutes). The memory_monitor ticks at 5s. This provides a 60x safety margin. No separate watchdog task needed.

#### Service File Changes

```ini
# deploy/blufio.service
# BEFORE
Type=simple

# AFTER
Type=notify
```

The existing `WatchdogSec=300` is already present. No other service file changes needed.

#### Stopping Notification

Add `sd_notify::notify(false, &[NotifyState::Stopping])` in the signal handler (currently in `blufio-agent/src/shutdown.rs::install_signal_handler()`), immediately after receiving SIGTERM/SIGINT and before cancelling the token. This tells systemd the service is intentionally shutting down.

#### Library Choice

**Use `sd-notify` 0.4.x** (crate name: `sd-notify`). Pure Rust, zero dependencies, MIT/Apache-2.0.

```rust
// After initialization complete (serve.rs)
sd_notify::notify(true, &[NotifyState::Ready]);

// In watchdog loop (serve.rs memory_monitor)
sd_notify::notify(false, &[NotifyState::Watchdog]);

// During shutdown (shutdown.rs signal handler)
sd_notify::notify(false, &[NotifyState::Stopping]);
```

On non-systemd platforms (macOS dev), these are silent no-ops (no NOTIFY_SOCKET = no effect). No `#[cfg(target_os = "linux")]` guards needed.

**Confidence:** HIGH -- docs.rs verified, pure Rust, zero-dependency crate.

---

### 2. SQLCipher -- Database Encryption at Rest

**Integration surface:** `blufio-storage`, `blufio-config`, binary crate, all crates that open DB connections
**New crate needed:** No
**Existing modifications:** Workspace `Cargo.toml`, `blufio-storage/Cargo.toml`, `blufio-storage/src/database.rs`, `blufio-config/src/model.rs`, `crates/blufio/src/serve.rs`, `crates/blufio/src/backup.rs`, `crates/blufio/src/main.rs`

#### The Critical Constraint: PRAGMA key MUST Be First

SQLCipher requires `PRAGMA key = '...'` as the **absolute first statement** on any database connection. Before WAL mode, before `PRAGMA synchronous`, before `PRAGMA foreign_keys` -- before everything. If any read or write occurs before keying, SQLCipher treats the file as unencrypted and either creates an unencrypted database or fails with "file is not a database."

This directly impacts `Database::open()` in `crates/blufio-storage/src/database.rs`, which currently does (line 57-64):

```rust
// Current order:
conn.execute_batch("PRAGMA journal_mode = WAL;");
conn.execute_batch("PRAGMA synchronous = NORMAL; ...");
```

**Required new order:**

```rust
// 1. PRAGMA key (MUST be first, before any other statement)
if let Some(ref key) = encryption_key {
    conn.execute_batch(&format!("PRAGMA key = \"x'{key}'\";"))?;
}
// 2. WAL mode (safe after keying)
conn.execute_batch("PRAGMA journal_mode = WAL;")?;
// 3. All other PRAGMAs
conn.execute_batch("PRAGMA synchronous = NORMAL; ...")?;
```

Note: WAL mode is persistent in the database header. Once set, subsequent opens automatically use WAL mode. But PRAGMA key must still be first on every connection.

#### Key Management Decision: Raw Hex Key via BLUFIO_DB_KEY

Three options were evaluated:

| Option | Verdict | Rationale |
|--------|---------|-----------|
| Raw hex key via env var | **RECOMMENDED** | SQLCipher skips internal PBKDF2 (256K iterations SHA-512) for raw keys. No startup latency. Matches existing `BLUFIO_VAULT_KEY` pattern. `EnvironmentFile=` in systemd keeps it off disk. |
| Passphrase via config/env | Rejected | SQLCipher runs PBKDF2 internally adding ~200ms per connection. 5+ connections at startup = 1+ second overhead. Confusion with vault's Argon2id KDF. |
| Key from vault | Rejected | Circular dependency: vault lives in same SQLite DB. Would need the DB key to open the DB to get the DB key. Solvable with two-phase open but adds complexity for no benefit. |

The key flows: `BLUFIO_DB_KEY` env var -> figment env provider -> `StorageConfig.encryption_key` -> `Database::open()` -> `PRAGMA key`.

Format: `BLUFIO_DB_KEY=2DD29CA851E7B56E4697B0E1F08507293D761A05CE4D1B628663F411A8086D99` (64 hex chars = 32 bytes). The `x'...'` wrapper is added by the code, not the operator.

#### Does SQLCipher Change the StorageAdapter Trait?

**No.** The `StorageAdapter` trait (`crates/blufio-core/src/traits/storage.rs`) is unchanged. Encryption is transparent to all consumers -- `create_session()`, `insert_message()`, etc. work identically. The only change is inside `Database::open()` which gains an optional key parameter.

#### Database::open() Signature Change

```rust
// BEFORE (database.rs line 36)
pub async fn open(path: &str) -> Result<Self, BlufioError>

// AFTER
pub async fn open(path: &str, encryption_key: Option<&str>) -> Result<Self, BlufioError>
```

#### StorageConfig Addition

```rust
// blufio-config/src/model.rs -- add field to StorageConfig
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    #[serde(default = "default_database_path")]
    pub database_path: String,
    #[serde(default = "default_wal_mode")]
    pub wal_mode: bool,
    /// Raw hex encryption key for SQLCipher. When set, database is encrypted at rest.
    /// Typically sourced from BLUFIO_DB_KEY environment variable, not config file.
    #[serde(default)]
    pub encryption_key: Option<String>,
}
```

#### The Multi-Connection Problem

The codebase opens **6 independent connections** to the same database file:

| Connection | Crate | Current Code Pattern |
|------------|-------|---------------------|
| Main storage | `blufio-storage` | `Database::open(&path)` in `adapter.rs` line 92 |
| Vault | `blufio-vault` | `tokio_rusqlite::Connection::open()` in `serve.rs` line 91 |
| Cost ledger | `blufio-cost` | `CostLedger::open()` in `serve.rs` line 158 |
| Memory store | `blufio-memory` | `tokio_rusqlite::Connection::open()` in `serve.rs` line 721 |
| MCP pin store | `blufio-mcp-client` | `PinStore::open()` in `serve.rs` line 221 |
| Skill store | `blufio-skill` | `tokio_rusqlite::Connection::open()` in `main.rs` line 387+ |

**Every connection must issue `PRAGMA key` before any operation.** If even one connection path skips it, that connection fails with "file is not a database" errors.

**Solution: Shared connection factory in `blufio-storage`:**

```rust
// blufio-storage/src/database.rs (new public function)

/// Open a raw tokio-rusqlite connection with encryption key and standard PRAGMAs.
///
/// Use this instead of `tokio_rusqlite::Connection::open()` directly.
/// Applies PRAGMA key (if encryption_key is Some), then WAL mode and
/// standard performance PRAGMAs.
pub async fn open_connection(
    path: &str,
    encryption_key: Option<&str>,
) -> Result<tokio_rusqlite::Connection, BlufioError> {
    let conn = tokio_rusqlite::Connection::open(path).await
        .map_err(|e| BlufioError::Storage { source: Box::new(e) })?;

    let key_owned = encryption_key.map(|k| k.to_string());
    conn.call(move |conn| {
        // PRAGMA key MUST be first
        if let Some(ref key) = key_owned {
            conn.execute_batch(&format!("PRAGMA key = \"x'{key}'\""))?;
        }
        conn.execute_batch("PRAGMA journal_mode = WAL;")?;
        conn.execute_batch(
            "PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA foreign_keys = ON;
             PRAGMA cache_size = -16000;
             PRAGMA temp_store = MEMORY;",
        )?;
        Ok(())
    }).await.map_err(|e| BlufioError::Storage { source: Box::new(e) })?;

    Ok(conn)
}
```

All 6 connection sites must switch from raw `tokio_rusqlite::Connection::open()` to `blufio_storage::open_connection()`. This is the most invasive change and requires touching code in vault, cost, memory, mcp-client, and skill crates (or their callsites in serve.rs/main.rs).

#### Feature Flag: Conditional SQLCipher Compilation

Use a Cargo feature flag `encryption` (default: off) to gate SQLCipher:

```toml
# Workspace Cargo.toml -- default: plain SQLite
[workspace.dependencies]
rusqlite = { version = "0.37", features = ["bundled"] }

# To enable SQLCipher, change to:
# rusqlite = { version = "0.37", features = ["bundled-sqlcipher-vendored-openssl"] }
```

The `bundled-sqlcipher-vendored-openssl` feature vendors OpenSSL, avoiding system library dependencies. Critical for the musl static binary deployment model.

**Build impact:** SQLCipher + vendored OpenSSL adds ~30-60 seconds to compile time and ~2-5 MB to binary size.

When the `encryption` feature is off and no key is provided, `PRAGMA key` is simply not issued. The code path is identical to current behavior. When a key is provided but SQLCipher is not compiled in (plain SQLite), `PRAGMA key` is a no-op -- SQLite ignores unknown PRAGMAs. However, the database will not actually be encrypted. A startup check should warn about this.

#### Migration: Plaintext to Encrypted

Existing deployments have unencrypted databases. SQLCipher provides `sqlcipher_export()`:

```sql
ATTACH DATABASE 'encrypted.db' AS encrypted KEY 'x''...''';
SELECT sqlcipher_export('encrypted');
DETACH DATABASE encrypted;
-- Then: mv encrypted.db blufio.db
```

This should be an explicit `blufio migrate-db` CLI subcommand, NOT automatic migration. Automatic migration risks data loss if interrupted.

**Confidence:** HIGH -- SQLCipher PRAGMA ordering verified via official Zetetic docs. rusqlite `bundled-sqlcipher` feature verified on docs.rs for version 0.37.0.

---

### 3. Minisign -- Binary Signature Verification

**Integration surface:** Binary crate only (new `update.rs` module)
**New crate needed:** No (code in binary crate)
**Existing modifications:** `crates/blufio/Cargo.toml` (add `minisign-verify` dep)

#### Where It Fits

Minisign verification is used exclusively during the self-update flow. It verifies that a downloaded binary was signed by the project's release key before replacing the running binary.

```
1. Download new binary to temp file
2. Download .minisig signature file
3. Load embedded public key (compiled into binary)
4. Verify: public_key.verify(&binary_bytes, &signature, false)
5. If valid -> proceed with self-replace
6. If invalid -> abort, delete temp files, report error
```

#### Public Key Embedding

The Minisign public key is **compiled into the binary** as a constant:

```rust
// crates/blufio/src/update.rs
const RELEASE_PUBLIC_KEY: &str = "RWSomeBase64KeyHere==";
```

This prevents MITM attacks where both binary and signature could be replaced. The key is trusted because it ships with the binary.

#### Library Choice

**Use `minisign-verify` 0.2.x** -- Zero dependencies, verify-only (no signing capability), MIT/Apache-2.0. By Frank Denis (libsodium author).

API:
```rust
let pk = PublicKey::from_base64(RELEASE_PUBLIC_KEY)?;
let sig = Signature::decode(&sig_text)?;
pk.verify(&binary_bytes, &sig, false)?; // false = allow both standard and prehashed
```

**Confidence:** HIGH -- docs.rs API verified, zero-dependency crate.

---

### 4. Self-Update -- `blufio update` with Rollback

**Integration surface:** Binary crate (new `update.rs` module, new CLI subcommand)
**New crate needed:** No
**Existing modifications:** `crates/blufio/src/main.rs` (add `Update` command), `crates/blufio/Cargo.toml`

#### Architecture

```
blufio update [--check] [--force]
  |
  v
1. Fetch latest release metadata from GitHub Releases API
   GET https://api.github.com/repos/{owner}/{repo}/releases/latest
  |
  v
2. Compare semver: current (from Cargo.toml) vs latest
   current >= latest && !force -> "already up to date", exit
  |
  v
3. Download binary for current platform
   Filter release assets by target triple (x86_64-unknown-linux-musl)
  |
  v
4. Download .minisig signature
   GET {binary_url}.minisig
  |
  v
5. Verify signature (Minisign -- Phase 4 dependency)
   If invalid -> abort, clean up temp files
  |
  v
6. Create rollback backup
   cp /usr/local/bin/blufio /usr/local/bin/blufio.rollback
  |
  v
7. Replace binary atomically (self-replace)
   rename() on Linux -- atomic
  |
  v
8. Print: "Updated v{old} -> v{new}. Restart to apply."
   "Rollback: blufio update --rollback"
```

#### Rollback Strategy

File-based rollback: before replacing, copy the current binary to `{path}.rollback`.

```bash
# Manual rollback (if new version has issues)
cp /usr/local/bin/blufio.rollback /usr/local/bin/blufio
systemctl restart blufio
```

Add a `--rollback` flag to automate this:

```rust
/// Check for and install binary updates.
Update {
    /// Only check for updates, don't install.
    #[arg(long)]
    check: bool,
    /// Force update even if already on latest version.
    #[arg(long)]
    force: bool,
    /// Restore the previous binary version from rollback backup.
    #[arg(long)]
    rollback: bool,
},
```

Automatic crash-detection rollback (new version fails to start) would require a supervisor pattern. Out of scope for v1.2 -- manual rollback is sufficient.

#### Library Choices

- **`self-replace` 1.3.x** -- Atomic binary replacement via `rename()`. Pure Rust.
- **`reqwest` (already in workspace)** -- HTTP downloads. Already used everywhere.
- **`semver` (already in workspace)** -- Version comparison. Already a dependency.

The higher-level `self_update` crate is NOT needed because it pulls in archive extraction, GitHub API wrappers, and other dependencies. We control the release format (bare binary + .minisig), so a lean implementation using reqwest + minisign-verify + self-replace is simpler and auditable.

#### Where Code Lives

All update logic in `crates/blufio/src/update.rs`. This is binary-distribution-specific logic. Not a library concern.

**Confidence:** MEDIUM -- self-replace crate verified on docs.rs. GitHub Releases API well-known. Specific release asset naming convention (target triple in filename) needs to be established as part of CI/CD.

---

### 5. Backup Integrity Verification

**Integration surface:** Binary crate (`crates/blufio/src/backup.rs`, `crates/blufio/src/doctor.rs`)
**New crate needed:** No
**Existing modifications:** `backup.rs` (add integrity check), `doctor.rs` (SQLCipher awareness)

#### Where It Fits

The existing `backup.rs` has `run_backup()` and `run_restore()`. Integrity check goes at the end of each, after `backup.run_to_completion()`:

```rust
// After backup completes (backup.rs ~line 56)
verify_integrity(backup_path, encryption_key.as_deref())?;

// After restore completes (backup.rs ~line 125)
verify_integrity(db_path, encryption_key.as_deref())?;
```

#### Implementation

```rust
/// Run PRAGMA integrity_check on a database file.
///
/// When SQLCipher is enabled, applies PRAGMA key first.
fn verify_integrity(db_path: &str, encryption_key: Option<&str>) -> Result<(), BlufioError> {
    let conn = Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ).map_err(|e| BlufioError::Storage { source: Box::new(e) })?;

    // PRAGMA key must be first if encrypted
    if let Some(key) = encryption_key {
        conn.execute_batch(&format!("PRAGMA key = \"x'{key}'\""))
            .map_err(|e| BlufioError::Storage { source: Box::new(e) })?;
    }

    let mut stmt = conn.prepare("PRAGMA integrity_check")
        .map_err(|e| BlufioError::Storage { source: Box::new(e) })?;
    let results: Vec<String> = stmt.query_map([], |row| row.get(0))
        .map_err(|e| BlufioError::Storage { source: Box::new(e) })?
        .filter_map(|r| r.ok())
        .collect();

    if results.len() == 1 && results[0] == "ok" {
        eprintln!("Integrity check: OK");
        Ok(())
    } else {
        Err(BlufioError::Storage {
            source: format!(
                "Integrity check failed: {}",
                results.join(", ")
            ).into(),
        })
    }
}
```

Note: This uses synchronous `rusqlite::Connection` (not tokio-rusqlite) because backup.rs already uses synchronous rusqlite for the backup API. The integrity check runs on the same thread.

#### Doctor Integration

The existing `check_db_integrity()` in `doctor.rs` (line 373) already runs `PRAGMA integrity_check`. The only change needed is applying `PRAGMA key` first when the encryption key is available. The encryption key should come from the config (which loads from `BLUFIO_DB_KEY` env var).

**Confidence:** HIGH -- `PRAGMA integrity_check` is standard SQLite. Already implemented in doctor.rs deep checks.

---

## Component Dependency Graph

```
                  +------------------+
                  |   blufio (bin)   |
                  +------------------+
                  | serve.rs         |  <-- sd_notify Ready + Watchdog
                  | backup.rs        |  <-- integrity_check post backup/restore
                  | update.rs (NEW)  |  <-- self-update + minisign verify
                  | doctor.rs        |  <-- SQLCipher-aware integrity check
                  | main.rs          |  <-- new Update command, migrate-db command
                  +--------+---------+
                           |
              +------------+-------------+
              |                          |
    +---------v--------+      +----------v---------+
    | blufio-storage   |      | blufio-config      |
    +------------------+      +--------------------+
    | database.rs      |      | model.rs           |
    |  PRAGMA key FIRST|      |  StorageConfig     |
    |  open_connection()|     |   +encryption_key  |
    +--------+---------+      +--------------------+
             |
             | (shared connection factory used by all DB consumers)
             |
    +--------+--------+--------+--------+--------+
    |        |        |        |        |        |
    v        v        v        v        v        v
  vault    cost    memory  mcp-client  skill   backup
```

## Suggested Build Order

Features are ordered by dependency chain and risk:

### Phase 1: Backup Integrity Verification

**Scope:** Add `PRAGMA integrity_check` to `run_backup()` and `run_restore()` in `backup.rs`.
**Dependencies:** None. Zero new crate dependencies.
**Risk:** Minimal -- modifies existing code with a pure addition.
**Rationale:** Standalone, no dependencies on other features, validates the pattern for database operations needed by SQLCipher phase.

### Phase 2: sd_notify Integration

**Scope:** Add `sd-notify` crate. Insert `NotifyState::Ready` in `serve.rs`. Add `NotifyState::Watchdog` in memory_monitor. Add `NotifyState::Stopping` in shutdown handler. Update service file Type=simple -> Type=notify.
**Dependencies:** None (sd-notify has zero deps).
**Risk:** Low -- 3 lines of code in serve.rs, 1 line in service file.
**Rationale:** Tiny integration surface, no impact on other crates, immediately testable.

### Phase 3: SQLCipher Database Encryption

**Scope:** Highest complexity feature. Touches multiple crates.
**Dependencies:** Phase 1 (backup integrity needed for safe migration testing).
**Risk:** HIGH -- modifies the database connection path used by 6+ consumers.

Sub-phases within Phase 3:

1. **Config:** Add `encryption_key` field to `StorageConfig` in `blufio-config/src/model.rs`
2. **Storage core:** Modify `Database::open()` to accept optional key. Add `open_connection()` public helper.
3. **Connection callers:** Update all raw `tokio_rusqlite::Connection::open()` callsites in `serve.rs` and `main.rs` to use `open_connection()`
4. **Backup awareness:** Wire encryption key into `backup.rs` integrity check and backup/restore flows
5. **Feature flag:** Add `encryption` feature that switches `rusqlite` from `bundled` to `bundled-sqlcipher-vendored-openssl`
6. **Migration CLI:** Add `blufio migrate-db` subcommand for plaintext-to-encrypted migration
7. **Testing:** All existing tests pass with both encrypted and unencrypted databases

### Phase 4: Minisign Signature Verification

**Scope:** Add `minisign-verify` crate. Create `crates/blufio/src/update.rs` with verification functions. Embed public key constant.
**Dependencies:** None.
**Risk:** Low -- new module, no existing code changes.
**Rationale:** Prerequisite for self-update. Verification logic tested independently before update flow.

### Phase 5: Self-Update with Rollback

**Scope:** Add `self-replace` crate. Complete `update.rs` with download, verify, backup, replace flow. Add `Update` CLI subcommand.
**Dependencies:** Phase 4 (Minisign verification).
**Risk:** Medium -- external integration (GitHub API, HTTP downloads, file operations).
**Rationale:** Last because it depends on Minisign and is the highest external-integration complexity.

### Phase Ordering Rationale

```
Phase 1 (backup integrity) ---> Phase 3 (SQLCipher needs safe migration)
Phase 2 (sd_notify)        ---> independent, low risk warm-up
Phase 4 (minisign)         ---> Phase 5 (self-update needs verification)
```

- Backup integrity first because SQLCipher migration needs it for safety
- sd_notify second as easy confidence builder
- SQLCipher third because it is the highest-risk feature and dominates the milestone
- Minisign fourth as standalone preparation for self-update
- Self-update last because it depends on Minisign and has the most external integration

## Crate Modification Summary

| Crate | Modified Files | Change Type | Phase |
|-------|---------------|-------------|-------|
| blufio (binary) | backup.rs | Add verify_integrity() call | 1 |
| blufio (binary) | serve.rs | sd_notify Ready + Watchdog | 2 |
| blufio (binary) | serve.rs | Pass encryption_key to connections | 3 |
| blufio (binary) | main.rs | migrate-db subcommand, Update subcommand | 3, 5 |
| blufio (binary) | doctor.rs | SQLCipher-aware integrity check | 3 |
| blufio (binary) | update.rs (NEW) | Self-update + Minisign verification | 4, 5 |
| blufio-storage | database.rs | PRAGMA key ordering, open_connection() helper | 3 |
| blufio-config | model.rs | encryption_key field in StorageConfig | 3 |
| blufio-agent | shutdown.rs | NotifyState::Stopping | 2 |
| deploy/ | blufio.service | Type=notify | 2 |

Crates that are **NOT modified** but whose callsites in serve.rs/main.rs change:
- blufio-vault (connection opened in serve.rs)
- blufio-cost (CostLedger::open() called in serve.rs)
- blufio-memory (connection opened in serve.rs)
- blufio-mcp-client (PinStore::open() called in serve.rs)

## Anti-Patterns to Avoid

### Anti-Pattern 1: Passphrase-Based SQLCipher Key
**What:** Using a passphrase for PRAGMA key instead of a raw hex key.
**Why bad:** SQLCipher runs 256K iterations of PBKDF2-SHA512 per connection open. With 5+ connections at startup, that adds 1+ second. The vault already has its own Argon2id KDF. Double-KDF is wasteful and confusing.
**Instead:** Raw 32-byte hex key via `BLUFIO_DB_KEY`. SQLCipher skips PBKDF2 for raw keys.

### Anti-Pattern 2: Automatic Plaintext-to-Encrypted Migration
**What:** Automatically migrating an unencrypted DB to encrypted on first start with a key.
**Why bad:** If interrupted (power loss, OOM kill), data loss. If the key is wrong or lost, old data gone.
**Instead:** Explicit `blufio migrate-db` command with confirmation. Keeps old file as backup.

### Anti-Pattern 3: Separate Crate for sd_notify
**What:** Creating a `blufio-systemd` crate for sd_notify integration.
**Why bad:** sd_notify is 3 function calls total. A separate crate is over-engineering.
**Instead:** Direct calls in serve.rs and shutdown.rs.

### Anti-Pattern 4: Self-Update Logic in a Library Crate
**What:** Creating `blufio-update` as a library crate.
**Why bad:** Self-update is binary-specific (GitHub releases, platform detection, binary replacement). No other crate needs this.
**Instead:** `crates/blufio/src/update.rs` module in binary crate.

### Anti-Pattern 5: Skipping PRAGMA key on Any Connection
**What:** Some connection paths use the helper, others use raw `tokio_rusqlite::Connection::open()`.
**Why bad:** One skipped connection = runtime "file is not a database" errors. Extremely hard to debug in production.
**Instead:** CI enforcement: grep for raw `Connection::open()` calls. All must go through the storage helper.

### Anti-Pattern 6: Running Minisign Verification After Binary Replacement
**What:** Replace binary first, then verify signature.
**Why bad:** If verification fails, the replaced binary may be malicious and already on disk.
**Instead:** Always verify BEFORE replacing. Download to temp dir, verify in-place, then atomic replace.

## Data Flow Changes

### Before (v1.1)

```
serve.rs -> SqliteStorage::new(config) -> Database::open(path)
                                            |-> PRAGMA journal_mode = WAL
                                            |-> PRAGMA synchronous = NORMAL
                                            |-> run_migrations()

serve.rs -> tokio_rusqlite::Connection::open(path)  [vault, memory, cost, mcp]

backup.rs -> rusqlite::Backup API -> done (no integrity check)
```

### After (v1.2)

```
serve.rs -> SqliteStorage::new(config) -> Database::open(path, key)
                                            |-> PRAGMA key = x'...'  (IF key)
                                            |-> PRAGMA journal_mode = WAL
                                            |-> PRAGMA synchronous = NORMAL
                                            |-> run_migrations()

serve.rs -> blufio_storage::open_connection(path, key)  [vault, memory, cost, mcp]
              |-> PRAGMA key = x'...'  (IF key)
              |-> PRAGMA journal_mode = WAL
              |-> standard PRAGMAs

serve.rs -> sd_notify::notify(Ready)     [after mux.connect(), before agent_loop.run()]
serve.rs -> memory_monitor loop:
              |-> jemalloc stats
              |-> sd_notify::notify(Watchdog)  [every 5s tick]

shutdown.rs -> sd_notify::notify(Stopping)  [on SIGTERM/SIGINT, before cancel]

backup.rs -> run_backup() -> verify_integrity(backup_path, key)
backup.rs -> run_restore() -> verify_integrity(db_path, key)

main.rs -> Commands::Update
             -> update::check_for_update()       [GitHub API]
             -> update::download_binary()         [reqwest]
             -> update::download_signature()      [reqwest]
             -> update::verify_signature()        [minisign-verify]
             -> fs::copy(current, current.rollback)
             -> self_replace::self_replace(new)
```

## New Dependencies Summary

| Crate | Version | Purpose | Size Impact | Transitive Deps |
|-------|---------|---------|-------------|-----------------|
| sd-notify | 0.4.x | systemd readiness + watchdog | ~10 KB | 0 |
| minisign-verify | 0.2.x | Release signature verification | ~15 KB | 0 |
| self-replace | 1.3.x | Atomic binary replacement | ~10 KB | 0 |

**Total new dependencies: 3 crates, 0 transitive dependencies, ~35 KB binary impact.**

SQLCipher does not add a new crate -- it changes the feature flag on the existing `rusqlite` from `bundled` to `bundled-sqlcipher-vendored-openssl`. This adds vendored OpenSSL (~2-5 MB binary increase, ~30-60s compile time increase).

## Scalability Considerations

| Concern | Impact | Notes |
|---------|--------|-------|
| SQLCipher I/O overhead | ~5-15% per page read/write | AES-256 encrypt/decrypt. Negligible for I/O-bound AI agent (LLM latency dwarfs DB latency). |
| SQLCipher startup cost | Near zero with raw hex key | PBKDF2 completely skipped. No measurable startup difference. |
| Backup + integrity check | +2-5 seconds per backup | integrity_check reads every DB page. Acceptable for manual CLI operation. |
| Self-update download | 25-50 MB network transfer | Same as manual download. Not a hot path. |
| Watchdog overhead | Negligible | One `sendmsg()` syscall every 5s. Unmeasurable. |

## Sources

- [sd-notify crate docs](https://docs.rs/sd-notify) -- HIGH confidence
- [sd-notify on lib.rs](https://lib.rs/crates/sd-notify) -- HIGH confidence
- [systemd sd_notify(3) man page](https://www.freedesktop.org/software/systemd/man/latest/sd_notify.html) -- HIGH confidence
- [SQLCipher API documentation (Zetetic)](https://www.zetetic.net/sqlcipher/sqlcipher-api/) -- HIGH confidence
- [rusqlite SQLCipher issue #219](https://github.com/rusqlite/rusqlite/issues/219) -- MEDIUM confidence
- [rusqlite 0.37.0 features](https://docs.rs/crate/rusqlite/0.37.0/features) -- HIGH confidence
- [minisign-verify crate docs](https://docs.rs/minisign-verify/latest/minisign_verify/) -- HIGH confidence
- [minisign-verify GitHub](https://github.com/jedisct1/rust-minisign-verify) -- HIGH confidence
- [self-replace crate docs](https://docs.rs/self-replace/latest/self_replace/) -- HIGH confidence
- [self_update crate](https://github.com/jaemk/self_update) -- MEDIUM confidence (evaluated, not recommended)
- Blufio codebase analysis (16 crates, serve.rs, backup.rs, database.rs, vault.rs, model.rs examined directly) -- HIGH confidence
