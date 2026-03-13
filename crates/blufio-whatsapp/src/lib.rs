// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

//! WhatsApp channel adapter for the Blufio agent framework.
//!
//! Supports two variants:
//! - **Cloud API** (default): Production-ready adapter using the official
//!   Meta WhatsApp Business Cloud API with webhook integration.
//! - **Web** (experimental, `whatsapp-web` feature): Stub adapter for
//!   future WhatsApp Web integration. Use at your own risk.

pub mod api;
pub mod cloud;
pub mod types;
pub mod webhook;

#[cfg(feature = "whatsapp-web")]
pub mod web;

pub use cloud::WhatsAppCloudChannel;

#[cfg(feature = "whatsapp-web")]
pub use web::WhatsAppWebChannel;

use blufio_config::model::WhatsAppConfig;
use blufio_core::error::BlufioError;
use blufio_core::traits::ChannelAdapter;

/// Factory function to create the appropriate WhatsApp channel adapter
/// based on configuration variant.
///
/// - `variant = "cloud"` or `None` (default): Creates [`WhatsAppCloudChannel`].
/// - `variant = "web"` (requires `whatsapp-web` feature): Creates [`WhatsAppWebChannel`].
pub fn create_whatsapp_channel(
    config: WhatsAppConfig,
) -> Result<Box<dyn ChannelAdapter + Send + Sync>, BlufioError> {
    match config.variant.as_deref() {
        Some("web") => {
            #[cfg(feature = "whatsapp-web")]
            {
                let channel = WhatsAppWebChannel::new(config)?;
                Ok(Box::new(channel))
            }
            #[cfg(not(feature = "whatsapp-web"))]
            {
                Err(BlufioError::Config(
                    "WhatsApp Web variant requires the 'whatsapp-web' feature flag".into(),
                ))
            }
        }
        Some("cloud") | None => {
            let channel = WhatsAppCloudChannel::new(config)?;
            Ok(Box::new(channel))
        }
        Some(other) => Err(BlufioError::Config(format!(
            "unknown WhatsApp variant '{other}', expected 'cloud' or 'web'"
        ))),
    }
}
