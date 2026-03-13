// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

//! IRC channel adapter for the Blufio agent framework.
//!
//! Connects to an IRC server with optional TLS, authenticates via SASL PLAIN
//! or NickServ IDENTIFY, and provides flood-protected message delivery with
//! word-boundary splitting for IRC line length limits.

pub mod flood;
pub mod sasl;
pub mod splitter;

use std::sync::Arc;

use async_trait::async_trait;
use blufio_config::model::IrcConfig;
use blufio_core::error::{BlufioError, ChannelErrorKind, ErrorContext};
use blufio_core::format::{FormatPipeline, split_at_paragraphs};
use blufio_core::traits::{ChannelAdapter, PluginAdapter};
use blufio_core::types::{
    AdapterType, ChannelCapabilities, FormattingSupport, HealthStatus, InboundMessage,
    MessageContent, MessageId, OutboundMessage, RateLimit, StreamingType,
};
use irc::client::Client;
use irc::client::prelude::Config as IrcClientConfig;
use irc::proto::{Command, Response};
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::flood::FloodProtectedSender;

/// IRC channel adapter implementing [`ChannelAdapter`].
///
/// Connects to an IRC server, joins configured channels, and handles messages
/// with flood protection and word-boundary splitting.
pub struct IrcChannel {
    config: IrcConfig,
    inbound_rx: Mutex<mpsc::Receiver<InboundMessage>>,
    inbound_tx: mpsc::Sender<InboundMessage>,
    health: Arc<Mutex<HealthStatus>>,
    flood_sender: Mutex<Option<FloodProtectedSender>>,
    receive_handle: Option<JoinHandle<()>>,
}

impl IrcChannel {
    /// Creates a new IRC channel adapter.
    ///
    /// Validates that `server` and `nickname` are configured.
    pub fn new(config: IrcConfig) -> Result<Self, BlufioError> {
        if config.server.is_none() {
            return Err(BlufioError::Config("irc: server must be configured".into()));
        }
        if config.nickname.is_none() {
            return Err(BlufioError::Config(
                "irc: nickname must be configured".into(),
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
            flood_sender: Mutex::new(None),
            receive_handle: None,
        })
    }
}

#[async_trait]
impl PluginAdapter for IrcChannel {
    fn name(&self) -> &str {
        "irc"
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
        debug!("IRC channel shutting down");
        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for IrcChannel {
    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_edit: false,
            supports_typing: false,
            supports_images: false,
            supports_documents: false,
            supports_voice: false,
            max_message_length: Some(450), // Conservative for PRIVMSG payloads.
            supports_embeds: false,
            supports_reactions: false,
            supports_threads: false,
            streaming_type: StreamingType::AppendOnly,
            formatting_support: FormattingSupport::PlainText,
            rate_limit: Some(RateLimit {
                messages_per_second: Some(2.0),
                burst_limit: Some(5),
                daily_limit: None,
            }),
            supports_code_blocks: false,
        }
    }

    async fn connect(&mut self) -> Result<(), BlufioError> {
        let server = self.config.server.clone().unwrap_or_default();
        let nickname = self.config.nickname.clone().unwrap_or_default();
        let tls = self.config.tls;
        let port = self.config.port.unwrap_or(if tls { 6697 } else { 6667 });

        // Build IRC client config.
        let mut irc_config = IrcClientConfig {
            nickname: Some(nickname.clone()),
            server: Some(server.clone()),
            port: Some(port),
            channels: self.config.channels.clone(),
            use_tls: Some(tls),
            ..Default::default()
        };

        // NickServ auth: set nick_password and should_ghost.
        if self.config.auth_method.as_deref() == Some("nickserv")
            && let Some(ref password) = self.config.password
        {
            irc_config.nick_password = Some(password.clone());
            irc_config.should_ghost = true;
        }

        // Create IRC client.
        let mut client = Client::from_config(irc_config)
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("irc", e))?;

        // SASL auth: request capability before identify.
        if self.config.auth_method.as_deref() == Some("sasl") && self.config.password.is_some() {
            sasl::request_sasl_cap(&client).await?;
            debug!("SASL CAP REQ sent, flow continues in message stream");
        }

        // Identify (complete connection registration).
        client
            .identify()
            .map_err(|e| BlufioError::channel_delivery_failed("irc", e))?;

        // Get the message stream BEFORE wrapping in Arc (requires &mut self).
        let stream = client
            .stream()
            .map_err(|e| BlufioError::channel_delivery_failed("irc", e))?;

        // Now wrap in Arc for shared access (send_privmsg uses &self).
        let client = Arc::new(client);

        // Create flood-protected sender.
        let flood = FloodProtectedSender::new(
            Arc::clone(&client),
            self.config.rate_limit_ms,
            nickname.clone(),
        );
        {
            let mut fs = self.flood_sender.lock().await;
            *fs = Some(flood);
        }

        // Update health.
        {
            let mut h = self.health.lock().await;
            *h = HealthStatus::Healthy;
        }

        info!(server = %server, port = port, tls = tls, "connected to IRC server");

        // Spawn message receive loop.
        let inbound_tx = self.inbound_tx.clone();
        let health = Arc::clone(&self.health);
        let bot_nick = nickname.clone();
        let allowed_users = self.config.allowed_users.clone();
        let auth_method = self.config.auth_method.clone();
        let password = self.config.password.clone();

        let handle = tokio::spawn(async move {
            let mut stream = stream;

            use futures::StreamExt;

            let mut sasl_authenticated = false;

            while let Some(result) = stream.next().await {
                match result {
                    Ok(message) => {
                        // Handle SASL flow messages.
                        if auth_method.as_deref() == Some("sasl") && !sasl_authenticated {
                            match &message.command {
                                Command::CAP(_, sub, _, _)
                                    if format!("{sub:?}").contains("ACK") =>
                                {
                                    // CAP ACK received, send AUTHENTICATE PLAIN.
                                    debug!("received CAP ACK for SASL");
                                    if let Err(e) = client.send(Command::Raw(
                                        "AUTHENTICATE".into(),
                                        vec!["PLAIN".into()],
                                    )) {
                                        warn!(error = %e, "failed to send AUTHENTICATE PLAIN");
                                    }
                                    continue;
                                }
                                Command::Raw(cmd, _) if cmd == "AUTHENTICATE" => {
                                    // Server sent AUTHENTICATE +, send credentials.
                                    if let Some(ref pw) = password {
                                        let encoded = sasl::encode_sasl_plain(&bot_nick, pw);
                                        if let Err(e) = client.send(Command::Raw(
                                            "AUTHENTICATE".into(),
                                            vec![encoded],
                                        )) {
                                            warn!(error = %e, "failed to send SASL credentials");
                                        }
                                    }
                                    continue;
                                }
                                Command::Response(Response::RPL_SASLSUCCESS, _) => {
                                    info!("SASL authentication successful");
                                    sasl_authenticated = true;
                                    if let Err(e) = sasl::finish_cap(&client).await {
                                        warn!(error = %e, "failed to send CAP END after SASL");
                                    }
                                    continue;
                                }
                                Command::Response(Response::ERR_SASLFAIL, _) => {
                                    error!("SASL authentication failed");
                                    if let Err(e) = sasl::finish_cap(&client).await {
                                        warn!(error = %e, "failed to send CAP END after SASL failure");
                                    }
                                    continue;
                                }
                                _ => {}
                            }
                        }

                        // Handle PRIVMSG.
                        if let Command::PRIVMSG(ref target, ref text) = message.command {
                            let sender_nick =
                                message.source_nickname().unwrap_or("unknown").to_string();

                            // Skip messages from self.
                            if sender_nick == bot_nick {
                                continue;
                            }

                            // Check allowed users filter.
                            if !allowed_users.is_empty() && !allowed_users.contains(&sender_nick) {
                                debug!(
                                    sender = %sender_nick,
                                    "skipping IRC message from non-allowed user"
                                );
                                continue;
                            }

                            let is_dm = target == &bot_nick;
                            let mut message_text = text.clone();

                            // In channels, only respond to @mention.
                            if !is_dm {
                                let mention_pattern = format!("{}:", bot_nick);
                                let mention_pattern_at = format!("@{}", bot_nick);

                                if message_text.starts_with(&mention_pattern) {
                                    message_text =
                                        message_text[mention_pattern.len()..].trim().to_string();
                                } else if message_text.starts_with(&mention_pattern_at) {
                                    message_text =
                                        message_text[mention_pattern_at.len()..].trim().to_string();
                                } else if message_text.contains(&mention_pattern_at) {
                                    message_text = message_text
                                        .replace(&mention_pattern_at, "")
                                        .trim()
                                        .to_string();
                                } else {
                                    // No mention in channel message, skip.
                                    continue;
                                }
                            }

                            let chat_id = if is_dm {
                                sender_nick.clone()
                            } else {
                                target.clone()
                            };

                            let metadata = serde_json::json!({
                                "chat_id": chat_id,
                                "is_dm": is_dm,
                            });

                            let inbound = InboundMessage {
                                id: uuid::Uuid::new_v4().to_string(),
                                session_id: None,
                                channel: "irc".to_string(),
                                sender_id: sender_nick,
                                content: MessageContent::Text(message_text),
                                metadata: Some(metadata.to_string()),
                                timestamp: chrono::Utc::now().to_rfc3339(),
                            };

                            if inbound_tx.send(inbound).await.is_err() {
                                warn!("IRC inbound channel closed");
                                return;
                            }
                        }

                        // Handle PING (auto-handled by irc crate, but log it).
                        if let Command::PING(ref data, _) = message.command {
                            debug!(data = %data, "received PING from IRC server");
                        }

                        // Handle numeric errors.
                        if let Command::Response(ref resp, ref args) = message.command {
                            let code = *resp as u16;
                            if (400..600).contains(&code) {
                                warn!(
                                    code = code,
                                    args = ?args,
                                    "IRC server error response"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "error reading IRC message stream");
                        let mut h = health.lock().await;
                        *h = HealthStatus::Unhealthy(format!("stream error: {e}"));
                        return;
                    }
                }
            }

            // Stream ended.
            warn!("IRC message stream ended");
            let mut h = health.lock().await;
            *h = HealthStatus::Unhealthy("disconnected".to_string());
        });

        self.receive_handle = Some(handle);
        Ok(())
    }

    async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> {
        let fs = self.flood_sender.lock().await;
        let sender = fs
            .as_ref()
            .ok_or_else(|| BlufioError::channel_connection_lost("irc"))?;

        // Extract target from metadata.
        let target = if let Some(ref metadata) = msg.metadata
            && let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata)
        {
            meta.get("chat_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        } else {
            msg.channel.clone()
        };

        if target.is_empty() {
            return Err(BlufioError::Channel {
                kind: ChannelErrorKind::DeliveryFailed,
                context: ErrorContext {
                    channel_name: Some("irc".to_string()),
                    ..Default::default()
                },
                source: None,
            });
        }

        let caps = self.capabilities();

        // Pipeline: detect_and_format -> no escape (PlainText) -> split at paragraphs -> send
        // Two-level split: split_at_paragraphs for paragraph-level, then FloodProtectedSender
        // handles PRIVMSG line-level splitting via splitter.rs
        let formatted = FormatPipeline::detect_and_format(&msg.content, &caps);
        let chunks = split_at_paragraphs(&formatted, caps.max_message_length);

        for chunk in &chunks {
            sender.send(&target, chunk).await?;
        }

        Ok(MessageId(uuid::Uuid::new_v4().to_string()))
    }

    async fn receive(&self) -> Result<InboundMessage, BlufioError> {
        let mut rx = self.inbound_rx.lock().await;
        rx.recv()
            .await
            .ok_or_else(|| BlufioError::channel_connection_lost("irc"))
    }

    async fn edit_message(
        &self,
        _chat_id: &str,
        _message_id: &str,
        _text: &str,
        _parse_mode: Option<&str>,
    ) -> Result<(), BlufioError> {
        // IRC does not support message editing.
        Ok(())
    }

    async fn send_typing(&self, _chat_id: &str) -> Result<(), BlufioError> {
        // IRC does not support typing indicators.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_requires_server() {
        let config = IrcConfig {
            nickname: Some("bot".into()),
            ..Default::default()
        };
        assert!(IrcChannel::new(config).is_err());
    }

    #[test]
    fn new_requires_nickname() {
        let config = IrcConfig {
            server: Some("irc.example.com".into()),
            ..Default::default()
        };
        assert!(IrcChannel::new(config).is_err());
    }

    #[test]
    fn new_accepts_valid_config() {
        let config = IrcConfig {
            server: Some("irc.example.com".into()),
            nickname: Some("bot".into()),
            ..Default::default()
        };
        assert!(IrcChannel::new(config).is_ok());
    }

    #[test]
    fn capabilities_correct() {
        let config = IrcConfig {
            server: Some("irc.example.com".into()),
            nickname: Some("bot".into()),
            ..Default::default()
        };
        let channel = IrcChannel::new(config).unwrap();
        let caps = channel.capabilities();
        assert!(!caps.supports_edit);
        assert!(!caps.supports_typing);
        assert!(!caps.supports_images);
        assert_eq!(caps.max_message_length, Some(450));
    }

    #[test]
    fn plugin_metadata() {
        let config = IrcConfig {
            server: Some("irc.example.com".into()),
            nickname: Some("bot".into()),
            ..Default::default()
        };
        let channel = IrcChannel::new(config).unwrap();
        assert_eq!(channel.name(), "irc");
        assert_eq!(channel.version(), semver::Version::new(0, 1, 0));
        assert_eq!(channel.adapter_type(), AdapterType::Channel);
    }

    #[test]
    fn default_tls_is_true() {
        let config = IrcConfig::default();
        assert!(config.tls);
    }

    #[test]
    fn default_rate_limit_is_2000() {
        let config = IrcConfig::default();
        assert_eq!(config.rate_limit_ms, 2000);
    }
}
