---
phase: 35-skill-registry-code-signing
verified: 2026-03-07T16:45:00Z
status: passed
score: 5/5 must-haves verified
re_verification: false
---

# Phase 35: Skill Registry & Code Signing Verification Report

**Phase Goal:** Local skill registry with install/list/remove/update CLI, SHA-256 content hashing, Ed25519 code signing, pre-execution verification gate, and capability enforcement at every WASM host function call site
**Verified:** 2026-03-07
**Status:** PASSED
**Re-verification:** No -- initial verification

---

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Local skill registry with install/list/remove/update CLI commands | VERIFIED | `crates/blufio/src/main.rs` lines 256-1460: `SkillCommands` enum with `Install`, `List`, `Remove`, `Update`, `Sign`, `Keygen`, `Verify`, `Info` variants; `crates/blufio-skill/src/store.rs` `SkillStore` with `install()`, `list()`, `remove()`, `update()`, `get()` methods backed by SQLite; V5 migration creates `installed_skills` table, V8 extends it with signing columns |
| 2 | Registry stores skill manifests with SHA-256 content hashes | VERIFIED | `crates/blufio-skill/src/signing.rs:91-96` `compute_content_hash()` uses `sha2::Sha256` to produce 64-char hex hash; `crates/blufio-skill/src/store.rs:29` `InstalledSkill.content_hash: Option<String>` field; `store.rs:71` `install()` accepts `content_hash` parameter; V8 migration adds `content_hash TEXT` column; test `compute_content_hash_is_consistent` verifies 64-char hex output |
| 3 | Ed25519 code signing for WASM artifacts | VERIFIED | `crates/blufio-skill/src/signing.rs:27-88` `PublisherKeypair` uses `ed25519_dalek::SigningKey`/`VerifyingKey`; `sign()` at line 74, `verify_signature()` at line 79; PEM-like file I/O (`save_keypair_to_file`, `load_private_key_from_file`, `load_public_key_from_file`); signature hex encoding/decoding (`signature_to_hex`, `signature_from_hex`); 12 unit tests in signing module all pass |
| 4 | Signature verification at install time AND before execution (dual verification) | VERIFIED | **Install-time:** `crates/blufio/src/main.rs` lines 1080-1143 (`SkillCommands::Install` handler) reads `.sig` file, verifies hash matches, verifies Ed25519 signature against WASM bytes before calling `store.install()`. **Pre-execution:** `crates/blufio-skill/src/sandbox.rs:132-181` `verify_before_execution()` checks content hash (line 142) and Ed25519 signature (line 154-177) against stored WASM bytes; called at top of `invoke()` (line 185) BEFORE Store creation or fuel allocation. 6 dedicated verification tests: `invoke_unsigned_skill_no_verification_block`, `invoke_signed_skill_with_valid_hash_passes`, `invoke_skill_with_hash_mismatch_blocks`, `invoke_signed_skill_with_valid_signature_passes`, `invoke_signed_skill_with_bad_signature_blocks`, `invoke_signed_skill_with_wrong_pubkey_blocks` |
| 5 | Capability enforcement at every WASM host function call site | VERIFIED | `crates/blufio-skill/src/sandbox.rs:328-677` `define_host_functions()` enforces capabilities per-call: **http_request** (line 433): `if !has_network` -> trap with "capability not permitted"; also validates domain against allowlist (line 455) and SSRF (line 469). **read_file** (line 525): `if !has_fs_read` -> trap; also validates path against `read_paths` (line 543). **write_file** (line 591): `if !has_fs_write` -> trap; also validates path against `write_paths` (line 614). **get_env** (line 656): `if !allowed_env.contains(&key)` -> returns -1. Tests: `sandbox_http_request_denied_produces_trap`, `sandbox_read_file_denied_produces_trap`, `sandbox_write_file_denied_produces_trap`, `sandbox_http_request_domain_not_allowed_traps`, `sandbox_read_file_outside_allowed_path_traps` all verify per-call enforcement with "capability not permitted" errors |

**Score:** 5/5 truths verified

---

## Required Artifacts

### Plan 01: Signing Infrastructure & CLI

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/blufio-skill/src/signing.rs` | Ed25519 PublisherKeypair with sign/verify, SHA-256 hashing, PEM file I/O | VERIFIED | 367 lines; PublisherKeypair with generate/from_bytes/sign/verify_signature; compute_content_hash; save/load keypair files; 12 unit tests |
| `crates/blufio-storage/migrations/V8__skill_signing.sql` | Schema migration for signing columns and publisher_keys table | VERIFIED | 16 lines; ALTER TABLE adds content_hash, signature, publisher_id; CREATE TABLE publisher_keys with TOFU fields |
| `crates/blufio-skill/src/store.rs` | Extended SkillStore with hash/sig storage, TOFU management, update method | VERIFIED | 976 lines; install() with signing params, update() with TOFU continuity check, get_verification_info(), check_or_store_publisher_key(), pin/unpin; 17 tests |
| `crates/blufio-skill/src/lib.rs` | signing module export and re-exports | VERIFIED | Module declaration `pub mod signing;` and re-exports for PublisherKeypair, compute_content_hash, and all file I/O functions |
| `crates/blufio/src/main.rs` | CLI subcommands: Install, Update, Sign, Keygen, Verify, Info | VERIFIED | SkillCommands enum (line 256) with all variants; Install handler (line 1080) with .sig verification; Sign handler (line 1300); Keygen handler (line 1336); Verify handler (line 1352); Info handler (line 1426) |
| `crates/blufio-skill/Cargo.toml` | sha2, ed25519-dalek, hex, rand dependencies | VERIFIED | Dependencies present for cryptographic operations |

### Plan 02: Pre-Execution Verification Gate

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/blufio-skill/src/sandbox.rs` | Pre-execution verification gate, wasm_bytes/verification HashMaps, updated load_skill() | VERIFIED | 1486 lines; `wasm_bytes: HashMap<String, Vec<u8>>` (line 55) for TOCTOU prevention; `verification: HashMap<String, VerificationInfo>` (line 57); `verify_before_execution()` (line 132-181) checks hash then signature; `load_skill()` (line 88-116) accepts `Option<VerificationInfo>`; `invoke()` (line 183-185) calls verification gate as FIRST operation; 6 verification-specific tests + 16 existing sandbox tests |

---

## Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `SkillCommands::Install` handler | `signing::compute_content_hash()` | CLI install flow | WIRED | main.rs:1114 computes hash from WASM bytes and verifies against .sig file hash |
| `SkillCommands::Install` handler | `PublisherKeypair::verify_signature()` | CLI install flow | WIRED | main.rs:1123-1143 verifies Ed25519 signature at install time |
| `WasmSkillRuntime::invoke()` | `verify_before_execution()` | First call in invoke | WIRED | sandbox.rs:185 — verification gate runs before Store creation, fuel allocation |
| `verify_before_execution()` | `compute_content_hash()` | Hash check | WIRED | sandbox.rs:142 — computes hash of stored WASM bytes and compares to expected |
| `verify_before_execution()` | `PublisherKeypair::verify_signature()` | Signature check | WIRED | sandbox.rs:177 — verifies Ed25519 signature using publisher's verifying key |
| `load_skill()` | `wasm_bytes` HashMap | TOCTOU prevention | WIRED | sandbox.rs:105 — stores raw bytes in memory; same bytes used for hash verification and module compilation |
| `SkillStore::install()` | `content_hash`/`signature`/`publisher_id` columns | SQLite storage | WIRED | store.rs:98-114 — INSERT OR REPLACE with all signing fields |
| `SkillStore::update()` | TOFU continuity check | Publisher key validation | WIRED | store.rs:154-165 — rejects updates from different publisher_id |
| `define_host_functions()` | Capability checks in closures | Per-call enforcement | WIRED | sandbox.rs:415-674 — each host function checks manifest capabilities via captured bool/Vec before executing |

---

## Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| SKILL-01 | 35-01 | Local skill registry with install/list/remove/update | SATISFIED | `SkillStore` CRUD methods + `SkillCommands` CLI enum; V5 migration creates table, V8 extends it; 17 store tests pass |
| SKILL-02 | 35-01 | Registry stores skill manifests with SHA-256 content hashes | SATISFIED | `compute_content_hash()` using sha2::Sha256; `InstalledSkill.content_hash` field; store.install() persists hash; 2 hash-specific signing tests |
| SKILL-03 | 35-01 | Ed25519 code signing for WASM artifacts | SATISFIED | `PublisherKeypair` with ed25519-dalek; sign/verify_signature methods; PEM file I/O; .sig detached signature format; 12 signing tests |
| SKILL-04 | 35-02 | Signature verification at install AND before execution | SATISFIED | **Dual verification:** (1) Install-time in main.rs SkillCommands::Install handler reads .sig, verifies hash and signature before store.install(). (2) Pre-execution in sandbox.rs verify_before_execution() called at top of invoke() before resource allocation. TOCTOU prevented via in-memory WASM bytes. 6 verification tests cover signed/unsigned/tampered/wrong-key scenarios |
| SKILL-05 | 35-02 | Capability enforcement at every WASM host function call site | SATISFIED | **Per-call enforcement:** http_request checks `has_network` boolean + domain allowlist + SSRF prevention. read_file checks `has_fs_read` boolean + path validation against allowed read paths. write_file checks `has_fs_write` boolean + path validation against allowed write paths. get_env checks `allowed_env.contains(&key)`. All checks occur INSIDE the host function closure on EVERY call, not just at registration. 5 capability-denial tests verify trap behavior with "capability not permitted" message |

All 5 requirements verified with code and test evidence.

---

## Anti-Patterns Found

No anti-patterns detected.

Scanned signing.rs, store.rs, sandbox.rs, and main.rs skill handlers for:
- TODO/FIXME/XXX/HACK/PLACEHOLDER comments: none found
- Empty implementations or stub returns: none found
- Capability checks at registration-time-only (instead of per-call): none found -- all checks are inside closures executed at call time

---

## Test Summary

| Module | Tests | Status |
|--------|-------|--------|
| signing (unit) | 12 | PASSED |
| store (unit + integration) | 17 | PASSED |
| sandbox (unit + integration) | 22 | PASSED |
| tool (unit) | 64 | PASSED |
| **Total blufio-skill** | **115** | **ALL PASSED** |

Verification-critical test breakdown:
- Hash computation: `compute_content_hash_is_consistent`, `compute_content_hash_differs_for_different_input`
- Signature: `sign_and_verify_succeeds`, `verify_rejects_tampered_data`, `verify_rejects_wrong_public_key`
- Pre-execution gate: `invoke_unsigned_skill_no_verification_block`, `invoke_signed_skill_with_valid_hash_passes`, `invoke_skill_with_hash_mismatch_blocks`, `invoke_signed_skill_with_valid_signature_passes`, `invoke_signed_skill_with_bad_signature_blocks`, `invoke_signed_skill_with_wrong_pubkey_blocks`
- Capability enforcement: `sandbox_http_request_denied_produces_trap`, `sandbox_read_file_denied_produces_trap`, `sandbox_write_file_denied_produces_trap`, `sandbox_http_request_domain_not_allowed_traps`, `sandbox_read_file_outside_allowed_path_traps`
- TOFU: `tofu_check_or_store_new_publisher`, `tofu_check_same_key_succeeds`, `tofu_check_different_key_fails`, `pin_and_unpin_publisher_key`

---

## Gaps Summary

No gaps. All 5 observable truths verified. All 8 artifacts exist and are substantive. All 9 key links are wired. All 5 requirements satisfied with code evidence. 115 tests pass across the blufio-skill crate.

---

_Verified: 2026-03-07_
_Verifier: Claude (gsd-executor)_
