# Phase 35: Skill Registry & Code Signing - Research

**Researched:** 2026-03-06
**Domain:** Cryptographic verification for WASM skill artifacts (Ed25519 signing, SHA-256 hashing, capability enforcement)
**Confidence:** HIGH

## Summary

Phase 35 extends the existing skill system with cryptographic verification at every execution boundary. The codebase already has all core dependencies (`ed25519-dalek`, `sha2`, `wasmtime` with capability gating) and established patterns (`DeviceKeypair` for Ed25519, `SkillStore` for SQLite CRUD, `SkillManifest` for TOML parsing, `WasmSkillRuntime` with capability-gated host functions). The work is primarily wiring — adding hash/signature columns to the registry, creating a `PublisherKeypair` type parallel to `DeviceKeypair`, adding verification gates to `invoke()`, and extending the CLI with new subcommands.

The trust model is TOFU (trust on first use) with optional key pinning. Unsigned skills are allowed as "unverified"; signed skills with failed verification are hard-blocked. This pragmatic approach balances developer convenience with production security.

**Primary recommendation:** Reuse existing `ed25519-dalek` patterns from `blufio-auth-keypair`, add `sha2` to `blufio-skill`, create a new V8 migration for hash/signature/pubkey columns, and gate `WasmSkillRuntime::invoke()` with pre-execution verification.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- TOFU (trust on first use) with optional key pinning: first install of a publisher's skill stores their public key; future updates must match; users can optionally pin keys via `blufio key pin <publisher>` for strict lockdown
- Unsigned skills are allowed and install as "unverified" — but if a skill HAS a signature and it fails verification, that's a hard block (tampered = rejected)
- Separate publisher keypair for skill signing, distinct from the device keypair used for auth (blufio-auth-keypair). Publisher keys are a different identity concept
- CLI command `blufio skill sign` for explicit signing after building — takes WASM path and private key, produces signature artifact
- Author runs signing as a post-build step, not automatically during build
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

### Deferred Ideas (OUT OF SCOPE)
None — discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| SKILL-01 | Local skill registry with install/list/remove/update commands | Existing `SkillStore` CRUD + `SkillCommands` enum; add Update/Sign/Keygen/Verify/Info subcommands |
| SKILL-02 | Registry stores skill manifests with SHA-256 content hashes | `sha2` workspace dep available; V8 migration adds `content_hash` column; hash computed at install and verified on every load |
| SKILL-03 | Ed25519 code signing for WASM skill artifacts | `ed25519-dalek` workspace dep; `PublisherKeypair` type parallel to `DeviceKeypair`; `blufio skill sign` CLI command |
| SKILL-04 | Signature verification at install time and before execution | Verification gate in `SkillStore::install()` and `WasmSkillRuntime::invoke()`; hard block on tampered signed skills |
| SKILL-05 | Capability enforcement checked at every WASM host function call site | Already implemented in `sandbox.rs` — host functions trap on denied capabilities; verify and add test coverage |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| ed25519-dalek | 2.1 | Ed25519 signing/verification | Already in workspace; used by blufio-auth-keypair; `SigningKey`, `VerifyingKey`, `Signature` types |
| sha2 | 0.10 | SHA-256 content hashing | Already in workspace; used by blufio-gateway and blufio-whatsapp |
| wasmtime | 40.x | WASM sandbox runtime | Already in workspace; capability gating in sandbox.rs |
| tokio-rusqlite | workspace | Async SQLite access | Already in blufio-skill deps; `Arc<Connection>` + `call()` pattern |
| hex | workspace | Hex encoding for hashes/keys | Already used by blufio-auth-keypair for public key display |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| rand | workspace | OsRng for keypair generation | Publisher keypair generation (same as DeviceKeypair) |
| base64 | workspace | Base64 encoding for signatures | Signature storage in manifest/registry |
| toml | workspace | Manifest parsing/writing | Extending skill.toml with signing metadata |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| ed25519-dalek | ring | ring is lower-level, doesn't have rand_core feature; ed25519-dalek already in workspace |
| SHA-256 (sha2) | BLAKE3 | BLAKE3 faster but sha2 already in workspace; SHA-256 is industry standard |
| SQLite for key storage | OS keyring | SQLite follows existing patterns (SkillStore, MemoryStore, CostLedger) |

## Architecture Patterns

### Recommended Structure
```
crates/blufio-skill/src/
├── lib.rs            # re-exports
├── manifest.rs       # SkillManifest TOML parsing (extend with signing metadata)
├── store.rs          # SkillStore SQLite CRUD (add hash/sig/pubkey columns)
├── sandbox.rs        # WasmSkillRuntime (add pre-execution verification gate)
├── signing.rs        # NEW: PublisherKeypair, sign/verify functions
├── scaffold.rs       # Skill project scaffolding
├── provider.rs       # Skill provider
├── tool.rs           # Skill tool
└── builtin/          # Built-in tools

crates/blufio-storage/migrations/
└── V8__skill_signing.sql  # NEW: Add hash/signature/pubkey columns
```

### Pattern 1: PublisherKeypair (parallel to DeviceKeypair)
**What:** Separate Ed25519 keypair type for skill signing
**When to use:** Publisher identity for signing skills
**Example:**
```rust
// Pattern from crates/blufio-auth-keypair/src/keypair.rs
pub struct PublisherKeypair {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl PublisherKeypair {
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = VerifyingKey::from(&signing_key);
        Self { signing_key, verifying_key }
    }

    pub fn from_bytes(private_bytes: &[u8; 32]) -> Result<Self, BlufioError> { ... }
    pub fn sign(&self, data: &[u8]) -> Signature { self.signing_key.sign(data) }
    pub fn verify(pubkey: &VerifyingKey, data: &[u8], sig: &Signature) -> Result<(), BlufioError> { ... }
}
```

### Pattern 2: Pre-execution Verification Gate
**What:** Verify WASM integrity before every `invoke()` call
**When to use:** At the top of `WasmSkillRuntime::invoke()` before creating Store
**Example:**
```rust
pub async fn invoke(&self, invocation: SkillInvocation) -> Result<SkillResult, BlufioError> {
    // NEW: Pre-execution verification
    if let Some(ref sig_info) = self.signatures.get(&invocation.skill_name) {
        self.verify_before_execution(&invocation.skill_name, sig_info)?;
    }
    // ... existing invoke logic
}
```

### Pattern 3: TOFU Key Storage in SQLite
**What:** Store publisher public keys in SQLite, trust on first install
**When to use:** When a signed skill is installed for the first time
**Example:**
```sql
-- V8 migration
CREATE TABLE IF NOT EXISTS publisher_keys (
    publisher_id TEXT PRIMARY KEY,
    public_key_hex TEXT NOT NULL,
    pinned INTEGER NOT NULL DEFAULT 0,
    first_seen TEXT NOT NULL,
    last_used TEXT NOT NULL
);

ALTER TABLE installed_skills ADD COLUMN content_hash TEXT;
ALTER TABLE installed_skills ADD COLUMN signature TEXT;
ALTER TABLE installed_skills ADD COLUMN publisher_id TEXT;
```

### Anti-Patterns to Avoid
- **Signing at build time:** User explicitly wants post-build signing as a separate step
- **Blocking unsigned skills:** Unsigned skills MUST be allowed as "unverified"
- **Verifying only at install:** Signature verification MUST happen before EVERY execution
- **Storing private keys in SQLite:** Only public keys stored; private keys are user-managed files

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Ed25519 signing | Custom crypto | ed25519-dalek `SigningKey::sign()` | Audited, constant-time, already in workspace |
| SHA-256 hashing | Custom hash | sha2 `Sha256::digest()` | Standard, already in workspace |
| Key serialization | Custom format | hex encoding (same as DeviceKeypair) | Consistent with existing patterns |
| TOML parsing | Custom parser | toml crate (already in deps) | Already used for manifest parsing |

**Key insight:** All crypto primitives are already available as workspace dependencies with established usage patterns.

## Common Pitfalls

### Pitfall 1: Forgetting to verify on re-load
**What goes wrong:** Skills verified at install but not on every execution — WASM file could be tampered on disk.
**Why it happens:** Developers often verify only at installation.
**How to avoid:** Hash check in `invoke()` compares stored hash against current file hash.
**Warning signs:** Missing verification gate at the top of `invoke()`.

### Pitfall 2: Race condition during file replacement
**What goes wrong:** WASM file replaced between hash check and module load.
**Why it happens:** TOCTOU (time-of-check-time-of-use) with filesystem operations.
**How to avoid:** Read WASM bytes once, compute hash, then pass bytes to wasmtime. Don't re-read from disk.
**Warning signs:** Separate read calls for hashing and module loading.

### Pitfall 3: Signature verification of wrong data
**What goes wrong:** Sign the WASM bytes but verify against different data (e.g., file path, module object).
**How to avoid:** Sign exactly the raw WASM bytes. Verify exactly the raw WASM bytes. Same data path.
**Warning signs:** Any transformation between signing and verification.

### Pitfall 4: Not handling migration for existing installed skills
**What goes wrong:** Existing skills in DB lack hash/signature columns after migration.
**Why it happens:** ALTER TABLE adds nullable columns; existing rows get NULL.
**How to avoid:** New columns must be nullable (NULL = unsigned/unhashed). Migration should NOT break existing skills. Compute hashes lazily on next load.
**Warning signs:** NOT NULL constraints on new columns without DEFAULT.

### Pitfall 5: Key pinning without unpinning
**What goes wrong:** User pins a key, publisher rotates key, no way to update.
**How to avoid:** Provide `blufio key unpin <publisher>` command.
**Warning signs:** Pin operation without corresponding unpin.

## Code Examples

### SHA-256 Content Hashing
```rust
use sha2::{Sha256, Digest};

fn compute_content_hash(wasm_bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(wasm_bytes);
    let result = hasher.finalize();
    hex::encode(result)
}
```

### Ed25519 Sign/Verify (following DeviceKeypair pattern)
```rust
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

fn sign_wasm(signing_key: &SigningKey, wasm_bytes: &[u8]) -> Signature {
    signing_key.sign(wasm_bytes)
}

fn verify_signature(
    verifying_key: &VerifyingKey,
    wasm_bytes: &[u8],
    signature: &Signature,
) -> Result<(), BlufioError> {
    verifying_key.verify(wasm_bytes, signature).map_err(|e| {
        BlufioError::Security(format!("signature verification failed: {e}"))
    })
}
```

### TOFU Key Lookup
```rust
async fn get_or_store_publisher_key(
    conn: &Connection,
    publisher_id: &str,
    public_key_hex: &str,
) -> Result<bool, BlufioError> {
    // Check if we already know this publisher
    let existing = conn.call(|c| {
        c.query_row(
            "SELECT public_key_hex, pinned FROM publisher_keys WHERE publisher_id = ?1",
            [publisher_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?))
        ).optional()
    }).await?;

    match existing {
        None => {
            // First time — TOFU: store and trust
            conn.call(|c| {
                c.execute("INSERT INTO publisher_keys ...", params![...])
            }).await?;
            Ok(true) // Trusted (first use)
        }
        Some((stored_key, pinned)) => {
            if stored_key == public_key_hex {
                Ok(true) // Key matches
            } else if pinned {
                Err(BlufioError::Security("pinned key mismatch".into()))
            } else {
                Err(BlufioError::Security("publisher key changed".into()))
            }
        }
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| ed25519-dalek 1.x | ed25519-dalek 2.x | 2023 | New API: `SigningKey`/`VerifyingKey` replaces `Keypair` |
| ring for Ed25519 | ed25519-dalek | Stable | Simpler API, better Rust ergonomics, rand_core support |
| Verify at install only | Verify at install + every execution | Best practice | Defense in depth against on-disk tampering |

**Deprecated/outdated:**
- ed25519-dalek 1.x `Keypair` type — replaced by `SigningKey` + `VerifyingKey` in 2.x (project already on 2.1)

## Open Questions

1. **What gets signed: WASM only vs WASM + manifest bundle?**
   - What we know: Signing WASM only is simplest and catches tampering of the executable. Manifest tampering (e.g., capability inflation) is a separate concern.
   - Recommendation: Sign WASM bytes only. Manifest integrity is ensured by storing the original manifest in SQLite at install time (already done via `manifest_toml` column). If manifest is tampered on disk, the stored version in SQLite takes precedence.

2. **Signature storage format: detached .sig vs embedded in manifest?**
   - What we know: Detached .sig files are simpler for the signing workflow. Embedded requires manifest format changes.
   - Recommendation: Store signature in the SQLite `installed_skills` table (alongside the skill). For the `blufio skill sign` command output, write a detached `<name>.sig` file containing hex-encoded signature + publisher public key. This file is consumed at install time.

3. **Publisher keypair file format?**
   - What we know: DeviceKeypair stores 32 bytes of private key. Need a human-friendly file format.
   - Recommendation: Use PEM-like format: `-----BEGIN BLUFIO PUBLISHER KEY-----\n{hex-encoded 32 bytes}\n-----END BLUFIO PUBLISHER KEY-----` for private key, similar for public key. Simple, greppable, distinct from SSH/TLS keys.

## Sources

### Primary (HIGH confidence)
- Existing codebase: `crates/blufio-auth-keypair/src/keypair.rs` — Ed25519 pattern with ed25519-dalek 2.1
- Existing codebase: `crates/blufio-skill/src/store.rs` — SQLite CRUD with `verification_status` field
- Existing codebase: `crates/blufio-skill/src/sandbox.rs` — wasmtime capability gating at host function level
- Existing codebase: `crates/blufio-skill/src/manifest.rs` — TOML manifest parsing
- Workspace `Cargo.toml` — `sha2 = "0.10"`, `ed25519-dalek = { version = "2.1", features = ["rand_core"] }`

### Secondary (MEDIUM confidence)
- ed25519-dalek 2.x API documentation — `SigningKey::sign()`, `VerifyingKey::verify()` semantics
- sha2 crate documentation — `Sha256::new().chain_update().finalize()` pattern

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All dependencies already in workspace with established patterns
- Architecture: HIGH - Extends existing patterns (SkillStore, DeviceKeypair, sandbox capability gating)
- Pitfalls: HIGH - Common crypto/verification pitfalls well-documented in security engineering literature

**Research date:** 2026-03-06
**Valid until:** 2026-04-06 (stable domain, no fast-moving dependencies)
