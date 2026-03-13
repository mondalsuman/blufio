// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio serve` command implementation.
//!
//! Starts the full Blufio agent with configured channel adapters, Anthropic
//! provider, SQLite storage, three-zone context engine, and cost tracking.
//! Uses the PluginRegistry to discover and initialize compiled-in adapters,
//! and the ChannelMultiplexer to aggregate multiple channels.
//!
//! Decomposed into focused sub-modules:
//! - [`storage`]: Database, cost ledger, tokenizer, context engine, memory init
//! - [`channels`]: Channel adapter initialization (Telegram, Discord, Slack, etc.)
//! - [`gateway`]: Gateway/API setup, provider registry, MCP transport, webhooks
//! - [`subsystems`]: EventBus, audit, resilience, cron, hooks, hot reload, etc.

mod channels;
mod gateway;
mod storage;
mod subsystems;

use std::sync::Arc;
use std::time::Duration;

use blufio_agent::shutdown;
use blufio_agent::{
    AgentLoop, DelegationRouter, DelegationTool, HeartbeatRunner,
};
use blufio_config::model::BlufioConfig;
use blufio_core::error::BlufioError;
use blufio_core::{ChannelAdapter, StorageAdapter};
use blufio_router::ModelRouter;
use tracing::{debug, error, info, warn};



/// A `MakeWriter` implementation that creates `RedactingWriter` instances.
///
/// Used by tracing-subscriber to ensure all log output passes through
/// secret redaction before reaching stderr.
struct RedactingMakeWriter {
    vault_values: std::sync::Arc<std::sync::RwLock<Vec<String>>>,
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for RedactingMakeWriter {
    type Writer = blufio_security::RedactingWriter<std::io::Stderr>;

    fn make_writer(&'a self) -> Self::Writer {
        blufio_security::RedactingWriter::new(std::io::stderr(), self.vault_values.clone())
    }
}

/// Tracing initialization state returned by [`init_tracing`].
struct TracingState {
    vault_values: std::sync::Arc<std::sync::RwLock<Vec<String>>>,
    #[cfg(feature = "otel")]
    otel_provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
}

/// Initializes the tracing subscriber with secret redaction and optional
/// OpenTelemetry layer.
fn init_tracing(log_level: &str, config: &BlufioConfig) -> TracingState {
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::prelude::*;

    let vault_values = std::sync::Arc::new(std::sync::RwLock::new(Vec::new()));

    let redacting_writer = RedactingMakeWriter {
        vault_values: vault_values.clone(),
    };

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("blufio={log_level},warn")));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_names(false)
        .with_writer(redacting_writer);

    #[cfg(feature = "otel")]
    {
        let otel_result = crate::otel::try_init_otel_layer(&config.observability.opentelemetry);
        if let Some((otel_layer, provider)) = otel_result {
            let otel_filter = EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(format!("blufio={log_level},warn")));
            let otel_writer = RedactingMakeWriter {
                vault_values: vault_values.clone(),
            };
            let otel_fmt = tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_thread_names(false)
                .with_writer(otel_writer);
            tracing_subscriber::registry()
                .with(otel_layer)
                .with(otel_filter)
                .with(otel_fmt)
                .init();
            return TracingState {
                vault_values,
                otel_provider: Some(provider),
            };
        }
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .init();
        return TracingState {
            vault_values,
            otel_provider: None,
        };
    }

    #[cfg(not(feature = "otel"))]
    {
        if config.observability.opentelemetry.enabled {
            eprintln!(
                "WARNING: OpenTelemetry enabled in config but 'otel' feature not compiled. \
                 Rebuild with --features otel"
            );
        }

        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .init();
        TracingState { vault_values }
    }
}

/// Runs the `blufio serve` command.
///
/// Initializes all adapters via the PluginRegistry pattern, creates a
/// ChannelMultiplexer for multi-channel support, and enters the main
/// agent loop. Supports graceful shutdown via signal handlers.
pub async fn run_serve(config: BlufioConfig) -> Result<(), BlufioError> {
    // Initialize tracing subscriber with secret redaction (SEC-08) and optional OTel layer.
    let tracing_state = init_tracing(&config.agent.log_level, &config);
    let vault_values = tracing_state.vault_values.clone();

    info!("starting blufio serve");

    // Initialize plugin registry.
    let _registry = subsystems::initialize_plugin_registry(&config);

    // Vault startup check and secret redaction registration.
    subsystems::vault_and_secret_redaction(&config, &vault_values).await?;

    // Initialize storage.
    let storage = storage::init_storage(&config).await?;

    // Litestream WAL replication pragma.
    storage::apply_litestream_pragma(&config).await?;

    // Mark stale sessions as interrupted (crash recovery).
    storage::mark_stale_sessions(storage.as_ref()).await?;

    // Initialize cost tracking.
    let (cost_ledger, budget_tracker) = storage::init_cost_tracking(&config).await?;

    // Initialize tokenizer cache.
    let token_cache = storage::init_tokenizer(&config);

    // Initialize context engine.
    let mut context_engine = storage::init_context_engine(&config, &token_cache).await?;

    // Initialize memory system.
    let (memory_provider, memory_extractor, memory_store, memory_embedder) =
        storage::init_memory_system(&config, &mut context_engine).await;

    // Initialize tool registry.
    let tool_registry = subsystems::init_tool_registry().await;

    // Create global event bus.
    let event_bus = subsystems::create_event_bus();

    // Initialize audit trail.
    let audit_writer = subsystems::init_audit(&config, &event_bus).await;

    // Initialize resilience subsystem.
    let resilience = subsystems::init_resilience(&config, &event_bus).await;

    // Register context providers (SkillProvider, ArchiveConditionalProvider).
    subsystems::register_context_providers(
        &config,
        &mut context_engine,
        &tool_registry,
        &token_cache,
    )
    .await?;

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

        let mcp_injection_classifier = subsystems::prepare_mcp_classifier(&config);

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

    let context_engine = Arc::new(context_engine);

    // Initialize Anthropic provider.
    let provider = gateway::init_provider(&config).await?;

    // Initialize Prometheus metrics.
    let prometheus_render = gateway::init_prometheus(&config);

    // Initialize channels.
    let mut channel_result = channels::init_channels(&config, &event_bus, &vault_values)?;

    // Install signal handler early.
    let cancel = shutdown::install_signal_handler();

    // Spawn MCP health monitor.
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

    // Spawn memory background tasks.
    subsystems::spawn_memory_tasks(
        &config,
        &memory_store,
        &memory_embedder,
        &event_bus,
        &cancel,
    )
    .await;

    // Holder for MCP tools-changed sender.
    #[cfg(feature = "mcp-server")]
    let mut _tools_changed_tx: Option<blufio_mcp_server::notifications::ToolsChangedSender> = None;

    // Initialize gateway channel.
    #[cfg(feature = "gateway")]
    let provider_registry = gateway::init_gateway(
        &config,
        &mut channel_result.mux,
        #[cfg(feature = "whatsapp")]
        &channel_result.whatsapp_webhook_state,
        &channel_result.imessage_webhook_state,
        &channel_result.sms_webhook_state,
        &event_bus,
        &storage,
        &tool_registry,
        &memory_store,
        &resilience.manager,
        &resilience.registry,
        &prometheus_render,
        &vault_values,
        &cancel,
        #[cfg(feature = "mcp-server")]
        &mut _tools_changed_tx,
    )
    .await?;

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
    channel_result.mux.connect().await?;
    info!(
        channels = channel_result.mux.channel_count(),
        "channel multiplexer connected"
    );
    #[cfg(unix)]
    {
        let ready_status = format!(
            "Ready: {} channel{}{}",
            channel_result.mux.channel_count(),
            if channel_result.mux.channel_count() == 1 { "" } else { "s" },
            if memory_provider.is_some() {
                ", memory enabled"
            } else {
                ""
            }
        );
        blufio_agent::sdnotify::notify_ready(&ready_status);
    }

    // Grab channel references for notification delivery BEFORE mux is moved.
    let notification_channels = channel_result.mux.connected_channels_ref();

    // Spawn degradation notification task (if resilience enabled).
    if resilience.manager.is_some() {
        let notif_rx = event_bus.subscribe_reliable(64).await;
        let channels = notification_channels.clone();
        let dedup_secs = resilience.notification_dedup_secs;
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

                    let message = if to_level > from_level {
                        format!(
                            "[Blufio] Degraded: {}. Current level: L{} {}.",
                            reason, to_level, to_name
                        )
                    } else {
                        format!(
                            "[Blufio] Recovered: {}. Current level: L{} {}.",
                            reason, to_level, to_name
                        )
                    };

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
        let bridge_channels = channel_result.mux.connected_channels_ref();

        let (mut bridge_rx, bridge_task) =
            blufio_bridge::spawn_bridge(event_bus.clone(), config.bridge.clone());

        let dispatch_task = tokio::spawn(async move {
            while let Some(bridged_msg) = bridge_rx.recv().await {
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

        let approval_router = Arc::new(blufio_node::ApprovalRouter::new(
            conn_manager.clone(),
            node_store.clone(),
            config.node.approval.clone(),
        ));
        conn_manager.set_approval_router(approval_router.clone());

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

        conn_manager.reconnect_all().await;

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

    // Initialize cron scheduler.
    subsystems::init_cron(&config, &event_bus, &cancel).await;

    // Initialize config hot reload.
    subsystems::init_hot_reload(&config, &event_bus, &cancel).await;

    // Initialize hook system.
    let hook_manager = subsystems::init_hooks(&config, &event_bus, &cancel).await;

    // Spawn memory monitor.
    {
        let daemon_config = config.daemon.clone();
        let mem_cancel = cancel.clone();
        tokio::spawn(async move {
            subsystems::memory_monitor(&daemon_config, mem_cancel).await;
        });
        info!(
            warn_mb = config.daemon.memory_warn_mb,
            limit_mb = config.daemon.memory_limit_mb,
            "memory monitor started"
        );
    }

    // Spawn sd_notify watchdog ping task.
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
    let resilience_drain_secs = if resilience.cancel_token.is_some() {
        Some(config.resilience.drain_timeout_secs)
    } else {
        None
    };

    let fallback_chain = config.resilience.fallback_chain.clone();

    // Build fallback provider registry (DEG-06).
    #[cfg(feature = "gateway")]
    let fallback_provider_registry = gateway::build_fallback_provider_registry(
        &config,
        &provider_registry,
        &resilience.registry,
    )
    .await;

    #[cfg(not(feature = "gateway"))]
    let fallback_provider_registry = gateway::build_fallback_provider_registry(
        &config,
        &None,
        &resilience.registry,
    )
    .await;

    // Initialize injection defense pipeline (INJC-06).
    let injection_pipeline = subsystems::init_injection_pipeline(&config, &event_bus);

    // Create and run agent loop with channel multiplexer.
    let mut agent_loop = AgentLoop::new(
        Box::new(channel_result.mux),
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

    // Wire EventBus into AgentLoop.
    agent_loop.set_event_bus(event_bus.clone());

    // Wire resilience subsystem into AgentLoop.
    if let Some(ref registry) = resilience.registry {
        agent_loop.set_circuit_breaker_registry(registry.clone());
    }
    if let Some(ref dm) = resilience.manager {
        agent_loop.set_degradation_manager(dm.clone());
    }
    agent_loop.set_provider_name("anthropic".to_string());

    // Wire fallback chain and provider registry (DEG-06).
    if let Some(reg) = fallback_provider_registry {
        agent_loop.set_provider_registry(reg);
        agent_loop.set_fallback_chain(fallback_chain.clone());
        info!(
            chain = ?fallback_chain,
            "fallback provider chain configured"
        );
    }

    // Wire injection defense pipeline (INJC-06).
    if let Some(ref pipeline) = injection_pipeline {
        agent_loop.set_injection_pipeline(pipeline.clone());
    }

    // Log integration status summary.
    {
        let security_status = "OK (TLS 1.2+ / SSRF protection)";
        let redaction_status = "OK (RedactingWriter active)";
        #[cfg(feature = "prometheus")]
        let metrics_status = if prometheus_render.is_some() {
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

    // Fire post_start hooks.
    if let Some(ref hm) = hook_manager {
        hm.execute_lifecycle_hooks("post_start", &event_bus).await;
    }

    // If resilience L5 shutdown is wired, propagate it to the main cancel token.
    if let Some(ref l5_token) = resilience.cancel_token {
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
            tokio::time::sleep(Duration::from_secs(drain_secs)).await;
            main_cancel.cancel();
        });
    }

    agent_loop.run(cancel).await?;

    // Flush and shut down OTel TracerProvider before other cleanup (OTEL-01).
    #[cfg(feature = "otel")]
    {
        if let Some(provider) = tracing_state.otel_provider {
            crate::otel::shutdown_otel(provider);
        }
    }

    // Fire pre_shutdown hooks.
    if let Some(ref hm) = hook_manager {
        hm.execute_lifecycle_hooks("pre_shutdown", &event_bus).await;
    }

    // Flush and shut down audit trail.
    if let Some(writer) = audit_writer {
        if let Err(e) = writer.flush().await {
            warn!(error = %e, "audit trail flush failed during shutdown");
        }
        match Arc::try_unwrap(writer) {
            Ok(w) => {
                w.shutdown().await;
                info!("audit trail flushed and stopped");
            }
            Err(_arc) => {
                info!("audit trail flushed (shutdown deferred to last reference drop)");
            }
        }
    }

    // Fire post_shutdown hooks.
    if let Some(ref hm) = hook_manager {
        hm.execute_lifecycle_hooks("post_shutdown", &event_bus)
            .await;
    }

    info!("blufio serve shutdown complete");
    Ok(())
}
