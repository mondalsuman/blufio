# Phase 35: Skill Registry & Code Signing - Context

**Gathered:** 2026-03-06
**Status:** Ready for planning

<domain>
## Phase Boundary

Users can install, manage, and trust WASM skills with cryptographic verification at every execution boundary. This includes: local skill registry with install/list/remove/update commands, SHA-256 content hashing, Ed25519 code signing, signature verification at install and before every WASM execution, and capability enforcement at every host function call site.

</domain>

<decisions>
## Implementation Decisions

### Trust model & key management
- TOFU (trust on first use) with optional key pinning: first install of a publisher's skill stores their public key; future updates must match; users can optionally pin keys via `blufio key pin <publisher>` for strict lockdown
- Unsigned skills are allowed and install as "unverified" — but if a skill HAS a signature and it fails verification, that's a hard block (tampered = rejected)
- Separate publisher keypair for skill signing, distinct from the device keypair used for auth (blufio-auth-keypair). Publisher keys are a different identity concept

### Signing workflow
- CLI command `blufio skill sign` for explicit signing after building — takes WASM path and private key, produces signature artifact
- Author runs signing as a post-build step, not automatically during build

### Verification failure behavior
- Hard block (refuse to run) when a signed skill fails signature verification at execution time — no tampered code runs, period
- Execution denied with clear error message explaining the verification failure

### Claude's Discretion
- Key storage location (SQLite table vs keyring file) — choose based on existing patterns (SkillStore uses SQLite)
- What gets signed (WASM only vs WASM + manifest bundle) — choose based on security requirements
- Signature storage format (detached .sig vs embedded in manifest)
- Whether to add `blufio skill keygen` command for publisher keypair generation
- Install failure behavior (reject only vs reject + cleanup partial files)
- Whether to add `blufio skill verify <name>` command for manual verification
- Whether to emit SkillVerified/SkillVerificationFailed events on the event bus
- Update command design: re-install from path, version checking, bulk update support
- Whether to track source_path in installed_skills table for update support
- Whether to add `blufio skill info <name>` command for detailed skill inspection

</decisions>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches. User wants a pragmatic security model: convenient for development (unsigned allowed), strict where it matters (tampered = blocked), with optional lockdown for production use.

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `blufio-skill::SkillStore` (store.rs): SQLite-backed CRUD for installed skills — already has `verification_status` field (always "unverified" currently). Needs hash/signature columns added
- `blufio-skill::SkillManifest` (manifest.rs): TOML manifest parsing with capabilities, resources, wasm_entry — can be extended for signing metadata
- `blufio-skill::WasmSkillRuntime` (sandbox.rs): wasmtime sandbox with capability gating at host function level — verification hook needed before invoke()
- `blufio-auth-keypair::DeviceKeypair` (keypair.rs): Ed25519 sign/verify_strict via ed25519-dalek — pattern to reuse for publisher keypair (separate type, same crypto)
- `blufio-bus::events`: Typed event system — can add skill verification events

### Established Patterns
- SQLite for persistence (SkillStore, MemoryStore, CostLedger all use `Arc<Connection>` + `call()`)
- `ed25519-dalek` already in workspace dependencies with `rand_core` feature
- CLI commands via clap `#[derive(Subcommand)]` in main.rs — `SkillCommands` enum already has Init/List/Install/Remove
- `BlufioError::Skill` and `BlufioError::Security` error variants exist
- wasmtime v40 with fuel/epoch/memory controls in sandbox.rs

### Integration Points
- `crates/blufio/src/main.rs`: `SkillCommands` enum needs Sign, Update, (optionally Keygen, Verify, Info) variants
- `crates/blufio-skill/src/store.rs`: `InstalledSkill` struct and `installed_skills` table need hash/signature/pubkey columns
- `crates/blufio-skill/src/sandbox.rs`: `WasmSkillRuntime::invoke()` needs pre-execution signature verification gate
- `crates/blufio-skill/src/manifest.rs`: May need signing metadata fields
- `Cargo.toml` workspace: `sha2` crate needed for SHA-256 hashing (not yet a dependency)

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 35-skill-registry-code-signing*
*Context gathered: 2026-03-06*
