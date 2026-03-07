// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio bundle` command implementation.
//!
//! Creates a tar.gz archive for air-gapped deployment containing the binary,
//! config, WASM skills, manifest, and an install.sh script. Verifies the
//! binary's Minisign signature before bundling.

use std::path::{Path, PathBuf};

use blufio_core::BlufioError;

/// Run the bundle command.
///
/// Creates a tar.gz archive suitable for air-gapped deployment.
/// Step 1: Verify binary signature
/// Step 2: Collect bundle contents
/// Step 3: Generate manifest.toml
/// Step 4: Generate install.sh
/// Step 5: Create tar.gz archive
pub fn run_bundle(output: Option<&str>, include_data: bool) -> Result<(), BlufioError> {
    let config = blufio_config::load_and_validate()
        .map_err(|errors| BlufioError::Config(format!("{} config error(s)", errors.len())))?;

    let version = env!("CARGO_PKG_VERSION");
    let platform = target_triple();

    // Step 1: Locate and verify binary signature
    let binary_path = std::env::current_exe()
        .map_err(|e| BlufioError::Internal(format!("cannot locate current binary: {e}")))?;

    eprintln!("blufio: verifying binary signature...");
    let sig_path = binary_path.with_extension("minisig");
    if sig_path.exists() {
        match blufio_verify::verify_signature(&binary_path, Some(&sig_path)) {
            Ok(_) => eprintln!("  Signature verified."),
            Err(e) => {
                return Err(BlufioError::Signature(format!(
                    "binary signature verification failed: {e}. Cannot create bundle with unverified binary."
                )));
            }
        }
    } else {
        eprintln!(
            "  WARNING: No .minisig signature found for binary. Proceeding without verification."
        );
        eprintln!("  For production use, sign the binary first: minisign -Sm blufio");
    }

    // Step 2: Collect bundle contents
    let mut contents: Vec<(String, Vec<u8>)> = Vec::new();

    // Binary
    let binary_data = std::fs::read(&binary_path)
        .map_err(|e| BlufioError::Internal(format!("cannot read binary: {e}")))?;
    contents.push(("blufio".to_string(), binary_data));

    // Config file (sanitized - strip API keys and vault secrets)
    let config_path = find_config_path();
    if let Some(ref path) = config_path {
        if path.exists() {
            let config_content = std::fs::read_to_string(path)
                .map_err(|e| BlufioError::Internal(format!("cannot read config: {e}")))?;
            let sanitized = sanitize_config(&config_content);
            contents.push(("blufio.toml".to_string(), sanitized.into_bytes()));
        }
    }

    // WASM skills
    let skills_dir = Path::new(&config.skill.skills_dir);
    let mut skill_names = Vec::new();
    if skills_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(skills_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("wasm") {
                    if let Ok(data) = std::fs::read(&path) {
                        let name = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        skill_names.push(name.clone());
                        contents.push((format!("skills/{name}"), data));
                    }
                }
            }
        }
    }

    // Optional: database backup
    if include_data {
        let db_path = Path::new(&config.storage.database_path);
        if db_path.exists() {
            let dir = tempfile::tempdir().map_err(|e| {
                BlufioError::Internal(format!("cannot create temp dir for db backup: {e}"))
            })?;
            let backup_path = dir.path().join("blufio.db");
            crate::backup::run_backup(
                &config.storage.database_path,
                backup_path.to_str().unwrap_or_default(),
            )?;
            let db_data = std::fs::read(&backup_path)
                .map_err(|e| BlufioError::Internal(format!("cannot read db backup: {e}")))?;
            contents.push(("data/blufio.db".to_string(), db_data));
        }
    }

    // Step 3: Generate manifest.toml
    let manifest = generate_manifest(version, &platform, &skill_names, include_data);
    contents.push(("manifest.toml".to_string(), manifest.into_bytes()));

    // Step 4: Generate install.sh
    let install_script = generate_install_script();
    contents.push(("install.sh".to_string(), install_script.into_bytes()));

    // Step 5: Create tar.gz archive
    let output_path = output
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("blufio-{version}-{platform}.tar.gz"));

    let tar_gz = std::fs::File::create(&output_path)
        .map_err(|e| BlufioError::Internal(format!("cannot create archive: {e}")))?;
    let enc = flate2::write::GzEncoder::new(tar_gz, flate2::Compression::default());
    let mut builder = tar::Builder::new(enc);

    for (name, data) in &contents {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(if name == "blufio" || name == "install.sh" {
            0o755
        } else {
            0o644
        });
        header.set_cksum();

        builder
            .append_data(&mut header, name, data.as_slice())
            .map_err(|e| BlufioError::Internal(format!("cannot write {name} to archive: {e}")))?;
    }

    let enc = builder
        .into_inner()
        .map_err(|e| BlufioError::Internal(format!("cannot finalize archive: {e}")))?;
    enc.finish()
        .map_err(|e| BlufioError::Internal(format!("cannot finish gzip: {e}")))?;

    // Step 6: Try to sign the archive (optional)
    let archive_sig_path = format!("{output_path}.minisig");
    eprintln!("  (archive signing skipped: requires signing key)");
    let _ = archive_sig_path; // suppress unused

    // Print summary
    let metadata = std::fs::metadata(&output_path)
        .map_err(|e| BlufioError::Internal(format!("cannot stat archive: {e}")))?;
    let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);

    eprintln!();
    eprintln!("  Bundle created: {output_path}");
    eprintln!("  Size: {size_mb:.1} MB");
    eprintln!("  Contents:");
    for (name, data) in &contents {
        let size_kb = data.len() as f64 / 1024.0;
        eprintln!("    {name} ({size_kb:.1} KB)");
    }
    eprintln!();

    Ok(())
}

/// Get the target triple for the current platform.
fn target_triple() -> String {
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    };

    let os = if cfg!(target_os = "linux") {
        "unknown-linux-gnu"
    } else if cfg!(target_os = "macos") {
        "apple-darwin"
    } else if cfg!(target_os = "windows") {
        "pc-windows-msvc"
    } else {
        "unknown"
    };

    format!("{arch}-{os}")
}

/// Find the config file path using standard XDG locations.
fn find_config_path() -> Option<PathBuf> {
    // Check XDG config dirs
    if let Some(config_dir) = dirs::config_dir() {
        let path = config_dir.join("blufio").join("blufio.toml");
        if path.exists() {
            return Some(path);
        }
    }

    // Check current directory
    let local = PathBuf::from("blufio.toml");
    if local.exists() {
        return Some(local);
    }

    None
}

/// Sanitize config content by removing API keys and secrets.
fn sanitize_config(content: &str) -> String {
    let mut output = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        // Strip lines that look like they contain secrets
        if trimmed.starts_with("api_key")
            || trimmed.starts_with("bot_token")
            || trimmed.starts_with("app_token")
            || trimmed.starts_with("access_token")
            || trimmed.starts_with("app_secret")
            || trimmed.starts_with("verify_token")
            || trimmed.starts_with("password")
            || trimmed.starts_with("bearer_token")
            || trimmed.starts_with("auth_token")
        {
            // Replace with a commented-out placeholder
            let key = trimmed.split('=').next().unwrap_or(trimmed).trim();
            output.push_str(&format!("# {key} = \"<REDACTED>\"\n"));
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }

    output
}

/// Generate the manifest.toml content.
fn generate_manifest(
    version: &str,
    platform: &str,
    skill_names: &[String],
    includes_data: bool,
) -> String {
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let skills_list: Vec<String> = skill_names.iter().map(|s| format!("\"{s}\"")).collect();

    format!(
        r#"[bundle]
version = "1.0"
created = "{timestamp}"
platform = "{platform}"
blufio_version = "{version}"

[contents]
binary = "blufio"
config = "blufio.toml"
skills = [{skills}]
includes_data = {includes_data}
"#,
        skills = skills_list.join(", "),
    )
}

/// Generate the install.sh script.
fn generate_install_script() -> String {
    r#"#!/bin/sh
set -e

# Blufio Installation Script
# Generated by: blufio bundle

INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
CONFIG_DIR="${CONFIG_DIR:-$HOME/.config/blufio}"
DATA_DIR="${DATA_DIR:-$HOME/.local/share/blufio}"
SKILLS_DIR="${SKILLS_DIR:-$DATA_DIR/skills}"

echo "Installing Blufio..."
echo "  Binary:  $INSTALL_DIR/blufio"
echo "  Config:  $CONFIG_DIR/"
echo "  Data:    $DATA_DIR/"

# Copy binary
if [ -f blufio ]; then
    mkdir -p "$INSTALL_DIR"
    cp blufio "$INSTALL_DIR/blufio"
    chmod +x "$INSTALL_DIR/blufio"
    echo "  [OK] Binary installed"
else
    echo "  [SKIP] No binary found in bundle"
fi

# Set up config
mkdir -p "$CONFIG_DIR"
if [ -f blufio.toml ]; then
    if [ -f "$CONFIG_DIR/blufio.toml" ]; then
        echo "  [SKIP] Config exists at $CONFIG_DIR/blufio.toml (not overwriting)"
    else
        cp blufio.toml "$CONFIG_DIR/"
        echo "  [OK] Config installed"
    fi
fi

# Install skills
if [ -d skills ]; then
    mkdir -p "$SKILLS_DIR"
    for wasm in skills/*.wasm; do
        [ -f "$wasm" ] || continue
        cp "$wasm" "$SKILLS_DIR/"
    done
    echo "  [OK] Skills installed"
fi

# Install data (if present)
if [ -d data ]; then
    mkdir -p "$DATA_DIR"
    if [ -f data/blufio.db ]; then
        if [ -f "$DATA_DIR/blufio.db" ]; then
            echo "  [SKIP] Database exists (not overwriting)"
        else
            cp data/blufio.db "$DATA_DIR/"
            echo "  [OK] Database installed"
        fi
    fi
fi

# Detect service manager and offer to install service
if command -v systemctl >/dev/null 2>&1; then
    echo ""
    echo "  Systemd detected. To install as a service:"
    echo "    sudo cp blufio.service /etc/systemd/system/"
    echo "    sudo systemctl daemon-reload"
    echo "    sudo systemctl enable --now blufio"
elif [ "$(uname)" = "Darwin" ]; then
    echo ""
    echo "  macOS detected. To install as a launch agent:"
    echo "    cp io.bluf.blufio.plist ~/Library/LaunchAgents/"
    echo "    launchctl load ~/Library/LaunchAgents/io.bluf.blufio.plist"
fi

echo ""
echo "Blufio installed successfully."
echo "Run 'blufio doctor' to verify the installation."
"#
    .to_string()
}
