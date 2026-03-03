# Phase 26: Minisign Signature Verification - Research

**Researched:** 2026-03-03
**Domain:** Cryptographic signature verification (Ed25519/Minisign format)
**Confidence:** HIGH

## Summary

Phase 26 adds a `blufio verify` CLI command and a `blufio-verify` library crate for verifying Minisign signatures. The Minisign format (created by Frank Denis, author of libsodium) uses Ed25519 for signing and is designed for simplicity and auditability. The Rust ecosystem provides `minisign-verify`, a zero-dependency crate that handles verification only (no signing), making it ideal for embedding in an agent binary.

The implementation follows the established Blufio pattern: a library crate (`blufio-verify`) exposes a `verify_signature()` function, and the CLI binary crate adds a `Verify` subcommand that calls it. The public key is embedded as a compile-time `const &str` in base64 format. Phase 27 (self-update) will consume the library function directly.

**Primary recommendation:** Use `minisign-verify` crate (v0.2.5, zero deps, MIT license) with compile-time embedded public key and auto-detection of `.minisig` sidecar files.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Auto-detect `.minisig` signature file next to the target file (e.g., `blufio verify blufio-v1.2.0` looks for `blufio-v1.2.0.minisig`)
- `--signature <path>` flag available for explicit override when auto-detect won't work
- Single file verification only — no batch mode (Phase 27 only needs single-file verify)
- New `blufio-verify` crate in `crates/blufio-verify/` — provides public `verify_signature()` library function
- CLI command in `crates/blufio/src/` calls the library function; Phase 27 self-update also calls it directly
- Clean one-liner on success: `Verified: <filename> (signed by <trusted comment signer>)`
- Trusted comments from the `.minisig` file are displayed as part of the success message
- Standard exit codes: 0 = signature valid, 1 = signature invalid or error
- Status/informational messages to stderr, final result to stdout (follows existing blufio command pattern)
- File name + what failed + actionable next step in every error message (meets SIGN-03)
- Distinct error messages for each failure type (file not found, sig not found, invalid format, content mismatch, key ID mismatch)
- Full Minisign format — compatible with standard `minisign` CLI tool for independent verification
- Use `minisign-verify` Rust crate (verify-only, smaller dependency surface, fewer attack vectors)
- Minisign public key embedded as compile-time `const &str` in the binary (SIGN-01)
- Generate a new Minisign key pair during implementation; public key goes into the binary, secret key stays with maintainer

### Claude's Discretion
- Internal module structure within `blufio-verify` crate
- Exact error message wording (within the guidelines above)
- Test strategy (unit tests for verification logic, integration tests for CLI)
- Whether to re-export types or keep the API minimal

### Deferred Ideas (OUT OF SCOPE)
- None — discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| SIGN-01 | Minisign public key is embedded as a compile-time constant in the binary | `const MINISIGN_PUBLIC_KEY: &str = "..."` pattern; `PublicKey::from_base64()` parses at runtime from compile-time constant |
| SIGN-02 | Downloaded binary signature is verified against embedded public key before any file operations | `PublicKey::verify(&content, &signature, false)` — verify-then-act pattern; library function returns Result |
| SIGN-03 | Signature verification failure aborts with clear error message | Distinct error variants in `BlufioError` for each failure type; `minisign-verify` returns typed errors |
| SIGN-04 | blufio verify CLI command verifies any file against a .minisig signature | New `Verify` variant in `Commands` enum with file path + optional `--signature` flag |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| minisign-verify | 0.2.5 | Verify Minisign Ed25519 signatures | Zero dependencies, verify-only (no signing code = smaller attack surface), MIT license, by Frank Denis (libsodium author), compatible with standard Minisign format |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| clap | 4.5 (workspace) | CLI argument parsing for `--signature` flag | Already in workspace; used for all Blufio commands |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| minisign-verify | minisign (full crate) | Full crate includes signing + key generation; larger dependency surface, unnecessary for verify-only use case |
| minisign-verify | ed25519-dalek (raw) | Already in workspace for agent keypair auth, but would require hand-rolling Minisign format parsing (untrusted comment, trusted comment, signature box format) |

**Installation:**
```toml
[dependencies]
minisign-verify = "0.2"
```

## Architecture Patterns

### Recommended Project Structure
```
crates/blufio-verify/
├── Cargo.toml
├── src/
│   └── lib.rs          # Public API: verify_signature(), VerifyError, embedded key const
crates/blufio/src/
├── verify.rs           # CLI command handler (mod verify)
└── main.rs             # Add Verify variant to Commands enum
```

### Pattern 1: Embedded Public Key as Compile-Time Constant
**What:** The Minisign public key is stored as a `const &str` in base64 format inside the `blufio-verify` crate. At verification time, `PublicKey::from_base64()` parses it.
**When to use:** When the trust anchor must be compiled into the binary — no external key file distribution needed.
**Example:**
```rust
// In crates/blufio-verify/src/lib.rs
/// Minisign public key for verifying Blufio releases.
/// Generated with: minisign -G -p blufio.pub -s blufio.key
const MINISIGN_PUBLIC_KEY: &str = "RW...base64...";

pub fn embedded_public_key() -> Result<minisign_verify::PublicKey, VerifyError> {
    minisign_verify::PublicKey::from_base64(MINISIGN_PUBLIC_KEY)
        .map_err(|e| VerifyError::InvalidKey(e.to_string()))
}
```

### Pattern 2: Auto-Detect Signature Sidecar
**What:** Given a file path, automatically look for `<path>.minisig` in the same directory. Fall back to `--signature` override.
**When to use:** Standard Minisign convention — `minisign -S` creates `.minisig` alongside the signed file.
**Example:**
```rust
fn resolve_signature_path(file_path: &Path, explicit_sig: Option<&Path>) -> Result<PathBuf, VerifyError> {
    if let Some(sig) = explicit_sig {
        if sig.exists() {
            return Ok(sig.to_path_buf());
        }
        return Err(VerifyError::SignatureNotFound {
            path: sig.display().to_string(),
            hint: None,
        });
    }
    // Auto-detect: append .minisig
    let auto_path = file_path.with_extension(
        format!("{}.minisig", file_path.extension().unwrap_or_default().to_str().unwrap_or(""))
    );
    // Simpler: just append .minisig to the full filename
    let mut sig_name = file_path.as_os_str().to_owned();
    sig_name.push(".minisig");
    let auto_path = PathBuf::from(sig_name);
    if auto_path.exists() {
        Ok(auto_path)
    } else {
        Err(VerifyError::SignatureNotFound {
            path: auto_path.display().to_string(),
            hint: Some("Use --signature <path> to specify manually".to_string()),
        })
    }
}
```

### Pattern 3: Verify-Then-Act
**What:** Verification returns a result type; callers decide what to do on failure. Never proceed with file operations after verification failure.
**When to use:** Both CLI (print error, exit 1) and library (return error to caller like Phase 27 self-update).
**Example:**
```rust
// Library API — returns structured result
pub fn verify_signature(
    file_path: &Path,
    signature_path: Option<&Path>,
) -> Result<VerifyResult, VerifyError> { ... }

pub struct VerifyResult {
    pub file_name: String,
    pub trusted_comment: String,
}
```

### Anti-Patterns to Avoid
- **Hand-rolling Minisign format parsing:** The `.minisig` file format has untrusted comment (line 1), signature (line 2), trusted comment (line 3), global signature (line 4). Use the crate's `Signature::decode()` or `Signature::from_file()` — do NOT parse manually.
- **Loading entire large files into memory:** Use streaming verification (`verify_stream()`) for files that could be large binaries. However, for typical Blufio binaries (25-50MB), reading fully into memory is acceptable and simpler.
- **Mixing error types:** Keep `VerifyError` as a dedicated enum in the verify crate, then convert to `BlufioError::Signature(...)` at the CLI boundary.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Minisign signature format parsing | Custom `.minisig` parser | `Signature::decode()` / `Signature::from_file()` | Format has 4 lines with specific base64 encoding, untrusted/trusted comments, global signature over trusted comment |
| Ed25519 verification | Custom crypto with `ed25519-dalek` | `PublicKey::verify()` | Minisign adds key ID matching, pre-hashed mode detection, trusted comment verification on top of raw Ed25519 |
| Public key format parsing | Custom base64+key ID extraction | `PublicKey::from_base64()` | Minisign public key format has algorithm identifier + key ID prefix before the Ed25519 key bytes |

**Key insight:** Minisign looks simple (it's "dead simple" by design) but the format has enough structure (key IDs, trusted comments, pre-hashed mode) that hand-rolling verification would miss edge cases that the established crate handles.

## Common Pitfalls

### Pitfall 1: Pre-Hashed vs Standard Signatures
**What goes wrong:** `minisign-verify` has a `prehashed` boolean parameter on `verify()`. Passing the wrong value causes verification to fail even with correct key/signature.
**Why it happens:** Newer versions of the `minisign` CLI default to pre-hashed signatures (Ed25519ph) for large files. Older versions use standard Ed25519.
**How to avoid:** The `Signature` struct from `minisign-verify` already knows whether it's pre-hashed (encoded in the signature algorithm byte). Pass `false` for the prehashed parameter — the crate handles detection internally through the signature's algorithm field.
**Warning signs:** "Signature verification failed" with known-good key and signature — check if prehashed flag is wrong.

### Pitfall 2: Key ID Mismatch Silent Failures
**What goes wrong:** The signature was made with a different key than the embedded one. Verification fails but the error message doesn't explain why.
**Why it happens:** The Minisign format embeds a key ID in both the public key and the signature. A mismatch means the signature was made by a different key.
**How to avoid:** Before calling `verify()`, compare key IDs from the public key and signature. If they differ, return a specific `KeyIdMismatch` error with both IDs. The `minisign-verify` crate handles this internally, but wrapping the error with context improves UX.
**Warning signs:** Verification failure right after key rotation or when testing with wrong key pair.

### Pitfall 3: `.minisig` Auto-Detection Path Construction
**What goes wrong:** Naive path extension replacement (`.with_extension()`) replaces the existing extension instead of appending. `blufio-v1.2.0.tar.gz` becomes `blufio-v1.2.0.tar.minisig` instead of `blufio-v1.2.0.tar.gz.minisig`.
**Why it happens:** Rust's `Path::with_extension()` replaces the last extension component.
**How to avoid:** Construct the `.minisig` path by appending to the full OsStr filename: `let mut sig = file.as_os_str().to_owned(); sig.push(".minisig");`
**Warning signs:** "Signature file not found" when the file clearly exists.

### Pitfall 4: Forgetting to Generate a Real Key Pair
**What goes wrong:** Placeholder key in source code passes tests but breaks real verification.
**Why it happens:** Tests use test key pairs; easy to forget to generate the real project key pair.
**How to avoid:** Generate a real Minisign key pair with `minisign -G -p blufio.pub -s blufio.key` during implementation. Embed the real public key. Store secret key securely (not in repo). Tests should use a separate test key pair.
**Warning signs:** Tests pass but `blufio verify` fails on real release artifacts.

### Pitfall 5: cargo-deny License/Ban Violations
**What goes wrong:** Adding a dependency that pulls in `openssl-sys` transitively.
**Why it happens:** Some crates have optional OpenSSL features that get activated by feature unification.
**How to avoid:** `minisign-verify` has zero dependencies — no risk here. Run `cargo deny check` after adding the dependency to verify.
**Warning signs:** CI `cargo deny check` failure.

## Code Examples

### Basic Verification with minisign-verify
```rust
// Source: https://github.com/jedisct1/rust-minisign-verify README
use minisign_verify::{PublicKey, Signature};

let public_key = PublicKey::from_base64("RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3")
    .expect("Unable to decode the public key");

let signature = Signature::decode("untrusted comment: signature from minisign secret key\n...")
    .expect("Unable to decode the signature");

let content = std::fs::read("file").expect("Unable to read the file");
public_key.verify(&content[..], &signature, false)
    .expect("Signature didn't verify");
```

### File-Based Verification
```rust
// Source: https://github.com/jedisct1/rust-minisign-verify README
use std::path::Path;
use minisign_verify::{PublicKey, Signature};

let public_key = PublicKey::from_base64(EMBEDDED_KEY)?;
let signature = Signature::from_file(Path::new("release.minisig"))?;
let content = std::fs::read("release")?;
public_key.verify(&content, &signature, false)?;
```

### Streaming Verification for Large Files
```rust
// Source: https://docs.rs/minisign-verify
use std::io::Read;

let mut verifier = public_key.verify_stream(&signature)?;
let mut buffer = [0u8; 8192];
let mut file = std::fs::File::open("large-binary")?;
loop {
    let n = file.read(&mut buffer)?;
    if n == 0 { break; }
    verifier.update(&buffer[..n]);
}
verifier.finalize()?;
```

### CLI Subcommand Pattern (Following Blufio Convention)
```rust
// In crates/blufio/src/main.rs — Commands enum addition
/// Verify a file's Minisign signature.
Verify {
    /// Path to the file to verify.
    file: String,
    /// Path to the .minisig signature file (auto-detected if omitted).
    #[arg(long)]
    signature: Option<String>,
},
```

### Error Variant Pattern
```rust
// In crates/blufio-core/src/error.rs
/// Signature verification errors.
#[error("signature error: {0}")]
Signature(String),
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| GPG signatures for releases | Minisign/signify Ed25519 signatures | ~2019+ adoption | Simpler UX, smaller keys, faster verification, no web-of-trust complexity |
| Checksums only (SHA256SUMS) | Checksums + cryptographic signatures | Always best practice | Checksums verify integrity; signatures verify authenticity |

**Deprecated/outdated:**
- GPG for binary signing: Still works but unnecessarily complex for single-developer projects. Minisign provides equivalent security with a fraction of the UX friction.

## Open Questions

1. **Test key pair for CI/tests**
   - What we know: Tests need a key pair to sign test fixtures and verify them. Production needs a separate key pair.
   - What's unclear: Whether to check in test fixtures (signed files) or generate them in test setup.
   - Recommendation: Generate a test key pair, check in the test public key and pre-signed test fixtures. This makes tests deterministic and fast. The test secret key can be checked in since it's only for test data.

## Sources

### Primary (HIGH confidence)
- [docs.rs/minisign-verify](https://docs.rs/minisign-verify) - Full API documentation: PublicKey, Signature, StreamVerifier types, verify() and verify_stream() methods
- [github.com/jedisct1/rust-minisign-verify](https://github.com/jedisct1/rust-minisign-verify) - README with code examples, zero-dependency verification library
- [crates.io/crates/minisign-verify](https://crates.io/crates/minisign-verify) - v0.2.5, MIT license, zero dependencies

### Secondary (MEDIUM confidence)
- [github.com/jedisct1/minisign](https://github.com/jedisct1/minisign) - Original Minisign tool documentation, format specification, CLI reference

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - `minisign-verify` is the canonical verify-only Rust crate by the Minisign author; zero deps; MIT license passes cargo-deny
- Architecture: HIGH - follows established Blufio workspace crate pattern exactly; CLI subcommand pattern matches existing commands
- Pitfalls: HIGH - well-documented format with clear API; main risks are path construction and pre-hashed mode, both documented above

**Research date:** 2026-03-03
**Valid until:** 2026-04-03 (stable library, infrequent updates)
