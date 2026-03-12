// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Config hot reload module with ArcSwap-based atomic config swapping.
//!
//! Watches the `blufio.toml` config file for changes, parses and validates the
//! new config, and atomically swaps via [`ArcSwap`]. On successful swap, emits
//! a [`ConfigEvent::Reloaded`] event on the [`EventBus`].
//!
//! Non-reloadable fields (bind address, database path, gateway host/port) are
//! detected and logged as warnings without blocking the reload.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use blufio_bus::events::{BusEvent, ConfigEvent, new_event_id, now_timestamp};
use blufio_bus::EventBus;
use blufio_config::model::BlufioConfig;
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
}
