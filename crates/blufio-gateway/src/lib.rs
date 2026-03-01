// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTP/WebSocket gateway implementing ChannelAdapter.
//!
//! The gateway provides REST API access alongside channel-based messaging.
//! By implementing the same ChannelAdapter trait as Telegram, the gateway
//! reuses the entire agent loop, session management, and tool pipeline.

pub mod auth;
pub mod handlers;
pub mod server;
pub mod sse;
pub mod ws;

use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use tokio::sync::{mpsc, Mutex};

use blufio_core::traits::adapter::PluginAdapter;
use blufio_core::traits::channel::ChannelAdapter;
use blufio_core::types::{
    AdapterType, ChannelCapabilities, HealthStatus, InboundMessage, MessageId, OutboundMessage,
};
use blufio_core::BlufioError;

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
}

impl std::fmt::Debug for GatewayChannelConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GatewayChannelConfig")
            .field("enabled", &self.enabled)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("bearer_token", &self.bearer_token.as_ref().map(|_| "[redacted]"))
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
        }
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
        }
    }

    async fn connect(&mut self) -> Result<(), BlufioError> {
        let server_config = ServerConfig {
            host: self.config.host.clone(),
            port: self.config.port,
            bearer_token: self.config.bearer_token.clone(),
        };

        let state = GatewayState {
            inbound_tx: self.inbound_tx.clone(),
            response_map: Arc::clone(&self.response_map),
            ws_senders: Arc::clone(&self.ws_senders),
            auth: AuthConfig {
                bearer_token: self.config.bearer_token.clone(),
                keypair_public_key: self.config.keypair_public_key,
            },
            health: HealthState {
                start_time: std::time::Instant::now(),
                prometheus_render: self.config.prometheus_render.clone(),
            },
        };

        let handle = tokio::spawn(async move {
            if let Err(e) = server::start_server(&server_config, state).await {
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
        if let Some(ws_id) = ws_id {
            if let Some(sender) = self.ws_senders.get(ws_id) {
                let ws_msg = serde_json::json!({
                    "type": ws::message_types::MESSAGE_COMPLETE,
                    "content": msg.content,
                    "session_id": msg.session_id,
                });
                let _ = sender.send(ws_msg.to_string()).await;
                return Ok(MessageId(request_id.to_string()));
            }
        }

        // Try HTTP response map.
        if !request_id.is_empty() {
            if let Some((_, sender)) = self.response_map.remove(request_id) {
                let _ = sender.send(msg.content);
                return Ok(MessageId(request_id.to_string()));
            }
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
        rx.recv().await.ok_or_else(|| BlufioError::Channel {
            message: "gateway inbound channel closed".to_string(),
            source: None,
        })
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
