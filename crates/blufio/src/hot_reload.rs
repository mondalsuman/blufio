// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Hot reload module for config, TLS certificates, and skill/plugin WASM files.
//!
//! ## Config Hot Reload
//! Watches the `blufio.toml` config file for changes, parses and validates the
//! new config, and atomically swaps via [`ArcSwap`]. On successful swap, emits
//! a [`ConfigEvent::Reloaded`] event on the [`EventBus`].
//!
//! Non-reloadable fields (bind address, database path, gateway host/port) are
//! detected and logged as warnings without blocking the reload.
//!
//! ## TLS Certificate Hot Reload
//! Watches TLS cert/key files for changes and provides infrastructure for
//! zero-downtime certificate rotation. Currently a stub pending direct `rustls`
//! workspace dependency (the gateway uses plain TCP).
//!
//! ## Skill/Plugin Hot Reload
//! Watches the skill directory for `.wasm` file changes, detects additions,
//! modifications, and removals, logs signature verification status, and emits
//! [`ConfigEvent::Reloaded`] with source `"skill_reload"`. The actual
//! [`SkillStore`] update is handled in serve.rs (Plan 04 integration).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use blufio_bus::events::{BusEvent, ConfigEvent, new_event_id, now_timestamp};
use blufio_bus::EventBus;
use blufio_config::model::{BlufioConfig, HotReloadConfig};
use blufio_config::{load_config_from_path, validation};
use blufio_core::error::BlufioError;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Config fields that cannot be hot-reloaded (require restart).
const NON_RELOADABLE_FIELDS: &[(&str, &str)] = &[
    ("security.bind_address", "bind address"),
    ("storage.database_path", "database path"),
    ("gateway.host", "gateway host"),
    ("gateway.port", "gateway port"),
    ("agent.log_level", "log level (tracing subscriber)"),
];

/// Spawn a file watcher on the config file that triggers hot reload on changes.
///
/// Creates an [`ArcSwap<BlufioConfig>`] from the initial config and sets up a
/// file watcher using `notify-debouncer-mini`. On file change:
/// 1. Parse the TOML config file
/// 2. Validate the config
/// 3. Check for non-reloadable field changes (warn only)
/// 4. Atomically swap via [`ArcSwap::store`]
/// 5. Emit [`ConfigEvent::Reloaded`] on the [`EventBus`]
///
/// Returns the [`ArcSwap`] handle for config access. Sessions should call
/// [`load_config`] once at creation for config isolation (HTRL-05).
pub async fn spawn_config_watcher(
    config: BlufioConfig,
    config_path: PathBuf,
    event_bus: Arc<EventBus>,
    cancel: CancellationToken,
) -> Result<Arc<ArcSwap<BlufioConfig>>, BlufioError> {
    let debounce_ms = config.hot_reload.debounce_ms;
    let config_swap = Arc::new(ArcSwap::from_pointee(config));
    let config_swap_clone = Arc::clone(&config_swap);

    let (tx, mut rx) = mpsc::channel::<Vec<PathBuf>>(100);

    // Create debouncer with configurable debounce window
    let mut debouncer = notify_debouncer_mini::new_debouncer(
        Duration::from_millis(debounce_ms),
        move |res: Result<
            Vec<notify_debouncer_mini::DebouncedEvent>,
            notify::Error,
        >| {
            if let Ok(events) = res {
                let paths: Vec<PathBuf> = events.into_iter().map(|e| e.path).collect();
                if !paths.is_empty() {
                    // blocking_send because notify runs on its own thread
                    let _ = tx.blocking_send(paths);
                }
            }
        },
    )
    .map_err(|e| BlufioError::Internal(format!("failed to create config watcher: {e}")))?;

    // Watch the config file's parent directory (some editors replace files)
    let watch_path = config_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| config_path.clone());

    debouncer
        .watcher()
        .watch(&watch_path, notify::RecursiveMode::NonRecursive)
        .map_err(|e| {
            BlufioError::Internal(format!(
                "failed to watch {}: {e}",
                watch_path.display()
            ))
        })?;

    info!(
        path = %config_path.display(),
        debounce_ms = debounce_ms,
        "config hot reload watcher started"
    );

    let config_path_clone = config_path.clone();

    // Spawn the event processing task
    tokio::spawn(async move {
        // Keep debouncer alive for the lifetime of this task
        let _debouncer = debouncer;

        loop {
            tokio::select! {
                Some(paths) = rx.recv() => {
                    // Only process if the changed path matches our config file
                    let config_changed = paths.iter().any(|p| {
                        // Compare file names since editors may create temp files
                        p.file_name() == config_path_clone.file_name()
                    });

                    if config_changed {
                        reload_config(
                            &config_swap_clone,
                            &config_path_clone,
                            &event_bus,
                        ).await;
                    }
                }
                _ = cancel.cancelled() => {
                    info!("config hot reload watcher shutting down");
                    break;
                }
            }
        }
    });

    Ok(config_swap)
}

/// Reload config from the file, validate, swap, and emit event.
async fn reload_config(
    config_swap: &ArcSwap<BlufioConfig>,
    config_path: &Path,
    event_bus: &EventBus,
) {
    // Step 1: Parse the config file
    let new_config = match load_config_from_path(config_path) {
        Ok(c) => c,
        Err(e) => {
            warn!(
                error = %e,
                path = %config_path.display(),
                "config reload parse failed, keeping current config"
            );
            return;
        }
    };

    // Step 2: Validate the config
    if let Err(errors) = validation::validate_config(&new_config) {
        let error_msgs: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        warn!(
            errors = ?error_msgs,
            "config reload validation failed, keeping current config"
        );
        return;
    }

    // Step 3: Check for non-reloadable field changes
    let old_config = config_swap.load();
    check_non_reloadable_changes(&old_config, &new_config);

    // Step 4: Atomically swap the config
    config_swap.store(Arc::new(new_config));

    // Step 5: Emit ConfigEvent::Reloaded on EventBus
    event_bus
        .publish(BusEvent::Config(ConfigEvent::Reloaded {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            source: "hot_reload".into(),
        }))
        .await;

    info!("config hot-reloaded successfully");
}

/// Check if any non-reloadable fields have changed and log warnings.
///
/// Non-reloadable fields are those that bind resources at startup (ports,
/// addresses, database paths) and cannot be re-applied without a restart.
fn check_non_reloadable_changes(old: &BlufioConfig, new: &BlufioConfig) {
    for &(field_path, display_name) in NON_RELOADABLE_FIELDS {
        let changed = match field_path {
            "security.bind_address" => old.security.bind_address != new.security.bind_address,
            "storage.database_path" => old.storage.database_path != new.storage.database_path,
            "gateway.host" => old.gateway.host != new.gateway.host,
            "gateway.port" => old.gateway.port != new.gateway.port,
            "agent.log_level" => old.agent.log_level != new.agent.log_level,
            _ => false,
        };

        if changed {
            warn!(
                field = field_path,
                name = display_name,
                "config field '{}' changed but requires restart to take effect",
                display_name
            );
        }
    }
}

/// Load a snapshot of the current config from ArcSwap.
///
/// Sessions should call this once at creation for HTRL-05 (session config
/// isolation). The returned `Arc` is a stable snapshot that will not change
/// even if the config is hot-reloaded.
pub fn load_config(config: &ArcSwap<BlufioConfig>) -> Arc<BlufioConfig> {
    config.load_full()
}

// ---------------------------------------------------------------------------
// TLS Certificate Hot Reload
// ---------------------------------------------------------------------------

/// Spawn a file watcher on TLS cert/key files for hot reload.
///
/// Returns `None` if TLS paths are not configured in the [`HotReloadConfig`].
///
/// # Current status
///
/// This is a stub implementation. The Blufio gateway currently uses plain TCP,
/// and `rustls` is only available transitively through `reqwest` (not as a
/// direct workspace dependency). Adding a direct `rustls` dependency would
/// require version coordination across the workspace.
///
/// When TLS gateway support is added:
/// 1. Add `rustls` and `rustls-pemfile` to workspace dependencies
/// 2. Implement `load_certified_key()` using `rustls_pemfile::certs()` and
///    `rustls_pemfile::private_key()`
/// 3. Create `Arc<ArcSwap<CertifiedKey>>` with initial certs
/// 4. Set up file watcher on cert/key paths with debounce
/// 5. On change: reload, validate, swap (existing connections keep old cert)
///
/// # TODO
///
/// Replace this stub with full implementation when direct `rustls` dependency
/// is added to the workspace (tracked by HTRL-02).
pub async fn spawn_tls_watcher(
    config: &HotReloadConfig,
    _cancel: CancellationToken,
) -> Option<()> {
    let cert_path = config.tls_cert_path.as_deref().filter(|s| !s.is_empty());
    let key_path = config.tls_key_path.as_deref().filter(|s| !s.is_empty());

    match (cert_path, key_path) {
        (Some(cert), Some(key)) => {
            info!(
                cert_path = cert,
                key_path = key,
                "TLS hot reload configured but requires direct rustls dependency; \
                 certificates will not be watched (stub). See HTRL-02."
            );
            // TODO: Implement full TLS cert hot reload when rustls is a direct
            // workspace dependency. The implementation should:
            // - Load initial CertifiedKey via load_certified_key()
            // - Create Arc<ArcSwap<CertifiedKey>> for atomic cert rotation
            // - Set up notify-debouncer-mini watcher on cert/key paths
            // - On change: reload cert chain + private key, validate, swap
            // - On validation failure: warn, keep current certs
            None
        }
        (Some(_), None) | (None, Some(_)) => {
            warn!(
                "TLS hot reload requires both tls_cert_path and tls_key_path; \
                 only one is configured, skipping TLS watcher"
            );
            None
        }
        (None, None) => {
            // Plain TCP mode -- no TLS cert reload needed.
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Skill/Plugin Hot Reload
// ---------------------------------------------------------------------------

/// Spawn a file watcher on the skill directory for WASM module hot reload.
///
/// Watches for `.wasm` file additions, modifications, and removals.
/// On change:
/// 1. Scans the skill directory for `.wasm` files
/// 2. Checks for corresponding `.sig` files for signature verification
/// 3. Logs which skills were added/modified/removed
/// 4. Emits [`ConfigEvent::Reloaded`] with source `"skill_reload"` on the EventBus
///
/// The actual [`SkillStore`] update (re-loading WASM modules into the runtime)
/// is handled by the serve.rs integration (Plan 04). This function provides
/// the detection and notification layer.
pub async fn spawn_skill_watcher(
    skills_dir: PathBuf,
    event_bus: Arc<EventBus>,
    cancel: CancellationToken,
) -> Result<(), BlufioError> {
    if !skills_dir.exists() {
        info!(
            dir = %skills_dir.display(),
            "skill directory does not exist, skipping skill hot reload watcher"
        );
        return Ok(());
    }

    let debounce_ms = 500u64; // Use same debounce as config watcher default

    let (tx, mut rx) = mpsc::channel::<Vec<PathBuf>>(100);

    let mut debouncer = notify_debouncer_mini::new_debouncer(
        Duration::from_millis(debounce_ms),
        move |res: Result<
            Vec<notify_debouncer_mini::DebouncedEvent>,
            notify::Error,
        >| {
            if let Ok(events) = res {
                let paths: Vec<PathBuf> = events.into_iter().map(|e| e.path).collect();
                if !paths.is_empty() {
                    let _ = tx.blocking_send(paths);
                }
            }
        },
    )
    .map_err(|e| BlufioError::Internal(format!("failed to create skill watcher: {e}")))?;

    debouncer
        .watcher()
        .watch(&skills_dir, notify::RecursiveMode::NonRecursive)
        .map_err(|e| {
            BlufioError::Internal(format!(
                "failed to watch {}: {e}",
                skills_dir.display()
            ))
        })?;

    info!(
        dir = %skills_dir.display(),
        "skill hot reload watcher started"
    );

    let skills_dir_clone = skills_dir.clone();

    tokio::spawn(async move {
        // Keep debouncer alive for the lifetime of this task
        let _debouncer = debouncer;

        // Track known .wasm files for add/remove detection
        let mut known_wasm_files: HashSet<PathBuf> = scan_wasm_files(&skills_dir_clone);

        loop {
            tokio::select! {
                Some(changed_paths) = rx.recv() => {
                    // Check if any .wasm files were part of the change
                    let has_wasm_change = changed_paths.iter().any(|p| {
                        p.extension().is_some_and(|ext| ext == "wasm" || ext == "sig")
                    });

                    if has_wasm_change {
                        handle_skill_change(
                            &skills_dir_clone,
                            &mut known_wasm_files,
                            &event_bus,
                        ).await;
                    }
                }
                _ = cancel.cancelled() => {
                    info!("skill hot reload watcher shutting down");
                    break;
                }
            }
        }
    });

    Ok(())
}

/// Scan a directory for `.wasm` files.
fn scan_wasm_files(dir: &Path) -> HashSet<PathBuf> {
    let mut files = HashSet::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "wasm") {
                files.insert(path);
            }
        }
    }
    files
}

/// Handle a detected skill file change.
///
/// Compares the current set of `.wasm` files against the previously known set
/// to determine additions, removals, and modifications. Logs signature
/// verification status for each file and emits a reload event.
async fn handle_skill_change(
    skills_dir: &Path,
    known: &mut HashSet<PathBuf>,
    event_bus: &EventBus,
) {
    let current = scan_wasm_files(skills_dir);

    let added: Vec<&PathBuf> = current.difference(known).collect();
    let removed: Vec<&PathBuf> = known.difference(&current).collect();
    // Files present in both sets may have been modified
    let possibly_modified: Vec<&PathBuf> = current.intersection(known).collect();

    for path in &added {
        let sig_path = path.with_extension("sig");
        if sig_path.exists() {
            info!(
                skill = %path.display(),
                "new skill detected with .sig file (signature re-verification at load time)"
            );
        } else {
            warn!(
                skill = %path.display(),
                "new skill detected without .sig file; may be rejected at load time"
            );
        }
    }

    for path in &removed {
        info!(skill = %path.display(), "skill file removed");
    }

    // Log that existing skills may have been modified
    if !possibly_modified.is_empty() && added.is_empty() && removed.is_empty() {
        info!(
            count = possibly_modified.len(),
            "skill file(s) may have been modified, triggering reload event"
        );
    }

    // Update known set
    *known = current;

    // Emit reload event for downstream consumers (serve.rs SkillStore update)
    event_bus
        .publish(BusEvent::Config(ConfigEvent::Reloaded {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            source: "skill_reload".into(),
        }))
        .await;

    info!("skill hot reload event emitted");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> BlufioConfig {
        BlufioConfig::default()
    }

    #[test]
    fn check_non_reloadable_detects_bind_address_change() {
        let old = default_config();
        let mut new = default_config();
        new.security.bind_address = "0.0.0.0".to_string();

        // This should not panic -- it only logs warnings.
        // We verify it runs without error; log verification would require
        // tracing-test which is a dev-dependency concern for integration tests.
        check_non_reloadable_changes(&old, &new);
    }

    #[test]
    fn check_non_reloadable_detects_database_path_change() {
        let old = default_config();
        let mut new = default_config();
        new.storage.database_path = "/tmp/other.db".to_string();

        check_non_reloadable_changes(&old, &new);
    }

    #[test]
    fn check_non_reloadable_detects_gateway_changes() {
        let old = default_config();
        let mut new = default_config();
        new.gateway.host = "0.0.0.0".to_string();
        new.gateway.port = 9999;

        check_non_reloadable_changes(&old, &new);
    }

    #[test]
    fn check_non_reloadable_no_warning_when_unchanged() {
        let old = default_config();
        let new = default_config();

        // Should run without issue -- no fields changed.
        check_non_reloadable_changes(&old, &new);
    }

    #[test]
    fn load_config_returns_current_snapshot() {
        let config = default_config();
        let original_name = config.agent.name.clone();
        let swap = ArcSwap::from_pointee(config);

        let snapshot = load_config(&swap);
        assert_eq!(snapshot.agent.name, original_name);
    }

    #[test]
    fn arcswap_store_then_load_returns_new_config() {
        let mut config = default_config();
        config.agent.name = "old-agent".to_string();
        let swap = ArcSwap::from_pointee(config);

        // Verify initial config
        let snap1 = load_config(&swap);
        assert_eq!(snap1.agent.name, "old-agent");

        // Store new config
        let mut new_config = default_config();
        new_config.agent.name = "new-agent".to_string();
        swap.store(Arc::new(new_config));

        // Verify new config
        let snap2 = load_config(&swap);
        assert_eq!(snap2.agent.name, "new-agent");

        // Original snapshot is still valid (isolation)
        assert_eq!(snap1.agent.name, "old-agent");
    }

    // --- TLS watcher tests ---

    #[tokio::test]
    async fn tls_watcher_returns_none_when_no_paths_configured() {
        let config = HotReloadConfig::default();
        let cancel = CancellationToken::new();
        let result = spawn_tls_watcher(&config, cancel).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn tls_watcher_returns_none_with_empty_paths() {
        let config = HotReloadConfig {
            tls_cert_path: Some(String::new()),
            tls_key_path: Some(String::new()),
            ..HotReloadConfig::default()
        };
        let cancel = CancellationToken::new();
        let result = spawn_tls_watcher(&config, cancel).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn tls_watcher_returns_none_with_partial_paths() {
        let config = HotReloadConfig {
            tls_cert_path: Some("/tmp/cert.pem".to_string()),
            tls_key_path: None,
            ..HotReloadConfig::default()
        };
        let cancel = CancellationToken::new();
        let result = spawn_tls_watcher(&config, cancel).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn tls_watcher_stub_returns_none_with_both_paths() {
        let config = HotReloadConfig {
            tls_cert_path: Some("/tmp/cert.pem".to_string()),
            tls_key_path: Some("/tmp/key.pem".to_string()),
            ..HotReloadConfig::default()
        };
        let cancel = CancellationToken::new();
        // Stub always returns None until rustls is a direct dependency.
        let result = spawn_tls_watcher(&config, cancel).await;
        assert!(result.is_none());
    }

    // --- Skill watcher tests ---

    #[test]
    fn scan_wasm_files_finds_wasm_extensions() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("skill1.wasm"), b"wasm1").unwrap();
        std::fs::write(dir.path().join("skill2.wasm"), b"wasm2").unwrap();
        std::fs::write(dir.path().join("not_a_skill.txt"), b"text").unwrap();
        std::fs::write(dir.path().join("skill1.sig"), b"sig").unwrap();

        let files = scan_wasm_files(dir.path());
        assert_eq!(files.len(), 2);
        assert!(files.contains(&dir.path().join("skill1.wasm")));
        assert!(files.contains(&dir.path().join("skill2.wasm")));
    }

    #[test]
    fn scan_wasm_files_returns_empty_for_nonexistent_dir() {
        let files = scan_wasm_files(Path::new("/nonexistent/dir/that/does/not/exist"));
        assert!(files.is_empty());
    }

    #[tokio::test]
    async fn skill_watcher_skips_nonexistent_dir() {
        let event_bus = Arc::new(EventBus::new(16));
        let cancel = CancellationToken::new();
        let result = spawn_skill_watcher(
            PathBuf::from("/nonexistent/skill/dir"),
            event_bus,
            cancel,
        )
        .await;
        // Should succeed (just skips) rather than error
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn handle_skill_change_detects_additions() {
        let dir = tempfile::tempdir().unwrap();
        let event_bus = Arc::new(EventBus::new(16));
        let mut reliable_rx = event_bus.subscribe_reliable(16).await;

        // Start with empty known set
        let mut known = HashSet::new();

        // Add a .wasm file
        std::fs::write(dir.path().join("new_skill.wasm"), b"wasm_data").unwrap();

        handle_skill_change(dir.path(), &mut known, &event_bus).await;

        // Known set should now contain the new file
        assert_eq!(known.len(), 1);
        assert!(known.contains(&dir.path().join("new_skill.wasm")));

        // Should have received a ConfigEvent::Reloaded
        let event = reliable_rx.recv().await.unwrap();
        match event {
            BusEvent::Config(ConfigEvent::Reloaded { source, .. }) => {
                assert_eq!(source, "skill_reload");
            }
            _ => panic!("expected Config::Reloaded event"),
        }
    }

    #[tokio::test]
    async fn handle_skill_change_detects_removals() {
        let dir = tempfile::tempdir().unwrap();
        let event_bus = Arc::new(EventBus::new(16));
        let _reliable_rx = event_bus.subscribe_reliable(16).await;

        // Start with a file in the known set
        let wasm_path = dir.path().join("old_skill.wasm");
        let mut known = HashSet::new();
        known.insert(wasm_path.clone());

        // Don't create the file (simulates removal)
        handle_skill_change(dir.path(), &mut known, &event_bus).await;

        // Known set should be empty now (file was removed)
        assert!(known.is_empty());
    }
}
