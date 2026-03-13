// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Email channel adapter for the Blufio agent framework.
//!
//! Uses IMAP for incoming messages and SMTP (lettre) for outgoing.
//! Supports thread-to-session mapping via In-Reply-To/References headers,
//! quoted-text stripping, and HTML-to-plaintext conversion.

pub mod imap;
pub mod parsing;
pub mod smtp;

use async_trait::async_trait;
use blufio_config::model::EmailConfig;
use blufio_core::error::BlufioError;
use blufio_core::format::{FormatPipeline, split_at_paragraphs};
use blufio_core::traits::{ChannelAdapter, PluginAdapter};
use blufio_core::types::{
    AdapterType, ChannelCapabilities, FormattingSupport, HealthStatus, InboundMessage, MessageId,
    OutboundMessage, StreamingType,
};
use lettre::{AsyncSmtpTransport, Tokio1Executor};
use tokio::sync::{Mutex, mpsc};

/// Email channel adapter.
///
/// Receives messages via IMAP polling and sends replies via SMTP.
/// Thread-to-session mapping uses In-Reply-To/References headers.
pub struct EmailChannel {
    config: EmailConfig,
    inbound_rx: Mutex<mpsc::Receiver<InboundMessage>>,
    inbound_tx: mpsc::Sender<InboundMessage>,
    smtp_transport: Mutex<Option<AsyncSmtpTransport<Tokio1Executor>>>,
    poll_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl EmailChannel {
    /// Create a new email channel adapter.
    ///
    /// Validates that required configuration fields are present:
    /// - `imap_host` must be set
    /// - `username` must be set
    /// - `from_address` must be set
    pub fn new(config: EmailConfig) -> Result<Self, BlufioError> {
        if config.imap_host.is_none() {
            return Err(BlufioError::Config(
                "email.imap_host is required for email adapter".into(),
            ));
        }

        if config.username.is_none() {
            return Err(BlufioError::Config(
                "email.username is required for email adapter".into(),
            ));
        }

        if config.from_address.is_none() {
            return Err(BlufioError::Config(
                "email.from_address is required for email adapter".into(),
            ));
        }

        // Basic format check on username.
        if let Some(ref user) = config.username {
            if !user.contains('@') {
                return Err(BlufioError::Config(
                    "email.username must contain '@'".into(),
                ));
            }
        }

        let (inbound_tx, inbound_rx) = mpsc::channel(100);

        Ok(Self {
            config,
            inbound_rx: Mutex::new(inbound_rx),
            inbound_tx,
            smtp_transport: Mutex::new(None),
            poll_handle: Mutex::new(None),
        })
    }

    /// Returns a sender handle for forwarding inbound messages.
    pub fn inbound_tx(&self) -> mpsc::Sender<InboundMessage> {
        self.inbound_tx.clone()
    }
}

#[async_trait]
impl PluginAdapter for EmailChannel {
    fn name(&self) -> &str {
        "email"
    }

    fn version(&self) -> semver::Version {
        semver::Version::new(0, 1, 0)
    }

    fn adapter_type(&self) -> AdapterType {
        AdapterType::Channel
    }

    async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
        let transport = self.smtp_transport.lock().await;
        if transport.is_some() {
            Ok(HealthStatus::Healthy)
        } else {
            Ok(HealthStatus::Unhealthy("email not connected".to_string()))
        }
    }

    async fn shutdown(&self) -> Result<(), BlufioError> {
        // Abort IMAP polling task if running.
        let mut handle = self.poll_handle.lock().await;
        if let Some(h) = handle.take() {
            h.abort();
        }
        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for EmailChannel {
    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_edit: false,
            supports_typing: false,
            supports_images: false,
            supports_documents: false,
            supports_voice: false,
            max_message_length: None,
            supports_embeds: false,
            supports_reactions: false,
            supports_threads: false,
            streaming_type: StreamingType::None,
            formatting_support: FormattingSupport::FullMarkdown,
            rate_limit: None,
            supports_code_blocks: true,
        }
    }

    async fn connect(&mut self) -> Result<(), BlufioError> {
        // Build and store SMTP transport.
        let transport = smtp::build_smtp_transport(&self.config).await?;
        {
            let mut smtp = self.smtp_transport.lock().await;
            *smtp = Some(transport);
        }

        // Start IMAP polling.
        let handle =
            imap::start_imap_polling(self.config.clone(), self.inbound_tx.clone()).await?;
        {
            let mut ph = self.poll_handle.lock().await;
            *ph = Some(handle);
        }

        Ok(())
    }

    async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> {
        let transport_guard = self.smtp_transport.lock().await;
        let transport = transport_guard
            .as_ref()
            .ok_or_else(|| BlufioError::channel_connection_lost("email"))?;

        // Extract reply metadata from the message.
        let (recipient, subject, in_reply_to, references) =
            if let Some(ref metadata) = msg.metadata
                && let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata)
            {
                let from = meta
                    .get("from")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let subject = meta
                    .get("subject")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(no subject)")
                    .to_string();
                let irt = meta
                    .get("message_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let refs = meta
                    .get("in_reply_to")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                (from, subject, irt, refs)
            } else {
                return Err(BlufioError::Config(
                    "email send: metadata with 'from' field is required".into(),
                ));
            };

        if recipient.is_empty() {
            return Err(BlufioError::Config(
                "email send: recipient address is empty".into(),
            ));
        }

        let caps = self.capabilities();

        // FormatPipeline: detect and format for FullMarkdown (passes through).
        let formatted = FormatPipeline::detect_and_format(&msg.content, &caps);

        // Split if needed (unlikely with None max_length).
        let chunks = split_at_paragraphs(&formatted, caps.max_message_length);

        let mut last_message_id = String::new();

        for chunk in &chunks {
            // Convert markdown to HTML for the HTML part.
            let html_body = parsing::markdown_to_html(chunk);

            let mid = smtp::send_email_reply(
                transport,
                &self.config,
                &recipient,
                &subject,
                chunk,
                &html_body,
                in_reply_to.as_deref(),
                references.as_deref(),
            )
            .await?;

            last_message_id = mid;
        }

        Ok(MessageId(last_message_id))
    }

    async fn receive(&self) -> Result<InboundMessage, BlufioError> {
        let mut rx = self.inbound_rx.lock().await;
        rx.recv()
            .await
            .ok_or_else(|| BlufioError::channel_connection_lost("email"))
    }

    async fn edit_message(
        &self,
        _chat_id: &str,
        _message_id: &str,
        _text: &str,
        _parse_mode: Option<&str>,
    ) -> Result<(), BlufioError> {
        // Email does not support message editing.
        Ok(())
    }

    async fn send_typing(&self, _chat_id: &str) -> Result<(), BlufioError> {
        // Email does not support typing indicators.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_valid_config() -> EmailConfig {
        EmailConfig {
            imap_host: Some("imap.example.com".into()),
            imap_port: Some(993),
            smtp_host: Some("smtp.example.com".into()),
            smtp_port: Some(587),
            username: Some("user@example.com".into()),
            password: Some("password".into()),
            smtp_username: None,
            smtp_password: None,
            from_address: Some("bot@example.com".into()),
            from_name: Some("Blufio".into()),
            poll_interval_secs: 30,
            folders: vec![],
            allow_insecure: false,
            allowed_senders: vec![],
            email_footer: None,
        }
    }

    #[test]
    fn test_new_rejects_missing_imap_host() {
        let mut config = make_valid_config();
        config.imap_host = None;
        assert!(EmailChannel::new(config).is_err());
    }

    #[test]
    fn test_new_rejects_missing_username() {
        let mut config = make_valid_config();
        config.username = None;
        assert!(EmailChannel::new(config).is_err());
    }

    #[test]
    fn test_new_rejects_missing_from_address() {
        let mut config = make_valid_config();
        config.from_address = None;
        assert!(EmailChannel::new(config).is_err());
    }

    #[test]
    fn test_new_accepts_valid_config() {
        let config = make_valid_config();
        assert!(EmailChannel::new(config).is_ok());
    }

    #[test]
    fn test_capabilities_correct() {
        let config = make_valid_config();
        let channel = EmailChannel::new(config).unwrap();
        let caps = channel.capabilities();
        assert_eq!(caps.streaming_type, StreamingType::None);
        assert_eq!(caps.formatting_support, FormattingSupport::FullMarkdown);
        assert_eq!(caps.max_message_length, None);
        assert!(caps.supports_code_blocks);
        assert!(!caps.supports_edit);
        assert!(!caps.supports_typing);
        assert!(!caps.supports_images);
    }

    #[test]
    fn test_plugin_adapter_metadata() {
        let config = make_valid_config();
        let channel = EmailChannel::new(config).unwrap();
        assert_eq!(channel.name(), "email");
        assert_eq!(channel.version(), semver::Version::new(0, 1, 0));
        assert_eq!(channel.adapter_type(), AdapterType::Channel);
    }
}
