# Phase 23: Backup Integrity Verification - Research

**Researched:** 2026-03-03
**Domain:** SQLite PRAGMA integrity_check, rusqlite synchronous connections, file-system safety operations
**Confidence:** HIGH

## Summary

Phase 23 adds post-operation integrity verification to the existing `blufio backup` and `blufio restore` commands. The core mechanism is SQLite's `PRAGMA integrity_check`, which is already used by `doctor.rs` (line 390) for the `--deep` diagnostic check. The backup module (`backup.rs`) uses synchronous `rusqlite::Connection` objects, so the integrity check can be implemented directly without async wrappers.

The implementation is straightforward: after the Backup API's `run_to_completion` call, open a read-only connection to the output file, run `PRAGMA integrity_check`, parse the result (single row "ok" means pass, anything else means failure), and either report success or delete the corrupt file and return an error. For restore, the flow is more involved: pre-check the backup file's integrity before restoring, create the `.pre-restore` safety copy, perform the restore, verify the restored database, and roll back if verification fails.

**Primary recommendation:** Add a synchronous `run_integrity_check(path: &Path) -> Result<(), BlufioError>` helper function in `backup.rs` that opens a read-only `rusqlite::Connection`, runs `PRAGMA integrity_check`, and returns `Ok(())` or an error containing the first failure row. Both `run_backup` and `run_restore` call this helper at the appropriate points.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Delete corrupt backup files immediately -- no quarantine, no .corrupt rename
- Only verify the output (backup file after write, restored DB after restore) -- source DB integrity is doctor's responsibility
- Auto-rollback on corrupt restore: remove the corrupt restored DB, put .pre-restore copy back as active DB
- Trust the Backup API's clean output -- no extra WAL/SHM cleanup needed
- Single-line status: "Backup complete: 5.2 MB, integrity: ok" -- compact, log-friendly
- On failure: show cause + action taken (e.g., "Backup FAILED: integrity check failed (corrupt data on page 42). Backup file deleted.")
- All output to stderr -- consistent with existing eprintln! pattern
- Single exit code (1) for any failure -- operator reads error message for details
- Pre-check: run integrity_check on the backup file before attempting restore -- fail early
- Skip .pre-restore safety backup when no existing DB (first-time restore)
- Keep .pre-restore file after successful restore -- low-cost safety net
- On corrupt restore: delete corrupt file, restore from .pre-restore, clear error message
- Show first integrity_check error row only -- concise, actionable
- Suggest next step: "Run 'blufio doctor' for full database diagnostics"
- Reuse BlufioError::Storage variant -- integrity failures are a storage concern

### Claude's Discretion
- Whether to extract a shared integrity check utility (sync function used by both backup.rs and doctor.rs) or keep separate implementations -- depends on sync vs async connection type mismatch
- Exact error message wording and formatting
- Test strategy for simulating corruption

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| BKUP-01 | Backup runs PRAGMA integrity_check on the backup file after write completes | Integrity check helper function called after `run_to_completion`; uses synchronous `rusqlite::Connection` opened read-only on backup file |
| BKUP-02 | Restore runs PRAGMA integrity_check on the restored database after completion | Same helper called after restore's `run_to_completion`; additionally pre-checks backup file before restore begins |
| BKUP-03 | Corrupt backup file is deleted and operation returns an error | `std::fs::remove_file` on backup path when integrity check fails; for restore, rollback from `.pre-restore` copy |
| BKUP-04 | Backup/restore reports integrity status to operator alongside file size | Modified `eprintln!` output: "Backup complete: {size} MB, integrity: ok" format matching success criteria #4 |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rusqlite | 0.37 | Synchronous SQLite binding, Backup API, PRAGMA execution | Already in workspace; `backup` feature already enabled; same library used by backup.rs |
| std::fs | (stdlib) | File deletion (`remove_file`), file copy, metadata | No external deps needed for file operations |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tempfile | 3 | Temporary directories for tests | Already a dev-dependency; used by existing backup tests |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Separate sync helper in backup.rs | Shared utility with doctor.rs | Doctor uses async `tokio_rusqlite::Connection`; backup uses sync `rusqlite::Connection`. Sharing would require either: (a) a generic function over connection types (not supported by rusqlite/tokio-rusqlite API), or (b) duplicating the ~10-line PRAGMA pattern. **Recommendation: keep separate.** The sync/async mismatch makes a shared utility more complex than the duplication it saves. |
| `PRAGMA integrity_check` | `PRAGMA quick_check` | quick_check is O(N) vs integrity_check O(N log N), but skips UNIQUE constraint and index consistency verification. For backup verification where correctness matters more than speed, integrity_check is the right choice. quick_check is deferred as BKUP-06. |

**Installation:** No new dependencies needed. All libraries already in workspace.

## Architecture Patterns

### Recommended Changes to backup.rs

```
crates/blufio/src/backup.rs    (existing file, modify)
├── run_integrity_check()      # NEW: sync PRAGMA integrity_check helper
├── run_backup()               # MODIFY: add integrity check + new output format
└── run_restore()              # MODIFY: add pre-check, post-check, rollback logic
    └── tests                  # MODIFY: add corruption and rollback tests
```

### Pattern 1: Synchronous Integrity Check Helper
**What:** A standalone function that opens a read-only connection, runs `PRAGMA integrity_check`, and returns success or first error row.
**When to use:** After any write operation that produces a SQLite file.
**Why keep in backup.rs:** The doctor.rs implementation uses async `tokio_rusqlite::Connection::call()` which requires a closure. backup.rs uses sync `rusqlite::Connection` directly. The PRAGMA query is identical but the connection plumbing differs. Extracting to a shared crate would require either two implementations or a trait abstraction -- neither is worth it for ~10 lines of PRAGMA logic.

```rust
// Pattern for synchronous integrity check
fn run_integrity_check(path: &Path) -> Result<(), BlufioError> {
    let conn = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| BlufioError::Storage { source: Box::new(e) })?;

    let mut stmt = conn
        .prepare("PRAGMA integrity_check")
        .map_err(|e| BlufioError::Storage { source: Box::new(e) })?;

    let rows: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| BlufioError::Storage { source: Box::new(e) })?
        .filter_map(|r| r.ok())
        .collect();

    if rows.len() == 1 && rows[0] == "ok" {
        Ok(())
    } else {
        let first_error = rows.first().map(|s| s.as_str()).unwrap_or("unknown error");
        Err(BlufioError::Storage {
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("integrity check failed ({first_error})"),
            )),
        })
    }
}
```
**Source:** Pattern adapted from doctor.rs line 388-397 (async version) and SQLite PRAGMA integrity_check documentation (https://www.sqlite.org/pragma.html#pragma_integrity_check)

### Pattern 2: Backup with Integrity Verification and Cleanup
**What:** After Backup API completes, verify the output. On failure, delete the backup file and return error.
**When to use:** `run_backup` function.

```rust
// After backup.run_to_completion() succeeds:
let backup_path = Path::new(backup_path_str);

if let Err(e) = run_integrity_check(backup_path) {
    // Delete corrupt backup
    let _ = std::fs::remove_file(backup_path);
    eprintln!(
        "Backup FAILED: {e}. Backup file deleted."
    );
    return Err(e);
}

// Report success with integrity status
let metadata = std::fs::metadata(backup_path)?;
let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
eprintln!("Backup complete: {size_mb:.1} MB, integrity: ok");
```

### Pattern 3: Restore with Pre-check, Post-check, and Rollback
**What:** Full restore safety flow: pre-check backup → create .pre-restore → restore → verify → rollback if corrupt.
**When to use:** `run_restore` function.

```rust
// 1. Pre-check: verify backup file integrity BEFORE restore
run_integrity_check(Path::new(restore_from))?;

// 2. Safety backup (if existing DB)
let dst_path = Path::new(db_path);
let pre_restore_path = format!("{db_path}.pre-restore");
if dst_path.exists() {
    // existing run_backup call for .pre-restore
}

// 3. Perform restore (existing Backup API code)
// ...

// 4. Post-check: verify restored database
if let Err(e) = run_integrity_check(dst_path) {
    // Rollback: delete corrupt restored DB
    let _ = std::fs::remove_file(dst_path);
    // Restore from .pre-restore if it exists
    if Path::new(&pre_restore_path).exists() {
        std::fs::copy(&pre_restore_path, dst_path)?;
        eprintln!("Restore FAILED: {e}. Database rolled back to pre-restore state.");
    } else {
        eprintln!("Restore FAILED: {e}. Corrupt database removed.");
    }
    return Err(e);
}

let metadata = std::fs::metadata(db_path)?;
let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
eprintln!("Restore complete: {size_mb:.1} MB, integrity: ok");
```

### Anti-Patterns to Avoid
- **Verifying the source database in backup:** Source DB integrity is the doctor's responsibility (per user decision). Only verify the output file.
- **Using `PRAGMA quick_check` instead of `integrity_check`:** quick_check skips UNIQUE and index consistency verification. For backup/restore verification, full integrity_check is required. quick_check is deferred (BKUP-06).
- **Quarantining corrupt files instead of deleting:** User decision is delete immediately. No `.corrupt` rename, no quarantine directory.
- **Using `std::fs::rename` for rollback:** On some platforms, rename across filesystems fails. Use `std::fs::copy` followed by `std::fs::remove_file` for safety, or use the Backup API itself (as done currently for `.pre-restore` creation).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| SQLite integrity verification | Custom page-level checks | `PRAGMA integrity_check` | SQLite's built-in checker handles all page formats, index consistency, constraint validation. Impossible to replicate correctly. |
| Atomic database copy | Manual file copy | rusqlite `Backup` API | The Backup API handles WAL mode, concurrent writers, page-level consistency. A file copy can produce a corrupt snapshot. |
| File size formatting | Custom formatter | `metadata.len() as f64 / (1024.0 * 1024.0)` with `{:.1}` | Already used in backup.rs and doctor.rs. Consistent with existing output. |

**Key insight:** The entire integrity verification mechanism is a SQLite built-in. The implementation is calling a PRAGMA and parsing its output. The complexity is in the control flow (when to check, what to do on failure), not in the verification itself.

## Common Pitfalls

### Pitfall 1: Connection Left Open During File Deletion
**What goes wrong:** If the `rusqlite::Connection` used for integrity check is still open when `std::fs::remove_file` is called, the file may be locked (especially on Windows, but also with SQLite's locking modes).
**Why it happens:** Rust's drop order isn't always obvious. The connection might still be alive in the same scope as the delete call.
**How to avoid:** Either (a) scope the integrity check in a block so the connection drops before deletion, or (b) explicitly `drop(conn)` before file operations. The helper function approach naturally handles this -- the connection is dropped when `run_integrity_check` returns.
**Warning signs:** Tests pass on macOS/Linux but file deletion fails on Windows or under heavy SQLite locking.

### Pitfall 2: Rollback Failure Leaves No Database
**What goes wrong:** If the `.pre-restore` copy fails to restore (e.g., disk full), the operator has no database at all: the original was overwritten, and the rollback failed.
**Why it happens:** The restore operation overwrites the active database in-place via the Backup API.
**How to avoid:** Use `std::fs::copy` for the rollback (which is a simple file copy) and report the `.pre-restore` path in the error message so the operator can manually recover. The `.pre-restore` file is never deleted on failure.
**Warning signs:** Error message says "rolled back" but the database file doesn't exist.

### Pitfall 3: PRAGMA integrity_check Returns Up to 100 Errors by Default
**What goes wrong:** On a badly corrupted database, `PRAGMA integrity_check` collects up to 100 rows of errors before stopping. Collecting all of them is unnecessary since we only show the first.
**Why it happens:** Default behavior of integrity_check.
**How to avoid:** Use `PRAGMA integrity_check(1)` to limit to a single error row. This is faster on badly corrupted databases and aligns with the user decision to show only the first error.
**Warning signs:** Slow integrity checks on large corrupt databases.

### Pitfall 4: Pre-check Passes But Post-check Fails
**What goes wrong:** The backup file passes integrity check before restore, but the restored database fails integrity check after restore.
**Why it happens:** The Backup API writes to the destination in steps. If the process is interrupted (OOM kill, power loss), the destination may be partially written. Also, the destination file may have pre-existing data if it was not empty.
**How to avoid:** This is expected edge case behavior -- the rollback mechanism handles it. The post-check is the authoritative verification.
**Warning signs:** Restore reports rollback even though backup file was "ok".

### Pitfall 5: Integrity Check on Empty/New Database
**What goes wrong:** Running integrity_check on a freshly created (empty) SQLite database.
**Why it happens:** First-time restore where no existing DB exists.
**How to avoid:** Not actually a problem -- `PRAGMA integrity_check` returns "ok" for empty databases. But verify this in tests.
**Warning signs:** None -- this works correctly.

## Code Examples

Verified patterns from the existing codebase:

### Existing Integrity Check Pattern (doctor.rs, async)
```rust
// Source: crates/blufio/src/doctor.rs lines 388-397
let result: Result<Vec<String>, tokio_rusqlite::Error> = conn
    .call(|conn| {
        let mut stmt = conn.prepare("PRAGMA integrity_check")?;
        let rows: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    })
    .await;

match result {
    Ok(rows) if rows.len() == 1 && rows[0] == "ok" => { /* pass */ }
    Ok(rows) => { /* fail: rows.len() issues found */ }
    Err(e) => { /* error running check */ }
}
```

### Existing Error Handling Pattern (backup.rs)
```rust
// Source: crates/blufio/src/backup.rs -- used throughout
.map_err(|e| BlufioError::Storage {
    source: Box::new(e),
})
```

### Existing Output Pattern (backup.rs)
```rust
// Source: crates/blufio/src/backup.rs lines 59-63
let metadata = std::fs::metadata(backup_path).map_err(|e| BlufioError::Storage {
    source: Box::new(e),
})?;
let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
eprintln!("Backup complete: {size_mb:.1} MB written to {backup_path}");
// CHANGE TO: eprintln!("Backup complete: {size_mb:.1} MB, integrity: ok");
```

### Existing Pre-restore Safety Pattern (backup.rs)
```rust
// Source: crates/blufio/src/backup.rs lines 98-104
let dst_path = Path::new(db_path);
if dst_path.exists() {
    let pre_restore_path = format!("{db_path}.pre-restore");
    eprintln!("Creating safety backup: {pre_restore_path}");
    run_backup(db_path, &pre_restore_path)?;
}
```

### Test Corruption Simulation
```rust
// Strategy: Create a valid SQLite database, then overwrite bytes in the middle
// to corrupt it while keeping the SQLite header intact (so it opens but fails
// integrity check).
fn create_corrupt_db(path: &Path) {
    // Create valid DB with data
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT);
         INSERT INTO test VALUES (1, 'data');",
    ).unwrap();
    drop(conn);

    // Corrupt the file by overwriting bytes after the header
    // SQLite header is 100 bytes. Page 1 starts at offset 0.
    // Writing garbage after the header corrupts page data.
    let mut data = std::fs::read(path).unwrap();
    if data.len() > 200 {
        // Overwrite bytes in the B-tree page area
        for i in 100..200 {
            data[i] = 0xFF;
        }
    }
    std::fs::write(path, &data).unwrap();
}
```

**Note on corruption simulation:** `PRAGMA integrity_check` may not always catch all forms of corruption, especially if only data (not structure) is modified. The most reliable way to trigger a failure is to corrupt B-tree page pointers or the freelist. Overwriting bytes 100-200 (within page 1's B-tree leaf structure) reliably triggers integrity_check failures for small databases. This should be validated in tests.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `SELECT 1` validation in restore | `PRAGMA integrity_check` pre and post | This phase | `SELECT 1` only verifies the file opens as SQLite; integrity_check verifies all pages, indexes, and constraints |
| No verification after backup | `PRAGMA integrity_check` on backup output | This phase | Catches silent corruption from disk errors, interrupted writes |
| No rollback on corrupt restore | Auto-rollback from `.pre-restore` copy | This phase | Operator's active database is protected from corrupt restores |

**Deprecated/outdated:**
- The `SELECT 1` validation in `run_restore` (line 91-95) is superseded by `PRAGMA integrity_check`. Remove or replace it.

## Open Questions

1. **Corruption simulation reliability**
   - What we know: Overwriting bytes in the B-tree page area (offsets 100-200) should trigger integrity_check failure for small databases.
   - What's unclear: Whether all test environments (CI, different SQLite versions) will consistently detect this specific corruption pattern.
   - Recommendation: Write the corruption test, validate it works. If flaky, try corrupting the freelist or overwriting entire pages. The test should assert that integrity_check returns something other than "ok", not assert a specific error message.

2. **Rollback mechanism: Backup API vs fs::copy**
   - What we know: The `.pre-restore` file is currently created using `run_backup()` (Backup API). For rollback, we need to copy it back.
   - What's unclear: Should rollback use `run_backup()` (Backup API from `.pre-restore` to active path) or `std::fs::copy()` (simple file copy)?
   - Recommendation: Use `std::fs::copy` for rollback. It is simpler, does not require opening the file as SQLite, and works even if the `.pre-restore` file has non-standard SQLite features. The Backup API's value is for live databases; for a static file copy, `fs::copy` is sufficient and more robust.

3. **Output format for pre-check integrity failure**
   - What we know: User wants "Backup FAILED: integrity check failed (...). Backup file deleted." format.
   - What's unclear: What to say when the backup file pre-check fails during restore (the backup file itself is corrupt before we even try to restore).
   - Recommendation: "Restore FAILED: backup file integrity check failed ({first_error}). Run 'blufio doctor' for full database diagnostics." -- no deletion needed since we haven't modified anything yet.

## Sources

### Primary (HIGH confidence)
- SQLite PRAGMA integrity_check documentation: https://www.sqlite.org/pragma.html#pragma_integrity_check -- return values, default limit of 100 errors, difference from quick_check
- SQLite Backup API documentation: https://www.sqlite.org/backup.html -- consistency guarantees, snapshot behavior
- rusqlite 0.37 Backup struct: https://docs.rs/rusqlite/0.37.0/rusqlite/backup/struct.Backup.html -- `run_to_completion` method, connection mutability requirements
- Existing codebase: `doctor.rs` lines 388-397 (integrity_check pattern), `backup.rs` full file (current backup/restore implementation)

### Secondary (MEDIUM confidence)
- None needed -- all findings verified against official SQLite docs and existing codebase.

### Tertiary (LOW confidence)
- Corruption simulation strategy (overwriting bytes 100-200): Based on SQLite file format knowledge. Should be validated in tests during implementation.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- no new dependencies, all libraries already in workspace, patterns proven in doctor.rs
- Architecture: HIGH -- straightforward modification of existing functions, clear control flow, proven PRAGMA pattern
- Pitfalls: HIGH -- connection lifecycle and file locking well understood in Rust/SQLite; corruption simulation is the only area requiring empirical validation

**Research date:** 2026-03-03
**Valid until:** 2026-04-03 (stable domain, no fast-moving dependencies)
