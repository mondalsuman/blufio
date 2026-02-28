# Phase 2: Persistence & Security Vault - Research

**Researched:** 2026-02-28
**Domain:** SQLite persistence, credential encryption (AES-256-GCM + Argon2id), network security hardening
**Confidence:** HIGH

## Summary

Phase 2 establishes two foundational subsystems: a SQLite-based persistence layer (sessions, messages, queue) and an encrypted credential vault (AES-256-GCM with Argon2id key derivation). Both are well-served by mature, audited Rust crates. The SQLite story in Rust is straightforward -- `rusqlite` with the `bundled` feature compiles SQLite statically into the binary, `tokio-rusqlite` wraps it for async use with a dedicated background thread (which naturally enforces the single-writer pattern required by PERS-05), and `refinery` embeds SQL migrations at compile time so the binary is fully self-contained. For encryption, `ring` (0.17.14) provides audited AES-256-GCM and the RustCrypto `argon2` crate (0.5.3) handles Argon2id key derivation with tunable parameters. The `secrecy` crate (0.10.3) wraps sensitive values with `Zeroize`-on-drop and `[[REDACTED]]` Debug output, preventing accidental log leakage. SSRF prevention requires a custom `reqwest` DNS resolver that checks resolved IPs against private ranges before connecting. All libraries are production-grade, actively maintained, and have established patterns in the Rust ecosystem.

**Primary recommendation:** Use `tokio-rusqlite` (single background thread = single writer) for all database access, `refinery` for compile-time embedded migrations, `ring` for AES-256-GCM, the RustCrypto `argon2` crate for Argon2id KDF, and `secrecy` + custom tracing Layer for secret redaction.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Both passphrase prompt (default) and environment variable (`BLUFIO_VAULT_KEY`) for headless deployments
- Passphrase prompt on interactive TTY, env var detected automatically for unattended startup (systemd, Docker)
- Vault created lazily on first `blufio config set-secret` call -- no upfront vault initialization step
- Agent fails to start with clear error if vault exists but is not unlocked -- no degraded/partial operation
- Key wrapping pattern: master key encrypted by passphrase-derived key (Argon2id). Changing passphrase re-wraps master key, does not re-encrypt all secrets
- `blufio config set-secret <key>` CLI command for adding/updating secrets
- Hidden prompt (no echo) for interactive use, stdin pipe support for scripting -- TTY detection selects mode automatically
- `blufio config list-secrets` shows names + masked preview (e.g., `sk-...4f2b`) -- values never fully displayed
- Auto-migrate plaintext secrets found in TOML config into vault on startup, remove from config file, warn user about migration
- Default location: XDG data directory (`~/.local/share/blufio/blufio.db` on Linux), configurable via `storage.database_path`
- Embedded SQL migrations compiled into binary, auto-applied on startup -- zero manual database operations
- Fully automatic: create parent directories + database file on first startup if they don't exist
- Schema version tracked in `_migrations` table
- Localhost connections (127.0.0.1/::1) exempt from TLS requirement automatically; all remote connections require TLS -- no config toggle needed
- Secret redaction: known patterns (API key prefixes, Bearer tokens, common formats) + all vault-stored values redacted from logs
- SSRF prevention: private IP ranges blocked by default, allowlist via `security.allowed_private_ips` config for explicit local service access, all allowlisted connections logged
- Security violations (TLS failure, SSRF blocked, invalid credential) hard-fail the operation immediately and log at ERROR level -- no silent swallowing, no fallback

### Claude's Discretion
- Single-writer concurrency pattern implementation (dedicated writer thread vs. connection pool) -- goal is zero SQLITE_BUSY errors per PERS-05
- Argon2id parameter tuning (memory cost, iterations, parallelism)
- Exact secret pattern regex for redaction
- Migration file organization and naming convention
- Database table schema design for sessions, messages, and queue

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| PERS-01 | All state stored in single SQLite database with WAL mode and ACID transactions | `rusqlite` with `bundled` feature, WAL mode via PRAGMA, `tokio-rusqlite` for async access. Standard PRAGMA set documented below. |
| PERS-02 | Sessions persist across restarts -- user can resume conversation after reboot | SQLite WAL mode with `synchronous=NORMAL` ensures durability. Schema design for sessions table with `refinery` migrations. |
| PERS-03 | Message queue is SQLite-backed and crash-safe -- zero message loss on crash | WAL mode + ACID transactions guarantee crash safety. Queue table with status column pattern documented below. |
| PERS-04 | Backup is `cp blufio.db blufio.db.bak` -- single file, no coordination needed | WAL mode checkpoint on shutdown ensures single-file backup works. SQLite WAL checkpoint API available via `wal_checkpoint`. |
| PERS-05 | Single-writer-thread pattern prevents SQLITE_BUSY under concurrent sessions | `tokio-rusqlite` uses a single dedicated background thread with mpsc channel -- naturally serializes all writes. Recommendation: use single `tokio-rusqlite::Connection` for writes, optional reader pool via `deadpool-sqlite` if read performance matters. |
| SEC-01 | Binary binds to 127.0.0.1 by default -- no open ports to the internet | Already configured in `SecurityConfig` with `default_bind_address()` returning `"127.0.0.1"`. Enforce at server bind time. |
| SEC-03 | AES-256-GCM encrypted credential vault stores all API keys and bot tokens | `ring::aead::AES_256_GCM` for encryption. Key wrapping pattern: master key encrypted by Argon2id-derived key. Vault stored in SQLite `vault_entries` table. |
| SEC-04 | Vault key derived from passphrase via Argon2id -- never stored on disk | RustCrypto `argon2` crate with `Argon2id` variant. `hash_password_into()` for raw key derivation (32 bytes for AES-256). Salt stored in DB, derived key held only in memory wrapped by `secrecy::SecretBox`. |
| SEC-08 | Secrets redacted from all logs and persisted data before storage | `secrecy::SecretString` for in-memory secret types (Debug shows `[[REDACTED]]`). Custom tracing `Layer` with regex patterns for log output scrubbing. Known vault values added to redaction set dynamically. |
| SEC-09 | SSRF prevention (private IP blocking) enabled by default | Custom `reqwest::dns::Resolve` implementation that checks resolved IPs against RFC 1918/RFC 4193/link-local ranges. Allowlist via `security.allowed_private_ips` config field. |
| SEC-10 | TLS required for all remote connections | `reqwest` client built with `min_tls_version(tls::Version::TLS_1_2)`. Localhost detection skips TLS requirement. Enforce at HTTP client construction, not per-request. |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| [rusqlite](https://crates.io/crates/rusqlite) | 0.38 | SQLite bindings with bundled SQLite | De facto Rust SQLite library. 100M+ downloads. `bundled` feature compiles SQLite statically -- zero system dependencies. |
| [tokio-rusqlite](https://crates.io/crates/tokio-rusqlite) | 0.6 | Async wrapper for rusqlite via dedicated background thread | Single-thread-per-connection architecture naturally enforces single-writer pattern. Used in 41+ crates. `#![forbid(unsafe_code)]`. |
| [refinery](https://crates.io/crates/refinery) | 0.9 | Compile-time embedded SQL migrations | `embed_migrations!` macro bakes migrations into binary. Tracks versions in `refinery_schema_history` table. Supports rusqlite natively. |
| [ring](https://crates.io/crates/ring) | 0.17.14 | AES-256-GCM AEAD encryption | BoringSSL-derived, audited cryptography. Hardware-accelerated AES-NI. Industry standard for Rust crypto. |
| [argon2](https://crates.io/crates/argon2) | 0.5.3 | Argon2id key derivation | RustCrypto pure-Rust implementation. Supports Argon2id variant with tunable parameters. `hash_password_into()` for raw KDF output. |
| [secrecy](https://crates.io/crates/secrecy) | 0.10.3 | Secret value wrapper with Zeroize-on-drop | `SecretString`/`SecretBox` types. Debug shows `[[REDACTED]]`. `ExposeSecret` trait makes access explicit and auditable. Zeroize ensures memory cleanup. |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| [rpassword](https://crates.io/crates/rpassword) | 7.3 | Terminal password prompt with no-echo | Vault passphrase input. `read_password()` disables terminal echo. TTY detection built-in. |
| [zeroize](https://crates.io/crates/zeroize) | 1.8 | Memory zeroing for sensitive data | Transitive via `secrecy`. Use `Zeroizing<Vec<u8>>` for raw key material, derived keys, decrypted plaintext. |
| [dirs](https://crates.io/crates/dirs) | 6 | XDG directory paths | Already in workspace. `dirs::data_dir()` returns `~/.local/share` (Linux), `~/Library/Application Support` (macOS). |
| [rand](https://crates.io/crates/rand) | 0.8 | Cryptographic random number generation | Salt generation for Argon2id, nonce generation for AES-256-GCM. Use `OsRng` for cryptographic randomness. |
| [regex](https://crates.io/crates/regex) | 1 | Regular expression matching | Secret pattern matching for log redaction (API key prefixes, Bearer tokens, etc.). |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `tokio-rusqlite` (single connection) | `deadpool-sqlite` (connection pool) | Pool provides concurrent reads but does NOT solve single-writer. `tokio-rusqlite` single connection naturally serializes all writes. For this project, one write connection + optional read pool is the correct pattern. |
| `ring` for AES-256-GCM | `aes-gcm` (RustCrypto) | `aes-gcm` is pure Rust and NCC-audited. `ring` is BoringSSL-derived with hardware acceleration. Both are solid choices. `ring` chosen because it has fewer transitive dependencies and is already battle-tested in `rustls`. |
| `refinery` for migrations | `rusqlite_migration` | `rusqlite_migration` (v2.4) is simpler and rusqlite-specific. `refinery` is more mature with broader ecosystem support and better ergonomics via `embed_migrations!` macro. Either works -- `refinery` preferred for its compile-time embedding pattern. |
| Custom single-writer | `r2d2-sqlite` connection pool | Connection pools do not prevent SQLITE_BUSY. The `tokio-rusqlite` single-thread approach is architecturally simpler and provides a hard guarantee of serialized writes. |

**Installation:**
```bash
# Add to workspace Cargo.toml [workspace.dependencies]
rusqlite = { version = "0.38", features = ["bundled"] }
tokio-rusqlite = "0.6"
refinery = { version = "0.9", features = ["rusqlite"] }
ring = "0.17"
argon2 = "0.5"
secrecy = { version = "0.10", features = ["serde"] }
rpassword = "7"
zeroize = { version = "1.8", features = ["derive"] }
rand = "0.8"
regex = "1"
```

## Architecture Patterns

### Recommended Project Structure
```
crates/
├── blufio-storage/          # New crate: SQLite persistence layer
│   ├── src/
│   │   ├── lib.rs           # Public API, re-exports
│   │   ├── database.rs      # Database connection management, PRAGMA setup
│   │   ├── writer.rs        # Single-writer wrapper around tokio-rusqlite
│   │   ├── models.rs        # Row types: Session, Message, QueueEntry
│   │   ├── queries/         # Query modules
│   │   │   ├── mod.rs
│   │   │   ├── sessions.rs  # Session CRUD
│   │   │   ├── messages.rs  # Message CRUD
│   │   │   └── queue.rs     # Queue operations (enqueue, dequeue, ack)
│   │   └── migrations.rs    # refinery embed_migrations! + runner
│   ├── migrations/          # SQL migration files
│   │   ├── V1__initial_schema.sql
│   │   └── V2__queue_table.sql
│   └── Cargo.toml
├── blufio-vault/            # New crate: Encrypted credential vault
│   ├── src/
│   │   ├── lib.rs           # Public API
│   │   ├── vault.rs         # Vault operations (store, retrieve, list, delete)
│   │   ├── crypto.rs        # AES-256-GCM encrypt/decrypt, key wrapping
│   │   ├── kdf.rs           # Argon2id key derivation
│   │   ├── prompt.rs        # Passphrase prompt (TTY vs env var)
│   │   └── migration.rs     # Plaintext-to-vault auto-migration
│   └── Cargo.toml
├── blufio-security/         # New crate: Network security enforcement
│   ├── src/
│   │   ├── lib.rs           # Public API
│   │   ├── tls.rs           # TLS enforcement for reqwest clients
│   │   ├── ssrf.rs          # SSRF prevention DNS resolver
│   │   └── redact.rs        # Secret redaction tracing Layer + regex patterns
│   └── Cargo.toml
```

### Pattern 1: Single-Writer via tokio-rusqlite

**What:** All database writes go through a single `tokio-rusqlite::Connection` that runs on a dedicated background thread. This serializes all write operations and eliminates SQLITE_BUSY errors entirely.

**When to use:** Always -- this is the primary database access pattern.

**Example:**
```rust
// Source: tokio-rusqlite docs + rusqlite PRAGMA patterns
use tokio_rusqlite::Connection;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub async fn open(path: &str) -> Result<Self, BlufioError> {
        let conn = Connection::open(path).await.map_err(|e| {
            BlufioError::Storage { source: Box::new(e) }
        })?;

        // Apply PRAGMAs on the background thread
        conn.call(|conn| {
            conn.execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA busy_timeout = 5000;
                 PRAGMA foreign_keys = ON;
                 PRAGMA cache_size = -16000;
                 PRAGMA temp_store = MEMORY;"
            )?;
            Ok(())
        }).await.map_err(|e| BlufioError::Storage { source: Box::new(e) })?;

        Ok(Self { conn })
    }

    pub async fn insert_message(&self, msg: &Message) -> Result<(), BlufioError> {
        let msg = msg.clone();
        self.conn.call(move |conn| {
            conn.execute(
                "INSERT INTO messages (id, session_id, role, content, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![msg.id, msg.session_id, msg.role, msg.content, msg.created_at],
            )?;
            Ok(())
        }).await.map_err(|e| BlufioError::Storage { source: Box::new(e) })
    }
}
```

### Pattern 2: Key Wrapping for Vault

**What:** A random master key encrypts all secrets. The master key itself is encrypted ("wrapped") by a key derived from the user's passphrase via Argon2id. Changing the passphrase only re-wraps the master key, not every secret.

**When to use:** Vault initialization and passphrase changes.

**Example:**
```rust
// Source: ring AEAD docs + argon2 crate docs
use argon2::Argon2;
use ring::aead::{self, LessSafeKey, UnboundKey, Nonce, Aad, AES_256_GCM};
use secrecy::{ExposeSecret, SecretBox};
use zeroize::Zeroizing;

pub struct VaultKeys {
    /// The unwrapped master key -- held only in memory
    master_key: SecretBox<[u8; 32]>,
}

/// Derive a 32-byte key from passphrase using Argon2id
fn derive_key(passphrase: &[u8], salt: &[u8; 16]) -> Result<Zeroizing<[u8; 32]>, BlufioError> {
    let params = argon2::Params::new(65536, 3, 4, Some(32))
        .map_err(|e| BlufioError::Internal(format!("argon2 params: {e}")))?;
    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon2.hash_password_into(passphrase, salt, key.as_mut())
        .map_err(|e| BlufioError::Internal(format!("argon2 kdf: {e}")))?;
    Ok(key)
}

/// Encrypt data with AES-256-GCM
fn seal(key: &[u8; 32], nonce_bytes: &[u8; 12], plaintext: &[u8]) -> Result<Vec<u8>, BlufioError> {
    let unbound = UnboundKey::new(&AES_256_GCM, key)
        .map_err(|_| BlufioError::Internal("bad key".into()))?;
    let key = LessSafeKey::new(unbound);
    let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
        .map_err(|_| BlufioError::Internal("bad nonce".into()))?;
    let mut in_out = plaintext.to_vec();
    key.seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| BlufioError::Internal("seal failed".into()))?;
    Ok(in_out)
}
```

### Pattern 3: SSRF Prevention via Custom DNS Resolver

**What:** A custom `reqwest::dns::Resolve` implementation that resolves DNS normally, then filters out private/reserved IP addresses before allowing the connection.

**When to use:** All outbound HTTP requests from the agent.

**Example:**
```rust
// Source: reqwest dns::Resolve trait docs
use reqwest::dns::{Resolve, Resolving, Name};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub struct SsrfSafeResolver {
    allowed_private_ips: Vec<IpAddr>,
}

impl SsrfSafeResolver {
    fn is_private(ip: &IpAddr) -> bool {
        match ip {
            IpAddr::V4(v4) => {
                v4.is_private()           // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                || v4.is_loopback()       // 127.0.0.0/8
                || v4.is_link_local()     // 169.254.0.0/16
                || v4.is_broadcast()      // 255.255.255.255
                || v4.is_unspecified()    // 0.0.0.0
                // AWS metadata endpoint
                || *v4 == Ipv4Addr::new(169, 254, 169, 254)
            }
            IpAddr::V6(v6) => {
                v6.is_loopback()          // ::1
                || v6.is_unspecified()    // ::
                // Unique local (fc00::/7)
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                // Link-local (fe80::/10)
                || (v6.segments()[0] & 0xffc0) == 0xfe80
            }
        }
    }
}
```

### Pattern 4: Secret Redaction via Custom Tracing Layer

**What:** A custom `tracing_subscriber::Layer` that intercepts log events and replaces sensitive patterns with `[REDACTED]` before they reach the formatter.

**When to use:** Installed once at application startup as part of the tracing subscriber stack.

**Example:**
```rust
// Pattern: wrap the formatted output, not individual fields
// This is simpler and catches secrets that appear in any field
use tracing_subscriber::fmt;
use std::io::Write;

pub struct RedactingWriter<W> {
    inner: W,
    patterns: Vec<regex::Regex>,
    vault_values: Vec<String>, // dynamically added as secrets are loaded
}

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let s = String::from_utf8_lossy(buf);
        let mut redacted = s.into_owned();
        for pattern in &self.patterns {
            redacted = pattern.replace_all(&redacted, "[REDACTED]").into_owned();
        }
        for val in &self.vault_values {
            redacted = redacted.replace(val, "[REDACTED]");
        }
        self.inner.write_all(redacted.as_bytes())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
```

### Anti-Patterns to Avoid
- **Multiple SQLite connections for writes:** Even with WAL mode, concurrent writers cause SQLITE_BUSY. A connection pool does NOT solve this -- use a single write connection via `tokio-rusqlite`.
- **Storing the vault passphrase or derived key on disk:** The derived key must live only in process memory. Use `secrecy::SecretBox` and `zeroize` to ensure cleanup on drop.
- **String-level URL filtering for SSRF:** Checking URL strings for "localhost" or "127.0.0.1" is trivially bypassed (e.g., `0x7f000001`, `[::1]`, decimal encoding). Always filter at the IP level after DNS resolution.
- **Regex-only secret redaction:** Regex catches known patterns but misses vault-stored values with unusual formats. Combine regex patterns with exact-match replacement of all known vault values.
- **PRAGMA journal_mode in a transaction:** Setting `journal_mode = WAL` inside a transaction will silently fail. It must be the first statement on a fresh connection.
- **Forgetting per-connection PRAGMAs:** `busy_timeout`, `foreign_keys`, `synchronous`, and `cache_size` are connection-scoped. They must be set every time a connection is opened, not just once.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| AES-256-GCM encryption | Custom AES implementation | `ring::aead` | Constant-time, hardware-accelerated, audited. Nonce handling is subtle (reuse = catastrophic). |
| Password-to-key derivation | Custom stretching/hashing | `argon2` crate with Argon2id | Memory-hard KDF resists GPU/ASIC attacks. Parameter tuning is well-documented. |
| Secret memory lifecycle | Manual memset/drop | `secrecy` + `zeroize` | Compiler optimizes away naive zeroing. `zeroize` uses `write_volatile` + memory fences. |
| Terminal password input | Custom termios/echo manipulation | `rpassword` | Cross-platform (Unix, Windows, WASM). Handles edge cases around pipe detection and TTY modes. |
| SQL migrations | Custom version tracking | `refinery` | Handles version ordering, divergence detection, and grouped transactions. Battle-tested in production. |
| Private IP detection | Hardcoded IP range checks | `std::net::Ipv4Addr::is_private()` + comprehensive checks | Standard library provides `is_private()`, `is_loopback()`, `is_link_local()`. Must also check IPv6 unique-local and link-local ranges. |

**Key insight:** Cryptography and secret management have an asymmetric risk profile -- getting it 99% right is the same as getting it wrong. Use audited libraries and established patterns. The complexity is in the edge cases (nonce reuse, timing attacks, memory remnants) that are invisible in testing but exploitable in production.

## Common Pitfalls

### Pitfall 1: SQLITE_BUSY Despite WAL Mode
**What goes wrong:** Multiple threads/connections try to write simultaneously. Even with WAL mode, SQLite allows only one writer at a time. Under contention, `SQLITE_BUSY` errors appear despite setting `busy_timeout`.
**Why it happens:** WAL allows concurrent reads during writes, but writes are still serialized. `busy_timeout` is a wait-then-retry mechanism, not a queue -- under sustained write pressure, timeouts still occur.
**How to avoid:** Use a single `tokio-rusqlite::Connection` for all writes. All write operations are sent to a single background thread via mpsc channel, guaranteeing serialization. Reads can use the same connection (they queue behind writes) or a separate read-only connection.
**Warning signs:** Any code path that opens a second `Connection` or uses a connection pool for writes.

### Pitfall 2: Nonce Reuse in AES-GCM
**What goes wrong:** Reusing a nonce with the same key in AES-GCM completely breaks confidentiality and authenticity. An attacker can recover the authentication key and forge ciphertexts.
**Why it happens:** Random nonces have a birthday-bound collision risk at ~2^48 encryptions with a 96-bit nonce. Counter-based nonces require careful persistence.
**How to avoid:** Use a random 96-bit nonce from `ring::rand::SystemRandom` for each encryption operation. For this vault use case (small number of secrets, infrequent writes), random nonces are safe -- collision probability is negligible. Store the nonce alongside the ciphertext in the database row.
**Warning signs:** Any code that generates nonces deterministically or reuses nonce values.

### Pitfall 3: WAL Checkpoint on Backup
**What goes wrong:** `cp blufio.db blufio.db.bak` copies the main DB file but the WAL file (`blufio.db-wal`) may contain uncommitted pages. The backup is inconsistent or appears to be missing recent data.
**Why it happens:** WAL mode keeps changes in a separate `-wal` file until checkpoint. A simple file copy misses this.
**How to avoid:** Two strategies: (1) On graceful shutdown, call `PRAGMA wal_checkpoint(TRUNCATE)` to merge WAL into main DB, then backup is a single file copy. (2) For live backup, use SQLite's online backup API (`rusqlite::backup::Backup`). Document that `cp` works correctly after clean shutdown.
**Warning signs:** Backup instructions that don't mention WAL checkpoint or backup API.

### Pitfall 4: Argon2id Parameters Too Low
**What goes wrong:** Weak parameters (low memory, low iterations) make the derived key vulnerable to brute-force attacks on the passphrase.
**Why it happens:** Developers use minimal parameters for fast tests, then ship those defaults.
**How to avoid:** Use OWASP-recommended minimums: `m_cost=65536` (64 MiB), `t_cost=3` (3 iterations), `p_cost=4` (4 lanes). These take ~0.5-1s on modern hardware -- acceptable for a one-time unlock prompt. Store parameters alongside the salt in the vault metadata so they can be tuned later without re-deriving.
**Warning signs:** `m_cost` below 32768 or `t_cost` below 2 in production code.

### Pitfall 5: DNS Rebinding Bypasses SSRF Filter
**What goes wrong:** An attacker's DNS server returns a public IP on first lookup (passes filter), then a private IP on subsequent lookups. The actual connection goes to the private IP.
**Why it happens:** DNS TTL can be set to 0, forcing re-resolution between filter check and connection.
**How to avoid:** Resolve DNS once, check the resolved IP, then connect directly to that IP (not re-resolving). The custom `Resolve` trait implementation in reqwest resolves once and returns filtered results -- reqwest uses those resolved IPs directly for connection.
**Warning signs:** Code that checks a URL's hostname against a blocklist without resolving and filtering the actual IP.

### Pitfall 6: Plaintext Secrets in Config Auto-Migration
**What goes wrong:** The auto-migration feature reads plaintext secrets from TOML, migrates them to the vault, then removes them from the config file. If the process crashes between vault write and config rewrite, secrets exist in both places. Or the config file has restricted permissions that prevent rewriting.
**Why it happens:** The migrate-then-delete sequence is not atomic.
**How to avoid:** (1) Write to vault first. (2) Rewrite config file without the plaintext secrets. (3) If config rewrite fails, warn but don't error -- the secret is safely in the vault, and the plaintext copy will be migrated (as a no-op) on next startup. Use a `migrated_secrets` flag in the vault metadata to avoid re-migration loops.
**Warning signs:** Code that deletes the config entry before confirming the vault write succeeded.

## Code Examples

Verified patterns from official sources:

### SQLite Connection with Recommended PRAGMAs
```rust
// Source: SQLite docs (sqlite.org/pragma.html, sqlite.org/wal.html)
// + rusqlite bundled feature docs

use tokio_rusqlite::Connection;

async fn open_database(path: &str) -> Result<Connection, Box<dyn std::error::Error>> {
    let conn = Connection::open(path).await?;
    conn.call(|conn| {
        // WAL mode -- must be outside a transaction
        conn.execute_batch("PRAGMA journal_mode = WAL;")?;
        // Durability: NORMAL is safe with WAL mode
        conn.execute_batch("PRAGMA synchronous = NORMAL;")?;
        // Wait up to 5 seconds for locks
        conn.execute_batch("PRAGMA busy_timeout = 5000;")?;
        // Enforce referential integrity
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        // 16MB page cache (negative = KiB)
        conn.execute_batch("PRAGMA cache_size = -16000;")?;
        // Keep temp tables in memory
        conn.execute_batch("PRAGMA temp_store = MEMORY;")?;
        Ok(())
    }).await?;
    Ok(conn)
}
```

### Refinery Embedded Migrations
```rust
// Source: github.com/rust-db/refinery README + rusqlite test examples

mod embedded {
    use refinery::embed_migrations;
    // Migrations are in crates/blufio-storage/migrations/
    embed_migrations!("migrations");
}

pub fn run_migrations(conn: &mut rusqlite::Connection) -> Result<(), Box<dyn std::error::Error>> {
    embedded::migrations::runner().run(conn)?;
    Ok(())
}

// Migration file naming convention: V{version}__{description}.sql
// Example: migrations/V1__initial_schema.sql
// Example: migrations/V2__add_queue_table.sql
```

### Vault Unlock Flow
```rust
// Source: rpassword docs + argon2 crate docs + ring AEAD docs

use rpassword::read_password;
use secrecy::{SecretString, ExposeSecret};
use std::io::{self, IsTerminal};

/// Get vault passphrase from TTY prompt or BLUFIO_VAULT_KEY env var
fn get_vault_passphrase() -> Result<SecretString, BlufioError> {
    // Check env var first (headless/Docker/systemd)
    if let Ok(key) = std::env::var("BLUFIO_VAULT_KEY") {
        return Ok(SecretString::from(key));
    }

    // Interactive TTY prompt
    if io::stdin().is_terminal() {
        eprint!("Vault passphrase: ");
        let password = read_password()
            .map_err(|e| BlufioError::Internal(format!("password read: {e}")))?;
        return Ok(SecretString::from(password));
    }

    Err(BlufioError::Internal(
        "Vault exists but no passphrase provided. Set BLUFIO_VAULT_KEY or run interactively.".into()
    ))
}
```

### Masked Secret Preview
```rust
// For `blufio config list-secrets` -- shows "sk-...4f2b" format
fn mask_secret(value: &str) -> String {
    if value.len() <= 8 {
        return "*".repeat(value.len());
    }
    let prefix_len = value.find('-').map(|i| i + 1).unwrap_or(3).min(6);
    let suffix_len = 4;
    format!(
        "{}...{}",
        &value[..prefix_len],
        &value[value.len() - suffix_len..]
    )
}
// mask_secret("sk-ant-api03-abc123xyz") => "sk-...xyz"
// mask_secret("1234567890abcdef")       => "123...cdef"
```

### Database Schema (Initial Migration)
```sql
-- V1__initial_schema.sql
-- Sessions table
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY NOT NULL,
    channel TEXT NOT NULL DEFAULT 'unknown',
    user_id TEXT,
    state TEXT NOT NULL DEFAULT 'active',
    metadata TEXT,  -- JSON blob for extensibility
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Messages table
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system', 'tool')),
    content TEXT NOT NULL,
    token_count INTEGER,
    metadata TEXT,  -- JSON blob (tool calls, model info, etc.)
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX idx_messages_session ON messages(session_id, created_at);

-- Message queue table (crash-safe, SQLite-backed)
CREATE TABLE IF NOT EXISTS queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    queue_name TEXT NOT NULL DEFAULT 'default',
    payload TEXT NOT NULL,  -- JSON serialized
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'processing', 'completed', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    locked_until TEXT  -- For at-most-once processing
);
CREATE INDEX idx_queue_status ON queue(queue_name, status, created_at);

-- Vault entries table
CREATE TABLE IF NOT EXISTS vault_entries (
    name TEXT PRIMARY KEY NOT NULL,
    ciphertext BLOB NOT NULL,  -- AES-256-GCM encrypted value
    nonce BLOB NOT NULL,       -- 12-byte nonce used for this entry
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Vault metadata (master key, salt, params)
CREATE TABLE IF NOT EXISTS vault_meta (
    key TEXT PRIMARY KEY NOT NULL,
    value BLOB NOT NULL
);
-- Stores: 'wrapped_master_key', 'master_key_nonce', 'kdf_salt', 'kdf_params'
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `rusqlite` with manual thread spawning | `tokio-rusqlite` wraps this pattern | 2023+ | Eliminates boilerplate for async SQLite access |
| `PRAGMA synchronous = FULL` | `PRAGMA synchronous = NORMAL` with WAL | SQLite 3.7+ | NORMAL is safe with WAL and 2-3x faster for writes |
| bcrypt/scrypt for KDF | Argon2id (RFC 9106, 2021) | 2021 | Memory-hard, resists GPU/ASIC attacks, OWASP recommended |
| `secrecy` 0.8 `Secret<T>` | `secrecy` 0.10 `SecretBox<T>` | 2024 | API redesign: `Secret` replaced by `SecretBox`, `SecretString` is now `SecretBox<str>` |
| Manual SSRF hostname checks | DNS-resolution-level IP filtering | Ongoing best practice | Prevents DNS rebinding, decimal IP encoding, IPv6 bypasses |

**Deprecated/outdated:**
- `secrecy` 0.8 API: The `Secret<T>` type was replaced by `SecretBox<T>` in 0.10. Code using the old API won't compile with 0.10+.
- `ring` versions < 0.17.12: Security advisories affect earlier versions. Use 0.17.14+.
- `argon2` 0.6.0-rc.2 exists but is not yet stable. Use 0.5.3 for production.

## Open Questions

1. **Read Concurrency Strategy**
   - What we know: `tokio-rusqlite` uses one background thread per `Connection`. All operations (reads and writes) go through this single thread. For most workloads (single user, ~10 sessions), this is more than sufficient.
   - What's unclear: If Phase 5 (Memory & Embeddings) introduces heavy vector search queries, read contention with the write thread could become a bottleneck.
   - Recommendation: Start with single connection for everything. If profiling shows read latency issues, add a second read-only `Connection` (SQLite WAL allows concurrent readers). Do not optimize prematurely.

2. **WAL Checkpoint Strategy**
   - What we know: SQLite auto-checkpoints when WAL reaches 1000 pages by default. `PRAGMA wal_checkpoint(TRUNCATE)` on shutdown ensures clean single-file state for backup.
   - What's unclear: Whether `TRUNCATE` checkpoint during graceful shutdown is sufficient for PERS-04, or whether periodic checkpoints during runtime are needed.
   - Recommendation: Checkpoint on graceful shutdown (`SIGTERM` handler). This satisfies PERS-04 for the common case. Document that hot backup requires `rusqlite::backup::Backup` API.

3. **Vault Storage Location**
   - What we know: User decided vault entries live alongside other data. The vault tables are in the same SQLite database.
   - What's unclear: Whether vault metadata (wrapped master key, salt, KDF params) should be in the same database or a separate file for isolation.
   - Recommendation: Same database. The vault is encrypted at the application level -- physical separation provides no additional security since an attacker with file access has both files. Single file simplifies backup (PERS-04).

## Sources

### Primary (HIGH confidence)
- [ring 0.17.14 docs](https://docs.rs/ring/0.17.14/ring/) - AEAD API, LessSafeKey, Nonce, seal/open operations
- [argon2 0.5.3 docs](https://docs.rs/argon2/latest/argon2/) - Argon2id key derivation, ParamsBuilder, hash_password_into
- [secrecy 0.10.3 docs](https://docs.rs/secrecy/latest/secrecy/) - SecretBox, SecretString, ExposeSecret trait
- [SQLite WAL documentation](https://sqlite.org/wal.html) - Write-ahead logging behavior, checkpoint mechanics
- [SQLite PRAGMA documentation](https://sqlite.org/pragma.html) - journal_mode, synchronous, busy_timeout, foreign_keys
- [deadpool-sqlite docs](https://crates.io/crates/deadpool-sqlite) - Connection pool pattern (alternative reference)

### Secondary (MEDIUM confidence)
- [tokio-rusqlite docs](https://docs.rs/tokio-rusqlite) - Single-thread-per-connection architecture, mpsc+oneshot pattern
- [refinery README](https://github.com/rust-db/refinery) - embed_migrations! macro, rusqlite feature, migration runner
- [reqwest dns::Resolve trait](https://docs.rs/reqwest/latest/reqwest/dns/trait.Resolve.html) - Custom DNS resolver interface for SSRF prevention
- [High Performance SQLite recommended PRAGMAs](https://highperformancesqlite.com/articles/sqlite-recommended-pragmas) - Verified against official SQLite docs
- [rpassword docs](https://docs.rs/rpassword/) - Terminal password input, TTY detection

### Tertiary (LOW confidence)
- SSRF bypass techniques (DNS rebinding, IP encoding) - Based on general security knowledge, verified pattern is sound but specific bypass enumeration may be incomplete. OWASP SSRF cheat sheet should be consulted during implementation.
- Secret redaction regex patterns - Specific regex patterns for API key formats (sk-ant-*, Bearer, etc.) need validation against actual provider key formats during implementation.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All libraries verified via Context7, official docs, and crates.io. Versions confirmed current.
- Architecture: HIGH - Single-writer pattern via tokio-rusqlite is well-documented and used in production. Key wrapping pattern is standard cryptographic practice.
- Pitfalls: HIGH - SQLite concurrency pitfalls are extensively documented. Crypto pitfalls (nonce reuse, weak KDF params) are well-known in security literature.

**Research date:** 2026-02-28
**Valid until:** 2026-03-28 (stable domain, 30-day validity)
