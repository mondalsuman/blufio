// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Slack channel adapter for the Blufio agent framework.
//!
//! Implements [`ChannelAdapter`] for the Slack API via slack-morphism,
//! providing Socket Mode WebSocket connection, @mention detection,
//! slash commands, Block Kit formatting, and streaming responses.

pub mod blocks;
pub mod commands;
pub mod handler;
pub mod markdown;
pub mod streaming;

use std::sync::Arc;

use async_trait::async_trait;
use blufio_config::model::SlackConfig;
use blufio_core::error::BlufioError;
use blufio_core::traits::{ChannelAdapter, PluginAdapter};
use blufio_core::types::{
    AdapterType, ChannelCapabilities, HealthStatus, InboundMessage, MessageId, OutboundMessage,
};
use slack_morphism::prelude::*;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Shared state passed to Socket Mode callbacks via user_state.
pub(crate) struct SlackHandlerState {
    pub inbound_tx: mpsc::Sender<InboundMessage>,
    pub bot_user_id: String,
    pub allowed_users: Vec<String>,
}

/// Slack channel adapter implementing [`ChannelAdapter`].
///
/// Connects to Slack via Socket Mode WebSocket using slack-morphism,
/// filters messages by authorization and mention detection, and delivers
/// responses with edit-in-place streaming.
pub struct SlackChannel {
    config: SlackConfig,
    inbound_rx: tokio::sync::Mutex<mpsc::Receiver<InboundMessage>>,
    inbound_tx: mpsc::Sender<InboundMessage>,
    bot_user_id: Option<String>,
    client: Option<Arc<SlackClient<SlackClientHyperHttpsConnector>>>,
    bot_token: Option<SlackApiToken>,
    socket_handle: Option<tokio::task::JoinHandle<()>>,
}

impl SlackChannel {
    /// Creates a new Slack channel adapter.
    ///
    /// Requires both `config.bot_token` and `config.app_token` to be set.
    pub fn new(config: SlackConfig) -> Result<Self, BlufioError> {
        let bot_token = config.bot_token.as_deref().ok_or_else(|| {
            BlufioError::Config("slack.bot_token is required for Slack adapter".into())
        })?;

        if bot_token.is_empty() {
            return Err(BlufioError::Config(
                "slack.bot_token cannot be empty".into(),
            ));
        }

        let app_token = config.app_token.as_deref().ok_or_else(|| {
            BlufioError::Config("slack.app_token is required for Slack Socket Mode".into())
        })?;

        if app_token.is_empty() {
            return Err(BlufioError::Config(
                "slack.app_token cannot be empty".into(),
            ));
        }

        let (inbound_tx, inbound_rx) = mpsc::channel(100);

        Ok(Self {
            config,
            inbound_rx: tokio::sync::Mutex::new(inbound_rx),
            inbound_tx,
            bot_user_id: None,
            client: None,
            bot_token: None,
            socket_handle: None,
        })
    }
}

#[async_trait]
impl PluginAdapter for SlackChannel {
    fn name(&self) -> &str {
        "slack"
    }

    fn version(&self) -> semver::Version {
        semver::Version::new(0, 1, 0)
    }

    fn adapter_type(&self) -> AdapterType {
        AdapterType::Channel
    }

    async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
        match (&self.client, &self.bot_token) {
            (Some(client), Some(token)) => {
                let session = client.open_session(token);
                match session.auth_test().await {
                    Ok(_) => Ok(HealthStatus::Healthy),
                    Err(e) => Ok(HealthStatus::Unhealthy(format!(
                        "Slack API unreachable: {e}"
                    ))),
                }
            }
            _ => Ok(HealthStatus::Unhealthy("Slack not connected".to_string())),
        }
    }

    async fn shutdown(&self) -> Result<(), BlufioError> {
        debug!("Slack channel shutting down");
        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for SlackChannel {
    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_edit: true,
            supports_typing: true,
            supports_images: true,
            supports_documents: true,
            supports_voice: false,
            max_message_length: Some(40000),
            supports_embeds: true,
            supports_reactions: true,
            supports_threads: true,
        }
    }

    async fn connect(&mut self) -> Result<(), BlufioError> {
        if self.socket_handle.is_some() {
            return Ok(()); // Already connected
        }

        let bot_token_str = self
            .config
            .bot_token
            .as_deref()
            .ok_or_else(|| BlufioError::Config("slack.bot_token is required".into()))?
            .to_string();

        let app_token_str = self
            .config
            .app_token
            .as_deref()
            .ok_or_else(|| BlufioError::Config("slack.app_token is required".into()))?
            .to_string();

        // Create Slack client.
        let client = Arc::new(SlackClient::new(SlackClientHyperConnector::new().map_err(
            |e| BlufioError::Channel {
                message: format!("failed to create Slack HTTP connector: {e}"),
                source: None,
            },
        )?));

        let token = SlackApiToken::new(SlackApiTokenValue::from(bot_token_str));

        // Discover bot user ID via auth.test.
        {
            let session = client.open_session(&token);
            match session.auth_test().await {
                Ok(resp) => {
                    let user_id = resp.user_id.to_string();
                    info!(bot_user_id = %user_id, "Slack auth.test succeeded");
                    self.bot_user_id = Some(user_id);
                }
                Err(e) => {
                    return Err(BlufioError::Channel {
                        message: format!("Slack auth.test failed: {e}"),
                        source: None,
                    });
                }
            }
        }

        self.client = Some(client.clone());
        self.bot_token = Some(token.clone());

        // Set up Socket Mode.
        let app_token = SlackApiToken::new(SlackApiTokenValue::from(app_token_str));

        let handler_state = SlackHandlerState {
            inbound_tx: self.inbound_tx.clone(),
            bot_user_id: self.bot_user_id.clone().unwrap_or_default(),
            allowed_users: self.config.allowed_users.clone(),
        };

        info!("starting Slack Socket Mode connection");

        let socket_mode_callbacks = SlackSocketModeListenerCallbacks::new()
            .with_push_events(push_events_callback)
            .with_command_events(command_events_callback);

        let listener_environment = Arc::new(
            SlackClientEventsListenerEnvironment::new(client)
                .with_error_handler(error_handler)
                .with_user_state(handler_state),
        );

        let socket_mode_listener = SlackClientSocketModeListener::new(
            &SlackClientSocketModeConfig::new(),
            listener_environment,
            socket_mode_callbacks,
        );

        // Register token and start listening.
        socket_mode_listener
            .listen_for(&app_token)
            .await
            .map_err(|e| BlufioError::Channel {
                message: format!("failed to start Slack Socket Mode: {e}"),
                source: None,
            })?;

        let handle = tokio::spawn(async move {
            socket_mode_listener.serve().await;
        });

        self.socket_handle = Some(handle);
        Ok(())
    }

    async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> {
        let client = self.client.as_ref().ok_or_else(|| BlufioError::Channel {
            message: "Slack not connected".into(),
            source: None,
        })?;
        let token = self
            .bot_token
            .as_ref()
            .ok_or_else(|| BlufioError::Channel {
                message: "Slack not connected".into(),
                source: None,
            })?;

        let channel_id = extract_channel_id(&msg)?;
        let formatted = markdown::markdown_to_mrkdwn(&msg.content);

        let session = client.open_session(token);
        let req = SlackApiChatPostMessageRequest::new(
            channel_id,
            SlackMessageContent::new().with_text(formatted),
        );

        let resp = session
            .chat_post_message(&req)
            .await
            .map_err(|e| BlufioError::Channel {
                message: format!("failed to send Slack message: {e}"),
                source: None,
            })?;

        Ok(MessageId(resp.ts.to_string()))
    }

    async fn receive(&self) -> Result<InboundMessage, BlufioError> {
        let mut rx = self.inbound_rx.lock().await;
        rx.recv().await.ok_or_else(|| BlufioError::Channel {
            message: "Slack inbound channel closed".into(),
            source: None,
        })
    }

    async fn edit_message(
        &self,
        chat_id: &str,
        message_id: &str,
        text: &str,
        _parse_mode: Option<&str>,
    ) -> Result<(), BlufioError> {
        let client = self.client.as_ref().ok_or_else(|| BlufioError::Channel {
            message: "Slack not connected".into(),
            source: None,
        })?;
        let token = self
            .bot_token
            .as_ref()
            .ok_or_else(|| BlufioError::Channel {
                message: "Slack not connected".into(),
                source: None,
            })?;

        let channel_id = SlackChannelId::new(chat_id.to_string());
        let ts: SlackTs = message_id.to_string().into();
        let formatted = markdown::markdown_to_mrkdwn(text);

        let session = client.open_session(token);
        let req = SlackApiChatUpdateRequest::new(
            channel_id,
            SlackMessageContent::new().with_text(formatted),
            ts,
        );

        session
            .chat_update(&req)
            .await
            .map_err(|e| BlufioError::Channel {
                message: format!("failed to edit Slack message: {e}"),
                source: None,
            })?;

        Ok(())
    }

    async fn send_typing(&self, _chat_id: &str) -> Result<(), BlufioError> {
        // Slack does not have a direct typing indicator API.
        // Socket Mode responses show typing automatically within a short window.
        Ok(())
    }
}

/// Socket Mode push events callback (fn pointer, not closure).
///
/// Accesses shared state via the user_state mechanism.
async fn push_events_callback(
    event: SlackPushEventCallback,
    _client: Arc<SlackClient<SlackClientHyperHttpsConnector>>,
    states: SlackClientEventsUserState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let states_r = states.read().await;
    let state = states_r
        .get_user_state::<SlackHandlerState>()
        .expect("SlackHandlerState must be registered");

    // SlackPushEventCallback is a struct with event: SlackEventCallbackBody
    if let SlackEventCallbackBody::Message(ref msg_event) = event.event {
        // Skip bot messages.
        if msg_event.sender.bot_id.is_some() {
            return Ok(());
        }

        let user_id = match &msg_event.sender.user {
            Some(uid) => uid.to_string(),
            None => return Ok(()),
        };

        let text = match &msg_event.content {
            Some(content) => match &content.text {
                Some(t) => t.clone(),
                None => return Ok(()),
            },
            None => return Ok(()),
        };

        let channel_id = match &msg_event.origin.channel {
            Some(ch) => ch.to_string(),
            None => return Ok(()),
        };

        let channel_type = match &msg_event.origin.channel_type {
            Some(ct) => ct.to_string(),
            None => "channel".to_string(),
        };

        // Check if we should respond.
        if !handler::should_respond(&text, &channel_type, &state.bot_user_id) {
            return Ok(());
        }

        // Check authorization.
        if !handler::is_authorized(&user_id, &state.allowed_users) {
            debug!(user_id = %user_id, "ignoring unauthorized Slack user");
            return Ok(());
        }

        // Strip @mention.
        let content = handler::strip_mention(&text, &state.bot_user_id);
        if content.is_empty() {
            return Ok(());
        }

        let ts = msg_event.origin.ts.to_string();
        let team_id = event.team_id.to_string();

        let inbound =
            handler::to_inbound_message(&ts, &user_id, &channel_id, Some(&team_id), content);

        if state.inbound_tx.send(inbound).await.is_err() {
            warn!("inbound channel closed, dropping Slack message");
        }
    }

    Ok(())
}

/// Socket Mode command events callback (fn pointer, not closure).
async fn command_events_callback(
    event: SlackCommandEvent,
    _client: Arc<SlackClient<SlackClientHyperHttpsConnector>>,
    states: SlackClientEventsUserState,
) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>> {
    let states_r = states.read().await;
    let state = states_r
        .get_user_state::<SlackHandlerState>()
        .expect("SlackHandlerState must be registered");

    let user_id = event.user_id.to_string();
    let channel_id = event.channel_id.to_string();
    let text = event.text.clone().unwrap_or_default();

    let response =
        commands::handle_slash_command(&text, &user_id, &channel_id, &state.inbound_tx).await;

    match response {
        commands::SlashCommandResponse::Blocks(_blocks) => {
            debug!("slash command responded with blocks");
            // Return a simple text response for now -- Block Kit responses
            // are sent via follow-up message, not the ack.
            Ok(SlackCommandEventResponse::new(
                SlackMessageContent::new().with_text("Processing...".to_string()),
            ))
        }
        commands::SlashCommandResponse::Text(t) => Ok(SlackCommandEventResponse::new(
            SlackMessageContent::new().with_text(t),
        )),
    }
}

/// Error handler for Socket Mode errors (fn pointer, returns StatusCode).
fn error_handler(
    err: Box<dyn std::error::Error + Send + Sync>,
    _client: Arc<SlackClient<SlackClientHyperHttpsConnector>>,
    _states: SlackClientEventsUserState,
) -> http::StatusCode {
    warn!(error = %err, "Slack Socket Mode error");
    http::StatusCode::OK
}

/// Extracts the channel ID from an outbound message's metadata.
fn extract_channel_id(msg: &OutboundMessage) -> Result<SlackChannelId, BlufioError> {
    // Try to get chat_id from metadata.
    if let Some(ref metadata) = msg.metadata
        && let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata)
        && let Some(chat_id_str) = meta.get("chat_id").and_then(|v| v.as_str())
    {
        return Ok(SlackChannelId::new(chat_id_str.to_string()));
    }

    // Fallback: try channel field.
    if msg.channel.starts_with('C') || msg.channel.starts_with('D') || msg.channel.starts_with('G')
    {
        return Ok(SlackChannelId::new(msg.channel.clone()));
    }

    Err(BlufioError::Channel {
        message: "no valid chat_id in message metadata or channel field".into(),
        source: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_requires_bot_token() {
        let config = SlackConfig {
            bot_token: None,
            app_token: Some("xapp-test".into()),
            allowed_users: vec![],
        };
        assert!(SlackChannel::new(config).is_err());
    }

    #[test]
    fn new_requires_app_token() {
        let config = SlackConfig {
            bot_token: Some("xoxb-test".into()),
            app_token: None,
            allowed_users: vec![],
        };
        assert!(SlackChannel::new(config).is_err());
    }

    #[test]
    fn new_rejects_empty_bot_token() {
        let config = SlackConfig {
            bot_token: Some(String::new()),
            app_token: Some("xapp-test".into()),
            allowed_users: vec![],
        };
        assert!(SlackChannel::new(config).is_err());
    }

    #[test]
    fn new_rejects_empty_app_token() {
        let config = SlackConfig {
            bot_token: Some("xoxb-test".into()),
            app_token: Some(String::new()),
            allowed_users: vec![],
        };
        assert!(SlackChannel::new(config).is_err());
    }

    #[test]
    fn new_accepts_valid_tokens() {
        let config = SlackConfig {
            bot_token: Some("xoxb-test-token".into()),
            app_token: Some("xapp-test-token".into()),
            allowed_users: vec!["U123".into()],
        };
        assert!(SlackChannel::new(config).is_ok());
    }

    #[test]
    fn capabilities_are_correct() {
        let config = SlackConfig {
            bot_token: Some("xoxb-test".into()),
            app_token: Some("xapp-test".into()),
            allowed_users: vec![],
        };
        let channel = SlackChannel::new(config).unwrap();
        let caps = channel.capabilities();
        assert!(caps.supports_edit);
        assert!(caps.supports_typing);
        assert!(caps.supports_images);
        assert!(caps.supports_documents);
        assert!(!caps.supports_voice);
        assert_eq!(caps.max_message_length, Some(40000));
        assert!(caps.supports_embeds);
        assert!(caps.supports_reactions);
        assert!(caps.supports_threads);
    }

    #[test]
    fn extract_channel_id_from_metadata() {
        let msg = OutboundMessage {
            session_id: None,
            channel: "slack".into(),
            content: "hello".into(),
            reply_to: None,
            parse_mode: None,
            metadata: Some(r#"{"chat_id":"C123456789"}"#.into()),
        };
        let id = extract_channel_id(&msg).unwrap();
        assert_eq!(id.to_string(), "C123456789");
    }

    #[test]
    fn extract_channel_id_from_channel_field() {
        let msg = OutboundMessage {
            session_id: None,
            channel: "C123456789".into(),
            content: "hello".into(),
            reply_to: None,
            parse_mode: None,
            metadata: None,
        };
        let id = extract_channel_id(&msg).unwrap();
        assert_eq!(id.to_string(), "C123456789");
    }

    #[test]
    fn extract_channel_id_dm() {
        let msg = OutboundMessage {
            session_id: None,
            channel: "D123456789".into(),
            content: "hello".into(),
            reply_to: None,
            parse_mode: None,
            metadata: None,
        };
        let id = extract_channel_id(&msg).unwrap();
        assert_eq!(id.to_string(), "D123456789");
    }

    #[test]
    fn extract_channel_id_fails_without_valid_id() {
        let msg = OutboundMessage {
            session_id: None,
            channel: "slack".into(),
            content: "hello".into(),
            reply_to: None,
            parse_mode: None,
            metadata: None,
        };
        assert!(extract_channel_id(&msg).is_err());
    }

    #[test]
    fn plugin_adapter_metadata() {
        let config = SlackConfig {
            bot_token: Some("xoxb-test".into()),
            app_token: Some("xapp-test".into()),
            allowed_users: vec![],
        };
        let channel = SlackChannel::new(config).unwrap();
        assert_eq!(channel.name(), "slack");
        assert_eq!(channel.version(), semver::Version::new(0, 1, 0));
        assert_eq!(channel.adapter_type(), AdapterType::Channel);
    }
}
