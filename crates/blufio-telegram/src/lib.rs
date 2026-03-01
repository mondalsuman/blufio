// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Telegram channel adapter for the Blufio agent framework.
//!
//! Implements [`ChannelAdapter`] for the Telegram Bot API via teloxide,
//! providing long polling, message routing, streaming responses,
//! and MarkdownV2 formatting.

pub mod handler;
pub mod markdown;
pub mod media;
pub mod streaming;

use std::sync::Arc;

use async_trait::async_trait;
use blufio_config::model::TelegramConfig;
use blufio_core::error::BlufioError;
use blufio_core::traits::{ChannelAdapter, PluginAdapter};
use blufio_core::types::{
    AdapterType, ChannelCapabilities, HealthStatus, InboundMessage, MessageId, OutboundMessage,
};
use teloxide::prelude::*;
use teloxide::types::{ChatAction, ChatId, ParseMode, Recipient};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Telegram channel adapter implementing [`ChannelAdapter`].
///
/// Connects to Telegram via long polling, filters messages by authorization
/// and chat type, and delivers responses with edit-in-place streaming.
pub struct TelegramChannel {
    bot: Bot,
    config: TelegramConfig,
    inbound_rx: tokio::sync::Mutex<mpsc::Receiver<InboundMessage>>,
    inbound_tx: mpsc::Sender<InboundMessage>,
    polling_handle: Option<tokio::task::JoinHandle<()>>,
}

impl TelegramChannel {
    /// Creates a new Telegram channel adapter.
    ///
    /// Requires `config.bot_token` to be set.
    pub fn new(config: TelegramConfig) -> Result<Self, BlufioError> {
        let token = config.bot_token.as_deref().ok_or_else(|| {
            BlufioError::Config("telegram.bot_token is required for Telegram adapter".into())
        })?;

        if token.is_empty() {
            return Err(BlufioError::Config(
                "telegram.bot_token cannot be empty".into(),
            ));
        }

        let bot = Bot::new(token);
        let (inbound_tx, inbound_rx) = mpsc::channel(100);

        Ok(Self {
            bot,
            config,
            inbound_rx: tokio::sync::Mutex::new(inbound_rx),
            inbound_tx,
            polling_handle: None,
        })
    }

    /// Returns a reference to the underlying teloxide Bot.
    pub fn bot(&self) -> &Bot {
        &self.bot
    }
}

#[async_trait]
impl PluginAdapter for TelegramChannel {
    fn name(&self) -> &str {
        "telegram"
    }

    fn version(&self) -> semver::Version {
        semver::Version::new(0, 1, 0)
    }

    fn adapter_type(&self) -> AdapterType {
        AdapterType::Channel
    }

    async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
        // Check if the bot token is valid by calling getMe.
        match self.bot.get_me().await {
            Ok(_) => Ok(HealthStatus::Healthy),
            Err(e) => Ok(HealthStatus::Unhealthy(format!(
                "Telegram bot unreachable: {e}"
            ))),
        }
    }

    async fn shutdown(&self) -> Result<(), BlufioError> {
        debug!("Telegram channel shutting down");
        // The polling handle will be dropped when TelegramChannel is dropped,
        // which aborts the task. For graceful shutdown, the agent loop should
        // stop calling receive() first.
        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for TelegramChannel {
    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_edit: true,
            supports_typing: true,
            supports_images: true,
            supports_documents: true,
            supports_voice: true,
            max_message_length: Some(4096),
        }
    }

    async fn connect(&mut self) -> Result<(), BlufioError> {
        if self.polling_handle.is_some() {
            return Ok(()); // Already connected
        }

        let bot = self.bot.clone();
        let tx = self.inbound_tx.clone();
        let allowed_users: Arc<Vec<String>> = Arc::new(self.config.allowed_users.clone());

        info!("starting Telegram long polling");

        let handle = tokio::spawn(async move {
            let handler = Update::filter_message().endpoint(
                move |bot: Bot, msg: Message| {
                    let tx = tx.clone();
                    let allowed = allowed_users.clone();
                    async move {
                        // Filter: DMs only
                        if !handler::is_dm(&msg) {
                            debug!(chat_id = msg.chat.id.0, "ignoring non-DM message");
                            return respond(());
                        }

                        // Filter: authorized users only
                        if !handler::is_authorized(&msg, &allowed) {
                            debug!(
                                chat_id = msg.chat.id.0,
                                "ignoring unauthorized user"
                            );
                            return respond(());
                        }

                        // Extract content
                        match handler::extract_content(&bot, &msg).await {
                            Ok(Some(content)) => {
                                let inbound = handler::to_inbound_message(&msg, content);
                                if tx.send(inbound).await.is_err() {
                                    warn!("inbound channel closed, dropping message");
                                }
                            }
                            Ok(None) => {
                                debug!(
                                    msg_id = msg.id.0,
                                    "ignoring unsupported message type"
                                );
                            }
                            Err(e) => {
                                error!(error = %e, "failed to extract message content");
                            }
                        }

                        respond(())
                    }
                },
            );

            Dispatcher::builder(bot, handler)
                .default_handler(|_| async {}) // Silently ignore non-message updates
                .build()
                .dispatch()
                .await;
        });

        self.polling_handle = Some(handle);
        Ok(())
    }

    async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> {
        let chat_id = extract_chat_id(&msg)?;
        let escaped = markdown::format_for_telegram(&msg.content);

        let result = if msg.parse_mode.as_deref() == Some("MarkdownV2")
            || msg.parse_mode.is_none()
        {
            // Try MarkdownV2 first, fall back to plain text
            match self
                .bot
                .send_message(Recipient::Id(chat_id), &escaped)
                .parse_mode(ParseMode::MarkdownV2)
                .await
            {
                Ok(sent) => Ok(sent),
                Err(e) => {
                    warn!(error = %e, "MarkdownV2 failed, sending as plain text");
                    self.bot
                        .send_message(Recipient::Id(chat_id), &msg.content)
                        .await
                        .map_err(|e| BlufioError::Channel {
                            message: format!("failed to send message: {e}"),
                            source: Some(Box::new(e)),
                        })
                }
            }
        } else {
            self.bot
                .send_message(Recipient::Id(chat_id), &msg.content)
                .await
                .map_err(|e| BlufioError::Channel {
                    message: format!("failed to send message: {e}"),
                    source: Some(Box::new(e)),
                })
        }?;

        Ok(MessageId(result.id.0.to_string()))
    }

    async fn receive(&self) -> Result<InboundMessage, BlufioError> {
        let mut rx = self.inbound_rx.lock().await;
        rx.recv().await.ok_or_else(|| BlufioError::Channel {
            message: "Telegram inbound channel closed".into(),
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
        let chat_id = chat_id
            .parse::<i64>()
            .map(ChatId)
            .map_err(|e| BlufioError::Channel {
                message: format!("invalid chat_id: {e}"),
                source: None,
            })?;

        let msg_id = message_id
            .parse::<i32>()
            .map(teloxide::types::MessageId)
            .map_err(|e| BlufioError::Channel {
                message: format!("invalid message_id: {e}"),
                source: None,
            })?;

        let escaped = markdown::format_for_telegram(text);

        let use_markdown = parse_mode
            .map(|p| p == "MarkdownV2")
            .unwrap_or(true);

        if use_markdown {
            let result = self
                .bot
                .edit_message_text(chat_id, msg_id, &escaped)
                .parse_mode(ParseMode::MarkdownV2)
                .await;

            match result {
                Ok(_) => Ok(()),
                Err(e) => {
                    let err_str = e.to_string();
                    if err_str.contains("message is not modified") {
                        Ok(())
                    } else if err_str.contains("can't parse entities") {
                        warn!(error = %e, "MarkdownV2 edit failed, retrying as plain text");
                        self.bot
                            .edit_message_text(chat_id, msg_id, text)
                            .await
                            .map_err(|e| BlufioError::Channel {
                                message: format!("failed to edit message: {e}"),
                                source: Some(Box::new(e)),
                            })?;
                        Ok(())
                    } else {
                        Err(BlufioError::Channel {
                            message: format!("failed to edit message: {e}"),
                            source: Some(Box::new(e)),
                        })
                    }
                }
            }
        } else {
            self.bot
                .edit_message_text(chat_id, msg_id, text)
                .await
                .map_err(|e| BlufioError::Channel {
                    message: format!("failed to edit message: {e}"),
                    source: Some(Box::new(e)),
                })?;
            Ok(())
        }
    }

    async fn send_typing(&self, chat_id: &str) -> Result<(), BlufioError> {
        let chat_id = chat_id
            .parse::<i64>()
            .map(ChatId)
            .map_err(|e| BlufioError::Channel {
                message: format!("invalid chat_id: {e}"),
                source: None,
            })?;

        self.bot
            .send_chat_action(chat_id, ChatAction::Typing)
            .await
            .map_err(|e| BlufioError::Channel {
                message: format!("failed to send typing indicator: {e}"),
                source: Some(Box::new(e)),
            })?;

        Ok(())
    }
}

/// Extracts the chat ID from an outbound message's metadata.
fn extract_chat_id(msg: &OutboundMessage) -> Result<ChatId, BlufioError> {
    // Try to get chat_id from metadata
    if let Some(ref metadata) = msg.metadata
        && let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata)
        && let Some(chat_id_str) = meta.get("chat_id").and_then(|v| v.as_str())
    {
        let id = chat_id_str.parse::<i64>().map_err(|e| BlufioError::Channel {
            message: format!("invalid chat_id in metadata: {e}"),
            source: None,
        })?;
        return Ok(ChatId(id));
    }

    // Fallback: try channel field as chat ID
    msg.channel.parse::<i64>().map(ChatId).map_err(|_| {
        BlufioError::Channel {
            message: "no valid chat_id in message metadata or channel field".into(),
            source: None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_requires_bot_token() {
        let config = TelegramConfig {
            bot_token: None,
            allowed_users: vec![],
        };
        assert!(TelegramChannel::new(config).is_err());
    }

    #[test]
    fn new_rejects_empty_token() {
        let config = TelegramConfig {
            bot_token: Some(String::new()),
            allowed_users: vec![],
        };
        assert!(TelegramChannel::new(config).is_err());
    }

    #[test]
    fn new_accepts_valid_token() {
        let config = TelegramConfig {
            bot_token: Some("123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11".into()),
            allowed_users: vec!["user1".into()],
        };
        assert!(TelegramChannel::new(config).is_ok());
    }

    #[test]
    fn capabilities_are_correct() {
        let config = TelegramConfig {
            bot_token: Some("test:token".into()),
            allowed_users: vec![],
        };
        let channel = TelegramChannel::new(config).unwrap();
        let caps = channel.capabilities();
        assert!(caps.supports_edit);
        assert!(caps.supports_typing);
        assert!(caps.supports_images);
        assert!(caps.supports_documents);
        assert!(caps.supports_voice);
        assert_eq!(caps.max_message_length, Some(4096));
    }

    #[test]
    fn extract_chat_id_from_metadata() {
        let msg = OutboundMessage {
            session_id: None,
            channel: "telegram".into(),
            content: "hello".into(),
            reply_to: None,
            parse_mode: None,
            metadata: Some(r#"{"chat_id":"12345"}"#.into()),
        };
        let id = extract_chat_id(&msg).unwrap();
        assert_eq!(id.0, 12345);
    }

    #[test]
    fn extract_chat_id_from_channel_field() {
        let msg = OutboundMessage {
            session_id: None,
            channel: "12345".into(),
            content: "hello".into(),
            reply_to: None,
            parse_mode: None,
            metadata: None,
        };
        let id = extract_chat_id(&msg).unwrap();
        assert_eq!(id.0, 12345);
    }

    #[test]
    fn extract_chat_id_fails_without_valid_id() {
        let msg = OutboundMessage {
            session_id: None,
            channel: "telegram".into(),
            content: "hello".into(),
            reply_to: None,
            parse_mode: None,
            metadata: None,
        };
        assert!(extract_chat_id(&msg).is_err());
    }

    #[test]
    fn plugin_adapter_metadata() {
        let config = TelegramConfig {
            bot_token: Some("test:token".into()),
            allowed_users: vec![],
        };
        let channel = TelegramChannel::new(config).unwrap();
        assert_eq!(channel.name(), "telegram");
        assert_eq!(channel.version(), semver::Version::new(0, 1, 0));
        assert_eq!(channel.adapter_type(), AdapterType::Channel);
    }
}
