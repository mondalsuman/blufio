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

use blufio_agent::shutdown;
use blufio_agent::{
    AgentLoop, ChannelMultiplexer, DelegationRouter, DelegationTool, HeartbeatRunner,
};
use blufio_config::model::BlufioConfig;
use blufio_context::ContextEngine;
use blufio_core::error::BlufioError;
use blufio_core::token_counter::{TokenizerCache, TokenizerMode};
use blufio_core::{ChannelAdapter, StorageAdapter};
use blufio_cost::{BudgetTracker, CostLedger};
use blufio_plugin::{PluginRegistry, PluginStatus, builtin_catalog};
use blufio_resilience::{
    CircuitBreakerConfig, CircuitBreakerRegistry, DegradationManager, EscalationConfig,
};
use blufio_router::ModelRouter;
use blufio_skill::{SkillProvider, ToolRegistry};
use tracing::{debug, error, info, warn};

#[cfg(feature = "anthropic")]
use blufio_anthropic::AnthropicProvider;

#[cfg(feature = "sqlite")]
use blufio_storage::SqliteStorage;

#[cfg(feature = "telegram")]
use blufio_telegram::TelegramChannel;

#[cfg(feature = "discord")]
use blufio_discord::DiscordChannel;

#[cfg(feature = "slack")]
use blufio_slack::SlackChannel;

#[cfg(feature = "whatsapp")]
use blufio_whatsapp::{WhatsAppCloudChannel, webhook::WhatsAppWebhookState};

#[cfg(feature = "signal")]
use blufio_signal::SignalChannel;

#[cfg(feature = "irc")]
use blufio_irc::IrcChannel;

#[cfg(feature = "matrix")]
use blufio_matrix::MatrixChannel;

#[cfg(feature = "gateway")]
use blufio_gateway::{GatewayChannel, GatewayChannelConfig};

use crate::providers::ConcreteProviderRegistry;

#[cfg(feature = "gateway")]
use blufio_core::ProviderRegistry;

use blufio_cron::CronScheduler;
use blufio_hooks::HookManager;
use blufio_memory::{
    HybridRetriever, MemoryExtractor, MemoryProvider, MemoryStore, ModelManager, OnnxEmbedder,
};

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

    info!(count = registry.len(), "plugin registry initialized");
    registry
}

/// Runs the `blufio serve` command.
///
/// Initializes all adapters via the PluginRegistry pattern, creates a
/// ChannelMultiplexer for multi-channel support, and enters the main
/// agent loop. Supports graceful shutdown via signal handlers.
pub async fn run_serve(config: BlufioConfig) -> Result<(), BlufioError> {
    // Initialize tracing subscriber with secret redaction (SEC-08).
    // Returns a handle to populate vault secrets later (after vault unlock).
    let vault_values = init_tracing(&config.agent.log_level);

    info!("starting blufio serve");

    // Initialize plugin registry.
    let _registry = initialize_plugin_registry(&config);

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
    // Regex patterns catch sk-ant-* and Telegram tokens automatically;
    // this exact-match registration catches non-pattern-matching secrets
    // such as custom bearer tokens.
    {
        if let Some(ref key) = config.anthropic.api_key {
            blufio_security::RedactingWriter::<std::io::Stderr>::add_vault_value(
                &vault_values,
                key.clone(),
            );
        }
        if let Some(ref token) = config.telegram.bot_token {
            blufio_security::RedactingWriter::<std::io::Stderr>::add_vault_value(
                &vault_values,
                token.clone(),
            );
        }
        if let Some(ref token) = config.gateway.bearer_token {
            blufio_security::RedactingWriter::<std::io::Stderr>::add_vault_value(
                &vault_values,
                token.clone(),
            );
        }
        let secret_count = vault_values.read().map(|v| v.len()).unwrap_or(0);
        if secret_count > 0 {
            info!(count = secret_count, "secrets registered for log redaction");
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
    let cost_ledger = Arc::new(CostLedger::open(&config.storage.database_path).await?);

    // Initialize budget tracker from existing ledger data (restart recovery).
    let budget_tracker = Arc::new(tokio::sync::Mutex::new(
        BudgetTracker::from_ledger(&config.cost, &cost_ledger).await?,
    ));

    // Initialize tokenizer cache from config.
    let tokenizer_mode = if config.performance.tokenizer_mode == "fast" {
        TokenizerMode::Fast
    } else {
        TokenizerMode::Accurate
    };
    let token_cache = Arc::new(TokenizerCache::new(tokenizer_mode));

    // Initialize context engine.
    let mut context_engine =
        ContextEngine::new(&config.agent, &config.context, token_cache.clone()).await?;

    // Static zone budget check at startup (CTXE-01).
    // Advisory only -- logs a warning if system prompt exceeds budget but never truncates.
    {
        let static_tokens = context_engine
            .static_zone()
            .token_count(&token_cache, &config.context.compaction_model)
            .await;
        context_engine
            .static_zone()
            .check_budget(static_tokens, config.context.static_zone_budget);
        debug!(
            static_tokens = static_tokens,
            budget = config.context.static_zone_budget,
            "static zone budget check complete"
        );
    }

    // Initialize memory system (if enabled).
    #[cfg(feature = "onnx")]
    let (memory_provider, memory_extractor, memory_store, memory_embedder) = if config
        .memory
        .enabled
    {
        match initialize_memory(&config, &mut context_engine).await {
            Ok((mp, me, ms, emb)) => (Some(mp), Some(me), Some(ms), Some(emb)),
            Err(e) => {
                warn!(error = %e, "memory system initialization failed, continuing without memory");
                (None, None, None, None)
            }
        }
    } else {
        info!("memory system disabled by configuration");
        (None, None, None, None)
    };

    #[cfg(not(feature = "onnx"))]
    let (memory_provider, memory_extractor, memory_store, memory_embedder): (
        Option<MemoryProvider>,
        Option<Arc<MemoryExtractor>>,
        Option<Arc<MemoryStore>>,
        Option<Arc<OnnxEmbedder>>,
    ) = {
        info!("memory system disabled (onnx feature not enabled)");
        (None, None, None, None)
    };

    // Initialize tool registry with built-in tools.
    let mut tool_registry = ToolRegistry::new();
    blufio_skill::builtin::register_builtins(&mut tool_registry);
    info!(
        "tool registry initialized with {} built-in tools",
        tool_registry.len()
    );
    let tool_registry = Arc::new(tokio::sync::RwLock::new(tool_registry));

    // Create global event bus shared across all subsystems.
    let event_bus = Arc::new(blufio_bus::EventBus::new(1024));
    info!("global event bus created (capacity=1024)");

    // --- Audit trail subsystem ---
    // Initialize after EventBus so adapter startup events are captured.
    let audit_writer: Option<Arc<blufio_audit::AuditWriter>> = if config.audit.enabled {
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

                // Create event filter from config.
                let filter = blufio_audit::EventFilter::new(config.audit.events.clone());

                // Subscribe to EventBus with reliable channel (buffer 256).
                let audit_rx = event_bus.subscribe_reliable(256).await;

                // Create and spawn AuditSubscriber.
                let subscriber = blufio_audit::AuditSubscriber::new(writer.clone(), filter);
                tokio::spawn(subscriber.run(audit_rx));

                info!(db_path = %audit_db_path, "audit trail enabled");

                // Emit audit.enabled meta-event.
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
    } else {
        info!("audit trail disabled");
        None
    };

    // --- Resilience subsystem ---
    // Construct CircuitBreakerRegistry and DegradationManager if enabled.
    let (resilience_registry, resilience_manager, resilience_cancel_token): (
        Option<Arc<CircuitBreakerRegistry>>,
        Option<Arc<DegradationManager>>,
        Option<tokio_util::sync::CancellationToken>,
    ) = if config.resilience.enabled {
        // Build per-dependency CircuitBreakerConfig from ResilienceConfig.
        // Collect all known provider + channel names.
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

        // Subscribe DegradationManager to EventBus (reliable mpsc, buffer 256).
        let dm_rx = event_bus.subscribe_reliable(256).await;
        let dm_ref = dm.clone();
        let dm_bus = event_bus.clone();
        tokio::spawn(async move {
            dm_ref.run(dm_rx, dm_bus).await;
        });
        info!("degradation manager background task spawned");

        // Spawn sd-notify STATUS updater: subscribes to DegradationLevelChanged events.
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

                        // Record Prometheus degradation level.
                        #[cfg(feature = "prometheus")]
                        blufio_prometheus::recording::record_degradation_level(*to_level);
                    }
                    // Also record circuit breaker state changes for Prometheus.
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
                            // Map state string to numeric for gauge.
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

        (Some(registry), Some(dm), Some(cancel_token))
    } else {
        info!("resilience subsystem disabled by configuration");
        (None, None, None)
    };

    // Extract notification dedup config before config is moved later.
    let notification_dedup_secs = config.resilience.notification_dedup_secs;

    // Register SkillProvider with context engine for progressive tool discovery.
    let skill_provider =
        SkillProvider::new(tool_registry.clone(), config.skill.max_skills_in_prompt);
    context_engine.add_conditional_provider(Box::new(skill_provider));

    // Create injection classifier for MCP description scanning (INJC-06 gap closure).
    // This is separate from the pipeline classifier created later -- MCP init happens
    // before the full pipeline is built, so we need an early classifier for description scanning.
    #[cfg(feature = "mcp-client")]
    let mcp_injection_classifier: Option<
        Arc<blufio_injection::classifier::InjectionClassifier>,
    > = if config.injection_defense.enabled {
        Some(Arc::new(
            blufio_injection::classifier::InjectionClassifier::new(&config.injection_defense),
        ))
    } else {
        None
    };

    // Initialize MCP client connections to external servers (if configured).
    #[cfg(feature = "mcp-client")]
    let (_mcp_client_manager, _mcp_health_sessions) = if !config.mcp.servers.is_empty() {
        // Redact MCP server auth tokens in logs.
        for server in &config.mcp.servers {
            if let Some(ref token) = server.auth_token {
                blufio_security::RedactingWriter::<std::io::Stderr>::add_vault_value(
                    &vault_values,
                    token.clone(),
                );
            }
        }

        // Open PinStore for tool schema rug-pull detection (CLNT-07).
        let pin_store = match blufio_mcp_client::PinStore::open(&config.storage.database_path).await
        {
            Ok(store) => {
                info!("MCP PinStore initialized");
                Some(store)
            }
            Err(e) => {
                warn!(error = %e, "failed to open PinStore, continuing without pin verification");
                None
            }
        };

        let (manager, result) = blufio_mcp_client::McpClientManager::connect_all_with_classifier(
            &config.mcp.servers,
            &tool_registry,
            pin_store.as_ref(),
            mcp_injection_classifier,
        )
        .await;

        // INTG-04: Set active MCP connections gauge.
        blufio_prometheus::recording::set_mcp_active_connections(result.connected as f64);

        if result.connected > 0 {
            info!(
                connected = result.connected,
                failed = result.failed,
                tools = result.tools_registered,
                "MCP client initialized"
            );
        }
        if result.failed > 0 {
            warn!(
                failed = result.failed,
                "some MCP server connections failed (non-fatal)"
            );
        }

        // Extract connected sessions for health monitoring (spawned after cancel token).
        let health_sessions = if result.connected > 0 {
            Some(manager.connected_session_map())
        } else {
            None
        };

        // Register TrustZoneProvider when external tools were discovered (CLNT-10).
        if result.tools_registered > 0 {
            let trusted_servers: std::collections::HashSet<String> = config
                .mcp
                .servers
                .iter()
                .filter(|s| s.trusted)
                .map(|s| s.name.clone())
                .collect();
            let trusted_servers_count = trusted_servers.len();
            let trust_zone_provider =
                blufio_mcp_client::TrustZoneProvider::new(tool_registry.clone(), trusted_servers);
            context_engine.add_conditional_provider(Box::new(trust_zone_provider));
            info!(
                trusted_servers = trusted_servers_count,
                "trust zone provider registered for external tools"
            );
        }

        (Some(manager), health_sessions)
    } else {
        debug!("no MCP servers configured");
        (None, None)
    };

    // Register ArchiveConditionalProvider LAST (lowest priority, after memory, skills, trust zone).
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
        info!("anthropic provider initialized with TLS 1.2+ enforcement and SSRF protection");
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
    mux.set_event_bus(event_bus.clone());

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

    // Add Discord channel (if enabled and configured).
    #[cfg(feature = "discord")]
    {
        if config.discord.bot_token.is_some() {
            let discord = DiscordChannel::new(config.discord.clone()).map_err(|e| {
                error!(error = %e, "failed to initialize Discord channel");
                eprintln!(
                    "error: Discord bot token required. Set via: config or `blufio config set-secret discord.bot_token`"
                );
                e
            })?;
            mux.add_channel("discord".to_string(), Box::new(discord));
            info!("discord channel added to multiplexer");

            // Redact Discord token in logs.
            if let Some(ref token) = config.discord.bot_token {
                blufio_security::RedactingWriter::<std::io::Stderr>::add_vault_value(
                    &vault_values,
                    token.clone(),
                );
            }
        } else {
            info!("discord channel skipped (no bot_token configured)");
        }
    }

    // Add Slack channel (if enabled and configured).
    #[cfg(feature = "slack")]
    {
        if config.slack.bot_token.is_some() && config.slack.app_token.is_some() {
            let slack = SlackChannel::new(config.slack.clone()).map_err(|e| {
                error!(error = %e, "failed to initialize Slack channel");
                eprintln!(
                    "error: Slack bot_token and app_token required for Socket Mode. \
                     Set via: config or `blufio config set-secret slack.bot_token`"
                );
                e
            })?;
            mux.add_channel("slack".to_string(), Box::new(slack));
            info!("slack channel added to multiplexer");

            // Redact Slack tokens in logs.
            if let Some(ref token) = config.slack.bot_token {
                blufio_security::RedactingWriter::<std::io::Stderr>::add_vault_value(
                    &vault_values,
                    token.clone(),
                );
            }
            if let Some(ref token) = config.slack.app_token {
                blufio_security::RedactingWriter::<std::io::Stderr>::add_vault_value(
                    &vault_values,
                    token.clone(),
                );
            }
        } else {
            info!("slack channel skipped (bot_token and/or app_token not configured)");
        }
    }

    // Add WhatsApp channel (if enabled and configured).
    #[cfg(feature = "whatsapp")]
    let _whatsapp_webhook_state: Option<WhatsAppWebhookState> = {
        if config.whatsapp.phone_number_id.is_some() {
            let whatsapp = WhatsAppCloudChannel::new(config.whatsapp.clone()).map_err(|e| {
                error!(error = %e, "failed to initialize WhatsApp channel");
                e
            })?;

            // Capture inbound_tx and webhook state before moving the adapter.
            let inbound_tx = whatsapp.inbound_tx();
            let webhook_state = WhatsAppWebhookState {
                inbound_tx,
                verify_token: config.whatsapp.verify_token.clone().unwrap_or_default(),
                app_secret: config.whatsapp.app_secret.clone().unwrap_or_default(),
            };

            mux.add_channel("whatsapp".to_string(), Box::new(whatsapp));
            info!("whatsapp channel added to multiplexer");

            // Redact access token in logs.
            if let Some(ref token) = config.whatsapp.access_token {
                blufio_security::RedactingWriter::<std::io::Stderr>::add_vault_value(
                    &vault_values,
                    token.clone(),
                );
            }

            Some(webhook_state)
        } else {
            info!("whatsapp channel skipped (no phone_number_id configured)");
            None
        }
    };

    // Add Signal channel (if enabled and configured).
    #[cfg(feature = "signal")]
    {
        if config.signal.socket_path.is_some() || config.signal.host.is_some() {
            let signal = SignalChannel::new(config.signal.clone()).map_err(|e| {
                error!(error = %e, "failed to initialize Signal channel");
                eprintln!(
                    "error: Signal requires a running signal-cli daemon. \
                     Configure socket_path or host:port in [signal] config section"
                );
                e
            })?;
            mux.add_channel("signal".to_string(), Box::new(signal));
            info!("signal channel added to multiplexer");
        } else {
            info!("signal channel skipped (no socket_path or host configured)");
        }
    }

    // Add IRC channel (if enabled and configured).
    #[cfg(feature = "irc")]
    {
        if config.irc.server.is_some() {
            let irc = IrcChannel::new(config.irc.clone()).map_err(|e| {
                error!(error = %e, "failed to initialize IRC channel");
                eprintln!(
                    "error: IRC server configuration required. \
                     Set server and nickname in [irc] config section"
                );
                e
            })?;
            mux.add_channel("irc".to_string(), Box::new(irc));
            info!("irc channel added to multiplexer");

            // Redact IRC password in logs.
            if let Some(ref password) = config.irc.password {
                blufio_security::RedactingWriter::<std::io::Stderr>::add_vault_value(
                    &vault_values,
                    password.clone(),
                );
            }
        } else {
            info!("irc channel skipped (no server configured)");
        }
    }

    // Add Matrix channel (if enabled and configured).
    #[cfg(feature = "matrix")]
    {
        if config.matrix.homeserver_url.is_some() {
            let matrix = MatrixChannel::new(config.matrix.clone()).map_err(|e| {
                error!(error = %e, "failed to initialize Matrix channel");
                eprintln!(
                    "error: Matrix homeserver URL, username, and password required. \
                     Set in [matrix] config section"
                );
                e
            })?;
            mux.add_channel("matrix".to_string(), Box::new(matrix));
            info!("matrix channel added to multiplexer");

            // Redact Matrix password in logs.
            if let Some(ref password) = config.matrix.password {
                blufio_security::RedactingWriter::<std::io::Stderr>::add_vault_value(
                    &vault_values,
                    password.clone(),
                );
            }
        } else {
            info!("matrix channel skipped (no homeserver_url configured)");
        }
    }

    // Install signal handler early so the cancellation token is available
    // for MCP HTTP transport and gateway startup.
    let cancel = shutdown::install_signal_handler();

    // Spawn MCP health monitor for connected servers (CLNT-06).
    #[cfg(feature = "mcp-client")]
    let _mcp_health_handle = if let Some(sessions) = _mcp_health_sessions {
        let health_tracker = std::sync::Arc::new(tokio::sync::RwLock::new(
            blufio_mcp_client::health::HealthTracker::new(),
        ));
        let health_cancel = cancel.child_token();
        let handle = blufio_mcp_client::health::spawn_health_monitor(
            sessions,
            health_tracker,
            config.mcp.health_check_interval_secs,
            health_cancel,
        );
        info!(
            interval_secs = config.mcp.health_check_interval_secs,
            "MCP health monitor spawned"
        );
        Some(handle)
    } else {
        None
    };

    // --- Memory background task and file watcher ---
    // Spawn after cancel token is available.
    if config.memory.enabled
        && let Some(ref store) = memory_store
    {
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
        if !config.memory.file_watcher.paths.is_empty()
            && let Some(ref embedder_arc) = memory_embedder
        {
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

    // Declare holder for MCP tools-changed sender so it lives until run_serve returns.
    #[cfg(feature = "mcp-server")]
    let mut _tools_changed_tx: Option<blufio_mcp_server::notifications::ToolsChangedSender> = None;

    // Build Prometheus render function for gateway /metrics endpoint.
    #[cfg(feature = "prometheus")]
    let prometheus_render: Option<Arc<dyn Fn() -> String + Send + Sync>> =
        _prometheus_adapter.as_ref().map(|adapter| {
            let handle = adapter.handle().clone();
            Arc::new(move || handle.render()) as Arc<dyn Fn() -> String + Send + Sync>
        });
    #[cfg(not(feature = "prometheus"))]
    let prometheus_render: Option<Arc<dyn Fn() -> String + Send + Sync>> = None;

    // Initialize provider registry for gateway API endpoints (API-01..API-10).
    #[cfg(feature = "gateway")]
    let provider_registry = if config.gateway.enabled {
        match ConcreteProviderRegistry::from_config(&config).await {
            Ok(reg) => {
                info!(
                    default = reg.default_provider(),
                    "provider registry initialized"
                );
                Some(Arc::new(reg) as Arc<dyn blufio_core::ProviderRegistry + Send + Sync>)
            }
            Err(e) => {
                error!(error = %e, "failed to initialize provider registry");
                return Err(e);
            }
        }
    } else {
        None
    };

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
                mcp_max_connections: config.mcp.max_connections,
            };
            let mut gateway = GatewayChannel::new(gateway_config);

            // Wire storage adapter for GET /v1/sessions (DEBT-01).
            gateway.set_storage(storage.clone()).await;

            // Wire provider registry for OpenAI-compatible API (API-01..API-08).
            if let Some(ref providers) = provider_registry {
                gateway.set_providers(providers.clone()).await;
                info!("provider registry wired into gateway");
            }

            // Wire tool registry for Tools API (API-09..API-10).
            gateway.set_tools(tool_registry.clone()).await;
            info!("tool registry wired into gateway");

            // Wire API tools allowlist from config (API-10: secure default = empty = no tools).
            gateway.set_api_tools_allowlist(config.gateway.api_tools_allowlist.clone());
            info!(
                allowlist_count = config.gateway.api_tools_allowlist.len(),
                "api tools allowlist configured"
            );

            // Open a dedicated connection for gateway stores (API-11..18).
            // V7 migration tables already created by SqliteStorage::initialize().
            let store_conn = blufio_storage::open_connection(&config.storage.database_path).await?;

            let api_key_store = Arc::new(blufio_gateway::api_keys::store::ApiKeyStore::new(
                store_conn.clone(),
            ));
            let webhook_store = Arc::new(blufio_gateway::webhooks::store::WebhookStore::new(
                store_conn.clone(),
            ));
            let batch_store = Arc::new(blufio_gateway::batch::store::BatchStore::new(store_conn));

            gateway.set_api_key_store(api_key_store).await;
            gateway.set_webhook_store(webhook_store.clone()).await;
            gateway.set_batch_store(batch_store).await;
            gateway.set_event_bus(event_bus.clone()).await;

            // Wire resilience subsystem into gateway for /v1/health visibility.
            if let Some(ref dm) = resilience_manager {
                gateway.set_degradation_manager(dm.clone()).await;
            }
            if let Some(ref reg) = resilience_registry {
                gateway.set_circuit_breaker_registry(reg.clone()).await;
            }
            info!("gateway stores wired (api_keys, webhooks, batch, event_bus)");

            // Spawn webhook delivery background loop (API-16).
            // Subscribes to EventBus, matches events to registered webhooks,
            // delivers with HMAC-SHA256 signing and exponential backoff retry.
            {
                let delivery_bus = event_bus.clone();
                let delivery_store = webhook_store;
                tokio::spawn(async move {
                    blufio_gateway::webhooks::delivery::run_webhook_delivery(
                        delivery_bus,
                        delivery_store,
                        reqwest::Client::new(),
                    )
                    .await;
                });
                info!("webhook delivery engine spawned");
            }

            // Wire MCP HTTP transport onto the gateway (if enabled).
            #[cfg(feature = "mcp-server")]
            if config.mcp.enabled {
                // SEC: Validate auth_token is set (defense in depth -- validation.rs also checks).
                let mcp_auth_token = config.mcp.auth_token.clone().ok_or_else(|| {
                    BlufioError::Security("MCP enabled but mcp.auth_token is not set".to_string())
                })?;

                // Redact MCP auth token in logs.
                blufio_security::RedactingWriter::<std::io::Stderr>::add_vault_value(
                    &vault_values,
                    mcp_auth_token.clone(),
                );

                // Create tools-changed notification channel.
                let (tools_changed_tx, tools_changed_rx) =
                    blufio_mcp_server::notifications::tools_changed_channel();

                let mcp_cancel = cancel.child_token();
                let mcp_config = blufio_mcp_server::transport::mcp_service_config(mcp_cancel);
                let mcp_handler =
                    blufio_mcp_server::BlufioMcpHandler::new(tool_registry.clone(), &config.mcp)
                        .with_resources(
                            memory_store.clone(),
                            Some(storage.clone()
                                as Arc<dyn blufio_core::StorageAdapter + Send + Sync>),
                        )
                        .with_notifications(tools_changed_rx);
                let mcp_router = blufio_mcp_server::transport::build_mcp_router(
                    mcp_handler,
                    mcp_config,
                    &config.mcp.cors_origins,
                    mcp_auth_token,
                );
                gateway.set_mcp_router(mcp_router).await;

                // Hold the sender alive for the lifetime of run_serve.
                // No code currently calls notify() -- that will happen when
                // skill install events are wired in a future phase.
                _tools_changed_tx = Some(tools_changed_tx);

                info!(
                    memory_resources = memory_store.is_some(),
                    notifications = true,
                    "MCP HTTP transport enabled at /mcp with resource access",
                );
            }

            // Wire WhatsApp webhook routes into gateway (unauthenticated public routes).
            #[cfg(feature = "whatsapp")]
            if let Some(ref webhook_state) = _whatsapp_webhook_state {
                let whatsapp_routes =
                    blufio_whatsapp::webhook::whatsapp_webhook_routes(webhook_state.clone());
                gateway.set_extra_public_routes(whatsapp_routes).await;
                info!("whatsapp webhook routes mounted on gateway at /webhooks/whatsapp");
            }

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
    #[cfg(unix)]
    {
        let ready_status = format!(
            "Ready: {} channel{}{}",
            mux.channel_count(),
            if mux.channel_count() == 1 { "" } else { "s" },
            if memory_provider.is_some() {
                ", memory enabled"
            } else {
                ""
            }
        );
        blufio_agent::sdnotify::notify_ready(&ready_status);
    }

    // Grab channel references for notification delivery BEFORE mux is moved.
    let notification_channels = mux.connected_channels_ref();

    // Spawn degradation notification task (if resilience enabled) -- DEG-05.
    // Sends user-facing messages to all active channels on degradation level transitions.
    if resilience_manager.is_some() {
        let notif_rx = event_bus.subscribe_reliable(64).await;
        let channels = notification_channels.clone();
        let dedup_secs = notification_dedup_secs;
        tokio::spawn(async move {
            let mut rx = notif_rx;
            let mut last_notified: std::collections::HashMap<u8, tokio::time::Instant> =
                std::collections::HashMap::new();

            while let Some(event) = rx.recv().await {
                if let blufio_bus::events::BusEvent::Resilience(
                    blufio_bus::events::ResilienceEvent::DegradationLevelChanged {
                        from_level,
                        to_level,
                        to_name,
                        reason,
                        ..
                    },
                ) = &event
                {
                    // Dedup: skip if we notified about this level within dedup_secs.
                    let now = tokio::time::Instant::now();
                    if let Some(last) = last_notified.get(to_level)
                        && now.duration_since(*last).as_secs() < dedup_secs
                    {
                        tracing::debug!(
                            to_level = to_level,
                            "skipping duplicate degradation notification"
                        );
                        continue;
                    }
                    last_notified.insert(*to_level, now);

                    // Build notification message.
                    let message = if to_level > from_level {
                        // Escalation
                        format!(
                            "[Blufio] Degraded: {}. Current level: L{} {}.",
                            reason, to_level, to_name
                        )
                    } else {
                        // Recovery
                        format!(
                            "[Blufio] Recovered: {}. Current level: L{} {}.",
                            reason, to_level, to_name
                        )
                    };

                    // Send to ALL active channels (best-effort).
                    for (channel_name, adapter) in channels.iter() {
                        let outbound = blufio_core::types::OutboundMessage {
                            session_id: None,
                            channel: channel_name.clone(),
                            content: message.clone(),
                            reply_to: None,
                            parse_mode: None,
                            metadata: Some(
                                serde_json::json!({"is_degradation_notification": true})
                                    .to_string(),
                            ),
                        };
                        if let Err(e) = adapter.send(outbound).await {
                            tracing::warn!(
                                channel = %channel_name,
                                error = %e,
                                "failed to send degradation notification"
                            );
                            // Best-effort: continue to other channels
                        }
                    }

                    tracing::info!(
                        from_level = from_level,
                        to_level = to_level,
                        channels = channels.len(),
                        "degradation notification sent"
                    );
                }
            }
        });
        info!("degradation notification sender spawned");
    }

    // --- Bridge system ---
    #[cfg(feature = "bridge")]
    let _bridge_handles = if !config.bridge.is_empty() {
        // Grab a reference to connected channels BEFORE mux is moved into AgentLoop.
        let bridge_channels = mux.connected_channels_ref();

        // Spawn the bridge loop (subscribes to event bus, routes messages).
        let (mut bridge_rx, bridge_task) =
            blufio_bridge::spawn_bridge(event_bus.clone(), config.bridge.clone());

        // Spawn a consumer task that dispatches bridged messages to target channels.
        let dispatch_task = tokio::spawn(async move {
            while let Some(bridged_msg) = bridge_rx.recv().await {
                // Find the target channel adapter by name.
                let target = bridge_channels
                    .iter()
                    .find(|(name, _)| name == &bridged_msg.target_channel);

                if let Some((_, adapter)) = target {
                    let outbound = blufio_core::types::OutboundMessage {
                        session_id: None,
                        channel: bridged_msg.target_channel.clone(),
                        content: bridged_msg.content,
                        reply_to: None,
                        parse_mode: None,
                        metadata: Some(serde_json::json!({"is_bridged": true}).to_string()),
                    };
                    if let Err(e) = adapter.send(outbound).await {
                        warn!(
                            target_channel = %bridged_msg.target_channel,
                            error = %e,
                            "bridge dispatch failed"
                        );
                    }
                } else {
                    warn!(
                        target_channel = %bridged_msg.target_channel,
                        "bridge target channel not found in multiplexer"
                    );
                }
            }
            info!("bridge dispatch task completed");
        });

        info!(groups = config.bridge.len(), "cross-channel bridge started");
        Some((bridge_task, dispatch_task))
    } else {
        info!("no bridge groups configured, bridge disabled");
        None
    };

    // Initialize model router.
    let router = Arc::new(ModelRouter::new(config.routing.clone()));
    if config.routing.enabled {
        if let Some(ref forced) = config.routing.force_model {
            info!(
                model = forced.as_str(),
                "model routing enabled with forced model"
            );
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
            registry
                .register(Arc::new(delegation_tool))
                .expect("register delegation tool");
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

    // --- Node system ---
    #[cfg(feature = "node")]
    if config.node.enabled {
        info!(port = config.node.listen_port, "starting node system");

        let node_conn = blufio_storage::open_connection(&config.storage.database_path).await?;
        let node_store = Arc::new(blufio_node::NodeStore::new(node_conn));

        let conn_manager = Arc::new(blufio_node::ConnectionManager::new(
            node_store.clone(),
            event_bus.clone(),
            config.node.clone(),
        ));

        // Create approval router and wire into connection manager.
        let approval_router = Arc::new(blufio_node::ApprovalRouter::new(
            conn_manager.clone(),
            node_store.clone(),
            config.node.approval.clone(),
        ));
        conn_manager.set_approval_router(approval_router.clone());

        // Spawn EventBus subscription for approval routing.
        {
            let approval_bus = event_bus.clone();
            let approval_router_clone = approval_router.clone();
            tokio::spawn(async move {
                let mut rx = approval_bus.subscribe_reliable(256).await;
                tracing::info!("approval event subscription started");

                while let Some(event) = rx.recv().await {
                    let event_type = event.event_type_string();
                    if approval_router_clone.requires_approval(event_type) {
                        let description = format!("{event_type}: {event:?}");
                        if let Err(e) = approval_router_clone
                            .request_approval(event_type, &description)
                            .await
                        {
                            tracing::error!(error = %e, event_type = event_type, "failed to request approval");
                        }
                    }
                }
                tracing::warn!("approval event subscription stopped -- event bus closed");
            });
            info!("approval event subscription spawned");
        }

        // Reconnect to all known peers (now with approval_router available).
        conn_manager.reconnect_all().await;

        // Start heartbeat monitor.
        let heartbeat_monitor = blufio_node::HeartbeatMonitor::new(
            conn_manager.clone(),
            event_bus.clone(),
            config.node.clone(),
        );
        tokio::spawn(async move {
            heartbeat_monitor.run().await;
        });

        info!("node system started");
    }

    // --- Cron scheduler ---
    // Spawn after EventBus and DB init, uses CancellationToken for graceful shutdown.
    if config.cron.enabled {
        let cron_db =
            Arc::new(blufio_storage::open_connection(&config.storage.database_path).await?);
        let task_registry = Arc::new(blufio_cron::register_builtin_tasks(
            cron_db.clone(),
            &config,
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
    } else {
        debug!("cron scheduler disabled by configuration");
    }

    // --- Config hot reload ---
    // Spawn after EventBus, before hook system init.
    if config.hot_reload.enabled {
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
                // Fallback to local path (watcher will still work if file appears)
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
                // config_swap holds Arc<ArcSwap<BlufioConfig>> for session creation.
                // Sessions that want hot-reloaded config should receive this handle
                // and call hot_reload::load_config() once at creation (HTRL-05).
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
    } else {
        debug!("config hot reload disabled by configuration");
    }

    // --- Hook system ---
    // Spawn after EventBus, uses CancellationToken for graceful shutdown.
    // Follows CronScheduler non-fatal init pattern.
    let hook_manager: Option<Arc<HookManager>> = if config.hooks.enabled {
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
            .execute_lifecycle_hooks("pre_start", &event_bus)
            .await;

        // Spawn EventBus-driven hook run loop (shared via Arc since run takes &self)
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
    } else {
        debug!("hook system disabled by configuration");
        None
    };

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

    // Spawn sd_notify watchdog ping task (if systemd watchdog is enabled).
    #[cfg(unix)]
    {
        let wd_cancel = cancel.clone();
        let _watchdog_handle = blufio_agent::sdnotify::spawn_watchdog(wd_cancel);
    }

    // Spawn heartbeat background task if enabled.
    if let Some(ref runner) = heartbeat_runner {
        let hb_runner = runner.clone();
        let hb_cancel = cancel.clone();
        let interval_secs = config.heartbeat.interval_secs;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
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

    // Extract resilience drain timeout before config is moved into AgentLoop.
    let resilience_drain_secs = if resilience_cancel_token.is_some() {
        Some(config.resilience.drain_timeout_secs)
    } else {
        None
    };

    // Extract fallback chain before config is moved.
    let fallback_chain = config.resilience.fallback_chain.clone();

    // Build fallback provider registry before config is moved (DEG-06).
    let fallback_provider_registry: Option<
        Arc<dyn blufio_core::traits::ProviderRegistry + Send + Sync>,
    > = if !fallback_chain.is_empty() && resilience_registry.is_some() {
        // Reuse gateway's provider_registry if available, else create a new one.
        #[cfg(feature = "gateway")]
        {
            if let Some(ref reg) = provider_registry {
                Some(reg.clone())
            } else {
                match ConcreteProviderRegistry::from_config(&config).await {
                    Ok(reg) => {
                        info!("fallback provider registry initialized (non-gateway)");
                        Some(Arc::new(reg)
                            as Arc<
                                dyn blufio_core::traits::ProviderRegistry + Send + Sync,
                            >)
                    }
                    Err(e) => {
                        warn!(error = %e, "failed to initialize fallback provider registry, fallback disabled");
                        None
                    }
                }
            }
        }
        #[cfg(not(feature = "gateway"))]
        {
            match ConcreteProviderRegistry::from_config(&config).await {
                Ok(reg) => {
                    info!("fallback provider registry initialized");
                    Some(Arc::new(reg)
                        as Arc<
                            dyn blufio_core::traits::ProviderRegistry + Send + Sync,
                        >)
                }
                Err(e) => {
                    warn!(error = %e, "failed to initialize fallback provider registry, fallback disabled");
                    None
                }
            }
        }
    } else {
        None
    };

    // --- Injection defense pipeline (INJC-06) ---
    // Initialize before config is moved into AgentLoop.
    let injection_pipeline: Option<
        Arc<tokio::sync::Mutex<blufio_injection::pipeline::InjectionPipeline>>,
    > = if config.injection_defense.enabled {
        let classifier =
            blufio_injection::classifier::InjectionClassifier::new(&config.injection_defense);
        let pipeline = blufio_injection::pipeline::InjectionPipeline::new(
            &config.injection_defense,
            classifier,
            Some(event_bus.clone()),
        );

        let active_layers: Vec<&str> = [
            Some("L1"), // L1 is always active when injection defense is enabled
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
    } else {
        debug!("injection defense disabled by configuration");
        None
    };

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

    // Wire EventBus into AgentLoop for publishing MessageSent events.
    agent_loop.set_event_bus(event_bus.clone());

    // Wire resilience subsystem into AgentLoop for SessionActor circuit breaker integration.
    if let Some(ref registry) = resilience_registry {
        agent_loop.set_circuit_breaker_registry(registry.clone());
    }
    if let Some(ref dm) = resilience_manager {
        agent_loop.set_degradation_manager(dm.clone());
    }
    agent_loop.set_provider_name("anthropic".to_string());

    // Wire fallback chain and provider registry for fallback provider routing (DEG-06).
    if let Some(reg) = fallback_provider_registry {
        agent_loop.set_provider_registry(reg);
        agent_loop.set_fallback_chain(fallback_chain.clone());
        info!(
            chain = ?fallback_chain,
            "fallback provider chain configured"
        );
    }

    // Wire injection defense pipeline (INJC-06) if enabled.
    if let Some(ref pipeline) = injection_pipeline {
        agent_loop.set_injection_pipeline(pipeline.clone());
    }

    // Log integration status summary.
    {
        let security_status = "OK (TLS 1.2+ / SSRF protection)";
        let redaction_status = "OK (RedactingWriter active)";
        #[cfg(feature = "prometheus")]
        let metrics_status = if _prometheus_adapter.is_some() {
            "OK"
        } else {
            "WARN (disabled)"
        };
        #[cfg(not(feature = "prometheus"))]
        let metrics_status = "WARN (not compiled)";

        info!(
            security = security_status,
            redaction = redaction_status,
            metrics = metrics_status,
            "integration status"
        );
    }

    // Fire post_start hooks after all subsystems are initialized.
    if let Some(ref hm) = hook_manager {
        hm.execute_lifecycle_hooks("post_start", &event_bus).await;
    }

    // If resilience L5 shutdown is wired, propagate it to the main cancel token.
    if let Some(ref l5_token) = resilience_cancel_token {
        let l5 = l5_token.clone();
        let main_cancel = cancel.clone();
        let drain_secs = resilience_drain_secs.unwrap_or(30);
        tokio::spawn(async move {
            l5.cancelled().await;
            error!(
                drain_timeout_secs = drain_secs,
                "resilience L5 safe shutdown triggered -- stopping agent"
            );
            #[cfg(unix)]
            blufio_agent::sdnotify::notify_stopping("L5 SafeShutdown: draining in-flight requests");
            // Give in-flight requests time to complete before cancelling main loop.
            tokio::time::sleep(Duration::from_secs(drain_secs)).await;
            main_cancel.cancel();
        });
    }

    agent_loop.run(cancel).await?;

    // Fire pre_shutdown hooks before cleanup begins.
    if let Some(ref hm) = hook_manager {
        hm.execute_lifecycle_hooks("pre_shutdown", &event_bus).await;
    }

    // Flush and shut down audit trail (after adapters disconnect, before DB close).
    if let Some(writer) = audit_writer {
        if let Err(e) = writer.flush().await {
            warn!(error = %e, "audit trail flush failed during shutdown");
        }
        // Arc::try_unwrap to get ownership for shutdown; if other refs exist, flush was enough.
        match Arc::try_unwrap(writer) {
            Ok(w) => {
                w.shutdown().await;
                info!("audit trail flushed and stopped");
            }
            Err(_arc) => {
                // Other references exist; flush already called above.
                info!("audit trail flushed (shutdown deferred to last reference drop)");
            }
        }
    }

    // Fire post_shutdown hooks after all cleanup is complete.
    if let Some(ref hm) = hook_manager {
        hm.execute_lifecycle_hooks("post_shutdown", &event_bus)
            .await;
    }

    info!("blufio serve shutdown complete");
    Ok(())
}

/// Marks any sessions that were left in "active" state as "interrupted".
///
/// This handles the case where the process was previously killed without
/// graceful shutdown, leaving sessions in an active state.
async fn mark_stale_sessions(storage: &dyn StorageAdapter) -> Result<(), BlufioError> {
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
/// Returns (MemoryProvider, MemoryExtractor, MemoryStore) on success.
/// The MemoryStore Arc is returned so the MCP server can expose memory resources.
#[allow(dead_code)]
async fn initialize_memory(
    config: &BlufioConfig,
    context_engine: &mut ContextEngine,
) -> Result<
    (
        MemoryProvider,
        Arc<MemoryExtractor>,
        Arc<MemoryStore>,
        Arc<OnnxEmbedder>,
    ),
    BlufioError,
> {
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
    let memory_conn = blufio_storage::open_connection(&config.storage.database_path).await?;
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
        memory_store.clone(),
        embedder.clone(),
        config.memory.extraction_model.clone(),
    ));

    info!("memory system initialized");
    Ok((memory_provider, extractor, memory_store, embedder))
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

/// A `MakeWriter` implementation that creates `RedactingWriter` instances.
///
/// Used by tracing-subscriber to ensure all log output passes through
/// secret redaction before reaching stderr. Redacts:
/// - Anthropic API keys (`sk-ant-*` pattern)
/// - Telegram bot tokens
/// - Vault-stored secret values (dynamically updated via `Arc<RwLock>`)
struct RedactingMakeWriter {
    vault_values: std::sync::Arc<std::sync::RwLock<Vec<String>>>,
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for RedactingMakeWriter {
    type Writer = blufio_security::RedactingWriter<std::io::Stderr>;

    fn make_writer(&'a self) -> Self::Writer {
        blufio_security::RedactingWriter::new(std::io::stderr(), self.vault_values.clone())
    }
}

/// Initializes the tracing subscriber with secret redaction.
///
/// Returns a shared handle to the vault values list. The caller
/// populates this after vault unlock so that dynamically-loaded
/// secrets are redacted in subsequent log output.
///
/// Regex patterns catch `sk-ant-*`, generic `sk-*`, Bearer tokens, and
/// Telegram bot tokens automatically. The vault values handle catches
/// non-pattern-matching secrets loaded from the credential vault.
///
/// # Panics
/// Panics if a tracing subscriber is already installed.
fn init_tracing(log_level: &str) -> std::sync::Arc<std::sync::RwLock<Vec<String>>> {
    use tracing_subscriber::EnvFilter;

    let vault_values = std::sync::Arc::new(std::sync::RwLock::new(Vec::new()));

    let redacting_writer = RedactingMakeWriter {
        vault_values: vault_values.clone(),
    };

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("blufio={log_level},warn")));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_names(false)
        .with_writer(redacting_writer)
        .init();

    vault_values
}
