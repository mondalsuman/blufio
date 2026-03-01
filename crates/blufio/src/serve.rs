// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio serve` command implementation.
//!
//! Starts the full Blufio agent with configured channel adapters, Anthropic
//! provider, SQLite storage, three-zone context engine, and cost tracking.
//! Uses the PluginRegistry to discover and initialize compiled-in adapters,
//! and the ChannelMultiplexer to aggregate multiple channels.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use blufio_agent::{AgentLoop, ChannelMultiplexer, DelegationRouter, DelegationTool, HeartbeatRunner};
use blufio_agent::shutdown;
use blufio_config::model::BlufioConfig;
use blufio_context::ContextEngine;
use blufio_cost::{BudgetTracker, CostLedger};
use blufio_core::error::BlufioError;
use blufio_core::{ChannelAdapter, StorageAdapter};
use blufio_plugin::{PluginRegistry, PluginStatus, builtin_catalog};
use blufio_router::ModelRouter;
use blufio_skill::{SkillProvider, ToolRegistry};
use tracing::{debug, error, info, warn};

#[cfg(feature = "anthropic")]
use blufio_anthropic::AnthropicProvider;

#[cfg(feature = "sqlite")]
use blufio_storage::SqliteStorage;

#[cfg(feature = "telegram")]
use blufio_telegram::TelegramChannel;

#[cfg(feature = "gateway")]
use blufio_gateway::{GatewayChannel, GatewayChannelConfig};

use blufio_memory::{MemoryExtractor, MemoryProvider, MemoryStore, HybridRetriever, OnnxEmbedder, ModelManager};

/// Initializes the plugin registry with the built-in catalog.
///
/// Each adapter in the catalog is registered with a status determined by
/// the user's plugin configuration overrides. By default, all compiled-in
/// adapters are enabled.
fn initialize_plugin_registry(config: &BlufioConfig) -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    let catalog = builtin_catalog();

    for manifest in catalog {
        let name = manifest.name.clone();
        // Determine status from config overrides.
        let status = if let Some(&enabled) = config.plugin.plugins.get(&name) {
            if enabled {
                PluginStatus::Enabled
            } else {
                PluginStatus::Disabled
            }
        } else {
            PluginStatus::Enabled // Default: all compiled-in adapters enabled
        };
        registry.register_with_status(manifest, None, status);
    }

    info!(
        count = registry.len(),
        "plugin registry initialized"
    );
    registry
}

/// Runs the `blufio serve` command.
///
/// Initializes all adapters via the PluginRegistry pattern, creates a
/// ChannelMultiplexer for multi-channel support, and enters the main
/// agent loop. Supports graceful shutdown via signal handlers.
pub async fn run_serve(config: BlufioConfig) -> Result<(), BlufioError> {
    // Initialize tracing subscriber.
    init_tracing(&config.agent.log_level);

    info!("starting blufio serve");

    // Initialize plugin registry.
    let _registry = initialize_plugin_registry(&config);

    // SEC-03: Vault startup check -- unlock vault if it exists so secrets
    // are available for provider initialization. Silent no-op when no vault.
    {
        let vault_conn = tokio_rusqlite::Connection::open(&config.storage.database_path)
            .await
            .map_err(|e| BlufioError::Storage { source: Box::new(e) })?;
        match blufio_vault::vault_startup_check(vault_conn, &config.vault).await {
            Ok(Some(_vault)) => {
                info!("vault unlocked -- secrets available");
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

    // Initialize storage.
    #[cfg(feature = "sqlite")]
    let storage = {
        let storage = SqliteStorage::new(config.storage.clone());
        storage.initialize().await?;
        Arc::new(storage)
    };

    #[cfg(not(feature = "sqlite"))]
    compile_error!("blufio requires the 'sqlite' feature for storage");

    // Mark stale sessions as interrupted (crash recovery).
    mark_stale_sessions(storage.as_ref()).await?;

    // Initialize cost ledger (opens its own connection to the same DB).
    let cost_ledger = Arc::new(
        CostLedger::open(&config.storage.database_path).await?
    );

    // Initialize budget tracker from existing ledger data (restart recovery).
    let budget_tracker = Arc::new(tokio::sync::Mutex::new(
        BudgetTracker::from_ledger(&config.cost, &cost_ledger).await?
    ));

    // Initialize context engine.
    let mut context_engine =
        ContextEngine::new(&config.agent, &config.context).await?;

    // Initialize memory system (if enabled).
    #[cfg(feature = "onnx")]
    let (memory_provider, memory_extractor) = if config.memory.enabled {
        match initialize_memory(&config, &mut context_engine).await {
            Ok((mp, me)) => (Some(mp), Some(me)),
            Err(e) => {
                warn!(error = %e, "memory system initialization failed, continuing without memory");
                (None, None)
            }
        }
    } else {
        info!("memory system disabled by configuration");
        (None, None)
    };

    #[cfg(not(feature = "onnx"))]
    let (memory_provider, memory_extractor): (Option<MemoryProvider>, Option<Arc<MemoryExtractor>>) = {
        info!("memory system disabled (onnx feature not enabled)");
        (None, None)
    };

    // Initialize tool registry with built-in tools.
    let mut tool_registry = ToolRegistry::new();
    blufio_skill::builtin::register_builtins(&mut tool_registry);
    info!("tool registry initialized with {} built-in tools", tool_registry.len());
    let tool_registry = Arc::new(tokio::sync::RwLock::new(tool_registry));

    // Register SkillProvider with context engine for progressive tool discovery.
    let skill_provider = SkillProvider::new(
        tool_registry.clone(),
        config.skill.max_skills_in_prompt,
    );
    context_engine.add_conditional_provider(Box::new(skill_provider));

    let context_engine = Arc::new(context_engine);

    // Initialize Anthropic provider.
    #[cfg(feature = "anthropic")]
    let provider = {
        let p = AnthropicProvider::new(&config).await.map_err(|e| {
            error!(error = %e, "failed to initialize Anthropic provider");
            eprintln!(
                "error: Anthropic API key required. Set via: config, ANTHROPIC_API_KEY env var, or `blufio config set-secret anthropic.api_key`"
            );
            e
        })?;
        Arc::new(p)
    };

    #[cfg(not(feature = "anthropic"))]
    compile_error!("blufio requires the 'anthropic' feature for the LLM provider");

    // Initialize Prometheus metrics (if enabled and compiled in).
    #[cfg(feature = "prometheus")]
    let _prometheus_adapter = if config.prometheus.enabled {
        match blufio_prometheus::PrometheusAdapter::new() {
            Ok(adapter) => {
                info!("prometheus metrics enabled");
                Some(adapter)
            }
            Err(e) => {
                warn!(error = %e, "prometheus initialization failed, continuing without metrics");
                None
            }
        }
    } else {
        debug!("prometheus metrics disabled by configuration");
        None
    };

    // Build channel multiplexer.
    let mut mux = ChannelMultiplexer::new();

    // Add Telegram channel (if enabled and configured).
    #[cfg(feature = "telegram")]
    {
        if config.telegram.bot_token.is_some() {
            let telegram = TelegramChannel::new(config.telegram.clone()).map_err(|e| {
                error!(error = %e, "failed to initialize Telegram channel");
                eprintln!(
                    "error: Telegram bot token required. Set via: config or `blufio config set-secret telegram.bot_token`"
                );
                e
            })?;
            mux.add_channel("telegram".to_string(), Box::new(telegram));
            info!("telegram channel added to multiplexer");
        } else {
            info!("telegram channel skipped (no bot_token configured)");
        }
    }

    // Build Prometheus render function for gateway /metrics endpoint.
    #[cfg(feature = "prometheus")]
    let prometheus_render: Option<Arc<dyn Fn() -> String + Send + Sync>> =
        _prometheus_adapter.as_ref().map(|adapter| {
            let handle = adapter.handle().clone();
            Arc::new(move || handle.render()) as Arc<dyn Fn() -> String + Send + Sync>
        });
    #[cfg(not(feature = "prometheus"))]
    let prometheus_render: Option<Arc<dyn Fn() -> String + Send + Sync>> = None;

    // Add Gateway channel (if enabled and compiled in).
    #[cfg(feature = "gateway")]
    {
        if config.gateway.enabled {
            // SEC-02: Load device keypair public key for gateway auth.
            #[cfg(feature = "keypair")]
            let keypair_public_key = {
                let kp = blufio_auth_keypair::DeviceKeypair::generate();
                info!(
                    public_key = kp.public_hex().as_str(),
                    "device keypair loaded for gateway auth"
                );
                Some(kp.verifying_key())
            };
            #[cfg(not(feature = "keypair"))]
            let keypair_public_key = None;

            // Fail-closed: refuse to start gateway with no auth configured.
            if config.gateway.bearer_token.is_none() && keypair_public_key.is_none() {
                return Err(BlufioError::Security(
                    "SEC-02: gateway enabled but no authentication configured. \
                     Set gateway.bearer_token or enable keypair feature."
                        .to_string(),
                ));
            }

            let gateway_config = GatewayChannelConfig {
                enabled: config.gateway.enabled,
                host: config.gateway.host.clone(),
                port: config.gateway.port,
                bearer_token: config.gateway.bearer_token.clone(),
                keypair_public_key,
                prometheus_render: prometheus_render.clone(),
            };
            let gateway = GatewayChannel::new(gateway_config);
            mux.add_channel("gateway".to_string(), Box::new(gateway));
            info!(
                host = config.gateway.host.as_str(),
                port = config.gateway.port,
                "gateway channel added to multiplexer"
            );
        } else {
            debug!("gateway channel disabled by configuration");
        }
    }

    // SEC-02 guard for non-gateway builds.
    #[cfg(not(feature = "keypair"))]
    {
        if config.gateway.enabled {
            return Err(BlufioError::Security(
                "SEC-02: device keypair authentication is required but keypair feature is disabled"
                    .to_string(),
            ));
        }
    }

    // Connect all channels via multiplexer.
    mux.connect().await?;
    info!(
        channels = mux.channel_count(),
        "channel multiplexer connected"
    );

    // Initialize model router.
    let router = Arc::new(ModelRouter::new(config.routing.clone()));
    if config.routing.enabled {
        if let Some(ref forced) = config.routing.force_model {
            info!(model = forced.as_str(), "model routing enabled with forced model");
        } else {
            info!(
                simple = config.routing.simple_model.as_str(),
                standard = config.routing.standard_model.as_str(),
                complex = config.routing.complex_model.as_str(),
                "model routing enabled"
            );
        }
    } else {
        info!(
            model = config.anthropic.default_model.as_str(),
            "model routing disabled, using default model"
        );
    }

    // Wire multi-agent delegation (if enabled and agents configured).
    if config.delegation.enabled && !config.agents.is_empty() {
        let delegation_router = Arc::new(DelegationRouter::new(
            &config.agents,
            provider.clone(),
            storage.clone() as Arc<dyn StorageAdapter + Send + Sync>,
            cost_ledger.clone(),
            budget_tracker.clone(),
            router.clone(),
            config.delegation.timeout_secs,
        ));
        let delegation_tool = DelegationTool::new(delegation_router);
        {
            let mut registry = tool_registry.write().await;
            registry.register(Arc::new(delegation_tool));
        }
        info!(
            agents = config.agents.len(),
            timeout_secs = config.delegation.timeout_secs,
            "multi-agent delegation enabled"
        );
    } else {
        debug!("multi-agent delegation disabled");
    }

    // Initialize heartbeat runner (if enabled).
    let heartbeat_runner = if config.heartbeat.enabled {
        let runner = Arc::new(HeartbeatRunner::new(
            config.heartbeat.clone(),
            provider.clone(),
            storage.clone(),
            cost_ledger.clone(),
        ));
        info!(
            interval_secs = config.heartbeat.interval_secs,
            delivery = config.heartbeat.delivery.as_str(),
            monthly_budget = config.heartbeat.monthly_budget_usd,
            "heartbeat system enabled"
        );
        Some(runner)
    } else {
        info!("heartbeat system disabled");
        None
    };

    // Install signal handler.
    let cancel = shutdown::install_signal_handler();

    // Spawn memory monitor background task.
    {
        let daemon_config = config.daemon.clone();
        let mem_cancel = cancel.clone();
        tokio::spawn(async move {
            memory_monitor(&daemon_config, mem_cancel).await;
        });
        info!(
            warn_mb = config.daemon.memory_warn_mb,
            limit_mb = config.daemon.memory_limit_mb,
            "memory monitor started"
        );
    }

    // Spawn heartbeat background task if enabled.
    if let Some(ref runner) = heartbeat_runner {
        let hb_runner = runner.clone();
        let hb_cancel = cancel.clone();
        let interval_secs = config.heartbeat.interval_secs;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                std::time::Duration::from_secs(interval_secs),
            );
            // Skip the first immediate tick.
            interval.tick().await;

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        match hb_runner.execute().await {
                            Ok(Some(result)) if result.has_content => {
                                info!(
                                    content_len = result.content.len(),
                                    "heartbeat generated actionable content"
                                );
                            }
                            Ok(Some(_)) => {
                                debug!("heartbeat executed but no actionable content");
                            }
                            Ok(None) => {
                                debug!("heartbeat skipped (unchanged state)");
                            }
                            Err(e) => {
                                warn!(error = %e, "heartbeat execution failed (non-fatal)");
                            }
                        }
                    }
                    _ = hb_cancel.cancelled() => {
                        info!("heartbeat task shutting down");
                        break;
                    }
                }
            }
        });
    }

    // Create and run agent loop with channel multiplexer.
    let mut agent_loop = AgentLoop::new(
        Box::new(mux),
        provider,
        storage,
        context_engine,
        cost_ledger,
        budget_tracker,
        memory_provider,
        memory_extractor,
        router,
        heartbeat_runner,
        tool_registry,
        config,
    )
    .await?;

    agent_loop.run(cancel).await?;

    info!("blufio serve shutdown complete");
    Ok(())
}

/// Marks any sessions that were left in "active" state as "interrupted".
///
/// This handles the case where the process was previously killed without
/// graceful shutdown, leaving sessions in an active state.
async fn mark_stale_sessions(
    storage: &dyn StorageAdapter,
) -> Result<(), BlufioError> {
    let active_sessions = storage.list_sessions(Some("active")).await?;
    if !active_sessions.is_empty() {
        info!(
            count = active_sessions.len(),
            "marking stale sessions as interrupted"
        );
        for session in &active_sessions {
            storage
                .update_session_state(&session.id, "interrupted")
                .await?;
        }
    }
    Ok(())
}

/// Initializes the memory system: downloads model, creates embedder, store,
/// retriever, provider, and extractor. Registers the provider with ContextEngine.
///
/// Returns (MemoryProvider, MemoryExtractor) on success.
#[allow(dead_code)]
async fn initialize_memory(
    config: &BlufioConfig,
    context_engine: &mut ContextEngine,
) -> Result<(MemoryProvider, Arc<MemoryExtractor>), BlufioError> {
    // Determine data directory (parent of the database path).
    let db_path = PathBuf::from(&config.storage.database_path);
    let data_dir = db_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    // Download model on first run.
    let model_manager = ModelManager::new(data_dir);
    info!("ensuring embedding model is available...");
    let model_path = model_manager.ensure_model().await?;
    info!(path = %model_path.display(), "embedding model ready");

    // Create ONNX embedder.
    let embedder = Arc::new(OnnxEmbedder::new(&model_path)?);

    // Create memory store (opens its own connection to the same DB).
    let memory_conn = tokio_rusqlite::Connection::open(&config.storage.database_path)
        .await
        .map_err(|e| BlufioError::Storage {
            source: Box::new(e),
        })?;
    let memory_store = Arc::new(MemoryStore::new(memory_conn));

    // Create hybrid retriever.
    let retriever = Arc::new(HybridRetriever::new(
        memory_store.clone(),
        embedder.clone(),
        config.memory.clone(),
    ));

    // Create memory provider and register with context engine.
    let memory_provider = MemoryProvider::new(retriever);
    context_engine.add_conditional_provider(Box::new(memory_provider.clone()));

    // Create memory extractor.
    let extractor = Arc::new(MemoryExtractor::new(
        memory_store,
        embedder,
        config.memory.extraction_model.clone(),
    ));

    info!("memory system initialized");
    Ok((memory_provider, extractor))
}

/// Background task that monitors memory usage via jemalloc stats and
/// /proc/self/statm (Linux). Exports Prometheus gauges every 5 seconds.
///
/// When heap allocation exceeds the warning threshold, triggers cache
/// shedding by purging jemalloc dirty pages and logging a warning.
#[cfg(not(target_env = "msvc"))]
async fn memory_monitor(
    config: &blufio_config::model::DaemonConfig,
    cancel: tokio_util::sync::CancellationToken,
) {
    let warn_bytes = config.memory_warn_mb as usize * 1024 * 1024;
    let mut interval = tokio::time::interval(Duration::from_secs(5));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Read jemalloc stats (requires epoch advance for fresh data).
                let _ = tikv_jemalloc_ctl::epoch::advance();
                let allocated = tikv_jemalloc_ctl::stats::allocated::read().unwrap_or(0);
                let resident = tikv_jemalloc_ctl::stats::resident::read().unwrap_or(0);

                // Read RSS (Linux-only, graceful fallback).
                let rss = read_rss_bytes().unwrap_or(0);

                // Export to Prometheus.
                #[cfg(feature = "prometheus")]
                {
                    blufio_prometheus::set_memory_heap(allocated as f64);
                    blufio_prometheus::set_memory_resident(resident as f64);
                    blufio_prometheus::set_memory_rss(rss as f64);
                }

                // Check warning threshold.
                if allocated > warn_bytes {
                    warn!(
                        allocated_mb = allocated / (1024 * 1024),
                        threshold_mb = config.memory_warn_mb,
                        "memory pressure: heap above warning threshold"
                    );
                    #[cfg(feature = "prometheus")]
                    blufio_prometheus::set_memory_pressure(1.0);

                    // Shed: attempt to purge jemalloc arenas to reclaim pages.
                    // This is best-effort — purge may not reduce allocated if
                    // all memory is actively used.
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
async fn memory_monitor(
    _config: &blufio_config::model::DaemonConfig,
    cancel: tokio_util::sync::CancellationToken,
) {
    cancel.cancelled().await;
}

/// Read the process RSS in bytes from /proc/self/statm (Linux only).
///
/// Returns None on non-Linux platforms or if the file cannot be read.
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

/// Initializes the tracing subscriber with the given log level.
fn init_tracing(log_level: &str) {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("blufio={log_level},warn")));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_names(false)
        .init();
}
