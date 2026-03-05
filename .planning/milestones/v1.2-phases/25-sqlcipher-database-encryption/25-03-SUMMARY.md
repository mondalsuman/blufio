---
phase: 25-sqlcipher-database-encryption
plan: 03
subsystem: cli
tags: [sqlcipher, encryption, keygen, encrypt-command, cli]

requires:
  - phase: 25-sqlcipher-database-encryption
    plan: 01
    provides: connection factory and is_plaintext_sqlite
provides:
  - blufio db keygen command for 256-bit hex key generation
  - blufio db encrypt command for plaintext-to-encrypted migration
  - three-file safety strategy for zero-downtime encryption
affects: [25-04, operator workflow]

key-files:
  created:
    - crates/blufio/src/encrypt.rs
  modified:
    - crates/blufio/src/main.rs
    - crates/blufio/Cargo.toml

key-decisions:
  - "Three-file safety: .encrypting temp, verify, .pre-encrypt backup, swap"
  - "Uses sqlcipher_export() via ATTACH DATABASE for migration"
  - "Interactive confirmation with --yes flag to skip"
  - "ring::rand::SystemRandom for cryptographic key generation"

patterns-established:
  - "DbCommands enum for blufio db subcommand group"
  - "encrypt module with run_keygen() and run_encrypt() public functions"

requirements-completed: [CIPH-09, CIPH-10, CIPH-11]

completed: 2026-03-03
---

# Plan 25-03: Encrypt CLI Commands Summary

**blufio db keygen and blufio db encrypt commands for key generation and plaintext-to-encrypted migration**

## Performance

- **Files modified:** 3
- **Files created:** 1
- **Tests added:** 6

## Accomplishments
- Created encrypt.rs with run_keygen() using ring::rand for 256-bit hex key generation
- Created run_encrypt() with three-file safety migration strategy
- Step 1: Export plaintext to .encrypting temp via sqlcipher_export()
- Step 2: Verify encrypted copy with PRAGMA integrity_check
- Step 3: Swap files (original -> .pre-encrypt, encrypted -> original)
- Added DbCommands enum with Encrypt and Keygen variants to main.rs
- Added ring and hex dependencies to blufio/Cargo.toml
- All 6 tests pass: keygen hex validation, error cases, full roundtrip, cleanup

## Files Created/Modified
- `crates/blufio/src/encrypt.rs` - New module: run_keygen(), run_encrypt(), 6 tests
- `crates/blufio/src/main.rs` - mod encrypt, DbCommands enum, Db command variant
- `crates/blufio/Cargo.toml` - Added ring and hex dependencies, serial_test dev-dep

## Issues Encountered
- Same Rust 2024 unsafe fn pattern as Plan 01 -- applied same fix

---
*Phase: 25-sqlcipher-database-encryption*
*Completed: 2026-03-03*
