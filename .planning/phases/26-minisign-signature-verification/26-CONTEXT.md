# Phase 26: Minisign Signature Verification - Context

**Gathered:** 2026-03-03
**Status:** Ready for planning

<domain>
## Phase Boundary

Operator can verify that any Blufio binary or file is authentically signed by the project maintainer. Includes a `blufio verify` CLI command and an embedded public key. Self-update (Phase 27) is a separate phase that will consume this verification capability.

</domain>

<decisions>
## Implementation Decisions

### Verify command interface
- Auto-detect `.minisig` signature file next to the target file (e.g., `blufio verify blufio-v1.2.0` looks for `blufio-v1.2.0.minisig`)
- `--signature <path>` flag available for explicit override when auto-detect won't work
- Single file verification only — no batch mode (Phase 27 only needs single-file verify)
- New `blufio-verify` crate in `crates/blufio-verify/` — provides public `verify_signature()` library function
- CLI command in `crates/blufio/src/` calls the library function; Phase 27 self-update also calls it directly

### Success/failure output
- Clean one-liner on success: `Verified: <filename> (signed by <trusted comment signer>)`
- Trusted comments from the `.minisig` file are displayed as part of the success message
- Standard exit codes: 0 = signature valid, 1 = signature invalid or error
- Status/informational messages to stderr, final result to stdout (follows existing blufio command pattern)

### Error detail level
- File name + what failed + actionable next step in every error message (meets SIGN-03)
- Distinct error messages for each failure type:
  - File not found
  - Signature file not found (with hint: `Use --signature <path> to specify manually`)
  - Invalid signature format
  - Signature doesn't match file content
  - Key ID mismatch (shows expected vs actual key ID)
- Crypto details hidden except key ID on mismatch — operators don't need algorithm internals

### Library & key format
- Full Minisign format — compatible with standard `minisign` CLI tool for independent verification
- Use `minisign-verify` Rust crate (verify-only, smaller dependency surface, fewer attack vectors)
- Minisign public key embedded as compile-time `const &str` in the binary (SIGN-01)
- Generate a new Minisign key pair during implementation; public key goes into the binary, secret key stays with maintainer

### Claude's Discretion
- Internal module structure within `blufio-verify` crate
- Exact error message wording (within the guidelines above)
- Test strategy (unit tests for verification logic, integration tests for CLI)
- Whether to re-export types or keep the API minimal

</decisions>

<specifics>
## Specific Ideas

- Verification should feel like `minisign -Vm <file> -p <pubkey>` — familiar to anyone who's used minisign before
- Must work in CI pipelines: exit codes, no interactive prompts, parseable output
- Phase 27 (self-update) is the primary internal consumer — the library API should be ergonomic for "verify this downloaded file before swapping"

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `blufio-auth-keypair` crate: Has Ed25519 signing/verification via `ed25519-dalek` for agent messages — different format (not Minisign) but related crypto domain; key management patterns can inform design
- `blufio-core::BlufioError`: Established error type with variants — new `Verify` or `Signature` variant needed
- `clap` derive pattern in `main.rs`: `Commands` enum with `Subcommand` derive — new `Verify` variant follows established pattern

### Established Patterns
- Each CLI subcommand has its own module file (`backup.rs`, `doctor.rs`, `encrypt.rs`) — `verify.rs` follows this
- Commands return `Result<(), BlufioError>` with `eprintln!` in main for error display
- Workspace has 21 crates — adding `blufio-verify` as crate #22 follows the modular pattern
- Other commands use `eprintln!` for status messages to stderr

### Integration Points
- `main.rs` `Commands` enum: Add `Verify` variant with file path + optional `--signature` flag
- `Cargo.toml` workspace members: Add `crates/blufio-verify`
- `blufio-core/src/error.rs`: Add signature verification error variant
- Phase 27 will depend on `blufio-verify` crate for pre-swap verification

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 26-minisign-signature-verification*
*Context gathered: 2026-03-03*
