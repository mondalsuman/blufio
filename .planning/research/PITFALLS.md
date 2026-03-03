# Domain Pitfalls: v1.2 Production Hardening

**Domain:** Adding sd_notify, SQLCipher, Minisign, self-update, and backup integrity to existing Rust AI agent platform
**Researched:** 2026-03-03
**Confidence:** HIGH (official SQLCipher docs, rusqlite crate features, sd_notify crate source, self-replace crate docs, minisign specification, systemd man pages)

---

## Critical Pitfalls

Mistakes that cause data loss, broken deployments, or require rewrites.

---

### Pitfall 1: SQLCipher Migration Destroys Existing Plaintext Data

**What goes wrong:**
Developers attempt to "encrypt in place" by opening the existing plaintext `blufio.db` and calling `PRAGMA key = 'mypassword'`. SQLCipher interprets this as "open an already-encrypted database with this key," fails to decrypt (because the DB is plaintext), and returns error code 26: "file is encrypted or is not a database." The developer, confused, deletes the "corrupted" database and starts fresh -- losing all sessions, conversation history, cost ledger data, vault credentials, memory embeddings, skill registry, and MCP pin store. On a production VPS that has been running for months, this is catastrophic.

**Why it happens:**
`PRAGMA key` and `sqlite3_key()` can ONLY be used when opening a brand-new database (first time) or when opening an already-encrypted database. They cannot retroactively encrypt an existing plaintext database. This is the single most documented SQLCipher misunderstanding -- Zetetic (the SQLCipher maintainers) have a dedicated FAQ page for this exact error. The correct procedure requires `ATTACH DATABASE` + `sqlcipher_export()` to copy data to a new encrypted file.

**Consequences:**
- Complete data loss if the operator mistakes the error for corruption and deletes the database.
- Blufio stores everything in one SQLite file: sessions, messages, cost ledger, vault (AES-256-GCM encrypted credentials), memory embeddings, skill registry, MCP tool hashes, refinery migration history. Losing the DB means losing ALL of this.
- The vault master key is derived via Argon2id from the operator's passphrase, but the encrypted vault entries are IN the SQLite file. No DB = no vault recovery.
- Even if the operator has a backup, restoring a plaintext backup into a now-SQLCipher-expecting application creates the same error in reverse.

**Prevention:**
- Implement a dedicated `blufio migrate-db --encrypt` CLI command that performs the migration as a multi-step atomic process:
  1. Verify the source DB is valid plaintext SQLite (`PRAGMA integrity_check`).
  2. Create encrypted destination file using `ATTACH DATABASE 'encrypted.db' AS encrypted KEY 'key'` + `SELECT sqlcipher_export('encrypted')`.
  3. Run `PRAGMA integrity_check` on the encrypted database to verify completeness.
  4. Compare row counts on every table between source and destination.
  5. Rename: `blufio.db` -> `blufio.db.plaintext-backup`, `encrypted.db` -> `blufio.db`.
  6. Print clear instructions: "Migration complete. Plaintext backup at blufio.db.plaintext-backup. Delete it after verifying the encrypted database works."
- NEVER delete the original plaintext file automatically. The operator decides when to shred it.
- The migration command must be idempotent: if interrupted, it can be re-run safely because the original file is untouched until the final rename.
- Add a `blufio doctor` check that detects when the config expects SQLCipher but the database file header is plaintext SQLite (bytes 0-15 are "SQLite format 3\0" for plaintext; encrypted databases have random bytes at offset 0).

**Detection:**
- Error "file is encrypted or is not a database" on startup after enabling SQLCipher in config.
- Database file size drops to 0 bytes after a failed migration attempt.
- `blufio doctor` reports "database format mismatch: config expects encrypted, file is plaintext."

**Phase to address:** This must be a standalone migration phase, executed before any other SQLCipher work. The migration command ships first, tested thoroughly, before the main application switches to SQLCipher mode.

---

### Pitfall 2: SQLCipher Migration Interrupted Mid-Export Leaves Corrupted State

**What goes wrong:**
The `sqlcipher_export()` function copies the entire database schema and data to the attached encrypted database. If the process is killed, the machine loses power, or disk space runs out during export, the destination encrypted database is incomplete. It may have some tables but not others, or partial data in tables. If the application then tries to open this partial file, it either fails (best case) or opens successfully but is missing data (worst case -- silent data loss).

**Why it happens:**
`sqlcipher_export()` is NOT atomic at the filesystem level. It executes a series of CREATE TABLE and INSERT statements internally. While SQLite's transaction mechanism protects individual statements, the export function's behavior on interruption is not guaranteed to leave the destination in a consistent state. Power failure during any write to the encrypted destination file can corrupt it because the encrypted file's WAL may not be fully flushed. Furthermore, Blufio's existing database has 6 migration versions worth of tables across sessions, messages, queue, cost_ledger, memory, skills, MCP wiring, and vault -- the export of all this data takes non-trivial time for a large database.

**Consequences:**
- Encrypted destination file is partial or corrupted.
- If the migration script assumed success and renamed files, the original is now gone and the encrypted version is broken.
- Refinery migration history (`refinery_schema_history` table) may be missing from the encrypted copy, causing refinery to re-run all migrations on next startup, which fails because some tables already exist.

**Prevention:**
- Wrap the entire `sqlcipher_export()` in a transaction on the destination database: `BEGIN EXCLUSIVE` before export, `COMMIT` after. If interrupted, the destination file's incomplete transaction is rolled back on next open.
- After export but before any renaming, run `PRAGMA integrity_check` on the destination AND verify row counts match the source for every table.
- Use a three-file strategy: original stays untouched, export writes to a `.tmp` file, only after verification does `.tmp` get renamed to the final name. `rename()` is atomic on the same filesystem.
- Check available disk space before starting: the encrypted database will be roughly the same size as the original (possibly slightly larger due to per-page encryption overhead and the default 4096-byte page size).
- Log progress: "Migrating table sessions (1/8)... Migrating table messages (2/8)..." so operators can see where it stopped if interrupted.

**Detection:**
- Encrypted destination file exists but is smaller than expected.
- `PRAGMA integrity_check` fails on the destination.
- Table count or row count mismatch between source and destination.
- Refinery migration history table missing from destination.

**Phase to address:** Same migration phase as Pitfall 1. The migration CLI command must implement all these safety checks.

---

### Pitfall 3: SQLCipher Changes rusqlite Feature Flags and Breaks the Build

**What goes wrong:**
Blufio currently uses `rusqlite = { version = "0.37", features = ["bundled"] }` which compiles vanilla SQLite from source. Switching to SQLCipher requires replacing the `bundled` feature with `bundled-sqlcipher` (or `bundled-sqlcipher-vendored-openssl` for static builds). This is a workspace-wide change because `rusqlite` is a workspace dependency used by 8 crates (blufio-storage, blufio-vault, blufio-cost, blufio-memory, blufio-skill, blufio-mcp-server, blufio-mcp-client, blufio binary). The feature change also pulls in OpenSSL (or requires a system crypto library), which dramatically changes the build process, CI pipeline, and binary size.

**Why it happens:**
The `bundled-sqlcipher` feature compiles SQLCipher (a fork of SQLite) from C source and links it statically. SQLCipher requires a crypto backend. Two options exist:
1. `bundled-sqlcipher`: Links against system-installed OpenSSL/LibreSSL. Works on dev machines but breaks `musl` static builds because there is no "system" crypto on a musl target.
2. `bundled-sqlcipher-vendored-openssl`: Bundles OpenSSL source and compiles it. Works everywhere but adds significant compile time (OpenSSL takes 2-5 minutes to compile) and binary size (estimated 2-5 MB increase for the OpenSSL symbols).

**Consequences:**
- Binary size increases from ~25MB to ~27-30MB due to statically linked OpenSSL. This is within the 50MB constraint but notable.
- Compile time increases significantly (OpenSSL C compilation on every clean build).
- The `release-musl` profile (used for production static binary) must use `bundled-sqlcipher-vendored-openssl` because there is no system OpenSSL to link against.
- CI must install OpenSSL dev headers (or use vendored) -- the existing CI pipeline that just runs `cargo build` will break.
- The `refinery` crate's `rusqlite` feature must also be compatible with SQLCipher. Refinery runs migrations via raw `rusqlite::Connection` -- it must receive the connection AFTER `PRAGMA key` has been set, not before.
- tokio-rusqlite wraps `rusqlite::Connection` in a background thread. The `PRAGMA key` must be the FIRST operation on the connection, before any migration or PRAGMA setup. The current `database.rs` applies PRAGMAs (WAL mode, synchronous, busy_timeout, foreign_keys, cache_size, temp_store) immediately after open. With SQLCipher, `PRAGMA key` must come before ALL of these.

**Prevention:**
- Use `bundled-sqlcipher-vendored-openssl` for all builds (dev and production). This eliminates the "works on dev, breaks in CI" problem. Accept the compile time cost.
- Modify the workspace Cargo.toml to change the feature: `rusqlite = { version = "0.37", features = ["bundled-sqlcipher-vendored-openssl"] }`.
- In `blufio-storage/src/database.rs`, restructure PRAGMA ordering:
  ```rust
  // MUST be first -- before ANY other PRAGMA or query
  conn.execute_batch(&format!("PRAGMA key = '{}';", key))?;
  // Now WAL mode and other PRAGMAs
  conn.execute_batch("PRAGMA journal_mode = WAL;")?;
  conn.execute_batch("PRAGMA synchronous = NORMAL; ...")?;
  // Now migrations
  crate::migrations::run_migrations(conn)?;
  ```
- Make the encryption key available to the database initialization code. Currently `Database::open()` takes only a path. It must also accept an optional encryption key.
- Add a feature flag `encryption` to blufio-storage that gates the `PRAGMA key` call, so development and testing can still use plaintext SQLite without the SQLCipher overhead if desired.

**Detection:**
- Build failure: "error: linking with `cc` failed" due to missing OpenSSL headers.
- Runtime: "file is encrypted or is not a database" because `PRAGMA key` was called after another PRAGMA touched the database.
- Runtime: refinery migrations fail because they ran before `PRAGMA key`.

**Phase to address:** This is the foundational build change that must happen first, before any SQLCipher feature code. All other SQLCipher work depends on the feature flags compiling correctly.

---

### Pitfall 4: Self-Update Replaces Binary While Service Is Running -- Partial Write Corruption

**What goes wrong:**
`blufio update` downloads a new binary and attempts to replace the running executable. On Unix, replacing a running binary with `rename()` is technically safe because the old file's inode stays valid as long as the process has it open. But if the download is interrupted (network failure, disk full, timeout), the partially downloaded file is written next to the running binary as a temp file. If the replacement logic uses `write()` + `rename()` but the `write()` is incomplete, the temp file contains a truncated binary. If the code doesn't verify the download before renaming, the replacement overwrites the working binary with a broken one.

**Why it happens:**
HTTP downloads are unreliable. A 25MB binary download on a $4/month VPS with limited bandwidth can easily be interrupted. The `self-replace` crate handles the atomic swap correctly (temp file + rename), but it trusts the caller to provide a valid replacement binary. If the caller (Blufio's update logic) passes an incomplete download to `self_replace::self_replace()`, the crate will dutifully swap it in.

**Consequences:**
- The running binary continues to work (Unix keeps the old inode open), but after restart, the corrupted binary fails to execute.
- On a remote VPS with no other access method, this bricks the deployment. The operator must SSH in and manually restore from backup.
- If the systemd service has `Restart=on-failure`, it will repeatedly try to start the corrupted binary and fail, filling logs.

**Prevention:**
- Download to a temp file first (`blufio-update.XXXX.tmp` in the same directory as the binary).
- Verify the download BEFORE calling self-replace:
  1. Check file size matches the expected size from the release manifest.
  2. Verify the Minisign signature of the downloaded file (this is why Minisign must be implemented before self-update).
  3. On Linux, attempt to verify the binary is a valid ELF: check magic bytes `\x7fELF` at offset 0.
- Only after all verification passes, call `self_replace::self_replace()` with the verified temp file.
- If verification fails, delete the temp file and report the error. The running binary is untouched.
- Create a rollback mechanism: before replacing, copy the current binary to `blufio.rollback`. If the new binary fails to start (detected by a health check within 30 seconds), restore from rollback.
- Implement download resume: if the download is interrupted, keep the partial file and resume with HTTP Range headers on retry.

**Detection:**
- Downloaded file size does not match expected size from release manifest.
- Minisign signature verification fails.
- Binary fails to start after update (exit code non-zero within 1 second).
- systemd shows rapid restart cycling after an update.

**Phase to address:** Self-update phase. Minisign verification must be implemented before self-update so that download integrity can be verified.

---

### Pitfall 5: Self-Update on Cross-Filesystem Mounts Fails with EXDEV

**What goes wrong:**
The `self-replace` crate places the new binary as a temp file next to the current executable and performs an atomic `rename()`. On Unix, `rename()` only works within the same filesystem. If `/usr/local/bin/blufio` is on one filesystem and `/tmp` (where downloads go) is on another, and the update logic tries to rename from `/tmp/blufio-new` to `/usr/local/bin/blufio`, the kernel returns `EXDEV` (Invalid cross-device link). The update fails.

More subtly: Docker containers, snap packages, and some VPS providers mount `/usr/local/bin` as a read-only overlay filesystem. `rename()` fails even if the source and destination appear to be on the same path.

**Why it happens:**
`rename(2)` is a filesystem-level operation that moves a directory entry. It cannot move data between filesystems -- that requires copy + delete. The `self-replace` crate's documentation notes that temporary files are placed "right next to the current executable," which should avoid EXDEV in normal cases. But if the download temp file is created elsewhere (e.g., in `/tmp` or a user-specified directory), EXDEV occurs.

**Consequences:**
- Update fails with an obscure OS error that most operators will not understand.
- If the code catches EXDEV and falls back to copy + delete, the copy is NOT atomic. A crash during copy leaves a partially written binary.

**Prevention:**
- Always create the download temp file in the same directory as the current executable, not in `/tmp`. Use `tempfile::NamedTempFile::new_in(executable_dir)`.
- If the temp file creation fails (read-only filesystem), fall back to a different strategy: download to `/tmp`, verify, then use `std::fs::copy()` + `std::fs::rename()` where the copy goes to a temp file in the same directory, then rename replaces the original.
- Handle EXDEV explicitly: if `rename()` returns EXDEV, fall back to copy-then-rename within the same directory.
- Detect read-only filesystems at update start: attempt to create a test file in the executable's directory. If it fails with EROFS/EPERM, tell the operator they need to update manually or remount.

**Detection:**
- Error message containing "Invalid cross-device link" or "EXDEV" or "errno 18".
- Error message containing "Read-only file system" or "EROFS".

**Phase to address:** Self-update phase. Test on Docker containers and various VPS configurations.

---

## Moderate Pitfalls

---

### Pitfall 6: sd_notify on Non-Systemd Systems Causes Compile or Runtime Failure

**What goes wrong:**
The `sd-notify` Rust crate depends on `libc` and uses Unix domain sockets to communicate with systemd via the `NOTIFY_SOCKET` environment variable. On macOS (the development platform), there is no systemd. Two failure modes:
1. **Compile failure on macOS**: If the crate uses Linux-specific socket types (e.g., `SOCK_CLOEXEC`), it won't compile on macOS.
2. **Runtime no-op confusion**: The underlying `sd_notify(3)` specification says if `NOTIFY_SOCKET` is not set, the function returns 0 (not an error). But the Rust crate's `notify()` function returns `Result<(), Error>` and its behavior when `NOTIFY_SOCKET` is absent may return `Ok(())` silently or `Err(...)` depending on the implementation.

The service file currently has `Type=simple` with `WatchdogSec=300`. Changing to `Type=notify` without implementing sd_notify means systemd will wait for the `READY=1` notification forever, then kill the service after the startup timeout (default 90 seconds). The service appears to hang on startup.

**Why it happens:**
Developers add `sd-notify` to dependencies, change the service file to `Type=notify`, deploy to production, and the binary never sends `READY=1` because the sd_notify call silently fails or was gated behind a compile-time feature that isn't enabled in the production build.

**Consequences:**
- Service fails to start on production systemd (Type=notify but no READY=1 sent).
- Watchdog kills the service every 300 seconds because no `WATCHDOG=1` pings are sent.
- On macOS development, the sd_notify calls either don't compile or are silent no-ops, so the developer never tests the actual notification path.

**Prevention:**
- Gate sd_notify behind a Cargo feature flag: `systemd` feature on the `blufio` crate. Only enabled in production builds.
  ```toml
  [features]
  systemd = ["dep:sd-notify"]
  ```
- The `blufio serve` startup code checks `std::env::var("NOTIFY_SOCKET")` before calling sd_notify. If absent, skip all sd_notify calls and log "sd_notify: NOTIFY_SOCKET not set, skipping (not running under systemd)".
- Update the service file from `Type=simple` to `Type=notify` ONLY in the same commit that adds the `READY=1` notification. Never change one without the other.
- Implement watchdog pinging in the heartbeat runner: since Blufio already has `HeartbeatRunner` that ticks periodically, add `sd_notify::notify(false, &[NotifyState::Watchdog])` to the heartbeat tick. This reuses existing infrastructure.
- Add a `blufio doctor` check: if `NOTIFY_SOCKET` is set, verify that the service file uses `Type=notify` and `WatchdogSec` is configured.
- On macOS, use `#[cfg(target_os = "linux")]` to compile-gate the sd_notify calls entirely. On non-Linux, the functions are no-ops.

**Detection:**
- systemd journal shows "blufio.service: State 'start' timed out. Killing."
- `systemctl status blufio` shows "activating (start)" indefinitely.
- Watchdog-triggered restarts every `WatchdogSec` seconds in the journal.

**Phase to address:** sd_notify phase. The feature flag and NOTIFY_SOCKET check are trivial but missing either one causes production outage.

---

### Pitfall 7: SQLCipher PRAGMA Key Ordering Breaks Existing PRAGMA Chain

**What goes wrong:**
Blufio's `database.rs` currently applies PRAGMAs in this order after opening the connection:
```rust
conn.execute_batch("PRAGMA journal_mode = WAL;")?;
conn.execute_batch("PRAGMA synchronous = NORMAL;
                     PRAGMA busy_timeout = 5000;
                     PRAGMA foreign_keys = ON;
                     PRAGMA cache_size = -16000;
                     PRAGMA temp_store = MEMORY;")?;
```

With SQLCipher, `PRAGMA key` MUST be the absolute first operation on the connection. SQLCipher uses "just-in-time" key derivation -- the key is applied when the database is first touched. If `PRAGMA journal_mode = WAL` runs before `PRAGMA key`, SQLCipher attempts to read the database header without the key, fails, and the connection is in an error state. All subsequent operations fail with "file is encrypted or is not a database."

**Why it happens:**
The developer adds `PRAGMA key` to the PRAGMA chain but places it after `journal_mode = WAL` (because WAL mode "must be set outside any transaction" per the existing comment in the code). With SQLCipher, the ordering constraint is even stricter: key FIRST, then everything else.

**Consequences:**
- Database connection fails on every open attempt after migration to SQLCipher.
- The error message is identical to Pitfall 1 ("file is encrypted or is not a database"), making debugging confusing -- is the DB not encrypted, or is the key wrong, or is the PRAGMA ordering wrong?
- tokio-rusqlite runs the PRAGMA setup on a background thread via `call()`. If the call fails, the error propagates as a generic storage error that may not clearly indicate the PRAGMA ordering issue.

**Prevention:**
- Restructure `Database::open()` to accept an optional encryption key parameter:
  ```rust
  pub async fn open(path: &str, encryption_key: Option<&str>) -> Result<Self, BlufioError>
  ```
- In the connection setup closure, enforce strict ordering:
  1. `PRAGMA key = '...'` (if encryption_key is Some)
  2. `PRAGMA journal_mode = WAL`
  3. All other PRAGMAs
  4. Migrations
- Add an integration test that opens an encrypted database with the wrong PRAGMA order and verifies it fails, then opens with the correct order and verifies it succeeds.
- Add a comment block in the code:
  ```rust
  // CRITICAL: SQLCipher requires PRAGMA key as the FIRST operation.
  // Do NOT add any PRAGMA or query before this line.
  // See: https://discuss.zetetic.net/t/...
  ```

**Detection:**
- "file is encrypted or is not a database" error on startup after enabling SQLCipher.
- Works in tests (which may use plaintext) but fails in production (which uses encrypted).

**Phase to address:** SQLCipher implementation phase, immediately after the build change (Pitfall 3).

---

### Pitfall 8: Refinery Migrations Cannot Run Before PRAGMA key

**What goes wrong:**
Blufio uses the `refinery` crate with `embed_migrations!` to run database migrations automatically on startup. Refinery's `runner().run()` method takes a `&mut rusqlite::Connection` and immediately queries the `refinery_schema_history` table to determine which migrations have been applied. With SQLCipher, this query happens BEFORE the developer has a chance to set `PRAGMA key` if the migration call is in the wrong position.

Additionally, the migration export during `sqlcipher_export()` copies the `refinery_schema_history` table along with everything else. But if the migration runner is invoked on the encrypted database and the key derivation has already been performed, refinery sees an up-to-date history and does nothing. If the key derivation fails silently, refinery tries to create its history table in an unkeyed connection and gets "file is encrypted or is not a database."

**Why it happens:**
The current `Database::open()` flow is: open connection -> apply PRAGMAs -> run migrations. With SQLCipher, the flow must be: open connection -> PRAGMA key -> apply PRAGMAs -> run migrations. But refinery's API takes the raw connection, and developers may call it before PRAGMA key if they refactor the initialization code carelessly.

**Consequences:**
- Startup crashes with an opaque refinery error that wraps the SQLCipher error.
- If refinery somehow runs against an unkeyed encrypted database, it may create a separate unencrypted `refinery_schema_history` table in a temp area, leading to state confusion.

**Prevention:**
- The `Database::open()` method is the single point of control. The sequence must be enforced in one function, not spread across modules:
  ```
  open -> PRAGMA key -> PRAGMAs -> run_migrations -> return Database
  ```
- Refinery never gets a connection that hasn't had `PRAGMA key` applied.
- Add a guard: after `PRAGMA key`, execute `SELECT count(*) FROM sqlite_master` as a canary. If this fails, the key is wrong -- abort with a clear error message BEFORE running migrations.
- Unit test: call `run_migrations()` on an encrypted connection without PRAGMA key first and verify the error is caught and reported clearly.

**Detection:**
- refinery error: "Migration error: file is encrypted or is not a database"
- `refinery_schema_history` table exists in destination but shows 0 migrations applied (migration state lost during export).

**Phase to address:** SQLCipher implementation phase, same as Pitfall 7.

---

### Pitfall 9: Minisign Public Key Embedded in Binary Creates Update Chicken-and-Egg

**What goes wrong:**
Minisign verification requires a public key to verify signatures. If the public key is compiled into the binary (the most secure approach -- no TOML config tampering), then rotating the key requires releasing a new binary. But the new binary is signed with the NEW key, and the old binary only knows the OLD key. The old binary cannot verify the new binary's signature, so the self-update mechanism refuses to install it. The operator is stuck on the old version forever unless they manually download and replace the binary.

**Why it happens:**
Key rotation is an inherent challenge with embedded public keys. Hardware security modules and certificate chains solve this in TLS, but Minisign is simpler -- it has no certificate chain, no key hierarchy, and no built-in rotation mechanism. The key ID in the signature identifies which key was used, but the verifier must have the corresponding public key already.

**Consequences:**
- Key rotation permanently breaks the self-update mechanism for all deployed binaries.
- Operators must manually update, defeating the purpose of self-update.
- If the signing private key is compromised, there is no way to revoke it and switch to a new key via the normal update path.

**Prevention:**
- Embed MULTIPLE public keys in the binary: the current key and one or two "next" keys. The verification logic tries each key until one succeeds.
  ```rust
  const SIGNING_KEYS: &[&str] = &[
      "RWQ...", // current key (2026)
      "RWR...", // next key (pre-generated, private key stored offline)
  ];
  ```
- When a new key is needed, the transitional binary (signed with the old key, but containing the new key in its embedded list) is released first. All deployed instances update to the transitional version. Then subsequent releases are signed with the new key, and all instances can verify because they have the transitional binary with both keys.
- Add a TOML config option `[update] trusted_keys = ["RWQ...", "RWR..."]` as an escape hatch. If the embedded keys cannot verify, check the config file. This is less secure (config can be tampered) but provides a recovery path.
- Print a warning when the signature was verified with a non-primary key: "Update verified with backup key. A key rotation may be in progress."
- Document the key rotation procedure in the operator guide.

**Detection:**
- Self-update reports "signature verification failed" after a key rotation.
- All deployed instances stop updating simultaneously.

**Phase to address:** Minisign phase. The multi-key strategy must be designed before the first public key is embedded. Changing the key embedding strategy after release is a breaking change for all deployed instances.

---

### Pitfall 10: Backup Integrity Check Blocks Backup Operation for Large Databases

**What goes wrong:**
Adding `PRAGMA integrity_check` after every backup sounds like a good idea, but it is a FULL TABLE SCAN of every page in the database. For a 100MB database (realistic after months of operation with memory embeddings), `integrity_check` takes 1-5 seconds. During this time, the backup operation holds the database connection, and on a single-writer architecture (tokio-rusqlite with one background thread), ALL database writes are blocked. On a busy agent handling multiple sessions, this causes visible latency spikes -- the agent stops responding for several seconds during backup.

**Why it happens:**
`PRAGMA integrity_check` verifies every B-tree page, every index, every overflow page. It is O(n) in database size. The existing `blufio doctor --deep` already runs `integrity_check` but it runs manually and infrequently. Running it on every backup (which may happen on a cron schedule, e.g., every hour) turns a manual diagnostic into a recurring performance hit.

**Consequences:**
- Backup takes seconds instead of milliseconds for large databases.
- During integrity check, the single-writer thread is occupied, so all `INSERT`, `UPDATE`, and `DELETE` operations queue up. Agent loop stalls.
- On a $4/month VPS with slow storage, integrity check on a 500MB database could take 10-30 seconds, causing Telegram API timeouts (30-second long-polling timeout).
- The watchdog timer (WatchdogSec=300) is unlikely to trigger, but accumulated backup latency over time degrades the operator experience.

**Prevention:**
- Use `PRAGMA quick_check` instead of `PRAGMA integrity_check` for post-backup verification. `quick_check` verifies page-level consistency without checking index ordering -- it is significantly faster (10-100x) for large databases.
- Run the integrity check on the BACKUP FILE, not the source database. Open a separate read-only connection to the backup file and run `integrity_check` there. This does not block the main writer thread.
- Make integrity checking configurable: `[backup] verify = "quick"` (default), `"full"`, or `"none"`.
- Run full integrity check only on restore operations (where correctness is critical and a one-time delay is acceptable).
- For scheduled backups, run `quick_check` on the backup file in a separate tokio task, not on the main writer thread. Report the result asynchronously.

**Detection:**
- Backup operation takes >1 second (previously took <100ms).
- Agent response times spike during backup windows.
- `blufio doctor` shows elevated writer thread queue depth during backup.

**Phase to address:** Backup integrity phase. The check must run on the backup file, not the source, and use `quick_check` by default.

---

### Pitfall 11: SQLCipher KDF Adds Startup Latency

**What goes wrong:**
SQLCipher uses PBKDF2-HMAC-SHA512 with 256,000 iterations by default (SQLCipher 4.x). Every time the database is opened and `PRAGMA key` is executed, the KDF runs to derive the encryption key. On a $4/month VPS with a low-end CPU, this takes 200-500ms. Blufio already has 2-5 second cold start time (embedding model load + TLS initialization). Adding 200-500ms per database open is noticeable. Worse, Blufio opens the database connection on EVERY `blufio` CLI invocation, including `blufio status`, `blufio doctor`, and `blufio backup` -- not just `blufio serve`. Quick commands that currently take <100ms now take 500ms+ due to KDF.

**Why it happens:**
The high iteration count is intentional -- it makes brute-force key attacks expensive. Reducing iterations weakens security. But the performance cost is paid on every connection open, not just once.

**Consequences:**
- `blufio serve` cold start goes from 2-5s to 2.5-5.5s. Acceptable but visible.
- `blufio status` (quick check) goes from <100ms to 500ms+. Feels sluggish.
- `blufio doctor` opens the database twice (once for config, once for deep checks). KDF runs twice: 1 second overhead.
- In tests, every test that opens a database gets 200-500ms slower. A test suite with 50 database tests adds 10-25 seconds.

**Prevention:**
- For CLI commands that only read (status, doctor), consider using a cached derived key stored in memory-mapped state (e.g., a Unix domain socket to the running `blufio serve` process).
- For tests, use a raw key (`PRAGMA key = "x'..."` with a hex key) instead of a passphrase, bypassing KDF entirely. Or use `PRAGMA kdf_iter = 1` in test builds (compile-time feature flag, NEVER in production).
- Accept the startup latency for `blufio serve` -- it runs once and stays up for months.
- For `blufio backup` and `blufio restore`, the KDF cost is acceptable because these are infrequent operations.
- Document the latency impact in the changelog so operators are not surprised.
- Consider offering `PRAGMA cipher_memory_security = OFF` for non-sensitive deployments (disables memory scrubbing, improves performance slightly).

**Detection:**
- Startup time regression measured in CI benchmarks.
- Operator complaints about CLI responsiveness.

**Phase to address:** SQLCipher implementation phase. Accepted tradeoff, not a bug. But test performance must be addressed with raw keys.

---

### Pitfall 12: Minisign Verify-Only Crate Does Not Support Trusted Comments

**What goes wrong:**
There are two Minisign Rust crates: `minisign` (full implementation, sign + verify) and `minisign-verify` (verify only, zero dependencies). The verify-only crate is attractive for a small binary, but it may not support all features needed for production use. Specifically, trusted comments in Minisign signatures can carry metadata like version numbers (for downgrade attack prevention), timestamps, and intended filenames. If the verify-only crate does not validate trusted comments or expose them for inspection, the self-update mechanism cannot use them for version checking.

**Why it happens:**
The `minisign-verify` crate is intentionally minimal -- "A small Rust crate to verify Minisign signatures." It handles the core Ed25519 signature verification but may not parse or validate the trusted comment's global signature (which is a separate signature over the signature + trusted comment concatenation).

**Consequences:**
- Self-update cannot use trusted comments for downgrade protection (e.g., refusing to "update" to an older version).
- If trusted comments are not validated, an attacker could modify the trusted comment (changing the version number) without detection.
- The full `minisign` crate is heavier (includes signing code and keygen code that Blufio doesn't need), increasing binary size unnecessarily.

**Prevention:**
- Use the `minisign-verify` crate and verify that it validates the trusted comment's global signature (the second signature that covers the trusted comment). Check the crate's source code or test this explicitly.
- If `minisign-verify` does not validate trusted comments: either use the full `minisign` crate, or implement trusted comment validation manually (it is just another Ed25519 signature verification over `signature || trusted comment`).
- Parse the trusted comment in Blufio's update logic to extract version metadata. Use it for downgrade protection: refuse to install a binary whose trusted comment version is less than or equal to the current version.
- Add integration tests that tamper with trusted comments and verify that verification fails.

**Detection:**
- Trusted comment modification goes undetected in tests.
- Self-update installs an older version (downgrade attack succeeds).

**Phase to address:** Minisign phase. Evaluate both crates before choosing. The crate choice determines whether trusted comment validation is automatic or manual.

---

## Minor Pitfalls

---

### Pitfall 13: sd_notify Watchdog Interval Miscalculated

**What goes wrong:**
The systemd service file has `WatchdogSec=300` (5 minutes). The `sd_notify` watchdog protocol requires pinging BEFORE half the watchdog interval elapses (i.e., every 150 seconds or less). If Blufio's heartbeat runner ticks every 60 seconds, this is fine. But if the heartbeat interval is configurable and an operator sets it to 180 seconds (for cost saving on Haiku heartbeats), the watchdog ping exceeds the half-interval, and systemd kills the service thinking it's hung.

**Prevention:**
- Read `WATCHDOG_USEC` from the environment (set by systemd) and calculate the ping interval as `WATCHDOG_USEC / 2 / 1_000_000` seconds. Do NOT rely on the heartbeat runner's interval.
- Spawn a dedicated watchdog task (lightweight, no LLM calls) that pings at the calculated interval, independent of the heartbeat runner.
- If `WATCHDOG_USEC` is not set, skip watchdog pinging entirely.
- Validate that `WatchdogSec` in the service file and the calculated ping interval are compatible. Log a warning if the ping interval would exceed the half-interval.

**Phase to address:** sd_notify phase. Simple but easy to get wrong.

---

### Pitfall 14: Self-Update Leaves Stale .tmp Files on Failure

**What goes wrong:**
The `self-replace` crate creates temporary files with dot-prefixed, randomly-suffixed names next to the current executable. If the update process fails (download error, verification failure, permission denied), these temp files are not cleaned up. Over multiple failed update attempts, the directory accumulates stale temp files. On a VPS with limited disk space, this wastes storage. More importantly, the stale temp files may contain partially downloaded (and thus unsigned) binaries that could be confused with legitimate files.

**Prevention:**
- After any update failure, explicitly delete the temp file in the error handling path.
- At update start, scan for and delete any existing `blufio-update.*.tmp` files from previous failed attempts.
- Use `tempfile::NamedTempFile` which auto-deletes on drop, rather than manual temp file management.
- Set a maximum age for temp files: delete any `.tmp` files in the executable's directory that are older than 1 hour.

**Phase to address:** Self-update phase.

---

### Pitfall 15: SQLCipher Encrypted Database Cannot Be Inspected with Standard sqlite3 CLI

**What goes wrong:**
After migrating to SQLCipher, the standard `sqlite3` command-line tool cannot open the encrypted database. It shows "file is encrypted or is not a database." Operators who use `sqlite3` for debugging, data inspection, or manual queries lose this capability. This affects operational workflows: checking session counts, inspecting cost ledger, verifying migration state, manual data fixes.

**Prevention:**
- Document this clearly in the migration guide: "After encryption, use `sqlcipher` CLI instead of `sqlite3`."
- Enhance `blufio doctor` to report key database statistics (session count, message count, cost total, migration version) so operators do not need to open the database manually.
- Add a `blufio db query <sql>` command that opens the encrypted database with the correct key and runs a read-only SQL query. This provides the debugging capability without requiring operators to install `sqlcipher`.
- In development builds (feature flag), optionally keep the database unencrypted for easier debugging.

**Phase to address:** SQLCipher phase. Documentation and tooling must ship with the encryption feature.

---

### Pitfall 16: Backup of Encrypted Database Produces Encrypted Backup

**What goes wrong:**
The current `backup.rs` uses `rusqlite::backup::Backup` to create an atomic copy of the database. When the source database is encrypted with SQLCipher, the backup will also be encrypted. The backup can only be opened with the same encryption key. If the operator loses the key, both the live database and all backups are unrecoverable. Additionally, the backup verification (`PRAGMA integrity_check` on the backup file) requires opening the backup with `PRAGMA key` first -- the verification code must also know the encryption key.

**Prevention:**
- Document that backups of encrypted databases are also encrypted and require the same key.
- Store key metadata (not the key itself) alongside backups: a key hint, key derivation parameters, or a hash of the key for verification.
- The `run_backup()` function must be updated to open both source and destination with `PRAGMA key` when encryption is enabled.
- Consider offering an option to export decrypted backups for archival: `blufio backup --decrypt /path/to/backup.db`. This would use `sqlcipher_export()` in reverse (encrypted -> plaintext). Must require explicit confirmation and the backup file should be marked as sensitive.
- Add the encryption key to the backup verification path: the integrity check on the backup file must open it with the correct key.

**Phase to address:** Backup integrity phase, which must come after or alongside SQLCipher implementation.

---

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|---|---|---|
| SQLCipher build change | Feature flag change breaks CI and musl static builds | Use `bundled-sqlcipher-vendored-openssl` for all targets. Test musl cross-compilation in CI first. |
| SQLCipher migration CLI | Data loss during plaintext-to-encrypted migration | Three-file strategy: original untouched, export to .tmp, verify, rename. NEVER auto-delete original. |
| SQLCipher PRAGMA ordering | `PRAGMA key` must be first operation, before WAL mode and other PRAGMAs | Restructure `Database::open()` to accept optional key. Enforce ordering in one function. |
| SQLCipher + refinery | Migration runner queries DB before `PRAGMA key` is set | Ensure `run_migrations()` is called AFTER `PRAGMA key` in the initialization sequence. Add canary query after key. |
| SQLCipher + tokio-rusqlite | Background thread setup must include PRAGMA key in the initialization closure | Pass encryption key into the `call()` closure that sets up PRAGMAs. |
| SQLCipher + backup.rs | Backup/restore must know the encryption key to open source and destination | Thread encryption key through `run_backup()` and `run_restore()` function signatures. |
| SQLCipher + tests | KDF slows every test by 200-500ms | Use raw hex key (`PRAGMA key = "x'...'"``) or low KDF iterations in test builds only. |
| sd_notify + service file | Changing to Type=notify without implementing READY=1 bricks the service | Change service file and code in the same commit. Feature-flag sd_notify for Linux only. |
| sd_notify + macOS dev | sd_notify calls on macOS either fail to compile or are silent no-ops | `#[cfg(target_os = "linux")]` gate. Check `NOTIFY_SOCKET` env var before calling. |
| sd_notify + watchdog | Watchdog ping interval derived from heartbeat interval may be too slow | Read `WATCHDOG_USEC` from env. Spawn dedicated watchdog task independent of heartbeat. |
| Minisign + key rotation | Embedded public key cannot be rotated without breaking old binaries | Embed multiple keys (current + next). Release transitional binary signed with old key but containing new key. |
| Minisign + trusted comments | verify-only crate may not validate trusted comment signature | Test explicitly. Implement manual validation if needed. Use trusted comments for downgrade protection. |
| Self-update + download | Partial download passed to self-replace corrupts the binary | Verify size + Minisign signature + ELF magic BEFORE calling self-replace. |
| Self-update + cross-filesystem | `rename()` fails with EXDEV on different mounts (Docker, snap) | Create temp file in same directory as executable. Handle EXDEV with copy fallback. |
| Self-update + rollback | New binary crashes on startup with no way to recover | Save current binary as `.rollback`. Health check within 30s. Auto-restore on failure. |
| Backup integrity + latency | `integrity_check` on source DB blocks single-writer thread | Run `quick_check` on backup file (not source). Separate read-only connection. Async task. |
| Backup + encryption | Encrypted backup requires same key to verify and restore | Thread encryption key through backup/restore/verify code paths. Document key management. |

---

## Integration Anti-Patterns

### Anti-Pattern 1: Making Encryption Always-On from Day One

**What it looks like:** Switching the default build to SQLCipher and requiring encryption for all database opens, including development, testing, and CI.

**Why it is wrong:** Breaks every existing deployment on upgrade (databases are plaintext). Forces every developer to deal with encryption key management for local development. Slows test suite by 200-500ms per database open.

**Instead:** Ship SQLCipher as opt-in (`[storage] encryption = true` in config). Plaintext remains the default. Provide a migration command. Only make encryption the default in a future major version after all operators have had time to migrate.

### Anti-Pattern 2: Implementing Self-Update Before Minisign

**What it looks like:** Building the download-and-replace mechanism first, planning to add signature verification "later."

**Why it is wrong:** Every update between now and "later" is an unsigned binary replacement. An attacker who compromises the release server (or performs a MITM on the download) can distribute a malicious binary that the update mechanism will happily install. Even HTTPS does not protect against a compromised release server.

**Instead:** Implement Minisign verification first. The self-update mechanism's FIRST feature is "verify before replace." There is no version of self-update that does not verify.

### Anti-Pattern 3: Running sd_notify in the Agent Loop

**What it looks like:** Sending `WATCHDOG=1` pings from within the agent's message processing loop because "that proves the agent is actually processing messages."

**Why it is wrong:** If no messages arrive for 5 minutes (normal for a low-traffic agent), no watchdog pings are sent, and systemd kills the service. The watchdog proves the PROCESS is alive, not that messages are being processed. Message processing health is checked by the heartbeat (which calls the LLM to verify the full stack works).

**Instead:** Watchdog pings go in a dedicated lightweight task (or the existing heartbeat runner). The ping says "I am alive." The heartbeat says "the full stack (LLM + DB + context engine) is working."

### Anti-Pattern 4: Using the Vault Key as the SQLCipher Encryption Key

**What it looks like:** Reusing the Argon2id-derived vault master key as the SQLCipher database encryption key because "it's already derived and in memory."

**Why it is wrong:** The vault key protects vault entries (AES-256-GCM encrypted credentials). The database encryption key protects the entire database at rest. These are different threat models with different rotation requirements. If the vault key is rotated (passphrase change), the entire database would need to be re-encrypted. If the database key leaks, all vault entries are also compromised (double exposure).

**Instead:** Derive separate keys. Use the operator's passphrase with Argon2id but with different salt/context for vault key vs. database key. Or use a random database key stored in the config (less secure but operationally simpler) and keep the vault key passphrase-derived.

---

## Dependency Ordering

Based on the pitfalls above, the implementation order matters critically:

```
1. SQLCipher build change (feature flags, Cargo.toml) -- foundation for everything else
2. SQLCipher PRAGMA ordering + Database::open() refactor -- before any SQLCipher usage
3. SQLCipher migration CLI (blufio migrate-db --encrypt) -- before enabling encryption
4. Minisign verification -- before self-update can ship
5. sd_notify -- independent, can be parallel with 1-4
6. Self-update (requires Minisign from step 4)
7. Backup integrity (requires SQLCipher awareness from step 2)
```

Violating this order (e.g., self-update before Minisign, or encryption enablement before migration CLI) creates the pitfalls described above.

---

## Sources

- [SQLCipher Plaintext to Encrypted Migration - Zetetic FAQ](https://discuss.zetetic.net/t/how-to-encrypt-a-plaintext-sqlite-database-to-use-sqlcipher-and-avoid-file-is-encrypted-or-is-not-a-database-errors/868) -- Official migration procedure and "file is encrypted" error explanation
- [SQLCipher API Reference - Zetetic](https://www.zetetic.net/sqlcipher/sqlcipher-api/) -- PRAGMA key ordering, sqlcipher_export, cipher_migrate, KDF settings, WAL mode interaction
- [rusqlite bundled-sqlcipher Issue #765](https://github.com/rusqlite/rusqlite/issues/765) -- Feature flag discussion and implementation
- [rusqlite Crate Documentation](https://docs.rs/crate/rusqlite/latest) -- Feature flags: bundled-sqlcipher, bundled-sqlcipher-vendored-openssl
- [sd-notify Crate - crates.io](https://crates.io/crates/sd-notify) -- Pure Rust sd_notify implementation
- [sd-notify GitHub Repository](https://github.com/lnicola/sd-notify) -- Source code and API
- [sd_notify(3) Linux Man Page](https://man7.org/linux/man-pages/man3/sd_notify.3.html) -- NOTIFY_SOCKET behavior when absent (returns 0, no-op)
- [systemd Type=notify Documentation](https://www.freedesktop.org/software/systemd/man/latest/sd_notify.html) -- Type=notify semantics, WATCHDOG_USEC protocol
- [self-replace Crate Documentation](https://docs.rs/self-replace/) -- Unix atomic rename, Windows workarounds, temp file handling
- [self_update Crate - GitHub](https://github.com/jaemk/self_update) -- Self-update patterns, download handling
- [std::fs::rename - Rust](https://doc.rust-lang.org/std/fs/fn.rename.html) -- EXDEV behavior on cross-filesystem rename
- [Minisign Specification](https://jedisct1.github.io/minisign/) -- Key format, trusted comments, key ID, signature structure
- [minisign-verify Crate - GitHub](https://github.com/jedisct1/rust-minisign-verify) -- Verify-only implementation, zero dependencies
- [minisign Full Crate - GitHub](https://github.com/jedisct1/rust-minisign) -- Complete Minisign implementation in Rust
- [SQLCipher Data Loss After Migration - Zetetic Forum](https://discuss.zetetic.net/t/data-is-lost-after-successful-room-migration-when-using-sqlcipher/6165) -- Real-world data loss report
- [SQLCipher WAL Mode Discussion](https://discuss.zetetic.net/t/can-i-use-pragma-journal-mode-wal-with-an-sqliteconnection/770) -- WAL compatibility with SQLCipher
