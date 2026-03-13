// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

//! SMS channel adapter (Twilio) for the Blufio agent framework.
//!
//! Uses Twilio REST API for outbound messages and webhook for inbound.
//! Validates X-Twilio-Signature HMAC on incoming webhooks for security.

pub mod api;
pub mod types;
pub mod webhook;

use api::TwilioClient;
use async_trait::async_trait;
use blufio_config::model::SmsConfig;
use blufio_core::error::BlufioError;
use blufio_core::format::{FormatPipeline, split_at_paragraphs};
use blufio_core::traits::{ChannelAdapter, PluginAdapter};
use blufio_core::types::{
    AdapterType, ChannelCapabilities, FormattingSupport, HealthStatus, InboundMessage, MessageId,
    OutboundMessage, RateLimit, StreamingType,
};
use tokio::sync::{Mutex, mpsc};

/// SMS channel adapter backed by the Twilio REST API.
///
/// Receives messages via webhook handlers and sends responses through the
/// Twilio Messages API. The webhook handlers push [`InboundMessage`]s through
/// the `inbound_tx` channel.
pub struct SmsChannel {
    config: SmsConfig,
    inbound_rx: Mutex<mpsc::Receiver<InboundMessage>>,
    inbound_tx: mpsc::Sender<InboundMessage>,
    api_client: Mutex<Option<TwilioClient>>,
}

impl SmsChannel {
    /// Creates a new SMS channel adapter.
    ///
    /// Validates that `account_sid`, `auth_token`, and `twilio_phone_number`
    /// are set, and that the phone number is in E.164 format.
    pub fn new(config: SmsConfig) -> Result<Self, BlufioError> {
        // Validate required fields.
        if config.account_sid.is_none() {
            return Err(BlufioError::Config(
                "sms.account_sid is required for SMS adapter".into(),
            ));
        }

        if config.auth_token.is_none() {
            return Err(BlufioError::Config(
                "sms.auth_token is required for SMS adapter".into(),
            ));
        }

        let phone_number = config.twilio_phone_number.as_deref().ok_or_else(|| {
            BlufioError::Config("sms.twilio_phone_number is required for SMS adapter".into())
        })?;

        if !api::validate_e164(phone_number) {
            return Err(BlufioError::Config(format!(
                "sms.twilio_phone_number must be E.164 format (e.g., +1234567890), got: {phone_number}"
            )));
        }

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
impl PluginAdapter for SmsChannel {
    fn name(&self) -> &str {
        "sms"
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
            Some(client) => match client.account_status().await {
                Ok(status) if status == "active" => Ok(HealthStatus::Healthy),
                Ok(status) => Ok(HealthStatus::Degraded(format!(
                    "Twilio account status: {status}"
                ))),
                Err(e) => Ok(HealthStatus::Unhealthy(format!(
                    "Twilio health check failed: {e}"
                ))),
            },
            None => Ok(HealthStatus::Unhealthy("SMS not connected".to_string())),
        }
    }

    async fn shutdown(&self) -> Result<(), BlufioError> {
        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for SmsChannel {
    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_edit: false,
            supports_typing: false,
            supports_images: false,
            supports_documents: false,
            supports_voice: false,
            max_message_length: Some(self.config.max_response_length),
            supports_embeds: false,
            supports_reactions: false,
            supports_threads: false,
            streaming_type: StreamingType::None,
            formatting_support: FormattingSupport::PlainText,
            rate_limit: Some(RateLimit {
                messages_per_second: Some(self.config.rate_limit_per_second),
                burst_limit: None,
                daily_limit: None,
            }),
            supports_code_blocks: false,
            supports_interactive: false,
        }
    }

    async fn connect(&mut self) -> Result<(), BlufioError> {
        let account_sid = self
            .config
            .account_sid
            .as_deref()
            .ok_or_else(|| BlufioError::channel_connection_lost("sms"))?;

        let auth_token = self
            .config
            .auth_token
            .as_deref()
            .ok_or_else(|| BlufioError::channel_connection_lost("sms"))?;

        let phone_number = self
            .config
            .twilio_phone_number
            .as_deref()
            .ok_or_else(|| BlufioError::channel_connection_lost("sms"))?;

        let client = TwilioClient::new(account_sid, auth_token, phone_number)?;

        // Verify credentials by checking account status.
        client.account_status().await?;

        *self.api_client.lock().await = Some(client);
        Ok(())
    }

    async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> {
        let guard = self.api_client.lock().await;
        let client = guard
            .as_ref()
            .ok_or_else(|| BlufioError::channel_connection_lost("sms"))?;

        // Extract recipient from metadata "From" field (reply to sender).
        let to = if let Some(ref metadata) = msg.metadata
            && let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata)
            && let Some(from) = meta.get("From").and_then(|v| v.as_str())
        {
            from.to_string()
        } else {
            return Err(BlufioError::Config(
                "SMS send requires From in metadata to reply to sender".into(),
            ));
        };

        let caps = self.capabilities();

        // FormatPipeline: PlainText degrades markdown.
        let formatted = FormatPipeline::detect_and_format(&msg.content, &caps);
        let chunks = split_at_paragraphs(&formatted, caps.max_message_length);

        let mut first_id = None;

        for chunk in &chunks {
            // Truncate to max_response_length.
            let truncated = if chunk.len() > self.config.max_response_length {
                &chunk[..self.config.max_response_length]
            } else {
                chunk.as_str()
            };

            let message_sid = client.send_message(&to, truncated).await?;
            if first_id.is_none() {
                first_id = Some(MessageId(message_sid));
            }
        }

        Ok(first_id.unwrap_or_else(|| MessageId(String::new())))
    }

    async fn receive(&self) -> Result<InboundMessage, BlufioError> {
        let mut rx = self.inbound_rx.lock().await;
        rx.recv()
            .await
            .ok_or_else(|| BlufioError::channel_connection_lost("sms"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_valid_config() -> SmsConfig {
        SmsConfig {
            account_sid: Some("AC1234567890abcdef".into()),
            auth_token: Some("test-auth-token".into()),
            twilio_phone_number: Some("+1234567890".into()),
            webhook_url: None,
            max_response_length: 1600,
            rate_limit_per_second: 1.0,
            allowed_numbers: vec![],
        }
    }

    #[test]
    fn test_new_rejects_missing_account_sid() {
        let mut config = make_valid_config();
        config.account_sid = None;
        assert!(SmsChannel::new(config).is_err());
    }

    #[test]
    fn test_new_rejects_missing_auth_token() {
        let mut config = make_valid_config();
        config.auth_token = None;
        assert!(SmsChannel::new(config).is_err());
    }

    #[test]
    fn test_new_rejects_missing_phone_number() {
        let mut config = make_valid_config();
        config.twilio_phone_number = None;
        assert!(SmsChannel::new(config).is_err());
    }

    #[test]
    fn test_new_rejects_invalid_e164() {
        let mut config = make_valid_config();
        config.twilio_phone_number = Some("not-a-number".into());
        assert!(SmsChannel::new(config).is_err());
    }

    #[test]
    fn test_new_accepts_valid_config() {
        let config = make_valid_config();
        assert!(SmsChannel::new(config).is_ok());
    }

    #[test]
    fn test_capabilities() {
        let config = make_valid_config();
        let channel = SmsChannel::new(config).unwrap();
        let caps = channel.capabilities();
        assert_eq!(caps.formatting_support, FormattingSupport::PlainText);
        assert_eq!(caps.max_message_length, Some(1600));
        assert!(!caps.supports_typing);
        assert!(!caps.supports_edit);
        assert!(!caps.supports_code_blocks);
        assert_eq!(caps.streaming_type, StreamingType::None);
        let rate = caps.rate_limit.unwrap();
        assert_eq!(rate.messages_per_second, Some(1.0));
        assert!(rate.burst_limit.is_none());
        assert!(rate.daily_limit.is_none());
    }

    #[test]
    fn test_plugin_adapter_metadata() {
        let config = make_valid_config();
        let channel = SmsChannel::new(config).unwrap();
        assert_eq!(channel.name(), "sms");
        assert_eq!(channel.version(), semver::Version::new(0, 1, 0));
        assert_eq!(channel.adapter_type(), AdapterType::Channel);
    }
}
