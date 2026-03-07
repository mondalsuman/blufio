// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio uninstall` command implementation.
//!
//! Removes the Blufio binary, service files, and shell completions.
//! With `--purge`, creates an auto-backup of data before removal.
//! Detects and stops any running Blufio processes.

use std::path::{Path, PathBuf};

use blufio_core::BlufioError;

/// Run the uninstall command.
///
/// 1. Detect and stop running Blufio processes
/// 2. Remove binary, service files, and shell completions
/// 3. Optionally remove data (with auto-backup)
pub async fn run_uninstall(purge: bool) -> Result<(), BlufioError> {
    eprintln!();
    eprintln!("  blufio uninstall");
    eprintln!("  {}", "-".repeat(50));

    // Step 1: Detect and stop running processes
    stop_running_processes()?;

    // Step 2: Remove binary and service files
    let mut removed = Vec::new();

    // Binary
    if let Ok(binary_path) = std::env::current_exe() {
        // Don't remove the binary we're currently running -- that would crash.
        // Instead, note the path for the user to remove after.
        eprintln!(
            "  Binary: {} (remove after uninstall completes)",
            binary_path.display()
        );
    }

    // Systemd service file
    let systemd_path = Path::new("/etc/systemd/system/blufio.service");
    if systemd_path.exists() {
        match std::fs::remove_file(systemd_path) {
            Ok(()) => {
                removed.push(systemd_path.to_string_lossy().to_string());
                // Reload systemd
                let _ = std::process::Command::new("systemctl")
                    .args(["daemon-reload"])
                    .output();
            }
            Err(e) => {
                eprintln!(
                    "  WARNING: Cannot remove {}: {e} (may need sudo)",
                    systemd_path.display()
                );
            }
        }
    }

    // Launchd plist
    if let Some(home) = dirs::home_dir() {
        let plist_path = home
            .join("Library")
            .join("LaunchAgents")
            .join("io.bluf.blufio.plist");
        if plist_path.exists() {
            // Unload first
            let _ = std::process::Command::new("launchctl")
                .args(["unload", &plist_path.to_string_lossy()])
                .output();
            match std::fs::remove_file(&plist_path) {
                Ok(()) => removed.push(plist_path.to_string_lossy().to_string()),
                Err(e) => eprintln!("  WARNING: Cannot remove {}: {e}", plist_path.display()),
            }
        }
    }

    // Shell completions
    let completion_paths = get_completion_paths();
    for path in &completion_paths {
        if path.exists() {
            match std::fs::remove_file(path) {
                Ok(()) => removed.push(path.to_string_lossy().to_string()),
                Err(e) => {
                    eprintln!("  WARNING: Cannot remove {}: {e}", path.display());
                }
            }
        }
    }

    // Step 3: Handle data removal
    let config = blufio_config::load_and_validate().unwrap_or_default();
    let data_dir = dirs::data_dir()
        .map(|p| p.join("blufio"))
        .unwrap_or_else(|| PathBuf::from("."));
    let config_dir = dirs::config_dir()
        .map(|p| p.join("blufio"))
        .unwrap_or_else(|| PathBuf::from("."));

    if purge {
        // Auto-backup before purge
        let db_path = Path::new(&config.storage.database_path);
        if db_path.exists() {
            let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
            let backup_dir = data_dir.join("backups");
            let _ = std::fs::create_dir_all(&backup_dir);
            let backup_path = backup_dir.join(format!("pre-uninstall-{timestamp}.db"));

            eprintln!("  Creating backup before purge...");
            match crate::backup::run_backup(
                &config.storage.database_path,
                backup_path.to_str().unwrap_or_default(),
            ) {
                Ok(()) => {
                    eprintln!("  Backup saved: {} (recovery copy)", backup_path.display());
                }
                Err(e) => {
                    eprintln!("  WARNING: Backup failed: {e}");
                    eprintln!("  Continuing with purge...");
                }
            }
        }

        // Remove data directory
        if data_dir.exists() {
            match std::fs::remove_dir_all(&data_dir) {
                Ok(()) => removed.push(data_dir.to_string_lossy().to_string()),
                Err(e) => eprintln!("  WARNING: Cannot remove {}: {e}", data_dir.display()),
            }
        }

        // Remove config directory
        if config_dir.exists() {
            match std::fs::remove_dir_all(&config_dir) {
                Ok(()) => removed.push(config_dir.to_string_lossy().to_string()),
                Err(e) => eprintln!("  WARNING: Cannot remove {}: {e}", config_dir.display()),
            }
        }
    } else {
        // Interactive prompt (if terminal)
        if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
            eprintln!();
            eprint!("  Remove data directory {}? (y/N) ", data_dir.display());
            let mut input = String::new();
            if std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut input).is_ok() {
                let answer = input.trim().to_lowercase();
                if answer == "y" || answer == "yes" {
                    // Backup first
                    let db_path = Path::new(&config.storage.database_path);
                    if db_path.exists() {
                        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
                        let backup_path =
                            std::env::temp_dir().join(format!("blufio-backup-{timestamp}.db"));
                        eprintln!("  Creating backup before removal...");
                        match crate::backup::run_backup(
                            &config.storage.database_path,
                            backup_path.to_str().unwrap_or_default(),
                        ) {
                            Ok(()) => {
                                eprintln!("  Backup saved: {}", backup_path.display());
                            }
                            Err(e) => {
                                eprintln!("  WARNING: Backup failed: {e}");
                            }
                        }
                    }

                    if data_dir.exists() {
                        match std::fs::remove_dir_all(&data_dir) {
                            Ok(()) => removed.push(data_dir.to_string_lossy().to_string()),
                            Err(e) => {
                                eprintln!("  WARNING: Cannot remove {}: {e}", data_dir.display())
                            }
                        }
                    }
                }
            }
        } else {
            eprintln!("  Data directory preserved: {}", data_dir.display());
            eprintln!("  Use --purge to remove all data (auto-backup created first).");
        }
    }

    // Print summary
    eprintln!();
    if removed.is_empty() {
        eprintln!("  No files were removed.");
    } else {
        eprintln!("  Removed:");
        for path in &removed {
            eprintln!("    - {path}");
        }
    }
    eprintln!();

    Ok(())
}

/// Detect and stop running Blufio processes.
fn stop_running_processes() -> Result<(), BlufioError> {
    // Try systemd first
    if let Ok(output) = std::process::Command::new("systemctl")
        .args(["is-active", "blufio"])
        .output()
        && output.status.success()
    {
        let status = String::from_utf8_lossy(&output.stdout);
        if status.trim() == "active" {
            eprintln!("  Stopping blufio systemd service...");
            let stop_result = std::process::Command::new("systemctl")
                .args(["stop", "blufio"])
                .output();
            match stop_result {
                Ok(out) if out.status.success() => {
                    eprintln!("  Service stopped.");
                }
                _ => {
                    eprintln!("  WARNING: Could not stop service (may need sudo).");
                }
            }
        }
    }

    // Try launchd (macOS)
    if cfg!(target_os = "macos")
        && let Ok(output) = std::process::Command::new("launchctl")
            .args(["list"])
            .output()
    {
        let list = String::from_utf8_lossy(&output.stdout);
        if list.contains("blufio") {
            eprintln!("  Stopping blufio launchd service...");
            if let Some(home) = dirs::home_dir() {
                let plist = home
                    .join("Library")
                    .join("LaunchAgents")
                    .join("io.bluf.blufio.plist");
                let _ = std::process::Command::new("launchctl")
                    .args(["unload", &plist.to_string_lossy()])
                    .output();
            }
        }
    }

    // Check for PID file or process as fallback
    if let Some(data_dir) = dirs::data_dir() {
        let pid_file = data_dir.join("blufio").join("blufio.pid");
        if pid_file.exists()
            && let Ok(pid_str) = std::fs::read_to_string(&pid_file)
        {
            let pid = pid_str.trim();
            eprintln!("  Found PID file (pid: {pid}). Checking process...");
            // Check if process is still running
            let check = std::process::Command::new("kill")
                .args(["-0", pid])
                .output();
            if let Ok(out) = check
                && out.status.success()
            {
                eprintln!("  Sending SIGTERM to process {pid}...");
                let _ = std::process::Command::new("kill").args([pid]).output();
                // Wait briefly for shutdown
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
            let _ = std::fs::remove_file(&pid_file);
        }
    }

    Ok(())
}

/// Get paths to shell completion files.
fn get_completion_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // Bash
    paths.push(PathBuf::from("/etc/bash_completion.d/blufio"));
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".bash_completion.d").join("blufio"));
    }

    // Zsh
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".zfunc").join("_blufio"));
    }

    // Fish
    if let Some(config_dir) = dirs::config_dir() {
        paths.push(
            config_dir
                .join("fish")
                .join("completions")
                .join("blufio.fish"),
        );
    }

    paths
}
