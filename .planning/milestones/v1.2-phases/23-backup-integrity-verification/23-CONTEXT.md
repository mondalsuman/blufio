# Phase 23: Backup Integrity Verification - Context

**Gathered:** 2026-03-03
**Status:** Ready for planning

<domain>
## Phase Boundary

Ensure operator can trust that backups are not silently corrupt and restores produce a valid database. Add PRAGMA integrity_check after backup and restore operations, with corruption handling and status reporting. Creating new backup strategies, scheduling, or encryption are separate phases.

</domain>

<decisions>
## Implementation Decisions

### Corruption handling
- Delete corrupt backup files immediately — no quarantine, no .corrupt rename
- Only verify the output (backup file after write, restored DB after restore) — source DB integrity is doctor's responsibility
- Auto-rollback on corrupt restore: remove the corrupt restored DB, put .pre-restore copy back as active DB
- Trust the Backup API's clean output — no extra WAL/SHM cleanup needed

### Operator output format
- Single-line status: "Backup complete: 5.2 MB, integrity: ok" — compact, log-friendly
- On failure: show cause + action taken (e.g., "Backup FAILED: integrity check failed (corrupt data on page 42). Backup file deleted.")
- All output to stderr — consistent with existing eprintln! pattern
- Single exit code (1) for any failure — operator reads error message for details

### Restore safety flow
- Pre-check: run integrity_check on the backup file before attempting restore — fail early, avoid unnecessary damage-then-rollback
- Skip .pre-restore safety backup when no existing DB (first-time restore)
- Keep .pre-restore file after successful restore — low-cost safety net, operator deletes manually if desired
- On corrupt restore: delete corrupt file, restore from .pre-restore, clear error message

### Error specificity
- Show first integrity_check error row only — concise, actionable (e.g., "row 1 missing from index idx_sessions")
- Suggest next step: "Run 'blufio doctor' for full database diagnostics"
- Reuse BlufioError::Storage variant — integrity failures are a storage concern, keeps error enum lean

### Claude's Discretion
- Whether to extract a shared integrity check utility (sync function used by both backup.rs and doctor.rs) or keep separate implementations — depends on sync vs async connection type mismatch
- Exact error message wording and formatting
- Test strategy for simulating corruption

</decisions>

<specifics>
## Specific Ideas

- Output format matches success criteria #4 exactly: "Backup complete: 5.2 MB, integrity: ok"
- Restore flow: pre-check backup → create .pre-restore (if DB exists) → restore → verify → rollback if corrupt
- Failure messages should include the action taken ("Backup file deleted" / "Database rolled back to pre-restore state")

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `doctor.rs` (line 390): Working PRAGMA integrity_check pattern — prepares statement, collects rows, checks for "ok"
- `backup.rs`: `run_backup()` and `run_restore()` — well-structured, needs integrity_check added after the Backup API call
- `BlufioError::Storage`: Existing error variant that wraps database-related errors via `Box<dyn Error>`

### Established Patterns
- Backup uses synchronous `rusqlite::Connection`; doctor uses async `tokio_rusqlite::Connection` — integrity check implementations may differ by connection type
- Error handling: `map_err(|e| BlufioError::Storage { source: Box::new(e) })` pattern throughout backup.rs
- Output: `eprintln!` for operator-facing messages (stderr)
- File size reporting: already computed via `std::fs::metadata` in both backup and restore

### Integration Points
- `main.rs` lines 197-206: Backup/Restore command dispatch — currently just calls functions and exits on error
- `run_restore` already creates `.pre-restore` safety copy (line 98-104) — extend this with rollback logic
- `run_restore` already validates source with `SELECT 1` (line 91-95) — replace with integrity_check for stronger validation

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 23-backup-integrity-verification*
*Context gathered: 2026-03-03*
