# Technology Stack: v1.2 Production Hardening

**Project:** Blufio
**Researched:** 2026-03-03
**Scope:** NEW crate additions and feature changes for sd_notify, SQLCipher, Minisign verification, self-update with rollback, and backup integrity verification.

> Existing stack (tokio, axum, rusqlite 0.37, ring, reqwest 0.13, ed25519-dalek, etc.) is validated and unchanged from v1.1. This document covers ONLY what changes.

---

## Recommended Stack Additions

### 1. sd_notify Integration

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| sd-notify | 0.4.5 | systemd Type=notify readiness + watchdog pings | Pure Rust, zero dependencies (only libc, already present). 1.2M+ downloads. Gracefully no-ops on non-systemd environments. Compiles on all Unix including macOS. |

**Key API surface:**
- `sd_notify::notify(false, &[NotifyState::Ready])` -- signal startup complete
- `sd_notify::notify(false, &[NotifyState::Watchdog])` -- ping watchdog timer
- `sd_notify::notify(false, &[NotifyState::Status("message")])` -- free-form status
- `sd_notify::notify(false, &[NotifyState::Stopping])` -- graceful shutdown signal
- `sd_notify::watchdog_enabled(false)` -- returns watchdog interval if configured

**Non-systemd behavior (HIGH confidence, verified from source):**

The crate uses `std::os::unix` (not `cfg(target_os = "linux")`), so it compiles on all Unix targets including macOS. The `notify()` function checks for the `NOTIFY_SOCKET` environment variable. When absent (macOS, non-systemd Linux, Docker without socket passthrough), `connect_notify_socket()` returns `Ok(None)` and `notify()` returns `Ok(())` immediately. No error, no panic, no log noise.

This means calling `sd_notify::notify()` unconditionally is safe on all platforms. No `#[cfg]` guards needed in application code.

**systemd unit file pattern:**
```ini
[Service]
Type=notify
WatchdogSec=30
NotifyAccess=main
```

**Integration points:**
1. Call `NotifyState::Ready` at the end of `run_serve()` after all adapters are initialized.
2. Spawn a tokio task that calls `NotifyState::Watchdog` at `WatchdogUsec / 2` interval (queried via `watchdog_enabled()`).
3. Call `NotifyState::Stopping` in the SIGTERM/SIGINT shutdown handler.
4. Optionally: `NotifyState::Status("sessions=5, cost=$0.42")` for `systemctl status` display.

**Confidence:** HIGH -- verified from crate source code at github.com/lnicola/sd-notify.

---

### 2. SQLCipher Database Encryption

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| rusqlite | 0.37.0 (existing, feature change) | SQLite/SQLCipher bindings | Add `bundled-sqlcipher-vendored-openssl` feature flag. REPLACES existing `bundled`. |

**What changes:** Replace the workspace-level `rusqlite` feature `bundled` with `bundled-sqlcipher-vendored-openssl`. This is a feature flag change on an existing dependency, not a version change.

**Feature dependency chain:**
```
bundled-sqlcipher-vendored-openssl
  -> bundled-sqlcipher
    -> bundled (cc + bundled_bindings)
  -> openssl-sys/vendored
```

**Why `bundled-sqlcipher-vendored-openssl` specifically:**

| Feature | Crypto Source | Musl Static Build | System Headers Needed |
|---------|-------------|-------------------|----------------------|
| `bundled-sqlcipher` | System OpenSSL/LibreSSL | BREAKS -- no system crypto in cross Docker | YES -- libcrypto headers |
| `bundled-sqlcipher-vendored-openssl` | Vendored OpenSSL (compiled from source) | WORKS -- fully self-contained | NO -- everything bundled |

The vendored variant compiles OpenSSL from source via the `openssl-sys` crate's build script, producing a static `libcrypto.a`. This is the standard approach for musl cross-compilation and is used by hundreds of Rust projects shipping static binaries.

**SQLCipher is a strict superset of SQLite (HIGH confidence):**
- When no key is provided, SQLCipher behaves identically to standard SQLite.
- Existing unencrypted databases continue to work without ANY code changes.
- Encryption is opt-in via `PRAGMA key = '...'` immediately after opening a connection.
- ALL existing APIs work unchanged: WAL mode, backup API, PRAGMA integrity_check, all SQL.
- Migration from unencrypted to encrypted uses `ATTACH DATABASE + sqlcipher_export()`.

**CRITICAL: `bundled` must be REMOVED, not kept alongside.** `bundled-sqlcipher` implies `bundled`. Having both in the feature list is redundant but harmless. However, the workspace Cargo.toml should cleanly specify only `bundled-sqlcipher-vendored-openssl` to avoid confusion.

**Encryption key management approach:**
The existing vault already derives a 256-bit key from a passphrase using Argon2id. For SQLCipher:
1. Derive a separate 256-bit key from the master key using HKDF (via `ring::hkdf`) with context `"blufio-sqlcipher-v1"`.
2. Pass as hex string via `PRAGMA key = "x'...'";` immediately after `Connection::open()`.
3. Hold key in `Zeroizing<String>` -- never log, never persist.
4. On `tokio-rusqlite::Connection::open()`, use the `.call()` method to run the PRAGMA on the inner connection.

**Build system implications:**

| Concern | Impact | Mitigation |
|---------|--------|------------|
| Compile time | +30-90s clean builds (OpenSSL from source) | Incremental builds unaffected. CI caching helps. |
| Binary size | +300-500KB (static libcrypto) | Acceptable for encryption-at-rest. |
| cross musl builds | openssl-sys vendored compiles inside cross Docker | Standard approach. Default cross images include C compiler + perl (needed by OpenSSL Configure). |
| macOS dev builds | openssl-sys vendored compiles on macOS natively | No Homebrew OpenSSL needed. Fully self-contained. |
| C compiler | Already required by existing `bundled` feature | No new toolchain dependency. |
| aarch64-unknown-linux-musl | openssl-sys vendored supports aarch64 | Cross default image works. |

**Risk: Cross Docker image compatibility (MEDIUM confidence).**
The default `ghcr.io/cross-rs/x86_64-unknown-linux-musl:main` image should have all prerequisites (cc, perl, make). If issues arise, a `Cross.toml` can pin a known-good image version. Test the cross build EARLY in the milestone.

**Why NOT upgrade to rusqlite 0.38:**
rusqlite 0.38 has 4 breaking changes (u64 ToSql/FromSql disabled by default, statement cache optional, min SQLite 3.34.1, Connection ownership checks for hooks). These require code changes across the workspace. The `bundled-sqlcipher-vendored-openssl` feature is fully supported on 0.37. Upgrade to 0.38 in a separate future milestone.

**Confidence:** HIGH for feature flags and API behavior. MEDIUM for musl cross-compilation (well-established pattern but should be validated early).

---

### 3. Minisign Binary Verification

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| minisign-verify | 0.2.5 | Verify Ed25519-based Minisign signatures on downloaded binaries | Zero dependencies. ~2500 lines. Maintained by jedisct1 (Minisign creator). Published 2026-03-03 (actively maintained). Supports streaming verification. |

**Why Minisign over alternatives:**

| Alternative | Why NOT |
|-------------|---------|
| GPG signatures | Complex keyring management, large dependency (gpgme). Overkill for single-key verification. |
| signify (BSD) | Less ecosystem tooling. Minisign is a superset with trusted comments. |
| sigstore/cosign | Requires certificate transparency infrastructure. Too heavy for a single-binary project. |
| ed25519-dalek directly | Could work (same curve), but Minisign adds trusted comments, key IDs, and a standard signature format. The minisign-verify crate handles all parsing. |
| minisign (full crate) | Includes signing code. We only need verification. minisign-verify has zero dependencies; minisign pulls in several. |

**Key API surface:**
```rust
use minisign_verify::{PublicKey, Signature};

// Embedded at compile time:
const MINISIGN_PK: &str = "untrusted comment: blufio release key\nRWQ...base64...";

let pk = PublicKey::from_base64("RWQ...base64...")?;
let sig = Signature::decode(&sig_content)?;

// Small file (in-memory):
pk.verify(&binary_data, &sig, false)?;

// Large file (streaming -- preferred for binaries):
let mut verifier = pk.verify_stream(&sig)?;
for chunk in file_chunks {
    verifier.update(&chunk)?;
}
verifier.finalize()?;
```

**Streaming verification note:** Only works with pre-hashed signatures (the default in modern Minisign). Ensure release signing uses `minisign -S` without the legacy `-H` flag.

**Integration point:**
1. Embed the Minisign public key as a `const` in the binary crate.
2. During self-update: download `{tarball}.minisig` alongside the tarball.
3. Stream-verify the tarball against the signature before extraction.
4. Reject the update if verification fails -- do not extract, do not swap.

**Release workflow addition:**
```yaml
# In .github/workflows/release.yml, after Package step:
- name: Install Minisign
  run: |
    curl -sL https://github.com/jedisct1/minisign/releases/download/0.11/minisign-0.11-linux-x86_64.tar.gz \
      | tar xz -C /usr/local/bin --strip-components=2

- name: Sign release artifacts
  run: |
    echo "${{ secrets.MINISIGN_SECRET_KEY }}" > /tmp/minisign.key
    for f in blufio-*.tar.gz; do
      minisign -Sm "$f" -s /tmp/minisign.key -t "blufio ${{ github.ref_name }}"
    done
    rm /tmp/minisign.key
```

Upload `.minisig` files as additional release assets alongside tarballs.

**Confidence:** HIGH -- zero dependencies, actively maintained, API verified from docs.rs.

---

### 4. Self-Update with Rollback

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| self-replace | 1.5.0 | Atomic binary replacement on disk | By Armin Ronacher (mitsuhiko). Uses atomic `rename()` on Unix: places new file adjacent, then renames. POSIX-standard, works on any filesystem including musl static binaries. |
| reqwest | 0.13 (existing) | GitHub releases API + binary download | Already in workspace with `json`, `rustls`, `stream` features. Fully sufficient. |
| flate2 | 1 | gzip decompression | Pure Rust (miniz_oxide backend). Decompress `.tar.gz` release tarballs. |
| tar | 0.4 | tar archive extraction | Extract the `blufio` binary from the decompressed stream. |
| tempfile | 3 (promote to regular dep) | Stage downloaded binary atomically | Already in workspace as dev-dependency. Promote to regular dependency in the binary crate. |

**Why NOT self_update (jaemk) crate:**

| Concern | self_update | Manual (reqwest + self-replace) |
|---------|------------|--------------------------------|
| Dependencies | Pulls reqwest, indicatif, zip, flate2, tar, serde, etc. | Uses existing reqwest. Adds only self-replace + flate2 + tar. |
| Control | Opinionated GitHub integration | Full control over flow, error handling, retry logic |
| Signature verification | None built-in | We integrate minisign-verify ourselves |
| Release format | Assumes specific naming conventions | We define our own (already have a release workflow) |
| Maintenance | Last major update ~2023 | Our code, our maintenance |

Building from primitives is the right call because:
1. We already have `reqwest` with the features we need.
2. We need Minisign verification integrated into the flow (self_update doesn't support it).
3. The actual logic is ~200 lines of straightforward code.

**self-replace on Unix (HIGH confidence):**
On Unix, `self_replace::self_replace(new_binary_path)` places the new binary next to the current executable and performs an atomic `rename()`. The `rename()` syscall is POSIX-standard and works on all Linux filesystems (including musl static binaries, tmpfs, overlayfs). The old inode remains valid for the running process (open file descriptors survive rename/unlink).

**Self-update flow:**
```
1. Query: GET https://api.github.com/repos/{owner}/{repo}/releases/latest
   -> Parse JSON for assets matching current target triple
   -> Compare semver with current version (skip if up to date)

2. Download:
   -> {binary}-{version}-{target}.tar.gz
   -> {binary}-{version}-{target}.tar.gz.minisig

3. Verify: minisign-verify signature against embedded public key
   -> REJECT if verification fails (do not proceed)

4. Extract: flate2 decompress -> tar extract -> binary to tempfile
   -> Verify extracted binary SHA-256 against release metadata

5. Set permissions: chmod +x on extracted binary

6. Backup: copy current binary to {path}.rollback

7. Swap: self_replace::self_replace(temp_path)
   -> Atomic rename on Unix

8. Verify: exec the new binary with --version, confirm it runs
   -> On failure: rename {path}.rollback back to {path}
```

**Rollback mechanism:**
- `blufio update` -- performs the flow above
- `blufio update --rollback` -- swaps `.rollback` back if it exists
- Keep exactly one `.rollback` file (not a history)
- The `.rollback` file is overwritten on each successful update

**GitHub API via reqwest (no new dependency):**
```rust
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

let release: Release = reqwest::Client::new()
    .get("https://api.github.com/repos/blufio/blufio/releases/latest")
    .header("User-Agent", "blufio-updater")
    .send().await?
    .json().await?;
```

**Target triple detection at compile time:**
```rust
const TARGET: &str = env!("TARGET"); // Set via build.rs or cargo
// Or use compile-time cfg:
#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "musl"))]
const TARGET_TRIPLE: &str = "x86_64-unknown-linux-musl";
```

**Confidence:** HIGH for self-replace mechanism. MEDIUM for full update flow (integration testing with real GitHub releases needed).

---

### 5. Backup Integrity Verification

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| ring | 0.17 (existing) | SHA-256 digest of backup files | Already in workspace (used by blufio-vault and blufio-mcp-client). No new dependency. |
| rusqlite | 0.37.0 (existing) | PRAGMA integrity_check on backup/restore | Already implemented in doctor.rs. Extend pattern to backup flow. |

**No new dependencies needed.**

**Enhancement to existing backup.rs:**

1. **After `run_backup()`:**
   - Open the backup file read-only.
   - Run `PRAGMA integrity_check` -- confirm `"ok"`.
   - Compute SHA-256 hash of the backup file (streaming via `ring::digest`).
   - Write sidecar `{backup_path}.meta.json` with: path, timestamp, sha256, size_bytes, integrity_status.
   - Print hash and integrity result to stderr.

2. **After `run_restore()`:**
   - Run `PRAGMA integrity_check` on the restored database -- confirm `"ok"`.
   - If integrity check fails, warn and suggest using the `.pre-restore` safety backup.

3. **New: `blufio backup --verify {path}`:**
   - Recompute SHA-256 of backup file.
   - Compare against stored `.meta.json` hash.
   - Run `PRAGMA integrity_check` on the backup.
   - Report pass/fail.

**SHA-256 via ring (existing dependency):**
```rust
use ring::digest;
use std::io::Read;

fn sha256_file(path: &str) -> Result<String, std::io::Error> {
    let mut ctx = digest::Context::new(&digest::SHA256);
    let mut file = std::fs::File::open(path)?;
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 { break; }
        ctx.update(&buf[..n]);
    }
    let hash = ctx.finish();
    Ok(hex::encode(hash.as_ref()))
}
```

**Confidence:** HIGH -- uses only existing dependencies and patterns already in the codebase.

---

## Summary: All Dependency Changes

### New Dependencies (5 crates)

| Crate | Version | Transitive Deps Added | Size Impact |
|-------|---------|----------------------|-------------|
| sd-notify | 0.4.5 | 0 (libc already present) | ~10KB |
| minisign-verify | 0.2.5 | 0 (zero dependencies) | ~15KB |
| self-replace | 1.5.0 | 0 on Unix | ~10KB |
| flate2 | 1 | miniz_oxide, crc32fast | ~50KB |
| tar | 0.4 | filetime, xattr | ~30KB |

**Total new transitive dependencies: ~4-5 crates**
**Binary size impact: ~200-400KB estimated**

### Feature Flag Changes (1 existing crate)

| Crate | Before | After | Impact |
|-------|--------|-------|--------|
| rusqlite | `bundled` | `bundled-sqlcipher-vendored-openssl` | Compiles SQLCipher + vendored OpenSSL from source. +300-500KB binary, +30-90s clean build. |

### Promotions (1 existing crate)

| Crate | Before | After |
|-------|--------|-------|
| tempfile | dev-dependency in blufio | regular dependency in blufio |

### Dependency Budget

| Metric | Before (v1.1) | After (v1.2) |
|--------|--------------|--------------|
| Direct workspace deps | ~37 | ~42 |
| Well within <80 constraint | Yes | Yes |
| New crates with zero deps | 3 of 5 | Minimal audit surface |

---

## What NOT to Add

| Temptation | Why Avoid |
|------------|-----------|
| `self_update` (jaemk) | Heavy dependency tree, no Minisign support, opinionated GitHub integration. We have reqwest already. |
| `sha2` / `sha256` crate | `ring` 0.17 already provides `ring::digest::SHA256`. Adding another SHA-256 crate is wasteful. |
| `openssl` (direct) | `openssl-sys` is pulled transitively by `bundled-sqlcipher-vendored-openssl`. Do NOT add openssl as a direct dependency -- it's an implementation detail of the SQLCipher build. |
| `systemd` crate | Full libsystemd C bindings. Massive overkill. sd-notify is pure Rust and covers Type=notify + watchdog. |
| `minisign` (full crate) | Includes signing code + dependencies (scrypt, getrandom, rpassword). We only need verification. |
| `rusqlite` 0.38 upgrade | 4 breaking changes. Not needed -- 0.37 supports all required SQLCipher features. Upgrade in a separate milestone. |
| `indicatif` (progress bars) | Nice-to-have for download progress, but adds dependency. Use simple stderr logging instead. Can add later if users request it. |
| Custom OpenSSL build | Vendored OpenSSL via openssl-sys is the correct approach. Do NOT download/compile OpenSSL separately in CI. |

---

## Workspace Cargo.toml Changes

```toml
[workspace.dependencies]
# CHANGED: Replace "bundled" with SQLCipher + vendored OpenSSL
rusqlite = { version = "0.37", features = ["bundled-sqlcipher-vendored-openssl"] }

# NEW: Production hardening dependencies
sd-notify = "0.4"
minisign-verify = "0.2"
self-replace = "1.5"
flate2 = "1"
tar = "0.4"
```

## Binary Crate Cargo.toml Changes

```toml
# crates/blufio/Cargo.toml additions:
[dependencies]
sd-notify = { workspace = true }
minisign-verify = { workspace = true }
self-replace = { workspace = true }
flate2 = { workspace = true }
tar = { workspace = true }
tempfile = "3"  # promoted from [dev-dependencies]

# CHANGED: Add backup feature (already present, just confirming)
rusqlite = { workspace = true, features = ["backup"] }
```

## Storage Crate Cargo.toml

No changes needed to `blufio-storage/Cargo.toml`. The rusqlite feature change at workspace level flows through automatically. The storage crate will compile against SQLCipher instead of SQLite transparently.

---

## Build System Impact Summary

| Build Target | Change | Risk |
|-------------|--------|------|
| macOS (dev, cargo build) | +OpenSSL vendored compile time | LOW -- works out of the box |
| x86_64-unknown-linux-musl (cross) | +OpenSSL vendored in Docker | MEDIUM -- test early, default cross image should work |
| aarch64-unknown-linux-musl (cross) | +OpenSSL vendored in Docker | MEDIUM -- same as x86_64 |
| x86_64-apple-darwin (native) | +OpenSSL vendored compile time | LOW -- works out of the box |
| aarch64-apple-darwin (native) | +OpenSSL vendored compile time | LOW -- works out of the box |
| CI (GitHub Actions) | +Minisign signing step in release workflow | LOW -- simple addition |

---

## Sources

### Crate Pages (version + metadata)
- [sd-notify 0.4.5 on crates.io](https://crates.io/crates/sd-notify) -- Published 2025-01-18
- [minisign-verify 0.2.5 on crates.io](https://crates.io/crates/minisign-verify) -- Published 2026-03-03
- [self-replace 1.5.0 on crates.io](https://crates.io/crates/self-replace) -- Published 2024-09-01
- [rusqlite 0.37.0 on crates.io](https://crates.io/crates/rusqlite) -- bundled-sqlcipher features documented

### API Documentation (verified)
- [sd-notify docs.rs](https://docs.rs/sd-notify/0.4.5/sd_notify/) -- NotifyState variants, notify() behavior
- [sd-notify source (lib.rs)](https://github.com/lnicola/sd-notify) -- Verified NOTIFY_SOCKET no-op behavior from source
- [minisign-verify docs.rs](https://docs.rs/minisign-verify/0.2.5/minisign_verify/) -- PublicKey, Signature, StreamVerifier API
- [self-replace docs.rs](https://docs.rs/self-replace/latest/self_replace/) -- self_replace() function, Unix behavior
- [rusqlite features](https://lib.rs/crates/rusqlite/features) -- Complete feature flag list with dependency chains

### Build System (verified)
- [libsqlite3-sys Cargo.toml](https://github.com/rusqlite/rusqlite/blob/master/libsqlite3-sys/Cargo.toml) -- bundled-sqlcipher-vendored-openssl feature definition
- [rusqlite issue #765](https://github.com/rusqlite/rusqlite/issues/765) -- bundled-sqlcipher design discussion
- [rusqlite issue #926](https://github.com/rusqlite/rusqlite/issues/926) -- bundled-sqlcipher usage guidance

### Protocol / Spec
- [SQLCipher (zetetic.net)](https://www.zetetic.net/sqlcipher/) -- SQLCipher is a strict SQLite superset
- [Minisign (jedisct1)](https://github.com/jedisct1/minisign) -- Minisign specification and tooling

---
*Stack research for: Blufio v1.2 Production Hardening*
*Researched: 2026-03-03*
