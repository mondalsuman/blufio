// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio update` command implementation.
//!
//! Provides self-update functionality:
//! - `blufio update` — download, verify, backup, swap, health-check
//! - `blufio update check` — report latest version without downloading
//! - `blufio update rollback` — revert to pre-update binary
//!
//! Output convention (matches existing blufio commands):
//! - Status messages to stderr (`eprintln!`)
//! - Final result to stdout (`println!`)
//! - Exit code 0 on success, 1 on any failure

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use blufio_core::BlufioError;
use serde::Deserialize;

/// GitHub repository for release lookups.
const GITHUB_REPO: &str = "mondalsuman/blufio";

/// GitHub API base URL.
const API_BASE: &str = "https://api.github.com";

/// GitHub release metadata.
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

/// A single release asset (binary or signature file).
#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// Parsed release information with resolved platform assets.
#[derive(Debug)]
struct ReleaseInfo {
    version: semver::Version,
    binary_url: String,
    signature_url: String,
    binary_size: u64,
}

// ---------------------------------------------------------------------------
// Version helpers
// ---------------------------------------------------------------------------

/// Get the current binary version from Cargo.toml (compile-time).
fn current_version() -> semver::Version {
    semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .expect("CARGO_PKG_VERSION must be valid semver")
}

/// Parse a version string, stripping an optional leading "v".
fn parse_version(tag: &str) -> Result<semver::Version, BlufioError> {
    let stripped = tag.strip_prefix('v').unwrap_or(tag);
    semver::Version::parse(stripped)
        .map_err(|e| BlufioError::Update(format!("invalid version '{tag}': {e}")))
}

/// Derive the platform-specific asset name.
///
/// Maps Rust target constants to the naming convention used in GitHub releases:
/// `blufio-{os}-{arch}` (e.g., `blufio-linux-x86_64`, `blufio-darwin-aarch64`).
fn platform_asset_name() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    let arch = std::env::consts::ARCH;
    format!("blufio-{os}-{arch}")
}

/// Locate the platform binary and its `.minisig` signature in a release's assets.
fn find_asset(release: &GitHubRelease) -> Result<ReleaseInfo, BlufioError> {
    let version = parse_version(&release.tag_name)?;
    let expected_name = platform_asset_name();
    let sig_name = format!("{expected_name}.minisig");

    let binary_asset = release
        .assets
        .iter()
        .find(|a| a.name == expected_name)
        .ok_or_else(|| {
            BlufioError::Update(format!(
                "no binary for this platform ({expected_name}) in release {}",
                release.tag_name
            ))
        })?;

    let sig_asset = release
        .assets
        .iter()
        .find(|a| a.name == sig_name)
        .ok_or_else(|| {
            BlufioError::Update(format!(
                "no signature file ({sig_name}) in release {}",
                release.tag_name
            ))
        })?;

    Ok(ReleaseInfo {
        version,
        binary_url: binary_asset.browser_download_url.clone(),
        signature_url: sig_asset.browser_download_url.clone(),
        binary_size: binary_asset.size,
    })
}

// ---------------------------------------------------------------------------
// Network
// ---------------------------------------------------------------------------

/// Fetch the latest release metadata from GitHub.
async fn fetch_latest_release() -> Result<ReleaseInfo, BlufioError> {
    let url = format!("{API_BASE}/repos/{GITHUB_REPO}/releases/latest");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| BlufioError::Update(format!("failed to create HTTP client: {e}")))?;

    let resp = client
        .get(&url)
        .header(
            "User-Agent",
            format!("blufio/{}", env!("CARGO_PKG_VERSION")),
        )
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| BlufioError::Update(format!("failed to check for updates: {e}")))?;

    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(BlufioError::Update(
            "no releases found for this repository".into(),
        ));
    }
    if status == reqwest::StatusCode::FORBIDDEN {
        return Err(BlufioError::Update(
            "GitHub API rate limit exceeded. Try again later.".into(),
        ));
    }
    if !status.is_success() {
        return Err(BlufioError::Update(format!(
            "GitHub API returned status {status}"
        )));
    }

    let release: GitHubRelease = resp
        .json()
        .await
        .map_err(|e| BlufioError::Update(format!("failed to parse release info: {e}")))?;

    find_asset(&release)
}

/// Download a URL to a temporary file in `dir`.
///
/// Creates the temp file in the same directory as the target binary so that
/// later rename/replace operations stay on the same filesystem.
async fn download_to_temp(url: &str, dir: &Path) -> Result<tempfile::NamedTempFile, BlufioError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| BlufioError::Update(format!("failed to create HTTP client: {e}")))?;

    let resp = client
        .get(url)
        .header(
            "User-Agent",
            format!("blufio/{}", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .map_err(|e| BlufioError::Update(format!("download failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(BlufioError::Update(format!(
            "download returned status {}",
            resp.status()
        )));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| BlufioError::Update(format!("failed to read download: {e}")))?;

    let mut tmp = tempfile::NamedTempFile::new_in(dir)
        .map_err(|e| BlufioError::Update(format!("failed to create temp file: {e}")))?;
    tmp.write_all(&bytes)
        .map_err(|e| BlufioError::Update(format!("failed to write temp file: {e}")))?;

    Ok(tmp)
}

// ---------------------------------------------------------------------------
// Verification
// ---------------------------------------------------------------------------

/// Verify a downloaded binary against its Minisign signature.
fn verify_download(binary_path: &Path, sig_path: &Path) -> Result<(), BlufioError> {
    blufio_verify::verify_signature(binary_path, Some(sig_path))
        .map(|_| ())
        .map_err(|e| BlufioError::Update(format!("signature verification failed: {e}")))
}

// ---------------------------------------------------------------------------
// Binary path helpers
// ---------------------------------------------------------------------------

/// Get the path to the currently running binary.
fn binary_path() -> Result<PathBuf, BlufioError> {
    std::env::current_exe()
        .map_err(|e| BlufioError::Update(format!("cannot determine binary path: {e}")))
}

/// Derive the `.bak` backup path from the binary path.
fn bak_path() -> Result<PathBuf, BlufioError> {
    let bin = binary_path()?;
    let mut bak_name: OsString = bin.as_os_str().to_owned();
    bak_name.push(".bak");
    Ok(PathBuf::from(bak_name))
}

// ---------------------------------------------------------------------------
// Backup & rollback
// ---------------------------------------------------------------------------

/// Back up the current binary to `<binary>.bak`.
///
/// Overwrites any existing `.bak` file (keeps exactly one backup).
/// Preserves file permissions.
fn backup_current() -> Result<(), BlufioError> {
    let bin = binary_path()?;
    let bak = bak_path()?;

    std::fs::copy(&bin, &bak).map_err(|e| {
        BlufioError::Update(format!(
            "failed to backup binary to '{}': {e}",
            bak.display()
        ))
    })?;

    // Preserve permissions.
    let perms = std::fs::metadata(&bin)
        .map_err(|e| BlufioError::Update(format!("failed to read binary metadata: {e}")))?
        .permissions();
    std::fs::set_permissions(&bak, perms)
        .map_err(|e| BlufioError::Update(format!("failed to set backup permissions: {e}")))?;

    Ok(())
}

/// Internal rollback: rename `.bak` back to binary path.
fn do_rollback() -> Result<(), BlufioError> {
    let bin = binary_path()?;
    let bak = bak_path()?;

    if !bak.exists() {
        return Err(BlufioError::Update(
            "No backup found. Nothing to rollback.".into(),
        ));
    }

    std::fs::rename(&bak, &bin).map_err(|e| {
        BlufioError::Update(format!("failed to rollback from '{}': {e}", bak.display()))
    })
}

// ---------------------------------------------------------------------------
// Health check
// ---------------------------------------------------------------------------

/// Run `blufio doctor` on the (possibly new) binary with a 30-second timeout.
///
/// Returns `true` only if: no timeout AND spawn succeeds AND exit status is 0.
async fn health_check() -> bool {
    let bin = match binary_path() {
        Ok(p) => p,
        Err(_) => return false,
    };

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::process::Command::new(&bin)
            .arg("doctor")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status(),
    )
    .await;

    match result {
        Ok(Ok(status)) => status.success(),
        Ok(Err(_)) => false, // Failed to spawn
        Err(_) => false,     // Timeout
    }
}

// ---------------------------------------------------------------------------
// Confirmation
// ---------------------------------------------------------------------------

/// Prompt the user for update confirmation.
///
/// Aborts if stdin is not a TTY (unless `--yes` was used at the call site).
fn confirm_update(current: &semver::Version, latest: &semver::Version) -> Result<(), BlufioError> {
    use std::io::BufRead;

    if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        return Err(BlufioError::Update(
            "update requires confirmation. Use --yes to skip, or run interactively.".into(),
        ));
    }

    eprint!("Update v{current} -> v{latest}? [y/N] ");
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| BlufioError::Update(format!("failed to read input: {e}")))?;

    if !line.trim().eq_ignore_ascii_case("y") {
        eprintln!("Aborted.");
        std::process::exit(0);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// `blufio update check` — report latest version without downloading.
pub async fn run_check() -> Result<(), BlufioError> {
    eprintln!("blufio: checking for updates...");
    let latest = fetch_latest_release().await?;
    let current = current_version();

    if latest.version > current {
        println!("Update available: v{current} -> v{}", latest.version);
    } else {
        println!("Up to date: v{current}");
    }
    Ok(())
}

/// `blufio update` — full self-update flow.
pub async fn run_update(yes: bool) -> Result<(), BlufioError> {
    let latest = fetch_latest_release().await?;
    let current = current_version();

    if latest.version <= current {
        println!("Up to date: v{current}");
        return Ok(());
    }

    if !yes {
        confirm_update(&current, &latest.version)?;
    }

    // Determine binary directory for temp files (same filesystem).
    let binary_dir = binary_path()?
        .parent()
        .ok_or_else(|| BlufioError::Update("cannot determine binary directory".into()))?
        .to_path_buf();

    // Download binary + signature.
    eprint!("Downloading v{}... ", latest.version);
    let binary_tmp = download_to_temp(&latest.binary_url, &binary_dir).await?;
    let sig_tmp = download_to_temp(&latest.signature_url, &binary_dir).await?;
    eprintln!(
        "done ({:.1} MB)",
        latest.binary_size as f64 / (1024.0 * 1024.0)
    );

    // Verify signature BEFORE any file operations.
    eprint!("Verifying signature... ");
    verify_download(binary_tmp.path(), sig_tmp.path())?;
    eprintln!("ok");

    // Set executable permission on downloaded binary (Unix).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(binary_tmp.path(), perms)
            .map_err(|e| BlufioError::Update(format!("failed to set permissions: {e}")))?;
    }

    // Backup current binary.
    eprint!("Backing up current binary... ");
    backup_current()?;
    eprintln!("done");

    // Atomic swap.
    eprint!("Swapping... ");
    self_replace::self_replace(binary_tmp.path())
        .map_err(|e| BlufioError::Update(format!("failed to replace binary: {e}")))?;
    eprintln!("done");

    // Health check.
    eprint!("Health check... ");
    if health_check().await {
        eprintln!("passed");
        println!("Updated: v{current} -> v{}", latest.version);
        Ok(())
    } else {
        eprintln!("FAILED");
        eprintln!("Rolling back...");
        do_rollback()?;
        Err(BlufioError::Update(
            "health check failed after update, rolled back to previous version".into(),
        ))
    }
}

/// `blufio update rollback` — revert to pre-update binary.
pub fn run_rollback() -> Result<(), BlufioError> {
    eprintln!("blufio: rolling back to previous version");
    do_rollback()?;
    println!("Rolled back successfully.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_version_is_valid() {
        let v = current_version();
        // Should parse without panic.
        assert!(!v.to_string().is_empty());
    }

    #[test]
    fn parse_version_strips_v_prefix() {
        let v = parse_version("v1.2.3").unwrap();
        assert_eq!(v, semver::Version::new(1, 2, 3));
    }

    #[test]
    fn parse_version_works_without_prefix() {
        let v = parse_version("1.2.3").unwrap();
        assert_eq!(v, semver::Version::new(1, 2, 3));
    }

    #[test]
    fn parse_version_rejects_invalid() {
        assert!(parse_version("not-a-version").is_err());
    }

    #[test]
    fn platform_asset_name_format() {
        let name = platform_asset_name();
        // Must start with "blufio-" and contain two hyphens.
        assert!(name.starts_with("blufio-"));
        let parts: Vec<&str> = name.split('-').collect();
        assert_eq!(parts.len(), 3, "expected blufio-os-arch, got: {name}");
        // OS part should not be "macos" (we map to "darwin").
        assert_ne!(parts[1], "macos");
    }

    #[test]
    fn find_asset_locates_correct_binary() {
        let expected = platform_asset_name();
        let sig_name = format!("{expected}.minisig");

        let release = GitHubRelease {
            tag_name: "v1.3.0".to_string(),
            assets: vec![
                GitHubAsset {
                    name: expected.clone(),
                    browser_download_url: format!("https://example.com/{expected}"),
                    size: 25_000_000,
                },
                GitHubAsset {
                    name: sig_name.clone(),
                    browser_download_url: format!("https://example.com/{sig_name}"),
                    size: 256,
                },
                GitHubAsset {
                    name: "blufio-other-arch".to_string(),
                    browser_download_url: "https://example.com/blufio-other-arch".to_string(),
                    size: 20_000_000,
                },
            ],
        };

        let info = find_asset(&release).unwrap();
        assert_eq!(info.version, semver::Version::new(1, 3, 0));
        assert!(info.binary_url.contains(&expected));
        assert!(info.signature_url.contains(&sig_name));
        assert_eq!(info.binary_size, 25_000_000);
    }

    #[test]
    fn find_asset_errors_when_binary_missing() {
        let release = GitHubRelease {
            tag_name: "v1.0.0".to_string(),
            assets: vec![GitHubAsset {
                name: "blufio-some-other-platform".to_string(),
                browser_download_url: "https://example.com/other".to_string(),
                size: 1000,
            }],
        };

        let result = find_asset(&release);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("no binary for this platform"),
            "expected platform error, got: {err}"
        );
    }

    #[test]
    fn find_asset_errors_when_signature_missing() {
        let expected = platform_asset_name();
        let release = GitHubRelease {
            tag_name: "v1.0.0".to_string(),
            assets: vec![GitHubAsset {
                name: expected,
                browser_download_url: "https://example.com/binary".to_string(),
                size: 1000,
            }],
        };

        let result = find_asset(&release);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("no signature file"),
            "expected signature error, got: {err}"
        );
    }

    #[test]
    fn github_release_deserializes_from_json() {
        let json = r#"{
            "tag_name": "v1.2.0",
            "assets": [
                {
                    "name": "blufio-linux-x86_64",
                    "browser_download_url": "https://github.com/mondalsuman/blufio/releases/download/v1.2.0/blufio-linux-x86_64",
                    "size": 30000000
                },
                {
                    "name": "blufio-linux-x86_64.minisig",
                    "browser_download_url": "https://github.com/mondalsuman/blufio/releases/download/v1.2.0/blufio-linux-x86_64.minisig",
                    "size": 256
                }
            ]
        }"#;

        let release: GitHubRelease = serde_json::from_str(json).unwrap();
        assert_eq!(release.tag_name, "v1.2.0");
        assert_eq!(release.assets.len(), 2);
        assert_eq!(release.assets[0].name, "blufio-linux-x86_64");
        assert_eq!(release.assets[0].size, 30_000_000);
    }

    #[test]
    fn binary_path_returns_valid_path() {
        let path = binary_path().unwrap();
        assert!(
            path.exists(),
            "binary path should exist: {}",
            path.display()
        );
    }

    #[test]
    fn bak_path_appends_bak_extension() {
        let bak = bak_path().unwrap();
        let bak_str = bak.to_string_lossy();
        assert!(
            bak_str.ends_with(".bak"),
            "bak path should end with .bak: {bak_str}"
        );
    }

    #[test]
    fn rollback_with_no_bak_errors() {
        // do_rollback uses the real binary path, which likely has no .bak in test.
        // We test the error message directly.
        let result = do_rollback();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("No backup found"),
            "expected 'No backup found', got: {err}"
        );
    }

    #[test]
    fn backup_and_rollback_roundtrip() {
        // Create a fake "binary" in a temp dir to test backup/rollback logic
        // without touching the real binary.
        let dir = tempfile::tempdir().unwrap();
        let fake_bin = dir.path().join("test-binary");
        std::fs::write(&fake_bin, b"original content").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o755);
            std::fs::set_permissions(&fake_bin, perms).unwrap();
        }

        // Simulate backup.
        let mut bak_name: OsString = fake_bin.as_os_str().to_owned();
        bak_name.push(".bak");
        let fake_bak = PathBuf::from(bak_name);

        std::fs::copy(&fake_bin, &fake_bak).unwrap();
        let perms = std::fs::metadata(&fake_bin).unwrap().permissions();
        std::fs::set_permissions(&fake_bak, perms).unwrap();

        // Verify backup exists and matches.
        assert!(fake_bak.exists());
        assert_eq!(std::fs::read(&fake_bak).unwrap(), b"original content");

        // Simulate binary replacement.
        std::fs::write(&fake_bin, b"new version content").unwrap();
        assert_eq!(std::fs::read(&fake_bin).unwrap(), b"new version content");

        // Simulate rollback.
        std::fs::rename(&fake_bak, &fake_bin).unwrap();
        assert_eq!(std::fs::read(&fake_bin).unwrap(), b"original content");
        assert!(!fake_bak.exists());
    }

    #[test]
    fn backup_overwrites_existing_bak() {
        let dir = tempfile::tempdir().unwrap();
        let fake_bin = dir.path().join("binary");
        std::fs::write(&fake_bin, b"v1").unwrap();

        let mut bak_name: OsString = fake_bin.as_os_str().to_owned();
        bak_name.push(".bak");
        let fake_bak = PathBuf::from(&bak_name);

        // First backup.
        std::fs::copy(&fake_bin, &fake_bak).unwrap();
        assert_eq!(std::fs::read(&fake_bak).unwrap(), b"v1");

        // Update binary and backup again.
        std::fs::write(&fake_bin, b"v2").unwrap();
        std::fs::copy(&fake_bin, &fake_bak).unwrap();
        assert_eq!(
            std::fs::read(&fake_bak).unwrap(),
            b"v2",
            "backup should be overwritten with v2"
        );
    }

    #[cfg(unix)]
    #[test]
    fn backup_preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let fake_bin = dir.path().join("exec-binary");
        std::fs::write(&fake_bin, b"content").unwrap();
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&fake_bin, perms).unwrap();

        let mut bak_name: OsString = fake_bin.as_os_str().to_owned();
        bak_name.push(".bak");
        let fake_bak = PathBuf::from(bak_name);

        std::fs::copy(&fake_bin, &fake_bak).unwrap();
        let orig_perms = std::fs::metadata(&fake_bin).unwrap().permissions();
        std::fs::set_permissions(&fake_bak, orig_perms.clone()).unwrap();

        let bak_perms = std::fs::metadata(&fake_bak).unwrap().permissions();
        assert_eq!(
            bak_perms.mode() & 0o777,
            0o755,
            "backup should preserve executable permission"
        );
    }

    #[test]
    fn update_error_formats_correctly() {
        let err = BlufioError::Update("test error".to_string());
        assert_eq!(err.to_string(), "update error: test error");
    }
}
