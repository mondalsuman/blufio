// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! EXPERIMENTAL: WhatsApp Web adapter.
//!
//! This uses unofficial APIs and may result in account bans.
//! Use at your own risk.
//!
//! This module is only compiled when the `whatsapp-web` feature flag is enabled.

#![cfg(feature = "whatsapp-web")]

use async_trait::async_trait;
use blufio_config::model::WhatsAppConfig;
use blufio_core::error::{BlufioError, ChannelErrorKind, ErrorContext};
use blufio_core::format::{FormatPipeline, split_at_paragraphs};
use blufio_core::traits::{ChannelAdapter, PluginAdapter};
use blufio_core::types::{
    AdapterType, ChannelCapabilities, FormattingSupport, HealthStatus, InboundMessage, MessageId,
    OutboundMessage, RateLimit, StreamingType,
};

/// Experimental WhatsApp Web channel adapter (stub).
///
/// All methods return an error indicating that the Web adapter is not yet
/// implemented. This exists as a placeholder for future development.
pub struct WhatsAppWebChannel {
    _config: WhatsAppConfig,
}

impl WhatsAppWebChannel {
    /// Creates a new WhatsApp Web adapter stub.
    ///
    /// Validates that variant is `"web"`.
    pub fn new(config: WhatsAppConfig) -> Result<Self, BlufioError> {
        match config.variant.as_deref() {
            Some("web") => Ok(Self { _config: config }),
            _ => Err(BlufioError::Config(
                "WhatsAppWebChannel requires variant='web'".into(),
            )),
        }
    }
}

#[async_trait]
impl PluginAdapter for WhatsAppWebChannel {
    fn name(&self) -> &str {
        "whatsapp-web"
    }

    fn version(&self) -> semver::Version {
        semver::Version::new(0, 1, 0)
    }

    fn adapter_type(&self) -> AdapterType {
        AdapterType::Channel
    }

    async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
        Ok(HealthStatus::Unhealthy(
            "WhatsApp Web adapter is experimental and not yet implemented".to_string(),
        ))
    }

    async fn shutdown(&self) -> Result<(), BlufioError> {
        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for WhatsAppWebChannel {
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
        Err(BlufioError::channel_unsupported_content("whatsapp-web"))
    }

    async fn send(&self, _msg: OutboundMessage) -> Result<MessageId, BlufioError> {
        Err(BlufioError::channel_unsupported_content("whatsapp-web"))
    }

    async fn receive(&self) -> Result<InboundMessage, BlufioError> {
        Err(BlufioError::channel_unsupported_content("whatsapp-web"))
    }
}
