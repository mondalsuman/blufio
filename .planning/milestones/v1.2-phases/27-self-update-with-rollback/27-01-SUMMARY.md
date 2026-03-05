---
phase: 27-self-update-with-rollback
plan: 01
subsystem: update
tags: [self-update, github-releases, download, minisign]

requirements-completed: [UPDT-01, UPDT-02, UPDT-03, UPDT-07, UPDT-08]

completed: 2026-03-04
---

# Plan 27-01 Summary

## What was built
Added the `blufio update` CLI command with GitHub Releases API client, version checking (`--check`), platform-appropriate binary download, Minisign signature verification, and interactive confirmation (`--yes` flag).

## Key decisions
- Module lives in `crates/blufio/src/update.rs` (no new crate -- thin orchestration logic)
- `self-replace` v1.5.0 and `tempfile` added as workspace dependencies
- `BlufioError::Update(String)` variant added to `blufio-core`
- Platform asset name maps `macos` -> `darwin` for GitHub release naming convention
- Temp files created in same directory as binary (avoids cross-device rename)

## Key files
- `crates/blufio/src/update.rs` -- update module with all logic
- `crates/blufio/src/main.rs` -- CLI wiring with Update/Check/Rollback subcommands
- `crates/blufio-core/src/error.rs` -- BlufioError::Update variant
- `Cargo.toml` -- workspace deps: self-replace, tempfile
- `crates/blufio/Cargo.toml` -- binary crate deps

## Test results
22 tests pass: version parsing (3), platform asset name (1), asset finding (3), JSON deserialization (1), binary/bak paths (2), backup/rollback (4), permissions (1), error formatting (1), CLI parsing (4), no bak rollback error (1), current version (1).

## Status
Complete -- all Plan 01 tasks executed successfully.
