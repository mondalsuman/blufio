// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Gateway/API setup for `blufio serve`.
//!
//! Handles the GatewayChannel construction, provider registry initialization,
//! store wiring, MCP transport, webhook route composition, and Prometheus setup.

use std::sync::Arc;

use blufio_agent::ChannelMultiplexer;
use blufio_config::model::BlufioConfig;
use blufio_core::error::BlufioError;
use blufio_memory::MemoryStore;
use blufio_resilience::{CircuitBreakerRegistry, DegradationManager};
use blufio_skill::ToolRegistry;
use tracing::{debug, error, info, warn};

#[cfg(feature = "gateway")]
use blufio_gateway::{GatewayChannel, GatewayChannelConfig};

#[cfg(feature = "gateway")]
use blufio_core::ProviderRegistry;

use crate::providers::ConcreteProviderRegistry;

/// Initialize Prometheus metrics adapter (if enabled and compiled).
pub(crate) fn init_prometheus(
    config: &BlufioConfig,
) -> Option<Arc<dyn Fn() -> String + Send + Sync>> {
    #[cfg(feature = "prometheus")]
    {
        if config.prometheus.enabled {
            match blufio_prometheus::PrometheusAdapter::new() {
                Ok(adapter) => {
                    info!("prometheus metrics enabled");
                    let handle = adapter.handle().clone();
                    return Some(
                        Arc::new(move || handle.render()) as Arc<dyn Fn() -> String + Send + Sync>
                    );
                }
                Err(e) => {
                    warn!(error = %e, "prometheus initialization failed, continuing without metrics");
                    return None;
                }
            }
        } else {
            debug!("prometheus metrics disabled by configuration");
            return None;
        }
    }

    #[cfg(not(feature = "prometheus"))]
    {
        let _ = config;
        None
    }
}

/// Initialize the Anthropic provider.
#[cfg(feature = "anthropic")]
pub(crate) async fn init_provider(
    config: &BlufioConfig,
) -> Result<Arc<blufio_anthropic::AnthropicProvider>, BlufioError> {
    let p = blufio_anthropic::AnthropicProvider::new(config).await.map_err(|e| {
        error!(error = %e, "failed to initialize Anthropic provider");
        eprintln!(
            "error: Anthropic API key required. Set via: config, ANTHROPIC_API_KEY env var, or `blufio config set-secret anthropic.api_key`"
        );
        e
    })?;
    info!("anthropic provider initialized with TLS 1.2+ enforcement and SSRF protection");
    Ok(Arc::new(p))
}

#[cfg(not(feature = "anthropic"))]
compile_error!("blufio requires the 'anthropic' feature for the LLM provider");

/// Initialize the gateway channel with all stores, providers, MCP, and webhooks.
///
/// This function handles the entire gateway setup including:
/// - Provider registry initialization
/// - Store wiring (api_keys, webhooks, batch)
/// - MCP transport setup
/// - Webhook route composition
/// - Resilience subsystem wiring
#[cfg(feature = "gateway")]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn init_gateway(
    config: &BlufioConfig,
    mux: &mut ChannelMultiplexer,
    #[cfg(feature = "whatsapp")]
    whatsapp_webhook_state: &Option<blufio_whatsapp::webhook::WhatsAppWebhookState>,
    #[cfg(feature = "imessage")]
    imessage_webhook_state: &Option<blufio_imessage::webhook::IMessageWebhookState>,
    #[cfg(not(feature = "imessage"))]
    _imessage_webhook_state: &Option<()>,
    #[cfg(feature = "sms")]
    sms_webhook_state: &Option<blufio_sms::webhook::SmsWebhookState>,
    #[cfg(not(feature = "sms"))]
    _sms_webhook_state: &Option<()>,
    event_bus: &Arc<blufio_bus::EventBus>,
    storage: &Arc<blufio_storage::SqliteStorage>,
    tool_registry: &Arc<tokio::sync::RwLock<ToolRegistry>>,
    memory_store: &Option<Arc<MemoryStore>>,
    resilience_manager: &Option<Arc<DegradationManager>>,
    resilience_registry: &Option<Arc<CircuitBreakerRegistry>>,
    prometheus_render: &Option<Arc<dyn Fn() -> String + Send + Sync>>,
    vault_values: &std::sync::Arc<std::sync::RwLock<Vec<String>>>,
    cancel: &tokio_util::sync::CancellationToken,
    #[cfg(feature = "mcp-server")]
    tools_changed_tx_holder: &mut Option<blufio_mcp_server::notifications::ToolsChangedSender>,
) -> Result<
    Option<Arc<dyn blufio_core::ProviderRegistry + Send + Sync>>,
    BlufioError,
> {
    if !config.gateway.enabled {
        debug!("gateway channel disabled by configuration");

        // Warn about webhooks configured without gateway.
        #[cfg(feature = "imessage")]
        if config.imessage.bluebubbles_url.is_some() {
            warn!(
                "iMessage webhooks configured but gateway is disabled -- incoming messages will not work"
            );
        }
        #[cfg(feature = "sms")]
        if config.sms.account_sid.is_some() {
            warn!(
                "SMS webhooks configured but gateway is disabled -- incoming messages will not work"
            );
        }

        return Ok(None);
    }

    // Initialize provider registry for gateway API endpoints (API-01..API-10).
    let provider_registry: Option<Arc<dyn blufio_core::ProviderRegistry + Send + Sync>> =
        match ConcreteProviderRegistry::from_config(config).await {
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
        };

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
    if let Some(dm) = resilience_manager {
        gateway.set_degradation_manager(dm.clone()).await;
    }
    if let Some(reg) = resilience_registry {
        gateway.set_circuit_breaker_registry(reg.clone()).await;
    }
    info!("gateway stores wired (api_keys, webhooks, batch, event_bus)");

    // Spawn webhook delivery background loop (API-16).
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
            vault_values,
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
        *tools_changed_tx_holder = Some(tools_changed_tx);

        info!(
            memory_resources = memory_store.is_some(),
            notifications = true,
            "MCP HTTP transport enabled at /mcp with resource access",
        );
    }

    // Compose ALL webhook routes into a single Router.
    // CRITICAL: set_extra_public_routes() REPLACES (not appends), so compose first.
    {
        let mut webhook_routes: Option<axum::Router> = None;

        #[cfg(feature = "whatsapp")]
        if let Some(state) = whatsapp_webhook_state {
            let routes = blufio_whatsapp::webhook::whatsapp_webhook_routes(state.clone());
            webhook_routes = Some(match webhook_routes {
                Some(existing) => existing.merge(routes),
                None => routes,
            });
            info!("whatsapp webhook routes added at /webhooks/whatsapp");
        }

        #[cfg(feature = "imessage")]
        if let Some(state) = imessage_webhook_state {
            let routes = blufio_imessage::webhook::imessage_webhook_routes(state.clone());
            webhook_routes = Some(match webhook_routes {
                Some(existing) => existing.merge(routes),
                None => routes,
            });
            info!("imessage webhook routes added at /webhooks/imessage");
        }

        #[cfg(feature = "sms")]
        if let Some(state) = sms_webhook_state {
            let routes = blufio_sms::webhook::sms_webhook_routes(state.clone());
            webhook_routes = Some(match webhook_routes {
                Some(existing) => existing.merge(routes),
                None => routes,
            });
            info!("sms webhook routes added at /webhooks/sms");
        }

        if let Some(routes) = webhook_routes {
            gateway.set_extra_public_routes(routes).await;
            info!("webhook routes mounted on gateway");
        }
    }

    mux.add_channel("gateway".to_string(), Box::new(gateway));
    info!(
        host = config.gateway.host.as_str(),
        port = config.gateway.port,
        "gateway channel added to multiplexer"
    );

    Ok(provider_registry)
}

/// Build the fallback provider registry for DEG-06 failover.
#[cfg(feature = "gateway")]
pub(crate) async fn build_fallback_provider_registry(
    config: &BlufioConfig,
    provider_registry: &Option<Arc<dyn blufio_core::ProviderRegistry + Send + Sync>>,
    resilience_registry: &Option<Arc<CircuitBreakerRegistry>>,
) -> Option<Arc<dyn blufio_core::traits::ProviderRegistry + Send + Sync>> {
    if config.resilience.fallback_chain.is_empty() || resilience_registry.is_none() {
        return None;
    }

    // Reuse gateway's provider_registry if available, else create a new one.
    if let Some(reg) = provider_registry {
        Some(reg.clone())
    } else {
        match ConcreteProviderRegistry::from_config(config).await {
            Ok(reg) => {
                info!("fallback provider registry initialized (non-gateway)");
                Some(Arc::new(reg)
                    as Arc<dyn blufio_core::traits::ProviderRegistry + Send + Sync>)
            }
            Err(e) => {
                warn!(error = %e, "failed to initialize fallback provider registry, fallback disabled");
                None
            }
        }
    }
}

/// Build the fallback provider registry for non-gateway builds.
#[cfg(not(feature = "gateway"))]
pub(crate) async fn build_fallback_provider_registry(
    config: &BlufioConfig,
    _provider_registry: &Option<()>,
    resilience_registry: &Option<Arc<CircuitBreakerRegistry>>,
) -> Option<Arc<dyn blufio_core::traits::ProviderRegistry + Send + Sync>> {
    if config.resilience.fallback_chain.is_empty() || resilience_registry.is_none() {
        return None;
    }

    match ConcreteProviderRegistry::from_config(config).await {
        Ok(reg) => {
            info!("fallback provider registry initialized");
            Some(Arc::new(reg)
                as Arc<dyn blufio_core::traits::ProviderRegistry + Send + Sync>)
        }
        Err(e) => {
            warn!(error = %e, "failed to initialize fallback provider registry, fallback disabled");
            None
        }
    }
}
