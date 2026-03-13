// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Subsystem startup for `blufio serve`.
//!
//! Initializes the EventBus, audit trail, resilience (circuit breakers and
//! degradation manager), cron scheduler, hook manager, hot reload watcher,
//! injection pipeline, and memory monitor.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use blufio_config::model::BlufioConfig;
use blufio_core::error::BlufioError;
use blufio_cron::CronScheduler;
use blufio_hooks::HookManager;
use blufio_memory::{MemoryStore, OnnxEmbedder};
use blufio_plugin::{PluginRegistry, PluginStatus, builtin_catalog};
use blufio_resilience::{
    CircuitBreakerConfig, CircuitBreakerRegistry, DegradationManager, EscalationConfig,
};
use blufio_skill::ToolRegistry;
use tracing::{debug, error, info, warn};

/// Initializes the plugin registry with the built-in catalog.
pub(crate) fn initialize_plugin_registry(config: &BlufioConfig) -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    let catalog = builtin_catalog();

    for manifest in catalog {
        let name = manifest.name.clone();
        let status = if let Some(&enabled) = config.plugin.plugins.get(&name) {
            if enabled {
                PluginStatus::Enabled
            } else {
                PluginStatus::Disabled
            }
        } else {
            PluginStatus::Enabled
        };
        registry.register_with_status(manifest, None, status);
    }

    info!(count = registry.len(), "plugin registry initialized");
    registry
}

/// Perform vault startup check and register config secrets for log redaction.
pub(crate) async fn vault_and_secret_redaction(
    config: &BlufioConfig,
    vault_values: &std::sync::Arc<std::sync::RwLock<Vec<String>>>,
) -> Result<(), BlufioError> {
    // SEC-03: Vault startup check -- unlock vault if it exists so secrets
    // are available for provider initialization. Silent no-op when no vault.
    {
        let vault_conn = blufio_storage::open_connection(&config.storage.database_path).await?;
        match blufio_vault::vault_startup_check(vault_conn, &config.vault).await {
            Ok(Some(_vault)) => {
                info!("vault unlocked -- secrets available");
                #[cfg(unix)]
                blufio_agent::sdnotify::notify_status("Initializing: vault unlocked");
            }
            Ok(None) => {
                debug!("no vault found -- skipping vault startup check");
            }
            Err(e) => {
                error!(error = %e, "vault startup check failed");
                eprintln!(
                    "error: Vault exists but cannot be unlocked. \
                     Set BLUFIO_VAULT_KEY environment variable or provide passphrase interactively."
                );
                return Err(e);
            }
        }
    }

    // Register known config secrets for log redaction (SEC-08).
    {
        if let Some(ref key) = config.anthropic.api_key {
            blufio_security::RedactingWriter::<std::io::Stderr>::add_vault_value(vault_values, key.clone());
        }
        if let Some(ref token) = config.telegram.bot_token {
            blufio_security::RedactingWriter::<std::io::Stderr>::add_vault_value(vault_values, token.clone());
        }
        if let Some(ref token) = config.gateway.bearer_token {
            blufio_security::RedactingWriter::<std::io::Stderr>::add_vault_value(vault_values, token.clone());
        }
        let secret_count = vault_values.read().map(|v| v.len()).unwrap_or(0);
        if secret_count > 0 {
            info!(count = secret_count, "secrets registered for log redaction");
        }
    }

    Ok(())
}

/// Create the global event bus.
pub(crate) fn create_event_bus() -> Arc<blufio_bus::EventBus> {
    let bus = Arc::new(blufio_bus::EventBus::new(1024));
    info!("global event bus created (capacity=1024)");
    bus
}

/// Initialize the audit trail subsystem.
pub(crate) async fn init_audit(
    config: &BlufioConfig,
    event_bus: &Arc<blufio_bus::EventBus>,
) -> Option<Arc<blufio_audit::AuditWriter>> {
    if !config.audit.enabled {
        info!("audit trail disabled");
        return None;
    }

    let audit_db_path = config.audit.db_path.clone().unwrap_or_else(|| {
        let db = std::path::Path::new(&config.storage.database_path);
        db.parent()
            .unwrap_or(std::path::Path::new("."))
            .join("audit.db")
            .to_string_lossy()
            .to_string()
    });

    match blufio_audit::AuditWriter::new(&audit_db_path).await {
        Ok(writer) => {
            let writer = Arc::new(writer);

            let filter = blufio_audit::EventFilter::new(config.audit.events.clone());
            let audit_rx = event_bus.subscribe_reliable(256).await;
            let subscriber = blufio_audit::AuditSubscriber::new(writer.clone(), filter);
            tokio::spawn(subscriber.run(audit_rx));

            info!(db_path = %audit_db_path, "audit trail enabled");

            let _ = event_bus
                .publish(blufio_bus::events::BusEvent::Audit(
                    blufio_bus::events::AuditMetaEvent::Enabled {
                        event_id: blufio_bus::events::new_event_id(),
                        timestamp: blufio_bus::events::now_timestamp(),
                    },
                ))
                .await;

            Some(writer)
        }
        Err(e) => {
            warn!(error = %e, "audit trail initialization failed, continuing without audit");
            None
        }
    }
}

/// Result of resilience subsystem initialization.
pub(crate) struct ResilienceState {
    pub registry: Option<Arc<CircuitBreakerRegistry>>,
    pub manager: Option<Arc<DegradationManager>>,
    pub cancel_token: Option<tokio_util::sync::CancellationToken>,
    pub notification_dedup_secs: u64,
}

/// Initialize the resilience subsystem (circuit breakers + degradation manager).
pub(crate) async fn init_resilience(
    config: &BlufioConfig,
    event_bus: &Arc<blufio_bus::EventBus>,
) -> ResilienceState {
    let notification_dedup_secs = config.resilience.notification_dedup_secs;

    if !config.resilience.enabled {
        info!("resilience subsystem disabled by configuration");
        return ResilienceState {
            registry: None,
            manager: None,
            cancel_token: None,
            notification_dedup_secs,
        };
    }

    // Build per-dependency CircuitBreakerConfig from ResilienceConfig.
    let mut cb_configs = std::collections::HashMap::new();
    let defaults = &config.resilience.defaults;

    // Providers: anthropic is always present; others by feature flags.
    let mut provider_names: Vec<String> = vec!["anthropic".to_string()];
    #[cfg(feature = "openai")]
    if config.providers.openai.api_key.is_some() {
        provider_names.push("openai".to_string());
    }
    #[cfg(feature = "ollama")]
    if config.providers.ollama.default_model.is_some() {
        provider_names.push("ollama".to_string());
    }
    #[cfg(feature = "openrouter")]
    if config.providers.openrouter.api_key.is_some() {
        provider_names.push("openrouter".to_string());
    }
    #[cfg(feature = "gemini")]
    if config.providers.gemini.api_key.is_some() {
        provider_names.push("gemini".to_string());
    }

    // Channels: add configured channels.
    let mut channel_names: Vec<String> = Vec::new();
    #[cfg(feature = "telegram")]
    if config.telegram.bot_token.is_some() {
        channel_names.push("telegram".to_string());
    }
    #[cfg(feature = "discord")]
    if config.discord.bot_token.is_some() {
        channel_names.push("discord".to_string());
    }
    #[cfg(feature = "slack")]
    if config.slack.bot_token.is_some() {
        channel_names.push("slack".to_string());
    }
    #[cfg(feature = "whatsapp")]
    if config.whatsapp.phone_number_id.is_some() {
        channel_names.push("whatsapp".to_string());
    }
    #[cfg(feature = "signal")]
    if config.signal.socket_path.is_some() || config.signal.host.is_some() {
        channel_names.push("signal".to_string());
    }
    #[cfg(feature = "irc")]
    if config.irc.server.is_some() {
        channel_names.push("irc".to_string());
    }
    #[cfg(feature = "matrix")]
    if config.matrix.homeserver_url.is_some() {
        channel_names.push("matrix".to_string());
    }
    #[cfg(feature = "email")]
    if config.email.imap_host.is_some() {
        channel_names.push("email".to_string());
    }
    #[cfg(feature = "imessage")]
    if config.imessage.bluebubbles_url.is_some() {
        channel_names.push("imessage".to_string());
    }
    #[cfg(feature = "sms")]
    if config.sms.account_sid.is_some() {
        channel_names.push("sms".to_string());
    }
    #[cfg(feature = "gateway")]
    if config.gateway.enabled {
        channel_names.push("gateway".to_string());
    }

    // Build config entries for all deps.
    for name in provider_names.iter().chain(channel_names.iter()) {
        let override_config = config.resilience.circuit_breakers.get(name);
        let cb_config = CircuitBreakerConfig {
            failure_threshold: override_config
                .and_then(|o| o.failure_threshold)
                .unwrap_or(defaults.failure_threshold),
            reset_timeout: Duration::from_secs(
                override_config
                    .and_then(|o| o.reset_timeout_secs)
                    .unwrap_or(defaults.reset_timeout_secs),
            ),
            half_open_probes: override_config
                .and_then(|o| o.half_open_probes)
                .unwrap_or(defaults.half_open_probes),
        };
        cb_configs.insert(name.clone(), cb_config);
    }

    let registry = Arc::new(CircuitBreakerRegistry::new(cb_configs));
    info!(
        deps = provider_names.len() + channel_names.len(),
        "circuit breaker registry initialized"
    );

    // Determine primary provider and primary channel.
    let primary_provider = provider_names
        .first()
        .cloned()
        .unwrap_or_else(|| "anthropic".to_string());
    let primary_channel = channel_names.first().cloned().unwrap_or_default();

    let escalation_config = EscalationConfig {
        primary_provider,
        primary_channel,
        hysteresis_secs: config.resilience.hysteresis_secs,
        drain_timeout_secs: config.resilience.drain_timeout_secs,
        provider_names: provider_names.clone(),
    };

    let cancel_token = tokio_util::sync::CancellationToken::new();
    let dm = Arc::new(DegradationManager::new(
        registry.clone(),
        escalation_config,
        cancel_token.clone(),
    ));

    // Subscribe DegradationManager to EventBus.
    let dm_rx = event_bus.subscribe_reliable(256).await;
    let dm_ref = dm.clone();
    let dm_bus = event_bus.clone();
    tokio::spawn(async move {
        dm_ref.run(dm_rx, dm_bus).await;
    });
    info!("degradation manager background task spawned");

    // Spawn sd-notify STATUS updater for degradation and circuit breaker events.
    {
        let status_rx = event_bus.subscribe_reliable(64).await;
        tokio::spawn(async move {
            let mut rx = status_rx;
            while let Some(event) = rx.recv().await {
                if let blufio_bus::events::BusEvent::Resilience(
                    blufio_bus::events::ResilienceEvent::DegradationLevelChanged {
                        to_level,
                        to_name,
                        ..
                    },
                ) = &event
                {
                    #[cfg(unix)]
                    {
                        let status = format!("Degradation: L{} {}", to_level, to_name);
                        blufio_agent::sdnotify::notify_status(&status);
                    }

                    #[cfg(feature = "prometheus")]
                    blufio_prometheus::recording::record_degradation_level(*to_level);
                }
                if let blufio_bus::events::BusEvent::Resilience(
                    blufio_bus::events::ResilienceEvent::CircuitBreakerStateChanged {
                        dependency,
                        from_state,
                        to_state,
                        ..
                    },
                ) = &event
                {
                    #[cfg(feature = "prometheus")]
                    {
                        let state_num = match to_state.as_str() {
                            "closed" => 0,
                            "half_open" => 1,
                            "open" => 2,
                            _ => 0,
                        };
                        blufio_prometheus::recording::record_circuit_breaker_state(
                            dependency, state_num,
                        );
                        blufio_prometheus::recording::record_circuit_breaker_transition(
                            dependency, from_state, to_state,
                        );
                    }
                }
            }
        });
        info!("sd-notify status updater spawned for degradation events");
    }

    // Validate fallback chain against known providers.
    let known_providers: Vec<&str> = provider_names.iter().map(|s| s.as_str()).collect();
    let validation_errors = config.resilience.validate_providers(&known_providers);
    for err in &validation_errors {
        warn!(error = err.as_str(), "resilience config validation warning");
    }

    ResilienceState {
        registry: Some(registry),
        manager: Some(dm),
        cancel_token: Some(cancel_token),
        notification_dedup_secs,
    }
}

/// Initialize tool registry with built-in tools.
pub(crate) async fn init_tool_registry() -> Arc<tokio::sync::RwLock<ToolRegistry>> {
    let mut tool_registry = ToolRegistry::new();
    blufio_skill::builtin::register_builtins(&mut tool_registry);
    info!(
        "tool registry initialized with {} built-in tools",
        tool_registry.len()
    );
    Arc::new(tokio::sync::RwLock::new(tool_registry))
}

/// Redact MCP server auth tokens and prepare injection classifier for MCP.
#[cfg(feature = "mcp-client")]
pub(crate) fn prepare_mcp_classifier(
    config: &BlufioConfig,
) -> Option<Arc<blufio_injection::classifier::InjectionClassifier>> {
    if config.injection_defense.enabled {
        Some(Arc::new(
            blufio_injection::classifier::InjectionClassifier::new(&config.injection_defense),
        ))
    } else {
        None
    }
}

/// Register SkillProvider and ArchiveConditionalProvider with the context engine.
pub(crate) async fn register_context_providers(
    config: &BlufioConfig,
    context_engine: &mut blufio_context::ContextEngine,
    tool_registry: &Arc<tokio::sync::RwLock<ToolRegistry>>,
    token_cache: &Arc<blufio_core::token_counter::TokenizerCache>,
) -> Result<(), BlufioError> {
    // Register SkillProvider with context engine for progressive tool discovery.
    let skill_provider =
        blufio_skill::SkillProvider::new(tool_registry.clone(), config.skill.max_skills_in_prompt);
    context_engine.add_conditional_provider(Box::new(skill_provider));

    // Register ArchiveConditionalProvider LAST (lowest priority).
    if config.context.archive_enabled {
        let archive_db =
            Arc::new(blufio_storage::Database::open(&config.storage.database_path).await?);
        let archive_provider = blufio_context::conditional::ArchiveConditionalProvider::new(
            archive_db,
            token_cache.clone(),
            config.context.conditional_zone_budget,
            config.context.archive_enabled,
            config.context.compaction_model.clone(),
        );
        context_engine.add_conditional_provider(Box::new(archive_provider));
        info!("archive conditional provider registered (lowest priority)");
    }

    Ok(())
}

/// Spawn memory background tasks (eviction, validation, file watcher).
pub(crate) async fn spawn_memory_tasks(
    config: &BlufioConfig,
    memory_store: &Option<Arc<MemoryStore>>,
    memory_embedder: &Option<Arc<OnnxEmbedder>>,
    event_bus: &Arc<blufio_bus::EventBus>,
    cancel: &tokio_util::sync::CancellationToken,
) {
    if config.memory.enabled {
        if let Some(store) = memory_store {
            // Spawn combined eviction + validation background task.
            let bg_store = store.clone();
            let bg_config = config.memory.clone();
            let bg_bus = Some(event_bus.clone());
            let bg_cancel = cancel.child_token();
            tokio::spawn(async move {
                blufio_memory::background::spawn_background_task(
                    bg_store, bg_config, bg_bus, bg_cancel,
                )
                .await;
            });
            info!(
                "memory background task started (eviction: {}s, validation: daily)",
                config.memory.eviction_sweep_interval_secs
            );

            // Start file watcher (if configured paths are non-empty).
            if !config.memory.file_watcher.paths.is_empty() {
                if let Some(embedder_arc) = memory_embedder {
                    // Initial scan of existing files.
                    match blufio_memory::watcher::initial_scan(
                        &config.memory.file_watcher,
                        store,
                        embedder_arc,
                    )
                    .await
                    {
                        Ok(count) => info!(files = count, "file watcher initial scan complete"),
                        Err(e) => warn!(error = %e, "file watcher initial scan failed"),
                    }
                    // Start watching for changes.
                    if let Err(e) = blufio_memory::watcher::start_file_watcher(
                        &config.memory.file_watcher,
                        store.clone(),
                        embedder_arc.clone(),
                        cancel.child_token(),
                    ) {
                        warn!(error = %e, "file watcher failed to start");
                    } else {
                        info!(paths = ?config.memory.file_watcher.paths, "file watcher started");
                    }
                }
            }
        }
    }
}

/// Initialize the cron scheduler.
pub(crate) async fn init_cron(
    config: &BlufioConfig,
    event_bus: &Arc<blufio_bus::EventBus>,
    cancel: &tokio_util::sync::CancellationToken,
) {
    if !config.cron.enabled {
        debug!("cron scheduler disabled by configuration");
        return;
    }

    let cron_db =
        match blufio_storage::open_connection(&config.storage.database_path).await {
            Ok(conn) => Arc::new(conn),
            Err(e) => {
                warn!(error = %e, "cron scheduler DB connection failed, skipping");
                return;
            }
        };
    let task_registry = Arc::new(blufio_cron::register_builtin_tasks(
        cron_db.clone(),
        config,
    ));
    match CronScheduler::new(
        cron_db,
        task_registry,
        Some(event_bus.clone()),
        config.cron.clone(),
    )
    .await
    {
        Ok(scheduler) => {
            let cron_cancel = cancel.child_token();
            tokio::spawn(async move {
                scheduler.run(cron_cancel).await;
            });
            info!(jobs = config.cron.jobs.len(), "cron scheduler started");
        }
        Err(e) => {
            warn!(error = %e, "cron scheduler initialization failed, continuing without cron");
        }
    }
}

/// Initialize the config hot reload system.
pub(crate) async fn init_hot_reload(
    config: &BlufioConfig,
    event_bus: &Arc<blufio_bus::EventBus>,
    cancel: &tokio_util::sync::CancellationToken,
) {
    if !config.hot_reload.enabled {
        debug!("config hot reload disabled by configuration");
        return;
    }

    // Determine config file path from XDG hierarchy (same precedence as loader).
    let config_path = {
        let local = PathBuf::from("blufio.toml");
        let xdg = dirs::config_dir().map(|d| d.join("blufio/blufio.toml"));
        let system = PathBuf::from("/etc/blufio/blufio.toml");

        if local.exists() {
            local
        } else if xdg.as_ref().is_some_and(|p| p.exists()) {
            xdg.unwrap()
        } else if system.exists() {
            system
        } else {
            local
        }
    };

    match crate::hot_reload::spawn_config_watcher(
        config.clone(),
        config_path,
        event_bus.clone(),
        cancel.child_token(),
    )
    .await
    {
        Ok(_config_swap) => {
            info!("config hot reload enabled");
        }
        Err(e) => {
            warn!(error = %e, "config hot reload initialization failed, continuing with static config");
        }
    }

    // TLS hot reload (if cert/key paths configured)
    if config.hot_reload.tls_cert_path.is_some() && config.hot_reload.tls_key_path.is_some() {
        let _tls_result =
            crate::hot_reload::spawn_tls_watcher(&config.hot_reload, cancel.child_token())
                .await;
        if _tls_result.is_some() {
            info!("TLS certificate hot reload enabled");
        }
    }

    // Skill/plugin hot reload (if configured)
    if config.hot_reload.watch_skills {
        let skills_dir = PathBuf::from(&config.skill.skills_dir);
        if skills_dir.exists() {
            let skill_bus = event_bus.clone();
            let skill_cancel = cancel.child_token();
            match crate::hot_reload::spawn_skill_watcher(skills_dir, skill_bus, skill_cancel)
                .await
            {
                Ok(()) => {
                    info!("skill hot reload watcher enabled");
                }
                Err(e) => {
                    warn!(error = %e, "skill hot reload watcher failed, continuing without skill watching");
                }
            }
        }
    }
}

/// Initialize the hook system.
pub(crate) async fn init_hooks(
    config: &BlufioConfig,
    event_bus: &Arc<blufio_bus::EventBus>,
    cancel: &tokio_util::sync::CancellationToken,
) -> Option<Arc<HookManager>> {
    if !config.hooks.enabled {
        debug!("hook system disabled by configuration");
        return None;
    }

    // Validate hook event names
    let warnings = blufio_hooks::manager::validate_hook_events(&config.hooks);
    for w in &warnings {
        warn!("{}", w);
    }

    let manager = Arc::new(HookManager::new(&config.hooks));
    let hook_rx = event_bus.subscribe_reliable(256).await;
    let hook_bus = event_bus.clone();
    let hook_cancel = cancel.child_token();

    // Execute pre_start hooks synchronously before main loop
    manager
        .execute_lifecycle_hooks("pre_start", event_bus)
        .await;

    // Spawn EventBus-driven hook run loop
    let run_manager = manager.clone();
    tokio::spawn(async move {
        run_manager.run(hook_rx, hook_bus, hook_cancel).await;
    });
    info!(
        hooks = config
            .hooks
            .definitions
            .iter()
            .filter(|d| d.enabled)
            .count(),
        "hook system started"
    );
    Some(manager)
}

/// Initialize the injection defense pipeline (INJC-06).
pub(crate) fn init_injection_pipeline(
    config: &BlufioConfig,
    event_bus: &Arc<blufio_bus::EventBus>,
) -> Option<Arc<tokio::sync::Mutex<blufio_injection::pipeline::InjectionPipeline>>> {
    if !config.injection_defense.enabled {
        debug!("injection defense disabled by configuration");
        return None;
    }

    let classifier =
        blufio_injection::classifier::InjectionClassifier::new(&config.injection_defense);
    let pipeline = blufio_injection::pipeline::InjectionPipeline::new(
        &config.injection_defense,
        classifier,
        Some(event_bus.clone()),
    );

    let active_layers: Vec<&str> = [
        Some("L1"),
        if config.injection_defense.hmac_boundaries.enabled {
            Some("L3")
        } else {
            None
        },
        if config.injection_defense.output_screening.enabled {
            Some("L4")
        } else {
            None
        },
        if config.injection_defense.hitl.enabled {
            Some("L5")
        } else {
            None
        },
    ]
    .iter()
    .filter_map(|l| *l)
    .collect();

    info!(
        layers = ?active_layers,
        dry_run = config.injection_defense.dry_run,
        "injection defense pipeline initialized"
    );

    Some(Arc::new(tokio::sync::Mutex::new(pipeline)))
}

/// Background task that monitors memory usage via jemalloc stats.
#[cfg(not(target_env = "msvc"))]
pub(crate) async fn memory_monitor(
    config: &blufio_config::model::DaemonConfig,
    cancel: tokio_util::sync::CancellationToken,
) {
    let warn_bytes = config.memory_warn_mb as usize * 1024 * 1024;
    let mut interval = tokio::time::interval(Duration::from_secs(5));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let _ = tikv_jemalloc_ctl::epoch::advance();
                let allocated = tikv_jemalloc_ctl::stats::allocated::read().unwrap_or(0);
                let resident = tikv_jemalloc_ctl::stats::resident::read().unwrap_or(0);

                let rss = read_rss_bytes().unwrap_or(0);

                #[cfg(feature = "prometheus")]
                {
                    blufio_prometheus::set_memory_heap(allocated as f64);
                    blufio_prometheus::set_memory_resident(resident as f64);
                    blufio_prometheus::set_memory_rss(rss as f64);
                }

                if allocated > warn_bytes {
                    warn!(
                        allocated_mb = allocated / (1024 * 1024),
                        threshold_mb = config.memory_warn_mb,
                        "memory pressure: heap above warning threshold"
                    );
                    #[cfg(feature = "prometheus")]
                    blufio_prometheus::set_memory_pressure(1.0);

                    let _ = tikv_jemalloc_ctl::epoch::advance();
                } else {
                    #[cfg(feature = "prometheus")]
                    blufio_prometheus::set_memory_pressure(0.0);
                }
            }
            _ = cancel.cancelled() => {
                info!("memory monitor shutting down");
                break;
            }
        }
    }
}

/// Stub memory monitor for MSVC (no jemalloc).
#[cfg(target_env = "msvc")]
pub(crate) async fn memory_monitor(
    _config: &blufio_config::model::DaemonConfig,
    cancel: tokio_util::sync::CancellationToken,
) {
    cancel.cancelled().await;
}

/// Read the process RSS in bytes from /proc/self/statm (Linux only).
#[cfg(not(target_env = "msvc"))]
fn read_rss_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
        let rss_pages = statm.split_whitespace().nth(1)?.parse::<u64>().ok()?;
        Some(rss_pages * 4096)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}
