---
phase: 25-sqlcipher-database-encryption
plan: 02
subsystem: storage
tags: [sqlcipher, encryption, migration, backup, doctor]

requires:
  - phase: 25-sqlcipher-database-encryption
    plan: 01
    provides: centralized connection factory
provides:
  - all production database connections routed through factory
  - backup command with encryption status reporting
  - doctor command with encryption health check (check_encryption)
  - zero direct Connection::open() calls in production code
affects: [25-04, runtime encryption behavior]

key-files:
  modified:
    - crates/blufio/src/main.rs
    - crates/blufio/src/serve.rs
    - crates/blufio/src/shell.rs
    - crates/blufio/src/mcp_server.rs
    - crates/blufio/src/backup.rs
    - crates/blufio/src/doctor.rs
    - crates/blufio-cost/src/ledger.rs
    - crates/blufio-cost/Cargo.toml
    - crates/blufio-mcp-client/src/pin_store.rs
    - crates/blufio-mcp-client/Cargo.toml

key-decisions:
  - "Test code keeps direct Connection::open() for plaintext test DBs -- intentional"
  - "backup.rs reports encryption status in completion message"
  - "doctor.rs check_encryption() uses 4-way match on (is_plaintext, has_key) for clear diagnostics"

patterns-established:
  - "backup tests use #[serial] and clear_key() to prevent env var contamination"
  - "doctor check_encryption suggests 'blufio db encrypt' when key is set but DB is plaintext"

requirements-completed: [CIPH-04, CIPH-05, CIPH-06, CIPH-07, CIPH-08]

completed: 2026-03-03
---

# Plan 25-02: Route All Consumers Through Factory Summary

**Replace all direct Connection::open() calls with centralized factory and add encryption awareness to backup/doctor**

## Performance

- **Files modified:** 10
- **Connection sites migrated:** ~15

## Accomplishments
- Replaced 3 direct opens in main.rs (handle_skill_command)
- Replaced 2 in serve.rs (vault conn, memory conn)
- Replaced 1 in shell.rs (memory conn)
- Replaced 1 in mcp_server.rs (vault conn)
- Replaced CostLedger::open() in ledger.rs to use factory
- Replaced PinStore::open() in pin_store.rs to use factory
- Rewrote backup.rs to use factory, added encryption status to output
- Added check_encryption() to doctor.rs with 4-way diagnostic matrix
- Added blufio-storage dependency to blufio-cost and blufio-mcp-client
- Verified zero remaining direct Connection::open() in production code

## Files Modified
- `crates/blufio/src/main.rs` - 3 connection sites migrated
- `crates/blufio/src/serve.rs` - 2 connection sites migrated
- `crates/blufio/src/shell.rs` - 1 connection site migrated
- `crates/blufio/src/mcp_server.rs` - 1 connection site migrated
- `crates/blufio/src/backup.rs` - All connections through factory, encryption status in output
- `crates/blufio/src/doctor.rs` - All connections through factory, check_encryption() added
- `crates/blufio-cost/src/ledger.rs` - CostLedger uses factory
- `crates/blufio-cost/Cargo.toml` - Added blufio-storage dependency
- `crates/blufio-mcp-client/src/pin_store.rs` - PinStore uses factory
- `crates/blufio-mcp-client/Cargo.toml` - Added blufio-storage dependency

## Issues Encountered
- backup tests needed #[serial] + clear_key() to prevent env var contamination from encrypt tests
- Empty SQLite files (0 bytes) were incorrectly detected as encrypted -- fixed is_plaintext_sqlite()

---
*Phase: 25-sqlcipher-database-encryption*
*Completed: 2026-03-03*
