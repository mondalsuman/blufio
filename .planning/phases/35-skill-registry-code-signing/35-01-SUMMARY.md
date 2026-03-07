---
phase: 35-skill-registry-code-signing
plan: 01
subsystem: security
tags: [ed25519, sha256, signing, wasm, skill-registry, tofu, sqlite]

# Dependency graph
requires:
  - phase: 07-wasm-skill-sandbox
    provides: WasmSkillRuntime, SkillManifest, SkillStore with install/list/remove
  - phase: 02-persistence-security-vault
    provides: SQLite migration pattern, tokio-rusqlite async pattern
provides:
  - PublisherKeypair Ed25519 signing module (signing.rs)
  - V8 migration adding content_hash, signature, publisher_id columns and publisher_keys table
  - Extended SkillStore with hash/signature storage, TOFU key management, update method
  - CLI subcommands: Sign, Update, Keygen, Verify, Info
  - .sig detached signature file format
affects: [35-02, skill-marketplace, node-system]

# Tech tracking
tech-stack:
  added: [sha2 0.10, ed25519-dalek 2.1, hex, rand]
  patterns: [PEM-like key file format, TOFU key management, detached signature files]

key-files:
  created:
    - crates/blufio-skill/src/signing.rs
    - crates/blufio-storage/migrations/V8__skill_signing.sql
  modified:
    - crates/blufio-skill/Cargo.toml
    - crates/blufio-skill/src/lib.rs
    - crates/blufio-skill/src/store.rs
    - crates/blufio/src/main.rs
    - crates/blufio/Cargo.toml

key-decisions:
  - "PublisherKeypair is separate from DeviceKeypair (skill author identity vs device identity)"
  - "PEM-like format with hex encoding for keypair files (not base64, matches existing hex patterns)"
  - "TOFU model: first publisher key seen for an ID is trusted, subsequent mismatches hard-blocked"
  - "Nullable signing columns in V8 migration preserve backward compatibility with existing installs"
  - "Detached .sig file format with publisher_id=, content_hash=, signature= lines"

patterns-established:
  - "TOFU key management: check_or_store_publisher_key() — new key stored, same key ok, different key error"
  - "Signature file convention: {wasm_path}.sig adjacent to WASM artifact"
  - "Verification status: 'verified' (signed + valid), 'unverified' (unsigned), 'failed' (check failed)"

requirements-completed: [SKILL-01, SKILL-02, SKILL-03]

# Metrics
duration: ~25min
completed: 2026-03-06
---

# Phase 35 Plan 01: Signing Infrastructure & CLI Summary

**Ed25519 publisher keypair module with SHA-256 content hashing, TOFU key management, and five new CLI subcommands (sign, update, keygen, verify, info)**

## Performance

- **Duration:** ~25 min
- **Started:** 2026-03-06
- **Completed:** 2026-03-06
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- Created signing.rs with PublisherKeypair (generate, sign, verify), content hashing, PEM file I/O, and signature serialization — 12 unit tests
- Extended SkillStore with hash/signature/publisher_id storage, TOFU key management (check_or_store, pin, unpin), update method — 17 unit tests
- Added V8 migration with nullable signing columns and publisher_keys table
- Added CLI subcommands: Sign, Update, Keygen, Verify, Info with full install-time verification flow

## Task Commits

Each task was committed atomically:

1. **Task 1 + Task 2: Signing module, V8 migration, store extensions, CLI commands** - `2b88e86` (feat)

## Files Created/Modified
- `crates/blufio-skill/src/signing.rs` - PublisherKeypair Ed25519 signing, SHA-256 hashing, PEM file I/O
- `crates/blufio-storage/migrations/V8__skill_signing.sql` - Schema migration for signing columns and publisher_keys table
- `crates/blufio-skill/src/store.rs` - Extended with hash/sig storage, TOFU management, update method, VerificationInfo
- `crates/blufio-skill/src/lib.rs` - Added signing module and re-exports
- `crates/blufio-skill/Cargo.toml` - Added sha2, ed25519-dalek, hex, rand dependencies
- `crates/blufio/src/main.rs` - Five new CLI subcommands with install-time verification
- `crates/blufio/Cargo.toml` - Added ed25519-dalek dependency

## Decisions Made
- Used hex encoding (not base64) for PEM-like key files — consistent with existing hex patterns in codebase
- PublisherKeypair completely separate from DeviceKeypair — different trust domain (author vs device)
- TOFU key management hard-blocks on key change — requires explicit re-trust (no silent key rotation)
- V8 migration columns are nullable so existing installed skills remain functional

## Deviations from Plan
None - plan executed as specified.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Signing infrastructure ready for Plan 02 (pre-execution verification gate)
- VerificationInfo struct available for sandbox integration
- All 29 tests passing (12 signing + 17 store)

---
*Phase: 35-skill-registry-code-signing*
*Plan: 01*
*Completed: 2026-03-06*
