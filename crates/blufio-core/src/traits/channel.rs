// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Channel adapter trait for messaging platform integrations (Telegram, Discord, etc.).

use async_trait::async_trait;

use crate::error::BlufioError;
use crate::traits::adapter::PluginAdapter;
use crate::types::{ChannelCapabilities, InboundMessage, MessageId, OutboundMessage};

/// Adapter for bidirectional messaging channel integrations.
///
/// Channel adapters connect Blufio to external messaging platforms,
/// handling message ingestion and delivery.
#[async_trait]
pub trait ChannelAdapter: PluginAdapter {
    /// Returns the capabilities supported by this channel.
    fn capabilities(&self) -> ChannelCapabilities;

    /// Establishes a connection to the messaging platform.
    async fn connect(&mut self) -> Result<(), BlufioError>;

    /// Sends a message through the channel.
    async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError>;

    /// Receives the next inbound message from the channel.
    async fn receive(&self) -> Result<InboundMessage, BlufioError>;
}
