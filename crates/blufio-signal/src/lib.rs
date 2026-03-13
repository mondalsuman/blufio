// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

//! Signal channel adapter for the Blufio agent framework.
//!
//! Communicates with a running signal-cli JSON-RPC daemon via TCP or Unix
//! socket. Handles reconnection with exponential backoff.

pub mod jsonrpc;
pub mod types;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use blufio_config::model::SignalConfig;
use blufio_core::error::BlufioError;
use blufio_core::format::{FormatPipeline, split_at_paragraphs};
use blufio_core::traits::{ChannelAdapter, PluginAdapter};
use blufio_core::types::{
    AdapterType, ChannelCapabilities, FormattingSupport, HealthStatus, InboundMessage,
    MessageContent, MessageId, OutboundMessage, StreamingType,
};
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::jsonrpc::JsonRpcClient;

/// Signal channel adapter implementing [`ChannelAdapter`].
///
/// Connects to an externally managed signal-cli JSON-RPC daemon and handles
/// reconnection gracefully with exponential backoff.
pub struct SignalChannel {
    config: SignalConfig,
    inbound_rx: Mutex<mpsc::Receiver<InboundMessage>>,
    inbound_tx: mpsc::Sender<InboundMessage>,
    health: Arc<Mutex<HealthStatus>>,
    receive_handle: Option<JoinHandle<()>>,
    client: Arc<Mutex<Option<JsonRpcClient>>>,
}

impl SignalChannel {
    /// Creates a new Signal channel adapter.
    ///
    /// Validates that either `socket_path` or `host` is configured.
    pub fn new(config: SignalConfig) -> Result<Self, BlufioError> {
        if config.socket_path.is_none() && config.host.is_none() {
            return Err(BlufioError::Config(
                "signal: either socket_path or host must be configured".into(),
            ));
        }

        let (inbound_tx, inbound_rx) = mpsc::channel(100);

        Ok(Self {
            config,
            inbound_rx: Mutex::new(inbound_rx),
            inbound_tx,
            health: Arc::new(Mutex::new(HealthStatus::Unhealthy(
                "not connected".to_string(),
            ))),
            receive_handle: None,
            client: Arc::new(Mutex::new(None)),
        })
    }
}

#[async_trait]
impl PluginAdapter for SignalChannel {
    fn name(&self) -> &str {
        "signal"
    }

    fn version(&self) -> semver::Version {
        semver::Version::new(0, 1, 0)
    }

    fn adapter_type(&self) -> AdapterType {
        AdapterType::Channel
    }

    async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
        let health = self.health.lock().await;
        Ok(health.clone())
    }

    async fn shutdown(&self) -> Result<(), BlufioError> {
        if let Some(ref handle) = self.receive_handle {
            handle.abort();
        }
        debug!("Signal channel shutting down");
        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for SignalChannel {
    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_edit: false,
            supports_typing: false,
            supports_images: false,
            supports_documents: false,
            supports_voice: false,
            max_message_length: Some(4096),
            supports_embeds: false,
            supports_reactions: false,
            supports_threads: false,
            streaming_type: StreamingType::None,
            formatting_support: FormattingSupport::PlainText,
            rate_limit: None,
            supports_code_blocks: false,
        }
    }

    async fn connect(&mut self) -> Result<(), BlufioError> {
        let config = self.config.clone();
        let inbound_tx = self.inbound_tx.clone();
        let health = Arc::clone(&self.health);
        let _client_arc = Arc::clone(&self.client);

        let handle = tokio::spawn(async move {
            let mut backoff_secs = 1u64;
            let max_backoff = 60u64;

            loop {
                // Attempt connection.
                match JsonRpcClient::connect(&config).await {
                    Ok(mut rpc_client) => {
                        info!("connected to signal-cli daemon");
                        {
                            let mut h = health.lock().await;
                            *h = HealthStatus::Healthy;
                        }
                        backoff_secs = 1; // Reset on successful connect.

                        // Store client for send().
                        // Note: We can't easily share the client for both reading
                        // and writing since it holds exclusive mutable references.
                        // For outbound, we'll reconnect if needed.

                        // Read loop.
                        loop {
                            match rpc_client.read_notification().await {
                                Ok(Some(notif)) => {
                                    let envelope = &notif.params.envelope;

                                    let Some(ref data_msg) = envelope.data_message else {
                                        continue;
                                    };
                                    let Some(ref message_text) = data_msg.message else {
                                        continue;
                                    };

                                    let sender = envelope
                                        .source_number
                                        .as_deref()
                                        .or(envelope.source.as_deref())
                                        .unwrap_or("unknown");

                                    let sender_name =
                                        envelope.source_name.as_deref().unwrap_or(sender);

                                    let is_group = data_msg.group_info.is_some();
                                    let chat_id = if let Some(ref group) = data_msg.group_info {
                                        group.group_id.clone()
                                    } else {
                                        sender.to_string()
                                    };

                                    let metadata = serde_json::json!({
                                        "chat_id": chat_id,
                                        "source_name": sender_name,
                                        "is_group": is_group,
                                    });

                                    let inbound = InboundMessage {
                                        id: uuid::Uuid::new_v4().to_string(),
                                        session_id: None,
                                        channel: "signal".to_string(),
                                        sender_id: sender.to_string(),
                                        content: MessageContent::Text(message_text.clone()),
                                        metadata: Some(metadata.to_string()),
                                        timestamp: envelope
                                            .timestamp
                                            .map(|t| t.to_string())
                                            .unwrap_or_default(),
                                    };

                                    if inbound_tx.send(inbound).await.is_err() {
                                        warn!("Signal inbound channel closed");
                                        return;
                                    }
                                }
                                Ok(None) => {
                                    // EOF or non-receive notification — connection lost.
                                    warn!("signal-cli connection lost (EOF)");
                                    break;
                                }
                                Err(e) => {
                                    error!(error = %e, "error reading from signal-cli");
                                    break;
                                }
                            }
                        }

                        // Connection lost — enter reconnection.
                        {
                            let mut h = health.lock().await;
                            *h = HealthStatus::Degraded("reconnecting to signal-cli".to_string());
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, backoff_secs, "failed to connect to signal-cli, retrying");
                    }
                }

                // Exponential backoff.
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                backoff_secs = (backoff_secs * 2).min(max_backoff);
            }
        });

        self.receive_handle = Some(handle);
        Ok(())
    }

    async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> {
        // For outbound, we create a new short-lived connection.
        // This avoids sharing the read/write client between tasks.
        let mut rpc_client = JsonRpcClient::connect(&self.config).await?;

        let caps = self.capabilities();

        // Pipeline: detect_and_format -> no escape (PlainText) -> split -> send each chunk
        let formatted = FormatPipeline::detect_and_format(&msg.content, &caps);
        let chunks = split_at_paragraphs(&formatted, caps.max_message_length);

        // Determine if group or DM from metadata.
        let (is_group, chat_id) = if let Some(ref metadata) = msg.metadata
            && let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata)
        {
            let is_group = meta
                .get("is_group")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let cid = meta
                .get("chat_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            (is_group, cid)
        } else {
            (false, msg.channel.clone())
        };

        let mut first_id = None;

        for chunk in &chunks {
            let params = if is_group {
                serde_json::json!({
                    "groupId": chat_id,
                    "message": chunk,
                })
            } else {
                serde_json::json!({
                    "recipient": chat_id,
                    "message": chunk,
                })
            };

            let response = rpc_client.send_request("send", params).await?;

            if first_id.is_none() {
                let msg_id = response
                    .result
                    .and_then(|v| v.get("timestamp").and_then(|t| t.as_u64()))
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                first_id = Some(MessageId(msg_id));
            }
        }

        Ok(first_id.unwrap_or_else(|| MessageId(uuid::Uuid::new_v4().to_string())))
    }

    async fn receive(&self) -> Result<InboundMessage, BlufioError> {
        let mut rx = self.inbound_rx.lock().await;
        rx.recv()
            .await
            .ok_or_else(|| BlufioError::channel_connection_lost("signal"))
    }

    async fn edit_message(
        &self,
        _chat_id: &str,
        _message_id: &str,
        _text: &str,
        _parse_mode: Option<&str>,
    ) -> Result<(), BlufioError> {
        // Signal does not support message editing.
        Ok(())
    }

    async fn send_typing(&self, _chat_id: &str) -> Result<(), BlufioError> {
        // Signal does not support typing indicators via signal-cli JSON-RPC.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_requires_socket_path_or_host() {
        let config = SignalConfig::default();
        assert!(SignalChannel::new(config).is_err());
    }

    #[test]
    fn new_accepts_socket_path() {
        let config = SignalConfig {
            socket_path: Some("/tmp/signal-cli.sock".into()),
            ..Default::default()
        };
        assert!(SignalChannel::new(config).is_ok());
    }

    #[test]
    fn new_accepts_host() {
        let config = SignalConfig {
            host: Some("127.0.0.1".into()),
            ..Default::default()
        };
        assert!(SignalChannel::new(config).is_ok());
    }

    #[test]
    fn capabilities_correct() {
        let config = SignalConfig {
            host: Some("127.0.0.1".into()),
            ..Default::default()
        };
        let channel = SignalChannel::new(config).unwrap();
        let caps = channel.capabilities();
        assert!(!caps.supports_edit);
        assert!(!caps.supports_typing);
        assert_eq!(caps.max_message_length, Some(4096));
    }

    #[test]
    fn plugin_metadata() {
        let config = SignalConfig {
            host: Some("127.0.0.1".into()),
            ..Default::default()
        };
        let channel = SignalChannel::new(config).unwrap();
        assert_eq!(channel.name(), "signal");
        assert_eq!(channel.version(), semver::Version::new(0, 1, 0));
        assert_eq!(channel.adapter_type(), AdapterType::Channel);
    }
}
