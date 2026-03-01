// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Channel multiplexer that aggregates multiple ChannelAdapters into one.
//!
//! The multiplexer spawns per-channel receive tasks that forward inbound
//! messages to a shared mpsc channel. Outbound messages are routed back to
//! the originating channel based on the `channel` field or `source_channel`
//! metadata.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, Mutex};
use tracing::{info, warn};

use blufio_core::traits::adapter::PluginAdapter;
use blufio_core::traits::channel::ChannelAdapter;
use blufio_core::types::{
    AdapterType, ChannelCapabilities, HealthStatus, InboundMessage, MessageId, OutboundMessage,
};
use blufio_core::BlufioError;

/// A multiplexer that aggregates multiple channel adapters into a single
/// `ChannelAdapter` interface.
///
/// On `connect()`, each child channel is connected and a background task
/// is spawned that forwards its inbound messages to a shared mpsc channel.
/// On `send()`, the outbound message is routed to the originating channel
/// based on the message's `channel` field.
pub struct ChannelMultiplexer {
    /// Named child channels, stored before connect().
    pending_channels: Vec<(String, Box<dyn ChannelAdapter + Send + Sync>)>,
    /// Connected child channels (moved here after connect()).
    connected_channels: Arc<Vec<(String, Arc<dyn ChannelAdapter + Send + Sync>)>>,
    /// Shared inbound receiver.
    inbound_rx: Mutex<mpsc::Receiver<InboundMessage>>,
    /// Shared inbound sender (cloned per background task).
    inbound_tx: mpsc::Sender<InboundMessage>,
}

impl Default for ChannelMultiplexer {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelMultiplexer {
    /// Create a new empty multiplexer.
    pub fn new() -> Self {
        let (inbound_tx, inbound_rx) = mpsc::channel(512);
        Self {
            pending_channels: Vec::new(),
            connected_channels: Arc::new(Vec::new()),
            inbound_rx: Mutex::new(inbound_rx),
            inbound_tx,
        }
    }

    /// Add a named channel to the multiplexer.
    ///
    /// Must be called before `connect()`. The channel name is used for
    /// routing outbound messages back to the correct channel.
    pub fn add_channel(
        &mut self,
        name: String,
        channel: Box<dyn ChannelAdapter + Send + Sync>,
    ) {
        self.pending_channels.push((name, channel));
    }

    /// Number of channels registered (pending + connected).
    pub fn channel_count(&self) -> usize {
        self.pending_channels.len() + self.connected_channels.len()
    }
}

#[async_trait]
impl PluginAdapter for ChannelMultiplexer {
    fn name(&self) -> &str {
        "multiplexer"
    }

    fn version(&self) -> semver::Version {
        semver::Version::new(0, 1, 0)
    }

    fn adapter_type(&self) -> AdapterType {
        AdapterType::Channel
    }

    async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
        let mut any_unhealthy = false;
        let mut degraded_reasons = Vec::new();

        for (name, channel) in self.connected_channels.iter() {
            match channel.health_check().await? {
                HealthStatus::Healthy => {}
                HealthStatus::Degraded(reason) => {
                    degraded_reasons.push(format!("{name}: {reason}"));
                }
                HealthStatus::Unhealthy(reason) => {
                    any_unhealthy = true;
                    degraded_reasons.push(format!("{name}: {reason}"));
                }
            }
        }

        if any_unhealthy || !degraded_reasons.is_empty() {
            Ok(HealthStatus::Degraded(degraded_reasons.join("; ")))
        } else {
            Ok(HealthStatus::Healthy)
        }
    }

    async fn shutdown(&self) -> Result<(), BlufioError> {
        for (name, channel) in self.connected_channels.iter() {
            if let Err(e) = channel.shutdown().await {
                warn!(channel = %name, error = %e, "channel shutdown error");
            }
        }
        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for ChannelMultiplexer {
    fn capabilities(&self) -> ChannelCapabilities {
        // Union of all channel capabilities.
        let mut caps = ChannelCapabilities {
            supports_edit: false,
            supports_typing: false,
            supports_images: false,
            supports_documents: false,
            supports_voice: false,
            max_message_length: None,
        };

        for (_, channel) in self.connected_channels.iter() {
            let child_caps = channel.capabilities();
            caps.supports_edit = caps.supports_edit || child_caps.supports_edit;
            caps.supports_typing = caps.supports_typing || child_caps.supports_typing;
            caps.supports_images = caps.supports_images || child_caps.supports_images;
            caps.supports_documents = caps.supports_documents || child_caps.supports_documents;
            caps.supports_voice = caps.supports_voice || child_caps.supports_voice;
            // Use the minimum max_message_length across all channels.
            caps.max_message_length = match (caps.max_message_length, child_caps.max_message_length)
            {
                (Some(a), Some(b)) => Some(a.min(b)),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            };
        }

        caps
    }

    async fn connect(&mut self) -> Result<(), BlufioError> {
        let mut connected: Vec<(String, Arc<dyn ChannelAdapter + Send + Sync>)> = Vec::new();

        // Take ownership of pending channels.
        let pending = std::mem::take(&mut self.pending_channels);

        for (name, mut channel) in pending {
            channel.connect().await?;
            info!(channel = %name, "channel connected via multiplexer");

            let arc_channel: Arc<dyn ChannelAdapter + Send + Sync> = Arc::from(channel);
            connected.push((name.clone(), Arc::clone(&arc_channel)));

            // Spawn a background receive task for this channel.
            let tx = self.inbound_tx.clone();
            let channel_name = name.clone();
            let recv_channel = arc_channel;

            tokio::spawn(async move {
                loop {
                    match recv_channel.receive().await {
                        Ok(mut msg) => {
                            // Tag the message with its source channel.
                            if msg.metadata.is_none() {
                                msg.metadata = Some(
                                    serde_json::json!({"source_channel": channel_name})
                                        .to_string(),
                                );
                            } else if let Some(ref meta_str) = msg.metadata
                                && let Ok(mut meta) =
                                    serde_json::from_str::<serde_json::Value>(meta_str)
                            {
                                meta["source_channel"] =
                                    serde_json::Value::String(channel_name.clone());
                                msg.metadata = Some(meta.to_string());
                            }

                            // Set the channel name on the message.
                            msg.channel = channel_name.clone();

                            if tx.send(msg).await.is_err() {
                                // Multiplexer was dropped.
                                break;
                            }
                        }
                        Err(e) => {
                            if e.to_string().contains("closed") {
                                info!(
                                    channel = %channel_name,
                                    "channel closed, stopping receive task"
                                );
                                break;
                            }
                            warn!(
                                error = %e,
                                channel = %channel_name,
                                "channel receive error"
                            );
                        }
                    }
                }
            });
        }

        self.connected_channels = Arc::new(connected);

        info!(
            channels = self.connected_channels.len(),
            "channel multiplexer connected"
        );
        Ok(())
    }

    async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> {
        // Route to the correct channel based on the message's channel field
        // or source_channel metadata.
        let target_channel = &msg.channel;

        // Try to find the target channel.
        for (name, channel) in self.connected_channels.iter() {
            if name == target_channel {
                return channel.send(msg).await;
            }
        }

        // Fall back to source_channel from metadata.
        if let Some(ref meta_str) = msg.metadata
            && let Ok(meta) = serde_json::from_str::<serde_json::Value>(meta_str)
            && let Some(source) = meta.get("source_channel").and_then(|v| v.as_str())
        {
            for (name, channel) in self.connected_channels.iter() {
                if name == source {
                    return channel.send(msg).await;
                }
            }
        }

        // If only one channel, use it as default.
        if self.connected_channels.len() == 1 {
            return self.connected_channels[0].1.send(msg).await;
        }

        warn!(
            target = %target_channel,
            "no matching channel found for outbound message"
        );
        Ok(MessageId("unknown".to_string()))
    }

    async fn receive(&self) -> Result<InboundMessage, BlufioError> {
        let mut rx = self.inbound_rx.lock().await;
        rx.recv().await.ok_or_else(|| BlufioError::Channel {
            message: "multiplexer inbound channel closed".to_string(),
            source: None,
        })
    }

    async fn edit_message(
        &self,
        chat_id: &str,
        message_id: &str,
        text: &str,
        parse_mode: Option<&str>,
    ) -> Result<(), BlufioError> {
        // Try all channels -- only the correct one will succeed.
        for (_, channel) in self.connected_channels.iter() {
            let _ = channel.edit_message(chat_id, message_id, text, parse_mode).await;
        }
        Ok(())
    }

    async fn send_typing(&self, chat_id: &str) -> Result<(), BlufioError> {
        // Send typing to all channels (only relevant ones will act on it).
        for (_, channel) in self.connected_channels.iter() {
            let _ = channel.send_typing(chat_id).await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiplexer_new() {
        let mux = ChannelMultiplexer::new();
        assert_eq!(mux.name(), "multiplexer");
        assert_eq!(mux.adapter_type(), AdapterType::Channel);
        assert_eq!(mux.channel_count(), 0);
    }

    #[test]
    fn multiplexer_version() {
        let mux = ChannelMultiplexer::new();
        assert_eq!(mux.version(), semver::Version::new(0, 1, 0));
    }

    #[tokio::test]
    async fn multiplexer_empty_health_check() {
        let mux = ChannelMultiplexer::new();
        let health = mux.health_check().await.unwrap();
        assert_eq!(health, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn multiplexer_empty_capabilities() {
        let mux = ChannelMultiplexer::new();
        let caps = mux.capabilities();
        assert!(!caps.supports_edit);
        assert!(!caps.supports_typing);
        assert!(!caps.supports_images);
        assert!(caps.max_message_length.is_none());
    }

    #[tokio::test]
    async fn multiplexer_empty_shutdown() {
        let mux = ChannelMultiplexer::new();
        assert!(mux.shutdown().await.is_ok());
    }
}
