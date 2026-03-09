// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! WhatsApp Cloud API adapter implementing [`ChannelAdapter`].

use crate::api;
use async_trait::async_trait;
use blufio_config::model::WhatsAppConfig;
use blufio_core::error::{BlufioError, ChannelErrorKind, ErrorContext};
use blufio_core::traits::{ChannelAdapter, PluginAdapter};
use blufio_core::types::{
    AdapterType, ChannelCapabilities, FormattingSupport, HealthStatus, InboundMessage, MessageId,
    OutboundMessage, RateLimit, StreamingType,
};
use tokio::sync::{Mutex, mpsc};

/// WhatsApp Cloud API channel adapter.
///
/// Receives messages via webhook handlers and sends responses via the
/// Meta Graph API. The webhook handlers push [`InboundMessage`]s through
/// the `inbound_tx` channel.
pub struct WhatsAppCloudChannel {
    config: WhatsAppConfig,
    inbound_rx: Mutex<mpsc::Receiver<InboundMessage>>,
    inbound_tx: mpsc::Sender<InboundMessage>,
    http_client: Option<reqwest::Client>,
}

impl WhatsAppCloudChannel {
    /// Creates a new WhatsApp Cloud API channel adapter.
    ///
    /// Validates that `phone_number_id` and `access_token` are set.
    /// Variant must be `"cloud"` or `None` (defaults to cloud).
    pub fn new(config: WhatsAppConfig) -> Result<Self, BlufioError> {
        // Validate variant.
        if let Some(ref variant) = config.variant
            && variant != "cloud"
        {
            return Err(BlufioError::Config(format!(
                "WhatsAppCloudChannel requires variant='cloud', got '{variant}'"
            )));
        }

        if config.phone_number_id.is_none() {
            return Err(BlufioError::Config(
                "whatsapp.phone_number_id is required for Cloud API adapter".into(),
            ));
        }

        if config.access_token.is_none() {
            return Err(BlufioError::Config(
                "whatsapp.access_token is required for Cloud API adapter".into(),
            ));
        }

        let (inbound_tx, inbound_rx) = mpsc::channel(100);

        Ok(Self {
            config,
            inbound_rx: Mutex::new(inbound_rx),
            inbound_tx,
            http_client: None,
        })
    }

    /// Returns a sender handle for webhook handlers to forward inbound messages.
    pub fn inbound_tx(&self) -> mpsc::Sender<InboundMessage> {
        self.inbound_tx.clone()
    }
}

#[async_trait]
impl PluginAdapter for WhatsAppCloudChannel {
    fn name(&self) -> &str {
        "whatsapp"
    }

    fn version(&self) -> semver::Version {
        semver::Version::new(0, 1, 0)
    }

    fn adapter_type(&self) -> AdapterType {
        AdapterType::Channel
    }

    async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
        match &self.http_client {
            Some(_) => Ok(HealthStatus::Healthy),
            None => Ok(HealthStatus::Unhealthy(
                "WhatsApp not connected".to_string(),
            )),
        }
    }

    async fn shutdown(&self) -> Result<(), BlufioError> {
        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for WhatsAppCloudChannel {
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
            formatting_support: FormattingSupport::BasicMarkdown,
            rate_limit: Some(RateLimit {
                messages_per_second: Some(80.0),
                burst_limit: Some(80),
                daily_limit: Some(1000),
            }),
            supports_code_blocks: false,
        }
    }

    async fn connect(&mut self) -> Result<(), BlufioError> {
        self.http_client = Some(reqwest::Client::new());
        Ok(())
    }

    async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> {
        let client = self
            .http_client
            .as_ref()
            .ok_or_else(|| BlufioError::channel_connection_lost("whatsapp"))?;

        let phone_number_id =
            self.config.phone_number_id.as_deref().ok_or_else(|| {
                BlufioError::Config("whatsapp.phone_number_id is required".into())
            })?;

        let access_token = self
            .config
            .access_token
            .as_deref()
            .ok_or_else(|| BlufioError::Config("whatsapp.access_token is required".into()))?;

        // Extract recipient from metadata chat_id.
        let to = if let Some(ref metadata) = msg.metadata
            && let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata)
            && let Some(chat_id) = meta.get("chat_id").and_then(|v| v.as_str())
        {
            chat_id.to_string()
        } else {
            return Err(BlufioError::Channel {
                kind: ChannelErrorKind::DeliveryFailed,
                context: ErrorContext {
                    channel_name: Some("whatsapp".to_string()),
                    ..Default::default()
                },
                source: None,
            });
        };

        let message_id =
            api::send_whatsapp_message(client, phone_number_id, access_token, &to, &msg.content)
                .await?;

        Ok(MessageId(message_id))
    }

    async fn receive(&self) -> Result<InboundMessage, BlufioError> {
        let mut rx = self.inbound_rx.lock().await;
        rx.recv()
            .await
            .ok_or_else(|| BlufioError::channel_connection_lost("whatsapp"))
    }

    async fn edit_message(
        &self,
        _chat_id: &str,
        _message_id: &str,
        _text: &str,
        _parse_mode: Option<&str>,
    ) -> Result<(), BlufioError> {
        // WhatsApp Cloud API does not support message editing.
        Ok(())
    }

    async fn send_typing(&self, _chat_id: &str) -> Result<(), BlufioError> {
        // WhatsApp Cloud API does not have a typing indicator endpoint.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_valid_config() -> WhatsAppConfig {
        WhatsAppConfig {
            variant: None,
            phone_number_id: Some("123456".into()),
            access_token: Some("access-token-test".into()),
            verify_token: Some("verify-token".into()),
            app_secret: Some("app-secret".into()),
            session_data_path: None,
            allowed_users: vec![],
        }
    }

    #[test]
    fn new_rejects_missing_phone_number_id() {
        let mut config = make_valid_config();
        config.phone_number_id = None;
        assert!(WhatsAppCloudChannel::new(config).is_err());
    }

    #[test]
    fn new_rejects_missing_access_token() {
        let mut config = make_valid_config();
        config.access_token = None;
        assert!(WhatsAppCloudChannel::new(config).is_err());
    }

    #[test]
    fn new_rejects_wrong_variant() {
        let mut config = make_valid_config();
        config.variant = Some("web".into());
        assert!(WhatsAppCloudChannel::new(config).is_err());
    }

    #[test]
    fn new_accepts_valid_config() {
        let config = make_valid_config();
        assert!(WhatsAppCloudChannel::new(config).is_ok());
    }

    #[test]
    fn new_accepts_cloud_variant() {
        let mut config = make_valid_config();
        config.variant = Some("cloud".into());
        assert!(WhatsAppCloudChannel::new(config).is_ok());
    }

    #[test]
    fn capabilities_are_correct() {
        let config = make_valid_config();
        let channel = WhatsAppCloudChannel::new(config).unwrap();
        let caps = channel.capabilities();
        assert!(!caps.supports_edit);
        assert!(!caps.supports_typing);
        assert!(!caps.supports_images);
        assert_eq!(caps.max_message_length, Some(4096));
    }

    #[test]
    fn plugin_adapter_metadata() {
        let config = make_valid_config();
        let channel = WhatsAppCloudChannel::new(config).unwrap();
        assert_eq!(channel.name(), "whatsapp");
        assert_eq!(channel.version(), semver::Version::new(0, 1, 0));
        assert_eq!(channel.adapter_type(), AdapterType::Channel);
    }
}
