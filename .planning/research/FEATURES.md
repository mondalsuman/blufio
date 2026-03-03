# Feature Research: v1.2 Production Hardening

**Domain:** Production hardening for existing Rust AI agent platform (Blufio v1.2)
**Researched:** 2026-03-03
**Confidence:** HIGH (verified against official crate docs, systemd manpages, SQLCipher API docs, existing codebase)

---

## Context

Blufio v1.1 shipped with 36,462 LOC Rust across 16 crates, 118 requirements verified across 2 milestones. This research covers ONLY the five production hardening features for v1.2:

1. **sd_notify integration** -- systemd Type=notify, watchdog pings
2. **SQLCipher encryption at rest** -- PRAGMA key, plaintext migration
3. **Minisign binary verification** -- verify-only (no key generation)
4. **Self-update with rollback** -- download, verify, atomic swap, health check, rollback
5. **Backup integrity verification** -- post-backup and post-restore PRAGMA integrity_check

### What Already Exists

| Area | Current State | Gap |
|------|--------------|-----|
| systemd | Unit file generation exists, Type=simple | No sd_notify, no READY=1, no watchdog |
| SQLite | WAL-mode with rusqlite 0.37 bundled, plaintext | No encryption at rest |
| Binary signing | Ed25519 for inter-agent messages | Not for binary verification |
| Self-update | No update mechanism | Entirely new feature |
| Backup | SQLite Backup API in backup.rs | No integrity check post-backup/restore |
| Doctor | PRAGMA integrity_check on live DB via --deep | Not wired into backup/restore flow |

---

## Feature 1: sd_notify Integration

### Table Stakes

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **READY=1 notification** | systemd Type=notify services MUST send READY=1 when initialization completes. Without it, systemd considers the service failed after TimeoutStartSec. | LOW | Send after vault unlock, storage init, channel connect, provider init -- right before agent_loop.run(). Single call: sd_notify::notify(true, &[NotifyState::Ready]). |
| **STOPPING=1 notification** | systemd should know when the service is shutting down gracefully vs. crashing. Sent when SIGTERM/SIGINT is received, before cleanup begins. | LOW | Send in the signal handler callback, before cancellation token fires. |
| **Watchdog ping (WATCHDOG=1)** | systemd kills services that stop pinging with SIGABRT. The WatchdogSec directive in the unit file defines the timeout. Best practice: ping at half the WatchdogSec interval. | LOW | Spawn a tokio task that calls sd_notify::notify(true, &[NotifyState::Watchdog]) on an interval derived from sd_notify::watchdog_enabled(). The watchdog task should only run if systemd reports watchdog is enabled. |
| **Unit file update to Type=notify** | The generated systemd unit file must change from Type=simple to Type=notify and add WatchdogSec= directive. | LOW | Update the unit file template. Add WatchdogSec=30 (30s is conservative for an agent with HTTP + LLM dependencies). Add NotifyAccess=main. |
| **Graceful no-op on non-systemd** | sd_notify must be a no-op on macOS/dev environments where NOTIFY_SOCKET is unset. The process should not crash or log errors. | LOW | sd_notify::notify(true, ...) already handles this -- the true parameter means "unset NOTIFY_SOCKET after sending" and the function returns Ok(()) when the socket is missing. |

### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **STATUS= messages during startup** | systemd shows startup progress: "Loading vault...", "Connecting channels...", "Ready". Visible via systemctl status blufio. Operators see exactly where startup is. | LOW | Sprinkle NotifyState::Status("phase") calls throughout run_serve(). Five or six calls total. |
| **ExtendTimeoutUsec during slow init** | If vault unlock or ONNX model download takes longer than TimeoutStartSec, extend the timeout dynamically rather than failing. | LOW | Before vault unlock and model download, send NotifyState::ExtendTimeoutUsec(30_000_000) (30s). Prevents false startup failures on first run. |
| **RELOADING=1 for config reload** | If hot-reload is implemented later, the service can signal it is reloading. Not needed now, but the infrastructure (sd_notify crate) supports it for free. | N/A | Defer. No config hot-reload in v1.2. |

### Anti-Features

| Anti-Feature | Why Problematic | Alternative |
|--------------|-----------------|-------------|
| **Linking against libsystemd** | Adds a native library dependency. Breaks static musl builds. The sd-notify crate uses raw Unix sockets instead. | Use sd-notify crate (pure Rust, zero dependencies beyond std). |
| **Watchdog tied to LLM health** | Tempting to only ping watchdog when an LLM call succeeds. But LLM outages are external -- restarting the agent fixes nothing. | Watchdog should verify internal health: tokio runtime alive, DB connection open. LLM outages are logged and metriced, not treated as daemon failure. |
| **Socket activation (systemd sockets)** | Adds complexity. Blufio already binds its own sockets in axum. | Keep self-managed socket binding. Socket activation is useful for on-demand services; Blufio is always-on. |

---

## Feature 2: SQLCipher Encryption at Rest

### Table Stakes

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **PRAGMA key on every connection open** | SQLCipher requires the key to be set immediately after opening a connection, before any other operation. Every connection in every crate that opens the DB must do this. | MEDIUM | Blufio opens connections in: blufio-storage (main), blufio-cost (ledger), blufio-memory (store), blufio-vault (vault), blufio-mcp-client (pin store), backup.rs, doctor.rs. All must be updated. Centralize key management. |
| **bundled-sqlcipher feature flag** | Switch from rusqlite features=["bundled"] to rusqlite features=["bundled-sqlcipher"]. These are mutually exclusive -- bundled-sqlcipher replaces bundled. Available in rusqlite 0.37. | LOW | Single Cargo.toml change in workspace dependencies. bundled-sqlcipher compiles SQLCipher from source and links it statically. Needs OpenSSL or bundled-sqlcipher-vendored-openssl for crypto. |
| **Key derivation from passphrase** | SQLCipher uses PBKDF2-HMAC-SHA512 with 256,000 iterations by default (SQLCipher 4.x). The operator provides a passphrase via env var or interactive prompt. | LOW | Re-use existing vault passphrase flow: BLUFIO_DB_KEY env var or rpassword interactive prompt. The passphrase goes to PRAGMA key = 'passphrase'. SQLCipher handles KDF internally. |
| **Plaintext-to-encrypted migration** | Existing v1.0/v1.1 deployments have plaintext databases. Must provide a one-time migration path. | MEDIUM | Use sqlcipher_export(): open plaintext DB, attach new encrypted DB with key, export all data, swap files. Implement as blufio db encrypt CLI command. This is a one-shot offline operation. |
| **Verify key correctness on open** | SQLCipher does not error on wrong key until the first read. Must immediately test with SELECT count(*) FROM sqlite_master after PRAGMA key. | LOW | Add a verification query right after PRAGMA key in the centralized connection opener. Fail fast with a clear error: "Database key is incorrect or database is not encrypted." |
| **Backup/restore with encrypted DB** | The SQLite Backup API works with SQLCipher -- both source and destination must use the same key. The backup.rs code must set the key on both connections. | LOW | Pass the DB key into run_backup() and run_restore(). Apply PRAGMA key to both source and destination connections before starting the backup. |

### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **PRAGMA rekey for key rotation** | Operators can change the encryption key without dump/restore. PRAGMA rekey = 'new_passphrase' re-encrypts all pages in-place. | LOW | Implement as blufio db rekey CLI command. Requires current key + new key. SQLCipher handles the heavy lifting. |
| **Raw key mode (skip KDF)** | For operators who manage their own key material: PRAGMA key = "x'<64-hex-chars>'" bypasses PBKDF2 entirely, saving ~200ms on every connection open. | LOW | Config option: storage.cipher_key_format = "passphrase" or "raw". Raw mode skips KDF. Document the security implications (operator responsible for key entropy). |
| **PRAGMA cipher_memory_security = ON** | Zeroes out SQLCipher internal key material when connections close. Defense against memory forensics on compromised machines. | LOW | Single PRAGMA call after key is set. Minor performance impact. Enable by default for production; document option to disable for development. |
| **doctor --deep checks encryption** | blufio doctor --deep reports whether the database is encrypted, the cipher version, page size, and KDF iterations. | LOW | Query PRAGMA cipher_version, PRAGMA cipher_settings. Report in doctor output. |

### Anti-Features

| Anti-Feature | Why Problematic | Alternative |
|--------------|-----------------|-------------|
| **Per-table encryption** | SQLCipher encrypts the entire database file, not individual tables. Attempting partial encryption requires two databases and cross-DB queries, which breaks the single-file deployment model. | Encrypt everything. The vault already stores secrets in the same DB file -- encryption at rest protects it all. |
| **Application-layer encryption (encrypt columns)** | The vault already does AES-256-GCM on credential values. Extending this to all data means encrypting every column, breaking queries, indexes, and full-text search. | SQLCipher encrypts at the page level transparently. Queries, indexes, and FTS work normally. Application-layer encryption is only needed for the vault extra-sensitive fields (which already has it). |
| **Supporting both plaintext and encrypted modes simultaneously** | Tempting to make encryption optional via feature flag. But this doubles the test matrix and means some deployments are unencrypted. | Make encryption the default for new installs. Provide a migration command for existing installs. After v1.2, all databases are encrypted. |
| **vendored-openssl on all platforms** | bundled-sqlcipher-vendored-openssl vendors OpenSSL source and compiles it. Adds ~2 minutes to build time and ~1MB to binary. | Use bundled-sqlcipher (not vendored) for CI/release builds where OpenSSL is available. Use bundled-sqlcipher-vendored-openssl only as a fallback for environments without system OpenSSL. For musl static builds, vendored OpenSSL is likely required. |

---

## Feature 3: Minisign Binary Verification

### Table Stakes

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **Verify binary signature before update** | Before replacing the running binary with a downloaded update, verify its Minisign signature against the project public key. Prevents supply chain attacks. | LOW | Use minisign-verify crate (zero dependencies, verify-only). Embed the project public key as a const in the binary. Download blufio.minisig alongside the binary. Call public_key.verify(binary_bytes, signature). |
| **Embedded public key** | The verification public key must be compiled into the binary, not loaded from a file (which could be tampered with). | LOW | const MINISIGN_PUBLIC_KEY: &str = "untrusted comment: ...\nRW..."; in the update module. Parsed once at verification time. |
| **Signature file convention** | The .minisig signature file must be distributed alongside every release binary. Standard naming: blufio-linux-amd64.minisig next to blufio-linux-amd64. | LOW | CI/release pipeline concern, not runtime code. Document the convention. |
| **Clear error on signature failure** | If verification fails, the update must abort with a clear message: "Signature verification failed. The downloaded binary may be tampered with." Never proceed with an unverified binary. | LOW | Return error from verify step. The self-update flow checks this before any file operations. |

### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Trusted comment verification** | Minisign signatures include a "trusted comment" (typically the filename and hash). Verify the trusted comment matches the expected binary name and version. Prevents signature reuse across different binaries. | LOW | minisign-verify includes trusted comment in the signature struct. Compare against expected values. |
| **blufio verify CLI command** | Standalone command to verify any file against a Minisign signature. Useful for operators who download binaries manually. blufio verify blufio-linux-amd64 blufio-linux-amd64.minisig. | LOW | Thin wrapper around the verify function. Takes file path + signature path as arguments. |
| **Streaming verification for large binaries** | For binaries >50MB (unlikely for Blufio, but future-proof), use StreamVerifier to avoid loading the entire binary into memory. | LOW | minisign-verify supports StreamVerifier with update() + finalize() API. Use for all verifications regardless of size -- the API is the same complexity. |

### Anti-Features

| Anti-Feature | Why Problematic | Alternative |
|--------------|-----------------|-------------|
| **Key generation in the binary** | Minisign key generation should happen in CI/release pipeline, not in the agent binary. Including key generation adds the full minisign crate instead of the lighter minisign-verify. | Use minisign-verify (verify-only, zero deps). Generate keys with the minisign CLI tool in CI. |
| **Multiple trusted public keys** | Key rotation scheme where multiple public keys are valid. Adds complexity for a single-developer project. | Single embedded public key. When the key rotates, ship a new binary signed with the new key. Old binaries verify with the old key; new binaries verify with the new key. |
| **GPG/PGP signatures** | GPG is the traditional approach but requires a GPG keyring, trust model, and large dependencies (gpgme or sequoia-pgp). | Minisign is purpose-built for file signing: simpler, smaller, and faster than GPG. Ed25519-based. Perfect fit for single-binary distribution. |

---

## Feature 4: Self-Update with Rollback

### Table Stakes

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **blufio update CLI command** | Check for newer version, download it, verify signature, replace binary, report result. | MEDIUM | New CLI subcommand. Orchestrates: version check -> download -> verify -> swap -> health check -> done/rollback. |
| **Version check against GitHub Releases** | Query GitHub Releases API for the latest version. Compare with current version (semver). Report "already up to date" or "update available: v1.2.1 -> v1.3.0". | LOW | Use reqwest (already in workspace) to hit the GitHub API releases/latest endpoint. Parse the tag_name field. Compare with the compile-time CARGO_PKG_VERSION. |
| **Download binary + signature** | Download the platform-appropriate binary and its .minisig signature from the GitHub Release assets. | LOW | Determine platform at compile time: target_os + target_arch. Download blufio-{os}-{arch} and blufio-{os}-{arch}.minisig. Use reqwest with progress callback. |
| **Minisign signature verification** | Verify the downloaded binary signature before any file operations. Abort on failure. | LOW | Calls the Minisign verify function from Feature 3. This is a hard dependency -- Feature 3 must be built first. |
| **Atomic binary swap** | Replace the running binary atomically. On Unix: rename new binary over old binary (atomic on same filesystem). The self-replace crate handles platform differences. | LOW | Use self-replace crate. On Unix it does an atomic rename. On Windows it uses a cleanup subprocess. Blufio targets Linux production; macOS dev only. |
| **Pre-swap backup of current binary** | Before replacing, copy the current binary to blufio.backup (or blufio.v1.2.0). This is the rollback target. | LOW | std::fs::copy(current_exe, backup_path) before the atomic swap. Store the backup path for the rollback command. |
| **blufio update --check (dry run)** | Check for updates without downloading or installing. Reports available version and changelog URL. | LOW | Skip download/verify/swap steps. Just the version check. |
| **Require --yes or interactive confirmation** | Do not auto-update without confirmation. Self-modifying binaries are dangerous. Show version diff and ask "Proceed? [y/N]". | LOW | Interactive prompt via rpassword-style input. --yes flag for non-interactive/CI use. |

### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Post-update health check** | After swapping the binary, spawn the new binary with blufio doctor (or a lightweight health subcommand). If it fails, auto-rollback. | MEDIUM | Spawn the new binary as a subprocess with "doctor" arg. If exit code != 0, trigger rollback. This catches binary corruption, missing dependencies, or ABI issues. |
| **blufio update --rollback** | Revert to the pre-update binary. Swaps the backup binary back. | LOW | Reverse the atomic swap: rename blufio.backup over the current binary. |
| **Changelog display** | Show the GitHub Release body (changelog) before confirming the update. Operators see what changed. | LOW | Parse the body field from the GitHub Releases API response. Render as plain text (strip markdown). |
| **Download progress bar** | Show download progress for large binaries. Blufio is ~25-50MB. On slow connections, silence is alarming. | LOW | reqwest streaming response with content-length. Print progress to stderr. |
| **Update channel configuration** | Config option: update.channel = "stable" or "pre-release". Default to stable (exclude pre-release GitHub tags). | LOW | Filter GitHub Releases by prerelease: false for stable channel. Include pre-releases for the pre-release channel. |

### Anti-Features

| Anti-Feature | Why Problematic | Alternative |
|--------------|-----------------|-------------|
| **Auto-update on startup** | Silent self-modification is hostile. Operators must control when updates happen. An always-on daemon that modifies itself could break during critical operations. | Manual blufio update only. Never auto-update. Operators schedule updates in maintenance windows. |
| **Delta/patch updates** | Binary diff (bsdiff/bspatch) saves bandwidth but adds complexity. Blufio is 25-50MB; full downloads are fine. | Full binary download. The simplicity is worth the bandwidth. |
| **Using self_update crate** | The self_update crate abstracts GitHub/S3 backends but does not support Minisign verification, health checks, or rollback. It uses zipsign (not Minisign) for verification. Its API is designed for simple cases and is hard to extend. | Build a custom update flow using: reqwest (download), minisign-verify (verify), self-replace (atomic swap). The individual steps are simple; the orchestration is where the value is. |
| **In-process restart after update** | Risky: if the new binary has a startup bug, you lose the running agent with no recovery. | Swap the binary, run health check as a subprocess, report success. The agent continues running as the old version until the operator restarts it (or systemd restarts it via systemctl restart blufio). |

---

## Feature 5: Backup Integrity Verification

### Table Stakes

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **PRAGMA integrity_check after backup** | After run_backup() completes, open the backup file and run PRAGMA integrity_check. Catches corruption from disk errors, interrupted writes, or buggy backup logic. | LOW | Open backup DB (read-only), run PRAGMA integrity_check, verify result is ["ok"]. If not, delete the corrupt backup and return error. With SQLCipher: must set key before integrity check. |
| **PRAGMA integrity_check after restore** | After run_restore() completes, run integrity check on the restored database. Catches corruption in the backup file that passed the "can query it" validation but has deeper issues. | LOW | The current restore validation is SELECT 1 -- insufficient. Replace with full PRAGMA integrity_check. This catches index corruption, missing pages, and malformed records. |
| **Report integrity status to operator** | Print integrity check result alongside backup/restore status. "Backup complete: 5.2 MB written, integrity: ok" or "Restore failed: integrity check found 3 issues". | LOW | Extend the existing eprintln! output in backup.rs. |
| **Fail backup on integrity failure** | If the backup file fails integrity check, do not report success. Return an error. The backup file should be deleted to avoid operators trusting a corrupt backup. | LOW | Delete corrupt backup file. Return BlufioError::Storage with descriptive message. |

### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **PRAGMA quick_check option** | quick_check is O(N) vs integrity_check O(NlogN). For large databases, quick_check is significantly faster. Offer as --quick flag on backup/restore. | LOW | Default to full integrity_check. --quick uses PRAGMA quick_check (skips index verification). Document the tradeoff. |
| **SHA-256 checksum of backup file** | After backup + integrity check, compute and print/store the SHA-256 hash of the backup file. Operators can verify the backup file has not been modified since creation. | LOW | ring::digest::SHA256 (ring already in workspace). Print hash alongside backup status. Optionally write to backup.sha256 sidecar file. |
| **Scheduled backup verification** | Background task that periodically runs integrity check on the most recent backup file. Catches bit rot on backup storage. | MEDIUM | Spawn a tokio task with configurable interval (e.g., daily). Log results. Emit Prometheus metric blufio_backup_last_integrity_check_ok. |
| **blufio backup --verify-only** | Run integrity check on an existing backup file without creating a new backup. Useful for auditing old backups. | LOW | Open the backup file, set key (if encrypted), run integrity check, report result. |
| **Foreign key check** | PRAGMA integrity_check does NOT check foreign key constraints. Add PRAGMA foreign_key_check as an additional verification step. | LOW | Run after integrity_check. Reports orphaned foreign key references. Blufio uses foreign keys for session-message relationships. |

### Anti-Features

| Anti-Feature | Why Problematic | Alternative |
|--------------|-----------------|-------------|
| **Integrity check on live DB during backup** | Running PRAGMA integrity_check on the live production database locks it and takes O(NlogN) time. Blocks all writes. | Only check the backup copy. The backup API creates a consistent snapshot; verify that. The live DB integrity check already exists in doctor --deep for offline diagnostics. |
| **Automatic repair on integrity failure** | SQLite has no built-in repair tool. "Repair" means dumping what is readable and creating a new DB, which loses corrupt data. Automatic repair risks silent data loss. | Report the failure. Let the operator decide: restore from a known-good backup, or run .dump manually. Never auto-repair. |
| **Encrypted backup checksums** | Computing SHA-256 of an encrypted backup file is meaningless for content verification (the hash changes with re-encryption). | The integrity_check runs AFTER decryption (inside SQLCipher). That is the true content verification. The SHA-256 checksum is for detecting file-level tampering, not content correctness. Both are useful for different purposes. |

---

## Feature Dependencies

```
[sd_notify Integration]
    |-- requires --> serve.rs startup sequence (existing)
    |-- requires --> signal handler / cancellation token (existing)
    |-- requires --> systemd unit file template (existing, update to Type=notify)
    |-- independent of all other v1.2 features

[SQLCipher Encryption at Rest]
    |-- requires --> rusqlite feature flag change (bundled -> bundled-sqlcipher)
    |-- requires --> centralized connection opener (new, refactor)
    |-- requires --> key management (env var / prompt, similar to vault)
    |-- requires --> migration command (new CLI subcommand)
    |-- affects --> backup.rs (must pass key to both connections)
    |-- affects --> doctor.rs (must set key before integrity check)

[Minisign Binary Verification]
    |-- requires --> minisign-verify crate (new dependency)
    |-- requires --> embedded public key (compile-time constant)
    |-- independent of SQLCipher and sd_notify
    |-- required by --> Self-Update (hard dependency)

[Self-Update with Rollback]
    |-- requires --> Minisign verification (Feature 3)
    |-- requires --> self-replace crate (new dependency)
    |-- requires --> reqwest (existing)
    |-- requires --> GitHub Releases API access
    |-- independent of SQLCipher and sd_notify

[Backup Integrity Verification]
    |-- requires --> backup.rs (existing)
    |-- requires --> doctor.rs integrity check pattern (existing, reuse)
    |-- affected by --> SQLCipher (must set key before integrity check)
    |-- should follow SQLCipher if both are in same milestone
```

### Dependency Notes

- **sd_notify is fully independent.** It touches only serve.rs and the unit file template. Can be built in any order.
- **SQLCipher has the widest blast radius.** It affects every crate that opens a DB connection. Must be done carefully with centralized key management.
- **Minisign must precede Self-Update.** Self-Update calls Minisign verify as a mandatory step.
- **Backup Integrity is mostly independent** but must account for SQLCipher if encryption is already active. Build after SQLCipher to avoid doing it twice.
- **No circular dependencies.** The dependency graph is a DAG with clear ordering.

---

## MVP Definition for v1.2

### Must Ship (all five features)

- [ ] **sd_notify READY=1 + STOPPING=1 + watchdog** -- Core systemd integration. Unit file update to Type=notify. Graceful no-op on non-systemd. STATUS= messages during startup.
- [ ] **SQLCipher encryption at rest** -- Feature flag switch, centralized key management, PRAGMA key on all connections, plaintext-to-encrypted migration command, backup/restore with key.
- [ ] **Minisign binary verification** -- Verify-only with embedded public key. Used by self-update. Standalone blufio verify command.
- [ ] **Self-update with rollback** -- blufio update command: version check, download, verify, atomic swap, pre-swap backup, health check, rollback.
- [ ] **Backup integrity check** -- PRAGMA integrity_check post-backup and post-restore. Fail on corruption. Report status.

### Add After Core (v1.2.x)

- [ ] **PRAGMA rekey** for key rotation -- Trigger: operator requests key change
- [ ] **SHA-256 checksum** of backup files -- Trigger: operator requests tamper detection
- [ ] **Download progress bar** for self-update -- Trigger: user feedback on large downloads
- [ ] **Scheduled backup verification** -- Trigger: long-running deployments need periodic checks
- [ ] **Update channel configuration** (stable/pre-release) -- Trigger: pre-release testing workflow

### Defer to v1.3+

- [ ] **Auto-update** -- Never auto-update an always-on daemon
- [ ] **Delta/patch updates** -- Full binary downloads are fine at 25-50MB
- [ ] **Automatic DB repair** -- Too dangerous for automatic execution
- [ ] **Per-table encryption** -- Contradicts single-file deployment model
- [ ] **Socket activation** -- Not needed for an always-on service

---

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Risk | Priority |
|---------|------------|---------------------|------|----------|
| sd_notify integration | HIGH | LOW | LOW | P1 |
| Backup integrity check | HIGH | LOW | LOW | P1 |
| SQLCipher encryption at rest | HIGH | MEDIUM | MEDIUM | P1 |
| Minisign binary verification | MEDIUM | LOW | LOW | P1 |
| Self-update with rollback | MEDIUM | MEDIUM | MEDIUM | P1 |
| PRAGMA rekey (key rotation) | MEDIUM | LOW | LOW | P2 |
| SHA-256 backup checksums | LOW | LOW | LOW | P2 |
| Doctor encryption reporting | LOW | LOW | LOW | P2 |
| Foreign key check in backup | LOW | LOW | LOW | P2 |
| Scheduled backup verification | LOW | MEDIUM | LOW | P3 |

**Priority key:**
- P1: Must have for v1.2 launch
- P2: Should have, add when possible (low-hanging fruit during implementation)
- P3: Nice to have, defer unless trivial

---

## Feature Sizing Estimates

| Feature | Estimated LOC | Risk | Notes |
|---------|--------------|------|-------|
| sd_notify integration (all table stakes + STATUS=) | 80-150 | LOW | ~5 notify calls in serve.rs, one watchdog task, unit file update |
| SQLCipher feature flag + centralized connection opener | 200-400 | MEDIUM | Refactor connection opening across 6+ crates to use shared key management |
| SQLCipher plaintext-to-encrypted migration | 150-250 | MEDIUM | CLI command using sqlcipher_export(). One-shot offline operation |
| Minisign verification module | 80-120 | LOW | Thin wrapper around minisign-verify. Embedded public key |
| blufio verify CLI command | 40-60 | LOW | CLI plumbing for the verify function |
| Self-update orchestration | 300-500 | MEDIUM | Version check + download + verify + swap + health check + rollback |
| Backup integrity check (post-backup + post-restore) | 80-120 | LOW | Add integrity_check calls to existing backup.rs functions |
| **Total estimated** | **930-1,600** | | ~3-4% of current codebase. Small, focused changes. |

---

## Competitor Feature Analysis

| Feature | OpenClaw (incumbent) | Blufio v1.2 |
|---------|---------------------|-------------|
| systemd integration | None (Node.js, uses pm2 or manual) | Type=notify, watchdog, STATUS= messages |
| Database encryption at rest | None (JSONL files, plaintext) | SQLCipher AES-256, PRAGMA key, migration |
| Binary verification | None (npm install, hundreds of deps) | Minisign Ed25519 signatures, embedded public key |
| Self-update | npm update (trust npm registry) | Download + verify + atomic swap + health check + rollback |
| Backup integrity | None (JSONL, no integrity tools) | PRAGMA integrity_check on backup + restore |

Every feature in v1.2 addresses a gap that the incumbent cannot fix due to its Node.js/JSONL architecture. These are structural advantages of the single-binary + SQLite model.

---

## Sources

### Crate Documentation (HIGH confidence)
- [sd-notify 0.4.5 -- NotifyState variants](https://docs.rs/sd-notify/0.4.5/sd_notify/enum.NotifyState.html) -- READY, STOPPING, Watchdog, Status, ExtendTimeoutUsec, and 8 more variants
- [sd-notify crate](https://crates.io/crates/sd-notify) -- Pure Rust, MIT OR Apache-2.0, zero native deps
- [minisign-verify 0.2.5](https://docs.rs/minisign-verify) -- Zero-dependency verify-only crate, StreamVerifier support
- [self-replace 1.3.6](https://docs.rs/self-replace/1.3.6/self_replace/) -- Atomic binary swap: Unix rename, Windows cleanup subprocess
- [rusqlite feature flags](https://lib.rs/crates/rusqlite/features) -- bundled-sqlcipher, bundled-sqlcipher-vendored-openssl

### Official Documentation (HIGH confidence)
- [systemd sd_notify manpage](https://www.freedesktop.org/software/systemd/man/latest/sd_notify.html) -- READY=1, STOPPING=1, WATCHDOG=1, STATUS=, ExtendTimeoutUsec
- [systemd.service manpage](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html) -- Type=notify, WatchdogSec, NotifyAccess
- [SQLCipher API](https://www.zetetic.net/sqlcipher/sqlcipher-api/) -- PRAGMA key, PRAGMA rekey, cipher_migrate, sqlcipher_export, cipher_memory_security
- [SQLCipher plaintext migration guide](https://discuss.zetetic.net/t/how-to-encrypt-a-plaintext-sqlite-database-to-use-sqlcipher-and-avoid-file-is-encrypted-or-is-not-a-database-errors/868)
- [SQLite PRAGMA integrity_check](https://sqlite.org/pragma.html) -- integrity_check vs quick_check, O(NlogN) vs O(N), partial table checks

### Architecture References (MEDIUM confidence)
- [Writing a proper systemd daemon in Rust](https://deterministic.space/writing-a-daemon.html) -- Type=notify, watchdog, structured logging, socket activation guidance
- [rusqlite SQLCipher issue #219](https://github.com/rusqlite/rusqlite/issues/219) -- SQLCipher support history, PRAGMA key/rekey wrappers
- [rusqlite bundled-sqlcipher issue #765](https://github.com/rusqlite/rusqlite/issues/765) -- Build integration details, crypto backend options
- [self_update crate](https://github.com/jaemk/self_update) -- GitHub releases backend, evaluated but not recommended (no Minisign, no rollback)
- [jedisct1/rust-minisign-verify](https://github.com/jedisct1/rust-minisign-verify) -- Official minisign-verify source

### Blufio Codebase (verified by reading source)
- crates/blufio/src/serve.rs -- Startup sequence where sd_notify calls will be inserted
- crates/blufio/src/backup.rs -- Backup API, restore with safety backup, no integrity check
- crates/blufio/src/doctor.rs -- Existing PRAGMA integrity_check in check_db_integrity()
- crates/blufio/Cargo.toml -- rusqlite with "bundled" + "backup" features
- Cargo.toml (workspace) -- rusqlite 0.37, all workspace dependencies

---
*Feature research for: v1.2 Production Hardening -- Blufio*
*Researched: 2026-03-03*
