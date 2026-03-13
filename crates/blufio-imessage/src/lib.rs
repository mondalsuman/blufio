// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

//! iMessage channel adapter for the Blufio agent framework.
//!
//! Integrates with BlueBubbles server running on macOS for sending and
//! receiving iMessage conversations. Experimental -- requires macOS host
//! with BlueBubbles installed.

pub mod api;
pub mod types;
pub mod webhook;

use api::BlueBubblesClient;
use async_trait::async_trait;
use blufio_config::model::IMessageConfig;
use blufio_core::error::BlufioError;
use blufio_core::format::{FormatPipeline, split_at_paragraphs};
use blufio_core::traits::{ChannelAdapter, PluginAdapter};
use blufio_core::types::{
    AdapterType, ChannelCapabilities, FormattingSupport, HealthStatus, InboundMessage, MessageId,
    OutboundMessage, StreamingType,
};
use tokio::sync::{Mutex, mpsc};
use tracing::warn;

/// iMessage channel adapter backed by the BlueBubbles REST API.
///
/// Receives messages via webhook handlers and sends responses through the
/// BlueBubbles HTTP API. The webhook handlers push [`InboundMessage`]s through
/// the `inbound_tx` channel.
pub struct IMessageChannel {
    config: IMessageConfig,
    inbound_rx: Mutex<mpsc::Receiver<InboundMessage>>,
    inbound_tx: mpsc::Sender<InboundMessage>,
    api_client: Mutex<Option<BlueBubblesClient>>,
}

impl IMessageChannel {
    /// Creates a new iMessage channel adapter.
    ///
    /// Validates that `bluebubbles_url` and `api_password` are set.
    /// Logs an experimental-status warning at construction time.
    pub fn new(config: IMessageConfig) -> Result<Self, BlufioError> {
        // Validate required fields.
        let url = config.bluebubbles_url.as_deref().ok_or_else(|| {
            BlufioError::Config("imessage.bluebubbles_url is required for iMessage adapter".into())
        })?;

        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(BlufioError::Config(format!(
                "imessage.bluebubbles_url must start with http:// or https://, got: {url}"
            )));
        }

        if config.api_password.is_none() {
            return Err(BlufioError::Config(
                "imessage.api_password is required for iMessage adapter".into(),
            ));
        }

        warn!("iMessage adapter is experimental: requires macOS host with BlueBubbles server");

        let (inbound_tx, inbound_rx) = mpsc::channel(100);

        Ok(Self {
            config,
            inbound_rx: Mutex::new(inbound_rx),
            inbound_tx,
            api_client: Mutex::new(None),
        })
    }

    /// Returns a sender handle for webhook handlers to forward inbound messages.
    pub fn inbound_tx(&self) -> mpsc::Sender<InboundMessage> {
        self.inbound_tx.clone()
    }
}

#[async_trait]
impl PluginAdapter for IMessageChannel {
    fn name(&self) -> &str {
        "imessage"
    }

    fn version(&self) -> semver::Version {
        semver::Version::new(0, 1, 0)
    }

    fn adapter_type(&self) -> AdapterType {
        AdapterType::Channel
    }

    async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
        let guard = self.api_client.lock().await;
        match guard.as_ref() {
            Some(client) => match client.server_info().await {
                Ok(_) => Ok(HealthStatus::Healthy),
                Err(e) => Ok(HealthStatus::Unhealthy(format!(
                    "BlueBubbles health check failed: {e}"
                ))),
            },
            None => Ok(HealthStatus::Unhealthy(
                "iMessage not connected".to_string(),
            )),
        }
    }

    async fn shutdown(&self) -> Result<(), BlufioError> {
        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for IMessageChannel {
    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_edit: false,
            supports_typing: true,
            supports_images: false,
            supports_documents: false,
            supports_voice: false,
            max_message_length: Some(20000),
            supports_embeds: false,
            supports_reactions: false,
            supports_threads: false,
            streaming_type: StreamingType::None,
            formatting_support: FormattingSupport::PlainText,
            rate_limit: None,
            supports_code_blocks: false,
            supports_interactive: true,
        }
    }

    async fn connect(&mut self) -> Result<(), BlufioError> {
        let base_url = self
            .config
            .bluebubbles_url
            .as_deref()
            .ok_or_else(|| BlufioError::channel_connection_lost("imessage"))?;

        let password = self
            .config
            .api_password
            .as_deref()
            .ok_or_else(|| BlufioError::channel_connection_lost("imessage"))?;

        let client = BlueBubblesClient::new(base_url, password);

        // Verify connectivity by fetching server info.
        client.server_info().await?;

        // Register webhook if callback URL is set.
        if let Some(ref callback_url) = self.config.webhook_callback_url {
            client.register_webhook(callback_url).await?;
        }

        *self.api_client.lock().await = Some(client);
        Ok(())
    }

    async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> {
        let guard = self.api_client.lock().await;
        let client = guard
            .as_ref()
            .ok_or_else(|| BlufioError::channel_connection_lost("imessage"))?;

        // Extract chat_guid from metadata.
        let chat_guid = if let Some(ref metadata) = msg.metadata
            && let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata)
            && let Some(cg) = meta.get("chat_guid").and_then(|v| v.as_str())
        {
            cg.to_string()
        } else {
            return Err(BlufioError::Config(
                "iMessage send requires chat_guid in metadata".into(),
            ));
        };

        let caps = self.capabilities();

        // FormatPipeline: PlainText strips markdown.
        let formatted = FormatPipeline::detect_and_format(&msg.content, &caps);
        let chunks = split_at_paragraphs(&formatted, caps.max_message_length);

        let mut first_id = None;

        for chunk in &chunks {
            let message_id = client.send_message(&chat_guid, chunk).await?;
            if first_id.is_none() {
                first_id = Some(MessageId(message_id));
            }
        }

        // Send read receipt (best-effort).
        let _ = client.send_read_receipt(&chat_guid).await;

        Ok(first_id.unwrap_or_else(|| MessageId(String::new())))
    }

    async fn receive(&self) -> Result<InboundMessage, BlufioError> {
        let mut rx = self.inbound_rx.lock().await;
        rx.recv()
            .await
            .ok_or_else(|| BlufioError::channel_connection_lost("imessage"))
    }

    async fn send_typing(&self, chat_id: &str) -> Result<(), BlufioError> {
        let guard = self.api_client.lock().await;
        if let Some(client) = guard.as_ref() {
            let _ = client.send_typing(chat_id).await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_valid_config() -> IMessageConfig {
        IMessageConfig {
            bluebubbles_url: Some("https://localhost:1234".into()),
            api_password: Some("test-password".into()),
            webhook_callback_url: None,
            webhook_secret: None,
            group_trigger: None,
            allowed_contacts: vec![],
        }
    }

    #[test]
    fn test_new_rejects_missing_url() {
        let mut config = make_valid_config();
        config.bluebubbles_url = None;
        assert!(IMessageChannel::new(config).is_err());
    }

    #[test]
    fn test_new_rejects_missing_password() {
        let mut config = make_valid_config();
        config.api_password = None;
        assert!(IMessageChannel::new(config).is_err());
    }

    #[test]
    fn test_new_rejects_invalid_url() {
        let mut config = make_valid_config();
        config.bluebubbles_url = Some("ftp://bad-scheme".into());
        assert!(IMessageChannel::new(config).is_err());
    }

    #[test]
    fn test_new_accepts_valid_config() {
        let config = make_valid_config();
        assert!(IMessageChannel::new(config).is_ok());
    }

    #[test]
    fn test_capabilities() {
        let config = make_valid_config();
        let channel = IMessageChannel::new(config).unwrap();
        let caps = channel.capabilities();
        assert_eq!(caps.formatting_support, FormattingSupport::PlainText);
        assert_eq!(caps.max_message_length, Some(20000));
        assert!(caps.supports_typing);
        assert!(!caps.supports_edit);
        assert!(!caps.supports_images);
        assert!(!caps.supports_code_blocks);
        assert_eq!(caps.streaming_type, StreamingType::None);
    }

    #[test]
    fn test_plugin_adapter_metadata() {
        let config = make_valid_config();
        let channel = IMessageChannel::new(config).unwrap();
        assert_eq!(channel.name(), "imessage");
        assert_eq!(channel.version(), semver::Version::new(0, 1, 0));
        assert_eq!(channel.adapter_type(), AdapterType::Channel);
    }
}
