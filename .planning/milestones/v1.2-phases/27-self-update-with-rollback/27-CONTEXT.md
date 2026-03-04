# Phase 27: Self-Update with Rollback - Context

**Gathered:** 2026-03-04
**Status:** Ready for planning

<domain>
## Phase Boundary

Operator can update Blufio in-place with `blufio update`, with Minisign signature verification and automatic rollback if the new binary is broken. Includes `--check` for version queries and `rollback` subcommand for manual revert. Release publishing and CI/CD pipeline are out of scope.

</domain>

<decisions>
## Implementation Decisions

### Release Asset Resolution
- Asset naming convention: `blufio-{os}-{arch}` (e.g., `blufio-linux-x86_64`, `blufio-darwin-aarch64`)
- Signature files alongside: `blufio-linux-x86_64.minisig`
- GitHub release tags are v-prefixed: `v1.2.0`, `v1.3.0`
- Source repository: hardcoded `mondalsuman/blufio` (hits `api.github.com/repos/mondalsuman/blufio/releases/latest`)
- Network: public GitHub only, no proxy or GitHub Enterprise support in v1

### Update Flow & Confirmation UX
- `blufio update --check` outputs a simple one-liner: "Update available: v1.2.0 -> v1.3.0" or "Up to date: v1.2.0"
- Confirmation: interactive prompt "Update v1.2.0 -> v1.3.0? [y/N]" with `--yes` flag to skip; abort if stdin is not a TTY and no `--yes`
- Progress: step-by-step `eprintln!` status messages (no progress bar, no indicatif dependency)
  - "Downloading v1.3.0... done (12.4 MB)"
  - "Verifying signature... ok"
  - "Backing up current binary... done"
  - "Swapping... done"
  - "Health check... passed"
- Output convention: final result to stdout ("Updated: v1.2.0 -> v1.3.0"), all progress to stderr (matches existing verify command pattern)

### Backup & Rollback Strategy
- Pre-update backup stored next to current binary as `<binary>.bak` (e.g., `/usr/local/bin/blufio.bak`)
- Keep exactly one backup -- each update overwrites the previous `.bak`
- `blufio update rollback`: atomic rename of `.bak` back to main binary, no confirmation needed (rollback is the safety net)
- If no `.bak` exists, error with clear message: "No backup found. Nothing to rollback."
- Atomic swap via `self_replace` crate for cross-platform safety (handles binary-in-use edge cases)

### Health Check & Failure Handling
- Post-swap health check: run new binary with `doctor` subcommand (quick checks only -- config, database, encryption; skip deep checks and network checks)
- Health check timeout: 30 seconds; treat timeout as failure
- If health check fails: automatic rollback (swap `.bak` back, report failure to operator)
- Download/verification failure: clean abort, delete temp files, current binary untouched, exit code 1
- Signature verification failure: abort immediately, clear error message, no file operations performed

### Claude's Discretion
- Temp file location for downloads (e.g., tempfile crate vs manual /tmp path)
- Exact error message wording beyond the patterns established above
- How to detect current binary path (std::env::current_exe vs argv[0])
- Whether to add a `--force` flag to skip version comparison (download even if same version)

</decisions>

<specifics>
## Specific Ideas

- Update flow should feel like a single atomic operation from the operator's perspective -- either it fully succeeds or nothing changes
- The command should be safe to run in CI pipelines with `--yes` flag and proper exit codes
- Rollback should be instant and require no confirmation -- it's the escape hatch

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `blufio_verify::verify_signature()`: Minisign verification with embedded public key, auto-detects `.minisig` sidecar files -- directly usable for UPDT-03
- `doctor::run_doctor()`: Existing health check infrastructure with Pass/Warn/Fail status -- directly usable for UPDT-05
- `reqwest` workspace dependency: Already configured with `json`, `rustls`, `stream` features -- covers download and GitHub API calls

### Established Patterns
- CLI commands use `clap` with `Subcommand` derive pattern -- `Update` subcommand with `Check`/`Rollback` sub-subcommands follows existing structure
- Output convention: `eprintln!` for status messages, `println!` for final results, `std::process::exit(1)` on failure
- Interactive confirmation pattern exists in `encrypt::run_encrypt()` with `--yes` flag
- Error types flow through `BlufioError` enum in `blufio-core`

### Integration Points
- New `Update` variant in `Commands` enum in `main.rs`
- New `update.rs` module in `crates/blufio/src/`
- May need new `blufio-update` crate if logic is substantial, or can live in the binary crate
- `blufio-verify` crate is the dependency for signature verification
- Current binary version available via `clap`'s built-in version (from Cargo.toml)

</code_context>

<deferred>
## Deferred Ideas

None -- discussion stayed within phase scope

</deferred>

---

*Phase: 27-self-update-with-rollback*
*Context gathered: 2026-03-04*
