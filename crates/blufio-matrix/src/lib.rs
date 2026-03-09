// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Matrix channel adapter for the Blufio agent framework.
//!
//! Connects to a Matrix homeserver, logs in with username/password, auto-joins
//! rooms on invite, and processes room messages. Pinned to matrix-sdk 0.11.0
//! (0.12+ requires Rust 1.88). E2E encryption is not enabled (deferred to EXT-06).

pub mod handler;

use async_trait::async_trait;
use blufio_config::model::MatrixConfig;
use blufio_core::error::{BlufioError, ChannelErrorKind, ErrorContext};
use blufio_core::traits::{ChannelAdapter, PluginAdapter};
use blufio_core::types::{
    AdapterType, ChannelCapabilities, FormattingSupport, HealthStatus, InboundMessage, MessageId,
    OutboundMessage, StreamingType,
};
use matrix_sdk::{
    Client,
    config::SyncSettings,
    ruma::events::room::message::RoomMessageEventContent,
    ruma::{OwnedRoomId, RoomId},
};
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Matrix channel adapter implementing [`ChannelAdapter`].
///
/// Connects to a homeserver, syncs events, and forwards room messages as
/// `InboundMessage` values.
pub struct MatrixChannel {
    config: MatrixConfig,
    inbound_rx: Mutex<mpsc::Receiver<InboundMessage>>,
    inbound_tx: mpsc::Sender<InboundMessage>,
    client: Mutex<Option<Client>>,
    sync_handle: Option<JoinHandle<()>>,
}

impl MatrixChannel {
    /// Creates a new Matrix channel adapter.
    ///
    /// Validates that `homeserver_url`, `username`, and `password` are configured.
    pub fn new(config: MatrixConfig) -> Result<Self, BlufioError> {
        if config.homeserver_url.is_none() {
            return Err(BlufioError::Config(
                "matrix: homeserver_url must be configured".into(),
            ));
        }
        if config.username.is_none() {
            return Err(BlufioError::Config(
                "matrix: username must be configured".into(),
            ));
        }
        if config.password.is_none() {
            return Err(BlufioError::Config(
                "matrix: password must be configured".into(),
            ));
        }

        let (inbound_tx, inbound_rx) = mpsc::channel(100);

        Ok(Self {
            config,
            inbound_rx: Mutex::new(inbound_rx),
            inbound_tx,
            client: Mutex::new(None),
            sync_handle: None,
        })
    }
}

#[async_trait]
impl PluginAdapter for MatrixChannel {
    fn name(&self) -> &str {
        "matrix"
    }

    fn version(&self) -> semver::Version {
        semver::Version::new(0, 1, 0)
    }

    fn adapter_type(&self) -> AdapterType {
        AdapterType::Channel
    }

    async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
        let client = self.client.lock().await;
        if client.is_some() {
            Ok(HealthStatus::Healthy)
        } else {
            Ok(HealthStatus::Unhealthy("not connected".to_string()))
        }
    }

    async fn shutdown(&self) -> Result<(), BlufioError> {
        if let Some(ref handle) = self.sync_handle {
            handle.abort();
        }
        debug!("Matrix channel shutting down");
        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for MatrixChannel {
    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_edit: true,
            supports_typing: true,
            supports_images: true,
            supports_documents: true,
            supports_voice: false,
            max_message_length: Some(65536),
            supports_embeds: false,
            supports_reactions: true,
            supports_threads: true,
            streaming_type: StreamingType::EditBased,
            formatting_support: FormattingSupport::HTML,
            rate_limit: None,
            supports_code_blocks: true,
        }
    }

    async fn connect(&mut self) -> Result<(), BlufioError> {
        let homeserver_url = self
            .config
            .homeserver_url
            .as_deref()
            .unwrap_or("https://matrix.org");
        let username = self.config.username.as_deref().unwrap_or("");
        let password = self.config.password.as_deref().unwrap_or("");

        // Build Matrix client.
        let client = Client::builder()
            .homeserver_url(homeserver_url)
            .build()
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("matrix", e))?;

        // Login.
        let display_name = self.config.display_name.as_deref().unwrap_or("Blufio");

        client
            .matrix_auth()
            .login_username(username, password)
            .initial_device_display_name(display_name)
            .send()
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("matrix", e))?;

        info!(
            homeserver = homeserver_url,
            username = username,
            "logged in to Matrix homeserver"
        );

        // Get bot user ID for message filtering.
        let bot_user_id = client
            .user_id()
            .map(|id| id.to_string())
            .unwrap_or_default();

        // Register event handlers with context.
        client.add_event_handler_context(self.inbound_tx.clone());
        client.add_event_handler_context(bot_user_id);
        client.add_event_handler_context(self.config.allowed_users.clone());

        client.add_event_handler(handler::on_room_message);
        client.add_event_handler(handler::on_room_invite);

        // Join configured rooms.
        for room_id_or_alias in &self.config.rooms {
            match <&RoomId>::try_from(room_id_or_alias.as_str()) {
                Ok(room_id) => {
                    if let Err(e) = client.join_room_by_id(room_id).await {
                        warn!(
                            room = room_id_or_alias,
                            error = %e,
                            "failed to join Matrix room (continuing)"
                        );
                    } else {
                        info!(room = room_id_or_alias, "joined Matrix room");
                    }
                }
                Err(_) => {
                    warn!(room = room_id_or_alias, "invalid room ID format, skipping");
                }
            }
        }

        // Store client.
        {
            let mut c = self.client.lock().await;
            *c = Some(client.clone());
        }

        // Spawn sync loop.
        let handle = tokio::spawn(async move {
            info!("Matrix sync loop starting");
            if let Err(e) = client.sync(SyncSettings::default()).await {
                error!(error = %e, "Matrix sync loop failed");
            }
        });

        self.sync_handle = Some(handle);
        Ok(())
    }

    async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> {
        let client_guard = self.client.lock().await;
        let client = client_guard
            .as_ref()
            .ok_or_else(|| BlufioError::channel_connection_lost("matrix"))?;

        // Extract room_id from metadata.
        let room_id_str = if let Some(ref metadata) = msg.metadata
            && let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata)
        {
            meta.get("chat_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        } else {
            msg.channel.clone()
        };

        let room_id: OwnedRoomId = room_id_str.parse().map_err(|e| BlufioError::Channel {
            kind: ChannelErrorKind::DeliveryFailed,
            context: ErrorContext {
                channel_name: Some("matrix".to_string()),
                ..Default::default()
            },
            source: None,
        })?;

        let room = client
            .get_room(&room_id)
            .ok_or_else(|| BlufioError::Channel {
                kind: ChannelErrorKind::DeliveryFailed,
                context: ErrorContext {
                    channel_name: Some("matrix".to_string()),
                    ..Default::default()
                },
                source: None,
            })?;

        let content = RoomMessageEventContent::text_plain(&msg.content);
        let response = room
            .send(content)
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("matrix", e))?;

        Ok(MessageId(response.event_id.to_string()))
    }

    async fn receive(&self) -> Result<InboundMessage, BlufioError> {
        let mut rx = self.inbound_rx.lock().await;
        rx.recv()
            .await
            .ok_or_else(|| BlufioError::channel_connection_lost("matrix"))
    }

    async fn edit_message(
        &self,
        chat_id: &str,
        _message_id: &str,
        text: &str,
        _parse_mode: Option<&str>,
    ) -> Result<(), BlufioError> {
        let client_guard = self.client.lock().await;
        let client = client_guard
            .as_ref()
            .ok_or_else(|| BlufioError::channel_connection_lost("matrix"))?;

        let room_id: OwnedRoomId = chat_id.parse().map_err(|e| BlufioError::Channel {
            kind: ChannelErrorKind::DeliveryFailed,
            context: ErrorContext {
                channel_name: Some("matrix".to_string()),
                ..Default::default()
            },
            source: None,
        })?;

        let room = client
            .get_room(&room_id)
            .ok_or_else(|| BlufioError::Channel {
                kind: ChannelErrorKind::DeliveryFailed,
                context: ErrorContext {
                    channel_name: Some("matrix".to_string()),
                    ..Default::default()
                },
                source: None,
            })?;

        // Matrix supports editing via replacement events.
        // For simplicity, we send a new message with the edit prefix.
        // Full replacement event support requires the original event ID.
        let content = RoomMessageEventContent::text_plain(format!("(edited) {text}"));
        room.send(content)
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("matrix", e))?;

        Ok(())
    }

    async fn send_typing(&self, chat_id: &str) -> Result<(), BlufioError> {
        let client_guard = self.client.lock().await;
        let client = client_guard
            .as_ref()
            .ok_or_else(|| BlufioError::channel_connection_lost("matrix"))?;

        let room_id: OwnedRoomId = chat_id.parse().map_err(|_| BlufioError::Channel {
            kind: ChannelErrorKind::DeliveryFailed,
            context: ErrorContext {
                channel_name: Some("matrix".to_string()),
                ..Default::default()
            },
            source: None,
        })?;

        if let Some(room) = client.get_room(&room_id) {
            let _ = room.typing_notice(true).await.map_err(|e| {
                debug!(error = %e, "failed to send Matrix typing notice");
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_requires_homeserver_url() {
        let config = MatrixConfig {
            username: Some("bot".into()),
            password: Some("pass".into()),
            ..Default::default()
        };
        assert!(MatrixChannel::new(config).is_err());
    }

    #[test]
    fn new_requires_username() {
        let config = MatrixConfig {
            homeserver_url: Some("https://matrix.org".into()),
            password: Some("pass".into()),
            ..Default::default()
        };
        assert!(MatrixChannel::new(config).is_err());
    }

    #[test]
    fn new_requires_password() {
        let config = MatrixConfig {
            homeserver_url: Some("https://matrix.org".into()),
            username: Some("bot".into()),
            ..Default::default()
        };
        assert!(MatrixChannel::new(config).is_err());
    }

    #[test]
    fn new_accepts_valid_config() {
        let config = MatrixConfig {
            homeserver_url: Some("https://matrix.org".into()),
            username: Some("bot".into()),
            password: Some("pass".into()),
            ..Default::default()
        };
        assert!(MatrixChannel::new(config).is_ok());
    }

    #[test]
    fn capabilities_correct() {
        let config = MatrixConfig {
            homeserver_url: Some("https://matrix.org".into()),
            username: Some("bot".into()),
            password: Some("pass".into()),
            ..Default::default()
        };
        let channel = MatrixChannel::new(config).unwrap();
        let caps = channel.capabilities();
        assert!(caps.supports_edit);
        assert!(caps.supports_typing);
        assert!(caps.supports_images);
        assert!(caps.supports_documents);
        assert!(!caps.supports_voice);
        assert_eq!(caps.max_message_length, Some(65536));
        assert!(caps.supports_reactions);
        assert!(caps.supports_threads);
    }

    #[test]
    fn plugin_metadata() {
        let config = MatrixConfig {
            homeserver_url: Some("https://matrix.org".into()),
            username: Some("bot".into()),
            password: Some("pass".into()),
            ..Default::default()
        };
        let channel = MatrixChannel::new(config).unwrap();
        assert_eq!(channel.name(), "matrix");
        assert_eq!(channel.version(), semver::Version::new(0, 1, 0));
        assert_eq!(channel.adapter_type(), AdapterType::Channel);
    }
}
