// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTP/WebSocket gateway implementing ChannelAdapter.
//!
//! The gateway provides REST API access alongside channel-based messaging.
//! By implementing the same ChannelAdapter trait as Telegram, the gateway
//! reuses the entire agent loop, session management, and tool pipeline.

pub mod api_keys;
pub mod auth;
pub mod batch;
pub mod handlers;
pub mod openai_compat;
pub mod rate_limit;
pub mod server;
pub mod sse;
pub mod webhooks;
pub mod ws;

use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use tokio::sync::{Mutex, mpsc};

use blufio_core::BlufioError;
use blufio_core::ProviderRegistry;
use blufio_core::StorageAdapter;
use blufio_core::traits::adapter::PluginAdapter;
use blufio_core::traits::channel::ChannelAdapter;
use blufio_core::types::{
    AdapterType, ChannelCapabilities, FormattingSupport, HealthStatus, InboundMessage, MessageId,
    OutboundMessage, StreamingType,
};
use blufio_skill::ToolRegistry;
use tokio::sync::RwLock;

use crate::auth::AuthConfig;
use crate::server::{GatewayState, HealthState, ServerConfig};

/// Gateway channel adapter configuration.
///
/// Mirrors `GatewayConfig` from `blufio-config` to avoid a dependency on
/// the config crate from the gateway crate.
#[derive(Clone)]
pub struct GatewayChannelConfig {
    /// Enable the gateway.
    pub enabled: bool,
    /// Host address to bind.
    pub host: String,
    /// Port to bind.
    pub port: u16,
    /// Bearer token for auth.
    pub bearer_token: Option<String>,
    /// Ed25519 public key for keypair signature verification.
    pub keypair_public_key: Option<ed25519_dalek::VerifyingKey>,
    /// Optional Prometheus metrics render function for /metrics endpoint.
    pub prometheus_render: Option<Arc<dyn Fn() -> String + Send + Sync>>,
    /// Maximum concurrent MCP connections (INTG-05). Default: 10.
    pub mcp_max_connections: usize,
}

impl std::fmt::Debug for GatewayChannelConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GatewayChannelConfig")
            .field("enabled", &self.enabled)
            .field("host", &self.host)
            .field("port", &self.port)
            .field(
                "bearer_token",
                &self.bearer_token.as_ref().map(|_| "[redacted]"),
            )
            .field("keypair_public_key", &self.keypair_public_key.is_some())
            .field(
                "prometheus_render",
                &self.prometheus_render.as_ref().map(|_| "<fn>"),
            )
            .finish()
    }
}

/// HTTP/WebSocket gateway implementing ChannelAdapter.
///
/// The gateway runs an axum server as a background task. HTTP handlers create
/// InboundMessages and push them to an mpsc channel. GatewayChannel::receive()
/// reads from this channel, and GatewayChannel::send() routes responses back
/// to waiting HTTP handlers via oneshot channels or WebSocket senders.
pub struct GatewayChannel {
    config: GatewayChannelConfig,
    inbound_tx: mpsc::Sender<InboundMessage>,
    inbound_rx: Mutex<mpsc::Receiver<InboundMessage>>,
    response_map: Arc<DashMap<String, tokio::sync::oneshot::Sender<String>>>,
    ws_senders: Arc<DashMap<String, mpsc::Sender<String>>>,
    server_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Optional MCP HTTP router to mount at /mcp on the gateway.
    /// Set via [`set_mcp_router`] before calling `connect()`.
    mcp_router: Mutex<Option<axum::Router>>,
    /// Optional storage adapter for session queries (DEBT-01).
    /// Set via [`set_storage`] before calling `connect()`.
    storage: Mutex<Option<Arc<dyn StorageAdapter + Send + Sync>>>,
    /// Optional provider registry for OpenAI-compatible API (API-01).
    /// Set via [`set_providers`] before calling `connect()`.
    providers: Mutex<Option<Arc<dyn ProviderRegistry + Send + Sync>>>,
    /// Optional tool registry for Tools API (API-09).
    /// Set via [`set_tools`] before calling `connect()`.
    tools: Mutex<Option<Arc<RwLock<ToolRegistry>>>>,
    /// Allowlist of tool names accessible via the Tools API (API-10).
    api_tools_allowlist: Vec<String>,
    /// Optional extra public routes (e.g., WhatsApp webhook routes).
    /// Set via [`set_extra_public_routes`] before calling `connect()`.
    extra_public_routes: Mutex<Option<axum::Router>>,
    /// Optional API key store for key-based authentication (API-11..12).
    /// Set via [`set_api_key_store`] before calling `connect()`.
    api_key_store: Mutex<Option<Arc<crate::api_keys::store::ApiKeyStore>>>,
    /// Optional webhook store for webhook management (API-13..14).
    /// Set via [`set_webhook_store`] before calling `connect()`.
    webhook_store: Mutex<Option<Arc<crate::webhooks::store::WebhookStore>>>,
    /// Optional batch store for batch request handling (API-15..17).
    /// Set via [`set_batch_store`] before calling `connect()`.
    batch_store: Mutex<Option<Arc<crate::batch::store::BatchStore>>>,
    /// Optional event bus for real-time event delivery (API-18).
    /// Set via [`set_event_bus`] before calling `connect()`.
    event_bus: Mutex<Option<Arc<blufio_bus::EventBus>>>,
}

impl GatewayChannel {
    /// Create a new GatewayChannel.
    pub fn new(config: GatewayChannelConfig) -> Self {
        let (inbound_tx, inbound_rx) = mpsc::channel(256);
        Self {
            config,
            inbound_tx,
            inbound_rx: Mutex::new(inbound_rx),
            response_map: Arc::new(DashMap::new()),
            ws_senders: Arc::new(DashMap::new()),
            server_handle: Mutex::new(None),
            mcp_router: Mutex::new(None),
            storage: Mutex::new(None),
            providers: Mutex::new(None),
            tools: Mutex::new(None),
            api_tools_allowlist: Vec::new(),
            extra_public_routes: Mutex::new(None),
            api_key_store: Mutex::new(None),
            webhook_store: Mutex::new(None),
            batch_store: Mutex::new(None),
            event_bus: Mutex::new(None),
        }
    }

    /// Sets extra public (unauthenticated) routes to merge into the gateway.
    ///
    /// Used for webhook endpoints that must be unauthenticated, such as
    /// WhatsApp webhooks (Meta sends without Bearer tokens).
    /// Must be called before `connect()`.
    pub async fn set_extra_public_routes(&self, routes: axum::Router) {
        let mut r = self.extra_public_routes.lock().await;
        *r = Some(routes);
    }

    /// Sets the MCP HTTP router to mount at `/mcp` on the gateway.
    ///
    /// Must be called before `connect()`. The router should include its
    /// own auth and CORS layers (applied by `blufio_mcp_server::transport`).
    pub async fn set_mcp_router(&self, router: axum::Router) {
        let mut mcp = self.mcp_router.lock().await;
        *mcp = Some(router);
    }

    /// Sets the storage adapter for session queries.
    ///
    /// Must be called before `connect()`. Enables GET /v1/sessions to return
    /// actual session data from the database.
    pub async fn set_storage(&self, storage: Arc<dyn StorageAdapter + Send + Sync>) {
        let mut s = self.storage.lock().await;
        *s = Some(storage);
    }

    /// Sets the provider registry for OpenAI-compatible API endpoints.
    ///
    /// Must be called before `connect()`. Enables /v1/chat/completions,
    /// /v1/models, and /v1/responses endpoints.
    pub async fn set_providers(&self, providers: Arc<dyn ProviderRegistry + Send + Sync>) {
        let mut p = self.providers.lock().await;
        *p = Some(providers);
    }

    /// Sets the tool registry for the Tools API endpoints.
    ///
    /// Must be called before `connect()`. Enables /v1/tools and
    /// /v1/tools/invoke endpoints.
    pub async fn set_tools(&self, tools: Arc<RwLock<ToolRegistry>>) {
        let mut t = self.tools.lock().await;
        *t = Some(tools);
    }

    /// Sets the allowlist of tool names accessible via the Tools API.
    ///
    /// Must be called before `connect()`. Tools not in this list will
    /// receive 403 responses when invoked via the API.
    pub fn set_api_tools_allowlist(&mut self, allowlist: Vec<String>) {
        self.api_tools_allowlist = allowlist;
    }

    /// Sets the API key store for key-based authentication.
    ///
    /// Must be called before `connect()`. Enables API key management
    /// endpoints and key-based request authentication.
    pub async fn set_api_key_store(&self, store: Arc<crate::api_keys::store::ApiKeyStore>) {
        let mut s = self.api_key_store.lock().await;
        *s = Some(store);
    }

    /// Sets the webhook store for webhook management.
    ///
    /// Must be called before `connect()`. Enables webhook CRUD
    /// endpoints and event-driven webhook delivery.
    pub async fn set_webhook_store(&self, store: Arc<crate::webhooks::store::WebhookStore>) {
        let mut s = self.webhook_store.lock().await;
        *s = Some(store);
    }

    /// Sets the batch store for batch request handling.
    ///
    /// Must be called before `connect()`. Enables batch request
    /// submission and status tracking endpoints.
    pub async fn set_batch_store(&self, store: Arc<crate::batch::store::BatchStore>) {
        let mut s = self.batch_store.lock().await;
        *s = Some(store);
    }

    /// Sets the event bus for real-time event delivery.
    ///
    /// Must be called before `connect()`. Enables SSE event streaming
    /// and webhook event dispatch.
    pub async fn set_event_bus(&self, bus: Arc<blufio_bus::EventBus>) {
        let mut s = self.event_bus.lock().await;
        *s = Some(bus);
    }
}

#[async_trait]
impl PluginAdapter for GatewayChannel {
    fn name(&self) -> &str {
        "gateway"
    }

    fn version(&self) -> semver::Version {
        semver::Version::new(0, 1, 0)
    }

    fn adapter_type(&self) -> AdapterType {
        AdapterType::Channel
    }

    async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
        let handle = self.server_handle.lock().await;
        if handle.is_some() {
            Ok(HealthStatus::Healthy)
        } else {
            Ok(HealthStatus::Unhealthy("server not started".to_string()))
        }
    }

    async fn shutdown(&self) -> Result<(), BlufioError> {
        let mut handle = self.server_handle.lock().await;
        if let Some(h) = handle.take() {
            h.abort();
        }
        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for GatewayChannel {
    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_edit: false,
            supports_typing: false, // WebSocket typing handled via ws_senders
            supports_images: false,
            supports_documents: false,
            supports_voice: false,
            max_message_length: None,
            supports_embeds: false,
            supports_reactions: false,
            supports_threads: false,
            streaming_type: StreamingType::AppendOnly,
            formatting_support: FormattingSupport::HTML,
            rate_limit: None,
            supports_code_blocks: true,
        }
    }

    async fn connect(&mut self) -> Result<(), BlufioError> {
        let server_config = ServerConfig {
            host: self.config.host.clone(),
            port: self.config.port,
            bearer_token: self.config.bearer_token.clone(),
        };

        // Take optional adapters (if set).
        let storage = self.storage.lock().await.take();
        let providers = self.providers.lock().await.take();
        let tools = self.tools.lock().await.take();
        let api_key_store = self.api_key_store.lock().await.take();
        let webhook_store = self.webhook_store.lock().await.take();
        let batch_store = self.batch_store.lock().await.take();
        let event_bus = self.event_bus.lock().await.take();

        let state = GatewayState {
            inbound_tx: self.inbound_tx.clone(),
            response_map: Arc::clone(&self.response_map),
            ws_senders: Arc::clone(&self.ws_senders),
            auth: AuthConfig {
                bearer_token: self.config.bearer_token.clone(),
                keypair_public_key: self.config.keypair_public_key,
                key_store: api_key_store,
            },
            health: HealthState {
                start_time: std::time::Instant::now(),
                prometheus_render: self.config.prometheus_render.clone(),
            },
            storage,
            providers,
            tools,
            api_tools_allowlist: self.api_tools_allowlist.clone(),
            max_batch_size: 100,
            webhook_store,
            batch_store,
            event_bus,
        };

        // Take the MCP router (if set) to pass to the server.
        let mcp_router = self.mcp_router.lock().await.take();
        let mcp_max_connections = self.config.mcp_max_connections;
        let extra_public_routes = self.extra_public_routes.lock().await.take();

        let handle = tokio::spawn(async move {
            if let Err(e) = server::start_server(
                &server_config,
                state,
                mcp_router,
                mcp_max_connections,
                extra_public_routes,
            )
            .await
            {
                tracing::error!("gateway server error: {e}");
            }
        });

        let mut server_handle = self.server_handle.lock().await;
        *server_handle = Some(handle);

        tracing::info!(
            "Gateway channel connected on {}:{}",
            self.config.host,
            self.config.port
        );
        Ok(())
    }

    async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> {
        // Extract request_id from metadata for response routing.
        let metadata = msg.metadata.as_deref().unwrap_or("{}");
        let meta: serde_json::Value =
            serde_json::from_str(metadata).unwrap_or(serde_json::Value::Null);

        let request_id = meta
            .get("request_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let ws_id = meta.get("ws_id").and_then(|v| v.as_str());

        // Try WebSocket sender first (if ws_id present).
        if let Some(ws_id) = ws_id
            && let Some(sender) = self.ws_senders.get(ws_id)
        {
            let ws_msg = serde_json::json!({
                "type": ws::message_types::MESSAGE_COMPLETE,
                "content": msg.content,
                "session_id": msg.session_id,
            });
            let _ = sender.send(ws_msg.to_string()).await;
            return Ok(MessageId(request_id.to_string()));
        }

        // Try HTTP response map.
        if !request_id.is_empty()
            && let Some((_, sender)) = self.response_map.remove(request_id)
        {
            let _ = sender.send(msg.content);
            return Ok(MessageId(request_id.to_string()));
        }

        // No matching handler found.
        tracing::warn!(
            "no response handler found for request_id={}, ws_id={:?}",
            request_id,
            ws_id
        );
        Ok(MessageId(request_id.to_string()))
    }

    async fn receive(&self) -> Result<InboundMessage, BlufioError> {
        let mut rx = self.inbound_rx.lock().await;
        rx.recv()
            .await
            .ok_or_else(|| BlufioError::channel_connection_lost("gateway"))
    }

    async fn send_typing(&self, chat_id: &str) -> Result<(), BlufioError> {
        // For WebSocket connections, send a typing indicator.
        if let Some(sender) = self.ws_senders.get(chat_id) {
            let typing_msg = serde_json::json!({
                "type": ws::message_types::TYPING,
            });
            let _ = sender.send(typing_msg.to_string()).await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> GatewayChannelConfig {
        GatewayChannelConfig {
            enabled: true,
            host: "127.0.0.1".to_string(),
            port: 0, // Will bind to random port
            bearer_token: None,
            keypair_public_key: None,
            prometheus_render: None,
            mcp_max_connections: 10,
        }
    }

    #[test]
    fn gateway_channel_new() {
        let channel = GatewayChannel::new(test_config());
        assert_eq!(channel.name(), "gateway");
        assert_eq!(channel.adapter_type(), AdapterType::Channel);
        assert_eq!(channel.version(), semver::Version::new(0, 1, 0));
    }

    #[test]
    fn gateway_capabilities() {
        let channel = GatewayChannel::new(test_config());
        let caps = channel.capabilities();
        assert!(!caps.supports_edit);
        assert!(!caps.supports_typing);
        assert!(!caps.supports_images);
        assert!(!caps.supports_documents);
        assert!(!caps.supports_voice);
        assert!(caps.max_message_length.is_none());
    }

    #[tokio::test]
    async fn gateway_health_check_before_connect() {
        let channel = GatewayChannel::new(test_config());
        let health = channel.health_check().await.unwrap();
        match health {
            HealthStatus::Unhealthy(msg) => assert!(msg.contains("not started")),
            _ => panic!("expected Unhealthy before connect"),
        }
    }
}
