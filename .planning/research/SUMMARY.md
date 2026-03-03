# Project Research Summary

**Project:** Blufio v1.2 -- Production Hardening
**Domain:** Production hardening for existing Rust AI agent platform
**Researched:** 2026-03-03
**Confidence:** HIGH

## Executive Summary

Blufio v1.2 adds five production hardening features to the existing 16-crate Rust workspace (36,462 LOC, 118 requirements verified across v1.0 and v1.1): systemd Type=notify integration (sd_notify), SQLCipher database encryption at rest, Minisign binary signature verification, self-update with rollback, and backup integrity verification. The total estimated implementation is 930--1,600 LOC (3--4% of the existing codebase), with only 5 new crate dependencies (3 of which are zero-dependency crates), keeping the workspace well within its <80 direct dependency constraint. The approach is conservative: all features integrate into existing crates, primarily the binary crate, with no new workspace crates required.

The recommended approach orders features by dependency chain and blast radius. SQLCipher is the dominant risk -- it touches the database connection path used by 6+ consumers across the workspace and requires strict PRAGMA key ordering (must be the absolute first statement on every connection). The recommended mitigation is a shared `open_connection()` factory in `blufio-storage` that centralizes key application, preventing any connection path from skipping encryption. sd_notify and backup integrity are low-risk, low-complexity warm-ups. Minisign must precede self-update because unsigned binary replacement is a hard security violation. The critical architecture decision is using raw hex keys for SQLCipher (via `BLUFIO_DB_KEY` env var) rather than passphrases, which eliminates the 200--500ms PBKDF2 penalty per connection open.

The top risks are: (1) SQLCipher plaintext-to-encrypted migration destroying existing data if done incorrectly -- the documented `PRAGMA key` on a plaintext DB returns "file is encrypted or is not a database" and confused operators may delete the "corrupted" file; (2) the `bundled-sqlcipher-vendored-openssl` feature flag changing the build system for all targets, including musl cross-compilation -- this must be validated early in CI; (3) self-update replacing the binary with a partial download if signature verification is not enforced before the swap. All three risks have clear, well-documented prevention strategies detailed in the research.

## Key Findings

### Recommended Stack

The existing stack (tokio, axum, rusqlite 0.37, ring, reqwest 0.13, ed25519-dalek) is unchanged. Five new crates are added, three of which have zero transitive dependencies, and one existing crate (`rusqlite`) changes its feature flag from `bundled` to `bundled-sqlcipher-vendored-openssl`. Total binary size impact is estimated at 500--900KB (including vendored OpenSSL for SQLCipher). Clean build time increases by 30--90 seconds due to OpenSSL compilation from source.

**Core technologies:**
- `sd-notify 0.4.5`: systemd Type=notify readiness + watchdog pings -- pure Rust, zero dependencies, silent no-op on non-systemd platforms (macOS, Docker)
- `rusqlite 0.37` with `bundled-sqlcipher-vendored-openssl`: replaces `bundled` feature flag to compile SQLCipher + vendored OpenSSL from source -- fully self-contained, works on all targets including musl static builds
- `minisign-verify 0.2.5`: Ed25519-based binary signature verification -- zero dependencies, verify-only (no signing code), by Frank Denis (libsodium author)
- `self-replace 1.5.0`: atomic binary replacement via POSIX `rename()` on Unix -- by Armin Ronacher, zero dependencies on Unix
- `flate2 1` + `tar 0.4`: gzip decompression and tar extraction for release tarballs during self-update
- `ring 0.17` (existing): SHA-256 digest for backup file checksums -- no new dependency needed

**Critical version requirements:**
- rusqlite must stay at 0.37 (0.38 has 4 breaking changes unrelated to this milestone)
- `bundled-sqlcipher-vendored-openssl` must REPLACE `bundled`, not coexist alongside it

See `.planning/research/STACK.md` for full dependency analysis, Cargo.toml changes, build system impact matrix, and what NOT to add.

### Expected Features

All five features are P1 (must ship for v1.2 launch). Each feature addresses a gap that the incumbent (OpenClaw, Node.js/JSONL) cannot fix due to its architecture. The estimated total LOC is 930--1,600 across all features.

**Must have (table stakes):**
- sd_notify READY=1, STOPPING=1, and watchdog pings with graceful no-op on non-systemd platforms
- SQLCipher encryption at rest with `PRAGMA key` on every connection, centralized key management via `BLUFIO_DB_KEY`, and plaintext-to-encrypted migration CLI
- Minisign binary signature verification with embedded public key (compiled into binary)
- Self-update: version check against GitHub Releases, download + verify + atomic swap + pre-swap backup + health check + rollback
- Backup integrity: `PRAGMA integrity_check` after both backup and restore, fail on corruption

**Should have (add during implementation or in v1.2.x):**
- STATUS= messages during systemd startup phases (visible via `systemctl status`)
- `PRAGMA rekey` for encryption key rotation
- SHA-256 checksum sidecar files for backups
- `blufio verify` standalone CLI command for manual binary verification
- Streaming signature verification for future-proofing larger binaries

**Defer to v1.3+:**
- Auto-update on startup (never auto-update an always-on daemon)
- Delta/patch updates (full binary downloads are fine at 25--50MB)
- Automatic database repair (too dangerous for automatic execution)
- Per-table encryption (contradicts single-file deployment model)
- Socket activation (not needed for an always-on service)

See `.planning/research/FEATURES.md` for full feature tables, dependency graph, sizing estimates, and competitor analysis.

### Architecture Approach

All five features integrate into existing crates with no new workspace crates needed. The highest-impact change is SQLCipher, which requires modifying `Database::open()` in `blufio-storage` to accept an optional encryption key and creating a shared `open_connection()` factory that all 6 database consumers must use. This ensures `PRAGMA key` is always the first statement on every connection. sd_notify adds 3 function calls (serve.rs, shutdown.rs, memory_monitor). Self-update and Minisign live entirely in a new `update.rs` module in the binary crate. Backup integrity adds `verify_integrity()` calls to existing `run_backup()` and `run_restore()` functions.

**Major components and their responsibilities:**
1. `blufio-storage/database.rs` -- Gains `open_connection()` public helper that centralizes PRAGMA key + WAL + performance PRAGMAs; all 6 connection consumers must use this instead of raw `tokio_rusqlite::Connection::open()`
2. `blufio-config/model.rs` -- Gains `encryption_key: Option<String>` on StorageConfig, sourced from `BLUFIO_DB_KEY` env var via figment
3. `crates/blufio/src/serve.rs` -- sd_notify Ready (after mux.connect), Watchdog (in memory_monitor loop), Stopping (in signal handler); passes encryption_key to all connection opens
4. `crates/blufio/src/update.rs` (NEW) -- Self-update orchestration (GitHub API + reqwest download + minisign-verify + self-replace + rollback) and standalone verify command
5. `crates/blufio/src/backup.rs` -- Gains `verify_integrity()` calls post-backup and post-restore, with SQLCipher key awareness
6. `deploy/blufio.service` -- Changes Type=simple to Type=notify (must change in same commit as sd_notify code)

**Key patterns:**
- SQLCipher raw hex key via env var eliminates PBKDF2 latency (near-zero startup cost)
- Minisign verify BEFORE self-replace (never swap an unverified binary)
- Integrity check runs on BACKUP file, not source DB (avoids blocking single-writer thread)
- sd_notify watchdog in existing memory_monitor loop (5s tick, 300s WatchdogSec = 60x safety margin)

See `.planning/research/ARCHITECTURE.md` for full component dependency graph, data flow diagrams (before/after), concrete code patterns, and anti-patterns.

### Critical Pitfalls

See `.planning/research/PITFALLS.md` for all 16 pitfalls with full prevention strategies and phase assignments.

1. **SQLCipher migration destroys plaintext data** -- `PRAGMA key` on an existing plaintext DB returns "file is encrypted or is not a database." Confused operators delete the "corrupted" file. Prevention: dedicated `blufio migrate-db --encrypt` CLI with three-file strategy (original untouched, export to .tmp, verify, rename). NEVER auto-delete the original.

2. **PRAGMA key ordering breaks existing PRAGMA chain** -- `PRAGMA key` MUST be the absolute first statement, before WAL mode, before synchronous, before everything. If `PRAGMA journal_mode = WAL` runs first, SQLCipher fails. Prevention: restructure `Database::open()` to enforce strict ordering in one function. Add CI grep enforcement against raw `Connection::open()` calls.

3. **Self-update replaces binary with partial download** -- Interrupted HTTP download passed to `self-replace` overwrites the working binary with a truncated file. After restart, the deployment is bricked. Prevention: download to temp file, verify Minisign signature + file size + ELF magic BEFORE calling self-replace. Create `.rollback` copy of current binary before swap.

4. **bundled-sqlcipher-vendored-openssl breaks musl cross-builds** -- Feature flag change pulls in OpenSSL compilation from source inside the cross Docker image. Default images should have prerequisites (cc, perl, make) but must be validated early. Prevention: test musl cross-compilation in CI as the FIRST task of the SQLCipher phase.

5. **Minisign public key rotation breaks all deployed binaries** -- Embedded public key cannot verify signatures from a new signing key. Prevention: embed multiple keys (current + pre-generated next key). Release transitional binary signed with old key containing new key in embedded list.

## Implications for Roadmap

Based on combined research across all four files, the suggested phase structure follows the dependency chain identified in ARCHITECTURE.md and the ordering constraints identified in PITFALLS.md.

### Phase 1: Backup Integrity Verification

**Rationale:** Zero new dependencies. Standalone feature with no cross-cutting impact. Validates the `PRAGMA integrity_check` pattern that SQLCipher migration will later depend on for safety. The lowest-risk feature to start with, building confidence before tackling SQLCipher.

**Delivers:** `verify_integrity()` function in backup.rs; `PRAGMA integrity_check` after `run_backup()` and `run_restore()`; failure deletes corrupt backup and returns error; clear operator-facing status messages ("Backup complete: 5.2 MB, integrity: ok").

**Addresses from FEATURES.md:** Backup integrity check (P1 table stakes), integrity status reporting.

**Avoids from PITFALLS.md:** Pitfall 10 (integrity check blocking writer) -- runs on backup file with separate read-only connection, not source DB. Uses `quick_check` by default for performance.

### Phase 2: sd_notify Integration

**Rationale:** Tiny integration surface (3 function calls + 1 service file change). Zero impact on other crates. Immediately testable with `systemctl status`. Independent of all other features. Easy confidence builder before SQLCipher complexity.

**Delivers:** `NotifyState::Ready` in serve.rs (after mux.connect), `NotifyState::Watchdog` in memory_monitor loop, `NotifyState::Stopping` in shutdown handler, service file Type=simple to Type=notify, STATUS= messages during startup phases.

**Addresses from FEATURES.md:** sd_notify READY=1 + STOPPING=1 + watchdog (P1 table stakes), STATUS= messages (differentiator).

**Avoids from PITFALLS.md:** Pitfall 6 (sd_notify on non-systemd) -- sd-notify crate returns `Ok(())` when NOTIFY_SOCKET is absent, verified from source; Pitfall 13 (watchdog miscalculation) -- watchdog ping in memory_monitor at 5s interval vs. 300s WatchdogSec = 60x safety margin.

### Phase 3: SQLCipher Database Encryption

**Rationale:** Highest complexity and widest blast radius. Touches 6+ connection consumers across the workspace. Must follow backup integrity (Phase 1) because the migration CLI needs `PRAGMA integrity_check` to verify export correctness. Contains multiple sub-steps that must be ordered carefully.

**Delivers:** `bundled-sqlcipher-vendored-openssl` feature flag in workspace Cargo.toml; `encryption_key` field in StorageConfig; `Database::open(path, key)` signature change; `open_connection()` centralized factory in blufio-storage; all 6 connection callsites updated; `PRAGMA key` as first statement on every connection; `blufio migrate-db --encrypt` CLI for plaintext-to-encrypted migration with three-file strategy; backup/restore with encryption key; doctor encryption awareness.

**Uses from STACK.md:** rusqlite `bundled-sqlcipher-vendored-openssl` feature (replaces `bundled`).

**Addresses from FEATURES.md:** SQLCipher encryption at rest (P1 table stakes), centralized key management, plaintext migration.

**Avoids from PITFALLS.md:** Pitfall 1 (migration destroys data -- three-file strategy, never auto-delete original), Pitfall 2 (interrupted export -- wrap in transaction, verify row counts), Pitfall 3 (feature flag breaks build -- vendored OpenSSL for all targets), Pitfall 7 (PRAGMA ordering -- enforce in one function), Pitfall 8 (refinery before PRAGMA key -- strict open sequence), Pitfall 11 (KDF latency -- raw hex key bypasses PBKDF2).

### Phase 4: Minisign Signature Verification

**Rationale:** Prerequisite for self-update. Must be implemented and tested independently before the update flow calls it. Low complexity (80--120 LOC). No existing code changes -- purely additive new module.

**Delivers:** `minisign-verify` crate added; verification functions in `update.rs`; embedded Minisign public key constant (with space for next-generation key); `blufio verify` standalone CLI command; streaming verification support.

**Uses from STACK.md:** minisign-verify 0.2.5 (zero dependencies).

**Addresses from FEATURES.md:** Minisign binary verification (P1 table stakes), standalone verify CLI (differentiator), trusted comment verification (differentiator).

**Avoids from PITFALLS.md:** Pitfall 9 (key rotation chicken-and-egg -- embed multiple keys from day one), Pitfall 12 (verify-only crate trusted comments -- test explicitly, implement manual validation if needed).

### Phase 5: Self-Update with Rollback

**Rationale:** Last because it depends on Minisign (Phase 4) and has the most external integration complexity (GitHub API, HTTP downloads, file operations, atomic swap). Self-update without signature verification is a hard security violation (Anti-Pattern 2 in PITFALLS.md).

**Delivers:** `blufio update` CLI command; version check against GitHub Releases API; platform-appropriate binary download; Minisign signature verification before any file operations; pre-swap backup to `.rollback`; atomic binary swap via self-replace; post-swap health check; `blufio update --rollback` to revert; `blufio update --check` dry run; confirmation prompt before proceeding.

**Uses from STACK.md:** self-replace 1.5.0, flate2 1, tar 0.4, reqwest 0.13 (existing), tempfile 3 (promoted from dev-dep).

**Addresses from FEATURES.md:** Self-update with rollback (P1 table stakes), version check, health check (differentiator), rollback command (differentiator).

**Avoids from PITFALLS.md:** Pitfall 4 (partial download corruption -- verify size + signature + ELF magic before swap), Pitfall 5 (cross-filesystem EXDEV -- create temp file in same directory as executable, handle EXDEV with copy fallback), Pitfall 14 (stale .tmp files -- use NamedTempFile auto-cleanup, sweep on startup).

### Phase 6: Release Pipeline and CI Integration

**Rationale:** The self-update feature requires Minisign-signed release artifacts to exist. The CI/CD pipeline must be updated to generate keypairs, sign release tarballs, upload `.minisig` files alongside release assets, and validate that musl cross-builds work with vendored OpenSSL. This is operational work, not application code, but it is a hard prerequisite for self-update to be usable in production.

**Delivers:** Minisign keypair generation (secret key in GitHub Secrets); release workflow step to sign all artifacts; `.minisig` files uploaded as release assets; musl cross-build validated with SQLCipher; build time benchmarks documented.

### Phase Ordering Rationale

- **Backup integrity first** because SQLCipher migration needs it for safety verification of exported data
- **sd_notify second** as an independent, low-risk confidence builder
- **SQLCipher third** because it is the highest-risk feature with the widest blast radius and dominates the milestone's complexity budget
- **Minisign fourth** as standalone preparation for self-update
- **Self-update fifth** because it depends on Minisign (hard dependency) and has the most external integration points
- **Release pipeline sixth** because self-update is unusable without signed release artifacts
- Security constraints embedded per phase, not deferred: migration safety in Phase 3, verify-before-swap in Phase 5, no unsigned binaries ever

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 3 (SQLCipher):** musl cross-compilation with `bundled-sqlcipher-vendored-openssl` must be validated early in CI. The default `ghcr.io/cross-rs/x86_64-unknown-linux-musl:main` image should work but test FIRST. If issues arise, a `Cross.toml` pinning a known-good image version resolves it.
- **Phase 5 (Self-Update):** Integration testing with real GitHub Releases API. Release asset naming convention (target triple in filename) must be established as part of CI/CD before self-update can discover the correct binary.

Phases with standard patterns (skip research-phase):
- **Phase 1 (Backup Integrity):** Uses existing `PRAGMA integrity_check` pattern already in doctor.rs. Well-documented SQLite behavior.
- **Phase 2 (sd_notify):** 3 function calls. Verified from crate source code. Pure Rust, no platform issues.
- **Phase 4 (Minisign):** Zero-dependency crate, API verified from docs.rs. Thin wrapper around well-documented Ed25519 verification.
- **Phase 6 (Release Pipeline):** Standard GitHub Actions workflow addition. Minisign CLI is a single binary.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All crates verified on crates.io and docs.rs with version-specific API checks. Feature flag chains traced through rusqlite source. Zero-dependency crates (sd-notify, minisign-verify, self-replace) have minimal audit surface. |
| Features | HIGH | Feature scope verified against existing codebase (6 source files read directly). Feature dependencies mapped as DAG with no circular dependencies. LOC estimates based on concrete API surface analysis. Competitor gap analysis confirms structural advantages. |
| Architecture | HIGH | Integration points identified by reading actual source files (serve.rs, database.rs, backup.rs, doctor.rs, model.rs, shutdown.rs). All 6 database connection callsites enumerated. Data flow before/after diagrams produced. Anti-patterns documented with alternatives. |
| Pitfalls | HIGH | SQLCipher pitfalls sourced from official Zetetic FAQ and API documentation. sd_notify behavior verified from crate source code (NOTIFY_SOCKET check path). self-replace Unix behavior verified from docs.rs. EXDEV cross-filesystem behavior verified from POSIX specification. |

**Overall confidence:** HIGH

### Gaps to Address

- **musl cross-compilation with vendored OpenSSL:** Well-established pattern but not yet validated for this specific workspace. Test the `cross` build for `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` as the first task of Phase 3. If the default cross Docker image lacks prerequisites, pin a known-good image in `Cross.toml`.

- **Self-update integration testing:** The update flow depends on GitHub Releases API conventions (asset naming, tag format). These conventions must be established in the release pipeline (Phase 6) before self-update can be fully tested. Recommend implementing Phase 6 in parallel with Phase 5 or immediately after.

- **minisign-verify trusted comment validation:** The verify-only crate should validate trusted comment signatures, but this must be confirmed by reading the crate source or writing an explicit test. If it does not, manual validation (another Ed25519 signature verification over `signature || trusted comment`) is straightforward.

- **Refinery migration runner with SQLCipher:** Refinery's `runner().run()` takes a `&mut Connection` and immediately queries `refinery_schema_history`. The connection must have `PRAGMA key` already applied. The current code structure in `Database::open()` handles this correctly if the ordering is enforced, but an explicit integration test with an encrypted database is needed.

## Sources

### Primary (HIGH confidence)
- [SQLCipher API Documentation (Zetetic)](https://www.zetetic.net/sqlcipher/sqlcipher-api/) -- PRAGMA key ordering, sqlcipher_export, cipher_migrate, KDF settings
- [SQLCipher Plaintext Migration FAQ](https://discuss.zetetic.net/t/how-to-encrypt-a-plaintext-sqlite-database-to-use-sqlcipher-and-avoid-file-is-encrypted-or-is-not-a-database-errors/868) -- Official migration procedure
- [sd-notify 0.4.5 docs.rs](https://docs.rs/sd-notify/0.4.5/sd_notify/) -- NotifyState variants, NOTIFY_SOCKET no-op behavior
- [sd-notify source (GitHub)](https://github.com/lnicola/sd-notify) -- Verified NOTIFY_SOCKET no-op path from source code
- [systemd sd_notify(3) man page](https://www.freedesktop.org/software/systemd/man/latest/sd_notify.html) -- READY=1, STOPPING=1, WATCHDOG=1, STATUS=, ExtendTimeoutUsec
- [minisign-verify 0.2.5 docs.rs](https://docs.rs/minisign-verify/0.2.5/minisign_verify/) -- PublicKey, Signature, StreamVerifier API
- [self-replace docs.rs](https://docs.rs/self-replace/latest/self_replace/) -- Unix atomic rename behavior
- [rusqlite 0.37 features](https://docs.rs/crate/rusqlite/0.37.0/features) -- bundled-sqlcipher feature flag chain
- [libsqlite3-sys Cargo.toml](https://github.com/rusqlite/rusqlite/blob/master/libsqlite3-sys/Cargo.toml) -- bundled-sqlcipher-vendored-openssl definition

### Secondary (MEDIUM confidence)
- [rusqlite bundled-sqlcipher issue #765](https://github.com/rusqlite/rusqlite/issues/765) -- Build integration discussion
- [rusqlite issue #926](https://github.com/rusqlite/rusqlite/issues/926) -- bundled-sqlcipher usage guidance
- [Minisign specification](https://jedisct1.github.io/minisign/) -- Key format, trusted comments, signature structure
- [SQLCipher WAL mode discussion](https://discuss.zetetic.net/t/can-i-use-pragma-journal-mode-wal-with-an-sqliteconnection/770) -- WAL compatibility with SQLCipher
- [Writing a proper systemd daemon in Rust](https://deterministic.space/writing-a-daemon.html) -- Type=notify patterns

### Codebase (HIGH confidence -- direct source reading)
- `crates/blufio/src/serve.rs` -- Startup sequence, sd_notify insertion points, connection opens
- `crates/blufio/src/backup.rs` -- Backup API, restore with safety backup, integrity check insertion points
- `crates/blufio/src/doctor.rs` -- Existing PRAGMA integrity_check in check_db_integrity()
- `crates/blufio-storage/src/database.rs` -- Database::open(), PRAGMA ordering, connection setup
- `crates/blufio-config/src/model.rs` -- StorageConfig struct for encryption_key addition
- `crates/blufio-agent/src/shutdown.rs` -- Signal handler for sd_notify Stopping
- `Cargo.toml` (workspace) -- rusqlite 0.37 with bundled feature, all workspace dependencies

---
*Research completed: 2026-03-03*
*Ready for roadmap: yes*
