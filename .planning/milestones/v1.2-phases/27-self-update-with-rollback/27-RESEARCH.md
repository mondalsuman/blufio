# Phase 27: Self-Update with Rollback - Research

**Researched:** 2026-03-04
**Domain:** Binary self-update with signature verification and rollback
**Confidence:** HIGH

## Summary

Phase 27 adds `blufio update` (with `--check` and `rollback` subcommands) for in-place binary self-update. The implementation downloads a platform-appropriate binary from GitHub Releases, verifies its Minisign signature (reusing the Phase 26 `blufio-verify` crate), backs up the current binary, performs an atomic swap via the `self_replace` crate, and runs `blufio doctor` as a health check.

The Rust ecosystem has mature, well-tested crates for every component: `self_replace` (v1.5.0) for cross-platform atomic binary replacement, `reqwest` (already in workspace) for HTTP downloads, `minisign-verify` (already in workspace via `blufio-verify`) for signature verification, and `semver` (already in workspace) for version comparison. No hand-rolled solutions are needed.

**Primary recommendation:** Implement as a single `update.rs` module in the binary crate (no new crate needed -- logic is thin: HTTP call, verify, swap). Use `self_replace::self_replace()` for the atomic swap, `blufio_verify::verify_signature()` for Minisign verification, and `tempfile` for secure temp file management during download.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Asset naming convention: `blufio-{os}-{arch}` (e.g., `blufio-linux-x86_64`, `blufio-darwin-aarch64`)
- Signature files alongside: `blufio-linux-x86_64.minisig`
- GitHub release tags are v-prefixed: `v1.2.0`, `v1.3.0`
- Source repository: hardcoded `mondalsuman/blufio` (hits `api.github.com/repos/mondalsuman/blufio/releases/latest`)
- Network: public GitHub only, no proxy or GitHub Enterprise support in v1
- `blufio update --check` outputs a simple one-liner: "Update available: v1.2.0 -> v1.3.0" or "Up to date: v1.2.0"
- Confirmation: interactive prompt "Update v1.2.0 -> v1.3.0? [y/N]" with `--yes` flag to skip; abort if stdin is not a TTY and no `--yes`
- Progress: step-by-step `eprintln!` status messages (no progress bar, no indicatif dependency)
- Output convention: final result to stdout ("Updated: v1.2.0 -> v1.3.0"), all progress to stderr
- Pre-update backup stored next to current binary as `<binary>.bak` (e.g., `/usr/local/bin/blufio.bak`)
- Keep exactly one backup -- each update overwrites the previous `.bak`
- `blufio update rollback`: atomic rename of `.bak` back to main binary, no confirmation needed
- If no `.bak` exists, error: "No backup found. Nothing to rollback."
- Atomic swap via `self_replace` crate for cross-platform safety
- Post-swap health check: run new binary with `doctor` subcommand (quick checks only)
- Health check timeout: 30 seconds; treat timeout as failure
- If health check fails: automatic rollback
- Download/verification failure: clean abort, delete temp files, current binary untouched, exit code 1
- Signature verification failure: abort immediately, clear error message, no file operations performed

### Claude's Discretion
- Temp file location for downloads (e.g., tempfile crate vs manual /tmp path)
- Exact error message wording beyond established patterns
- How to detect current binary path (std::env::current_exe vs argv[0])
- Whether to add a `--force` flag to skip version comparison

### Deferred Ideas (OUT OF SCOPE)
- None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| UPDT-01 | blufio update checks latest version against GitHub Releases API | GitHub Releases API `GET /repos/{owner}/{repo}/releases/latest` returns `tag_name` for version comparison via `semver` crate |
| UPDT-02 | blufio update downloads platform-appropriate binary and .minisig | `reqwest` with `bytes()` response for binary download; asset name resolution via `{os}-{arch}` pattern |
| UPDT-03 | Downloaded binary is Minisign-verified before any file operations | `blufio_verify::verify_signature()` directly reusable -- accepts file path + optional sig path |
| UPDT-04 | Current binary is backed up before atomic swap via self-replace | `self_replace::self_replace()` for atomic swap; `std::fs::copy()` for pre-swap `.bak` backup |
| UPDT-05 | Post-swap health check runs blufio doctor on new binary | `std::process::Command::new(binary_path).arg("doctor").status()` with 30s timeout via `tokio::time::timeout` |
| UPDT-06 | blufio update rollback reverts to pre-update binary | `std::fs::rename()` from `.bak` to binary path -- instant atomic operation |
| UPDT-07 | blufio update --check reports available version without downloading | Same GitHub API call as UPDT-01, just skip download; compare with `semver::Version` |
| UPDT-08 | Update requires --yes flag or interactive confirmation | TTY detection via `std::io::IsTerminal` (same pattern as `encrypt.rs`); `--yes` flag on `Update` subcommand |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| self_replace | 1.5.0 | Atomic binary replacement | De facto standard by Armin Ronacher (mitsuhiko); handles Unix rename + Windows binary-in-use edge cases |
| reqwest | 0.13 (workspace) | HTTP client for GitHub API + binary download | Already in workspace with `json`, `rustls`, `stream` features |
| minisign-verify | 0.2 (workspace) | Signature verification | Already used by `blufio-verify` crate; embedded public key pattern established |
| semver | 1 (workspace) | Version parsing and comparison | Already in workspace; standard for SemVer operations |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tempfile | 3 (dev-dependency) | Secure temp file for download | Download binary to temp file before verification |
| serde_json | 1 (existing) | Parse GitHub API JSON response | Deserialize release metadata |
| serde | 1 (existing) | Derive Deserialize for API types | GitHub release/asset structs |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| self_replace | Manual rename + copy | self_replace handles Windows edge cases (binary-in-use), not worth reimplementing |
| tempfile | Manual /tmp paths | tempfile handles cleanup on drop, unique naming, cross-platform temp dirs |
| Full GitHub client crate | reqwest directly | Overkill for one API endpoint; reqwest is already a dependency |

**New dependency to add:**
```toml
# In workspace Cargo.toml [workspace.dependencies]
self-replace = "1.5"
tempfile = "3"

# In crates/blufio/Cargo.toml [dependencies]
self-replace = { workspace = true }
tempfile = { workspace = true }
```

Note: `tempfile` is already a dev-dependency of the binary crate. It needs to become a regular dependency for production use in the update module.

## Architecture Patterns

### Recommended Module Structure
```
crates/blufio/src/
├── update.rs          # New: update command implementation
├── main.rs            # Modified: add Update command variant
├── verify.rs          # Existing: used for delegation
├── doctor.rs          # Existing: health check infrastructure
└── encrypt.rs         # Existing: --yes confirmation pattern reference
```

No new crate needed. The update logic is straightforward orchestration code (HTTP call -> verify -> backup -> swap -> health check) that ties together existing crates. Keeping it in the binary crate follows the pattern of `verify.rs`, `encrypt.rs`, and `doctor.rs`.

### Pattern 1: Update Flow State Machine
**What:** Linear state machine with clean abort at each step
**When to use:** Always -- the update flow is inherently sequential
**Example:**
```rust
pub async fn run_update(yes: bool) -> Result<(), BlufioError> {
    // 1. Check latest version (GitHub API)
    let latest = fetch_latest_release().await?;
    let current = current_version();

    if latest.version <= current {
        println!("Up to date: v{current}");
        return Ok(());
    }

    // 2. Confirm with user
    if !yes {
        confirm_update(&current, &latest.version)?;
    }

    // 3. Download binary + signature to temp files
    eprintln!("Downloading v{}... ", latest.version);
    let (binary_tmp, sig_tmp) = download_assets(&latest).await?;

    // 4. Verify signature (abort before any file ops if fail)
    eprintln!("Verifying signature... ");
    verify_download(&binary_tmp, &sig_tmp)?;

    // 5. Backup current binary
    eprintln!("Backing up current binary... ");
    backup_current()?;

    // 6. Atomic swap
    eprintln!("Swapping... ");
    self_replace::self_replace(&binary_tmp)?;

    // 7. Health check
    eprintln!("Health check... ");
    if !health_check().await {
        eprintln!("Health check failed. Rolling back...");
        rollback()?;
        return Err(BlufioError::Update("health check failed, rolled back".into()));
    }

    println!("Updated: v{current} -> v{}", latest.version);
    Ok(())
}
```

### Pattern 2: Platform Asset Resolution
**What:** Map `std::env::consts::{OS, ARCH}` to GitHub release asset names
**When to use:** When downloading platform-appropriate binary
**Example:**
```rust
fn platform_asset_name() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "aarch64",
        "x86_64" => "x86_64",
        other => other,
    };
    format!("blufio-{os}-{arch}")
}
```

### Pattern 3: CLI Subcommand with Sub-subcommands
**What:** `blufio update [check|rollback]` with optional `--yes` flag
**Example:**
```rust
/// Update Blufio to the latest version.
Update {
    #[command(subcommand)]
    action: Option<UpdateCommands>,
    /// Skip interactive confirmation.
    #[arg(long)]
    yes: bool,
}

#[derive(Subcommand, Debug)]
enum UpdateCommands {
    /// Check for available updates without downloading.
    Check,
    /// Rollback to the pre-update binary.
    Rollback,
}
```

### Anti-Patterns to Avoid
- **In-process restart after update:** The CONTEXT.md explicitly excludes this. Swap binary, let systemd restart if needed.
- **Downloading to working directory:** Use tempfile crate for secure, auto-cleaned temp files.
- **Silently proceeding on signature failure:** Must abort immediately with clear error.
- **Using `std::env::args().next()` for binary path:** Use `std::env::current_exe()` which resolves symlinks and gives the actual path.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Atomic binary replacement | Manual rename dance | `self_replace::self_replace()` | Handles Windows binary-in-use, Unix atomicity, temp file placement |
| Temp file management | Manual /tmp paths + cleanup | `tempfile::NamedTempFile` | Auto-cleanup on drop, unique names, secure creation |
| Version comparison | String comparison | `semver::Version::parse()` + `PartialOrd` | Handles pre-release, build metadata correctly |
| Minisign verification | Custom crypto | `blufio_verify::verify_signature()` | Already implemented and tested in Phase 26 |
| TTY detection | Manual isatty calls | `std::io::IsTerminal::is_terminal()` | Stable in std since Rust 1.70 |

**Key insight:** Every complex component (crypto verification, binary replacement, temp files, version parsing) has a battle-tested crate already in the workspace or available. The update module is pure orchestration glue.

## Common Pitfalls

### Pitfall 1: Temp file on different filesystem
**What goes wrong:** `self_replace` uses rename internally. If temp file is on a different mount than the binary, rename fails with `EXDEV`.
**Why it happens:** `tempfile::NamedTempFile::new()` uses system temp dir (e.g., `/tmp`), but binary may be at `/usr/local/bin`.
**How to avoid:** Use `tempfile::NamedTempFile::new_in(binary_dir)` to create temp file in the same directory as the binary.
**Warning signs:** Works on dev machine, fails on production with "cross-device link" error.

### Pitfall 2: Permission denied on binary directory
**What goes wrong:** User running `blufio update` doesn't have write permission to the directory containing the binary.
**Why it happens:** System-installed binaries in `/usr/local/bin` may require root/sudo.
**How to avoid:** Check write permission early (before downloading). Provide clear error: "Cannot write to /usr/local/bin. Try running with sudo."
**Warning signs:** Download succeeds but swap fails.

### Pitfall 3: Health check runs old binary
**What goes wrong:** After `self_replace`, running `blufio doctor` spawns the OLD binary from the OS process cache.
**Why it happens:** `self_replace` replaces the file, but `std::env::current_exe()` returns the path (which now points to the new binary). The issue is if you run `Command::new("blufio")` and `blufio` resolves via PATH to a different location.
**How to avoid:** Use `std::env::current_exe()` as the command path, not `"blufio"`.
**Warning signs:** Health check always passes even when new binary is broken.

### Pitfall 4: GitHub API rate limiting
**What goes wrong:** Unauthenticated GitHub API has 60 requests/hour limit.
**Why it happens:** Multiple update checks in quick succession.
**How to avoid:** For v1, this is acceptable (operators don't check 60 times/hour). Handle 403/429 with clear error message.
**Warning signs:** Intermittent "API rate limit exceeded" errors.

### Pitfall 5: Backup file permissions
**What goes wrong:** `.bak` file has different permissions than original binary.
**Why it happens:** `std::fs::copy()` copies content but may not preserve permissions on all platforms.
**How to avoid:** After creating backup, explicitly copy permissions: `std::fs::set_permissions(&bak_path, original_perms)`.
**Warning signs:** Rollback produces a binary that can't be executed.

### Pitfall 6: Self-replace on the running binary itself
**What goes wrong:** Confusion about what `self_replace` does vs manual rename.
**Why it happens:** `self_replace::self_replace(path)` replaces the *current running executable* with the file at `path`. It does NOT replace `path`.
**How to avoid:** Download new binary to temp file, then call `self_replace::self_replace(&temp_file_path)`.
**Warning signs:** New binary ends up in temp dir, old binary unchanged.

## Code Examples

### GitHub Releases API Call
```rust
use serde::Deserialize;

const GITHUB_REPO: &str = "mondalsuman/blufio";
const API_BASE: &str = "https://api.github.com";

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

async fn fetch_latest_release(client: &reqwest::Client) -> Result<GitHubRelease, BlufioError> {
    let url = format!("{API_BASE}/repos/{GITHUB_REPO}/releases/latest");
    let resp = client
        .get(&url)
        .header("User-Agent", format!("blufio/{}", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| BlufioError::Update(format!("failed to check for updates: {e}")))?;

    if !resp.status().is_success() {
        return Err(BlufioError::Update(format!(
            "GitHub API returned status {}",
            resp.status()
        )));
    }

    resp.json::<GitHubRelease>()
        .await
        .map_err(|e| BlufioError::Update(format!("failed to parse release info: {e}")))
}
```

### Binary Download to Temp File
```rust
use std::io::Write;
use tempfile::NamedTempFile;

async fn download_to_temp(
    client: &reqwest::Client,
    url: &str,
    dir: &std::path::Path,
) -> Result<NamedTempFile, BlufioError> {
    let resp = client
        .get(url)
        .header("User-Agent", format!("blufio/{}", env!("CARGO_PKG_VERSION")))
        .send()
        .await
        .map_err(|e| BlufioError::Update(format!("download failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(BlufioError::Update(format!(
            "download returned status {}",
            resp.status()
        )));
    }

    let bytes = resp.bytes().await
        .map_err(|e| BlufioError::Update(format!("failed to read download: {e}")))?;

    let mut tmp = NamedTempFile::new_in(dir)
        .map_err(|e| BlufioError::Update(format!("failed to create temp file: {e}")))?;
    tmp.write_all(&bytes)
        .map_err(|e| BlufioError::Update(format!("failed to write temp file: {e}")))?;

    Ok(tmp)
}
```

### Health Check with Timeout
```rust
async fn health_check(binary_path: &std::path::Path) -> bool {
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::process::Command::new(binary_path)
            .arg("doctor")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status(),
    )
    .await;

    match result {
        Ok(Ok(status)) => status.success(),
        Ok(Err(_)) => false,  // Failed to spawn
        Err(_) => false,       // Timeout
    }
}
```

### Interactive Confirmation (matches encrypt.rs pattern)
```rust
fn confirm_update(current: &semver::Version, latest: &semver::Version) -> Result<(), BlufioError> {
    if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        return Err(BlufioError::Update(
            "update requires confirmation. Use --yes to skip, or run interactively.".into(),
        ));
    }

    eprint!("Update v{current} -> v{latest}? [y/N] ");
    let mut line = String::new();
    std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line)
        .map_err(|e| BlufioError::Update(format!("failed to read input: {e}")))?;

    if !line.trim().eq_ignore_ascii_case("y") {
        eprintln!("Aborted.");
        std::process::exit(0);
    }
    Ok(())
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Manual binary download + chmod | `self_replace` crate | 2023+ | Cross-platform atomic replacement without manual rename dance |
| `std::env::args().next()` for exe path | `std::env::current_exe()` | Rust 1.0+ (stable) | Resolves symlinks, gives actual file path |
| Custom isatty checks | `std::io::IsTerminal` | Rust 1.70 | Standard library trait, no external crate needed |

## Open Questions

1. **`--force` flag**: CONTEXT.md mentions discretion on whether to add `--force` to skip version comparison. **Recommendation:** Skip for v1. The version check is a safety feature and the flow is simple enough. If needed later, it's a one-line addition.

2. **Current exe detection**: `std::env::current_exe()` is the right choice. It resolves symlinks which is exactly what we want -- we need the real binary path for backup and replacement. `argv[0]` could be a relative path or symlink.

3. **Temp file location**: Use `tempfile::NamedTempFile::new_in(binary_parent_dir)` to ensure same-filesystem for rename. Fall back to `NamedTempFile::new()` only if the binary directory is not writable.

## Sources

### Primary (HIGH confidence)
- [self_replace docs.rs](https://docs.rs/self-replace/latest/self_replace/) - API surface, platform support, v1.5.0
- [GitHub Releases API](https://docs.github.com/en/rest/releases/releases) - Latest release endpoint, asset schema
- [reqwest (seanmonstar/reqwest)](https://github.com/seanmonstar/reqwest) - HTTP client, already in workspace

### Secondary (MEDIUM confidence)
- [self-replace crates.io](https://crates.io/crates/self-replace/1.3.6) - Download stats, version history

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - all crates verified via docs.rs/crates.io, most already in workspace
- Architecture: HIGH - follows established patterns from verify.rs, encrypt.rs, doctor.rs
- Pitfalls: HIGH - based on verified crate documentation (cross-device link, permissions)

**Research date:** 2026-03-04
**Valid until:** 2026-04-04 (stable crates, unlikely to change)
