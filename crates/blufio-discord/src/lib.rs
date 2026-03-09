// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Discord channel adapter for the Blufio agent framework.
//!
//! Implements [`ChannelAdapter`] for the Discord Bot API via serenity,
//! providing Gateway WebSocket connection, @mention detection, slash commands,
//! embeds, and streaming responses.

pub mod commands;
pub mod handler;
pub mod markdown;
pub mod streaming;

use std::sync::Arc;

use async_trait::async_trait;
use blufio_config::model::DiscordConfig;
use blufio_core::error::{BlufioError, ChannelErrorKind, ErrorContext};
use blufio_core::traits::{ChannelAdapter, PluginAdapter};
use blufio_core::types::{
    AdapterType, ChannelCapabilities, FormattingSupport, HealthStatus, InboundMessage, MessageId,
    OutboundMessage, RateLimit, StreamingType,
};
use serenity::all::{ChannelId, CreateMessage, EditMessage, GatewayIntents};
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Shared state accessible from the serenity EventHandler.
struct HandlerState {
    inbound_tx: mpsc::Sender<InboundMessage>,
    allowed_users: Vec<String>,
}

/// Serenity event handler that forwards messages to the inbound channel.
struct Handler {
    state: Arc<HandlerState>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, _ctx: Context, msg: Message) {
        // Skip messages from bots (including ourselves).
        if msg.author.bot {
            return;
        }

        // Get the bot's user ID from cache.
        let bot_id = _ctx.cache.current_user().id;

        // Check if we should respond (DMs always, guilds only when @mentioned).
        if !handler::should_respond(&msg, bot_id) {
            debug!(channel_id = %msg.channel_id, "ignoring message (not mentioned)");
            return;
        }

        // Check authorization.
        if !handler::is_authorized(&msg, &self.state.allowed_users) {
            debug!(
                user_id = %msg.author.id,
                "ignoring unauthorized user"
            );
            return;
        }

        // Strip @mention from content.
        let content = handler::strip_mention(&msg.content, bot_id);

        if content.is_empty() {
            debug!(msg_id = %msg.id, "ignoring empty message after mention strip");
            return;
        }

        // Convert to InboundMessage and send to channel.
        let inbound = handler::to_inbound_message(&msg, content);
        if self.state.inbound_tx.send(inbound).await.is_err() {
            warn!("inbound channel closed, dropping Discord message");
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        info!(
            user = %ready.user.name,
            guilds = ready.guilds.len(),
            "Discord bot connected"
        );

        // Warn about MESSAGE_CONTENT intent if connected to guilds.
        if !ready.guilds.is_empty() {
            info!(
                "MESSAGE_CONTENT privileged intent requested; \
                 ensure it is enabled in the Discord Developer Portal"
            );
        }

        // Register slash commands globally.
        commands::register_commands(&ctx).await;
    }

    async fn interaction_create(&self, ctx: Context, interaction: serenity::all::Interaction) {
        commands::handle_interaction(&ctx, &interaction, &self.state.inbound_tx).await;
    }
}

/// Discord channel adapter implementing [`ChannelAdapter`].
///
/// Connects to Discord via Gateway WebSocket using serenity, filters messages
/// by authorization and mention detection, and delivers responses with
/// edit-in-place streaming.
pub struct DiscordChannel {
    config: DiscordConfig,
    inbound_rx: tokio::sync::Mutex<mpsc::Receiver<InboundMessage>>,
    inbound_tx: mpsc::Sender<InboundMessage>,
    http: Option<Arc<serenity::http::Http>>,
    gateway_handle: Option<tokio::task::JoinHandle<()>>,
}

impl DiscordChannel {
    /// Creates a new Discord channel adapter.
    ///
    /// Requires `config.bot_token` to be set.
    pub fn new(config: DiscordConfig) -> Result<Self, BlufioError> {
        let token = config.bot_token.as_deref().ok_or_else(|| {
            BlufioError::Config("discord.bot_token is required for Discord adapter".into())
        })?;

        if token.is_empty() {
            return Err(BlufioError::Config(
                "discord.bot_token cannot be empty".into(),
            ));
        }

        let (inbound_tx, inbound_rx) = mpsc::channel(100);

        Ok(Self {
            config,
            inbound_rx: tokio::sync::Mutex::new(inbound_rx),
            inbound_tx,
            http: None,
            gateway_handle: None,
        })
    }
}

#[async_trait]
impl PluginAdapter for DiscordChannel {
    fn name(&self) -> &str {
        "discord"
    }

    fn version(&self) -> semver::Version {
        semver::Version::new(0, 1, 0)
    }

    fn adapter_type(&self) -> AdapterType {
        AdapterType::Channel
    }

    async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
        match &self.http {
            Some(http) => match http.get_current_user().await {
                Ok(_) => Ok(HealthStatus::Healthy),
                Err(e) => Ok(HealthStatus::Unhealthy(format!(
                    "Discord API unreachable: {e}"
                ))),
            },
            None => Ok(HealthStatus::Unhealthy("Discord not connected".to_string())),
        }
    }

    async fn shutdown(&self) -> Result<(), BlufioError> {
        debug!("Discord channel shutting down");
        // The gateway handle will be dropped when DiscordChannel is dropped,
        // which aborts the task. For graceful shutdown, the agent loop should
        // stop calling receive() first.
        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for DiscordChannel {
    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_edit: true,
            supports_typing: true,
            supports_images: true,
            supports_documents: true,
            supports_voice: false,
            max_message_length: Some(2000),
            supports_embeds: true,
            supports_reactions: true,
            supports_threads: true,
            streaming_type: StreamingType::EditBased,
            formatting_support: FormattingSupport::FullMarkdown,
            rate_limit: Some(RateLimit {
                messages_per_second: Some(5.0),
                burst_limit: Some(5),
                daily_limit: None,
            }),
            supports_code_blocks: true,
        }
    }

    async fn connect(&mut self) -> Result<(), BlufioError> {
        if self.gateway_handle.is_some() {
            return Ok(()); // Already connected
        }

        let token = self
            .config
            .bot_token
            .as_deref()
            .ok_or_else(|| BlufioError::Config("discord.bot_token is required".into()))?
            .to_string();

        let intents = GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT
            | GatewayIntents::GUILD_MESSAGE_REACTIONS;

        let handler_state = Arc::new(HandlerState {
            inbound_tx: self.inbound_tx.clone(),
            allowed_users: self.config.allowed_users.clone(),
        });

        let handler = Handler {
            state: handler_state,
        };

        let mut client = Client::builder(&token, intents)
            .event_handler(handler)
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("discord", e))?;

        // Clone the HTTP client BEFORE start() consumes the client.
        self.http = Some(client.http.clone());

        info!("starting Discord Gateway WebSocket connection");

        let handle = tokio::spawn(async move {
            if let Err(e) = client.start().await {
                error!(error = %e, "Discord gateway connection error");
            }
        });

        self.gateway_handle = Some(handle);
        Ok(())
    }

    async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> {
        let http = self
            .http
            .as_ref()
            .ok_or_else(|| BlufioError::channel_connection_lost("discord"))?;

        let channel_id = extract_channel_id(&msg)?;
        let formatted = markdown::format_for_discord(&msg.content);

        let sent = channel_id
            .send_message(http, CreateMessage::new().content(&formatted))
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("discord", e))?;

        Ok(MessageId(sent.id.to_string()))
    }

    async fn receive(&self) -> Result<InboundMessage, BlufioError> {
        let mut rx = self.inbound_rx.lock().await;
        rx.recv().await.ok_or_else(|| BlufioError::channel_connection_lost("discord"))
    }

    async fn edit_message(
        &self,
        chat_id: &str,
        message_id: &str,
        text: &str,
        _parse_mode: Option<&str>,
    ) -> Result<(), BlufioError> {
        let http = self
            .http
            .as_ref()
            .ok_or_else(|| BlufioError::channel_connection_lost("discord"))?;

        let channel_id: u64 = chat_id.parse().map_err(|e| BlufioError::Channel {
            kind: ChannelErrorKind::DeliveryFailed,
            context: ErrorContext {
                channel_name: Some("discord".to_string()),
                ..Default::default()
            },
            source: None,
        })?;
        let channel_id = ChannelId::new(channel_id);

        let msg_id: u64 = message_id.parse().map_err(|e| BlufioError::Channel {
            kind: ChannelErrorKind::DeliveryFailed,
            context: ErrorContext {
                channel_name: Some("discord".to_string()),
                ..Default::default()
            },
            source: None,
        })?;
        let msg_id = serenity::model::id::MessageId::new(msg_id);

        let formatted = markdown::format_for_discord(text);

        channel_id
            .edit_message(http, msg_id, EditMessage::new().content(&formatted))
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("discord", e))?;

        Ok(())
    }

    async fn send_typing(&self, chat_id: &str) -> Result<(), BlufioError> {
        let http = self
            .http
            .as_ref()
            .ok_or_else(|| BlufioError::channel_connection_lost("discord"))?;

        let channel_id: u64 = chat_id.parse().map_err(|e| BlufioError::Channel {
            kind: ChannelErrorKind::DeliveryFailed,
            context: ErrorContext {
                channel_name: Some("discord".to_string()),
                ..Default::default()
            },
            source: None,
        })?;
        let channel_id = ChannelId::new(channel_id);

        channel_id
            .broadcast_typing(http)
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("discord", e))?;

        Ok(())
    }
}

/// Extracts the channel ID from an outbound message's metadata.
fn extract_channel_id(msg: &OutboundMessage) -> Result<ChannelId, BlufioError> {
    // Try to get chat_id from metadata (same field name as other adapters).
    if let Some(ref metadata) = msg.metadata
        && let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata)
        && let Some(chat_id_str) = meta.get("chat_id").and_then(|v| v.as_str())
    {
        let id = chat_id_str
            .parse::<u64>()
            .map_err(|e| BlufioError::Channel {
                kind: ChannelErrorKind::DeliveryFailed,
                context: ErrorContext {
                    channel_name: Some("discord".to_string()),
                    ..Default::default()
                },
                source: None,
            })?;
        return Ok(ChannelId::new(id));
    }

    // Fallback: try channel field as channel ID.
    msg.channel
        .parse::<u64>()
        .map(ChannelId::new)
        .map_err(|_| BlufioError::Channel {
            kind: ChannelErrorKind::DeliveryFailed,
            context: ErrorContext {
                channel_name: Some("discord".to_string()),
                ..Default::default()
            },
            source: None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_requires_bot_token() {
        let config = DiscordConfig {
            bot_token: None,
            application_id: None,
            allowed_users: vec![],
        };
        assert!(DiscordChannel::new(config).is_err());
    }

    #[test]
    fn new_rejects_empty_token() {
        let config = DiscordConfig {
            bot_token: Some(String::new()),
            application_id: None,
            allowed_users: vec![],
        };
        assert!(DiscordChannel::new(config).is_err());
    }

    #[test]
    fn new_accepts_valid_token() {
        let config = DiscordConfig {
            bot_token: Some("Bot MTIzNDU2Nzg5.test.token".into()),
            application_id: Some(123456789),
            allowed_users: vec!["user1".into()],
        };
        assert!(DiscordChannel::new(config).is_ok());
    }

    #[test]
    fn capabilities_are_correct() {
        let config = DiscordConfig {
            bot_token: Some("test-token".into()),
            application_id: None,
            allowed_users: vec![],
        };
        let channel = DiscordChannel::new(config).unwrap();
        let caps = channel.capabilities();
        assert!(caps.supports_edit);
        assert!(caps.supports_typing);
        assert!(caps.supports_images);
        assert!(caps.supports_documents);
        assert!(!caps.supports_voice);
        assert_eq!(caps.max_message_length, Some(2000));
        assert!(caps.supports_embeds);
        assert!(caps.supports_reactions);
        assert!(caps.supports_threads);
        assert_eq!(caps.streaming_type, StreamingType::EditBased);
        assert_eq!(caps.formatting_support, FormattingSupport::FullMarkdown);
        assert!(caps.rate_limit.is_some());
        assert!(caps.supports_code_blocks);
    }

    #[test]
    fn extract_channel_id_from_metadata() {
        let msg = OutboundMessage {
            session_id: None,
            channel: "discord".into(),
            content: "hello".into(),
            reply_to: None,
            parse_mode: None,
            metadata: Some(r#"{"chat_id":"123456789012345678"}"#.into()),
        };
        let id = extract_channel_id(&msg).unwrap();
        assert_eq!(id.get(), 123456789012345678);
    }

    #[test]
    fn extract_channel_id_from_channel_field() {
        let msg = OutboundMessage {
            session_id: None,
            channel: "123456789012345678".into(),
            content: "hello".into(),
            reply_to: None,
            parse_mode: None,
            metadata: None,
        };
        let id = extract_channel_id(&msg).unwrap();
        assert_eq!(id.get(), 123456789012345678);
    }

    #[test]
    fn extract_channel_id_fails_without_valid_id() {
        let msg = OutboundMessage {
            session_id: None,
            channel: "discord".into(),
            content: "hello".into(),
            reply_to: None,
            parse_mode: None,
            metadata: None,
        };
        assert!(extract_channel_id(&msg).is_err());
    }

    #[test]
    fn plugin_adapter_metadata() {
        let config = DiscordConfig {
            bot_token: Some("test-token".into()),
            application_id: None,
            allowed_users: vec![],
        };
        let channel = DiscordChannel::new(config).unwrap();
        assert_eq!(channel.name(), "discord");
        assert_eq!(channel.version(), semver::Version::new(0, 1, 0));
        assert_eq!(channel.adapter_type(), AdapterType::Channel);
    }
}
