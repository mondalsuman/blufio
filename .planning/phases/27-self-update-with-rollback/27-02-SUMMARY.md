---
phase: 27-self-update-with-rollback
plan: 02
subsystem: update
tags: [self-update, backup, atomic-swap, health-check, rollback]

requirements-completed: [UPDT-04, UPDT-05, UPDT-06]

completed: 2026-03-04
---

# Plan 27-02 Summary

## What was built
Completed the full self-update flow with backup, atomic swap via `self_replace`, post-swap health check with 30-second timeout, automatic rollback on failure, and manual rollback via `blufio update rollback`.

## Key decisions
- Backup stored as `<binary>.bak` next to current binary (overwrites previous)
- Health check spawns `blufio doctor` as a child process (not in-process) with 30s timeout
- Auto-rollback on health check failure: swap .bak back, report error
- Manual rollback (`blufio update rollback`) is instant, no confirmation needed
- Unix executable permissions (0o755) set on downloaded binary before swap

## Key files
- `crates/blufio/src/update.rs` -- complete with backup, swap, health check, rollback

## Test results
93 total tests pass (22 update-specific + 71 existing). Zero failures. Clippy clean. Fmt clean.

## Requirement coverage
- UPDT-01: `fetch_latest_release()` queries GitHub Releases API
- UPDT-02: `download_to_temp()` downloads platform binary + .minisig
- UPDT-03: `verify_download()` calls blufio_verify before file operations
- UPDT-04: `backup_current()` + `self_replace::self_replace()` for atomic swap
- UPDT-05: `health_check()` runs `blufio doctor` with 30s timeout
- UPDT-06: `run_rollback()` / `do_rollback()` reverts from .bak
- UPDT-07: `run_check()` reports version without downloading
- UPDT-08: `confirm_update()` with --yes flag and TTY detection

## Status
Complete -- all Plan 02 tasks executed successfully. Phase 27 fully implemented.
