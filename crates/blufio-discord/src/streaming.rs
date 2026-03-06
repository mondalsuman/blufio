// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Discord streaming message editing via the shared StreamingEditorOps trait.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use blufio_core::error::BlufioError;
use blufio_core::streaming::{StreamingBuffer, StreamingEditorOps};
use serenity::all::{ChannelId, CreateMessage, EditMessage};
use serenity::http::Http;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::markdown;

/// Discord-specific split threshold (~90% of 2000 char limit).
const SPLIT_THRESHOLD: usize = 1800;

/// Discord-specific streaming operations.
///
/// Wraps serenity HTTP client and channel ID for send/edit.
pub(crate) struct DiscordStreamOps {
    http: Arc<Http>,
    channel_id: ChannelId,
}

#[async_trait]
impl StreamingEditorOps for DiscordStreamOps {
    async fn send_initial(&mut self, text: &str) -> Result<String, BlufioError> {
        let formatted = markdown::format_for_discord(text);
        let msg = self
            .channel_id
            .send_message(&self.http, CreateMessage::new().content(&formatted))
            .await
            .map_err(|e| BlufioError::Channel {
                message: format!("failed to send Discord message: {e}"),
                source: Some(Box::new(e)),
            })?;
        Ok(msg.id.to_string())
    }

    async fn edit_message(&mut self, msg_id: &str, text: &str) -> Result<(), BlufioError> {
        let msg_id: u64 = msg_id.parse().map_err(|e| BlufioError::Channel {
            message: format!("invalid Discord message_id: {e}"),
            source: None,
        })?;
        let msg_id = serenity::model::id::MessageId::new(msg_id);

        let formatted = markdown::format_for_discord(text);
        self.channel_id
            .edit_message(&self.http, msg_id, EditMessage::new().content(&formatted))
            .await
            .map_err(|e| BlufioError::Channel {
                message: format!("failed to edit Discord message: {e}"),
                source: Some(Box::new(e)),
            })?;
        Ok(())
    }

    fn max_message_length(&self) -> usize {
        2000
    }

    fn throttle_interval(&self) -> Duration {
        Duration::from_millis(1000)
    }
}

/// Discord streaming editor wrapping the shared StreamingBuffer.
pub struct DiscordStreamingEditor {
    ops: DiscordStreamOps,
    buffer: StreamingBuffer,
}

impl DiscordStreamingEditor {
    /// Create a new Discord streaming editor.
    pub fn new(http: Arc<Http>, channel_id: ChannelId) -> Self {
        Self {
            ops: DiscordStreamOps { http, channel_id },
            buffer: StreamingBuffer::new(SPLIT_THRESHOLD),
        }
    }

    /// Append a text chunk.
    pub async fn push_chunk(&mut self, text: &str) -> Result<(), BlufioError> {
        self.buffer.push_chunk(&mut self.ops, text).await
    }

    /// Send the final version.
    pub async fn finalize(&mut self) -> Result<(), BlufioError> {
        self.buffer.finalize(&mut self.ops).await
    }

    /// Returns the IDs of all messages sent.
    pub fn message_ids(&self) -> &[String] {
        self.buffer.message_ids()
    }
}

/// Starts a background task that sends typing indicators every 5 seconds.
pub fn start_typing_indicator(
    http: Arc<Http>,
    channel_id: ChannelId,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    debug!("Discord typing indicator cancelled");
                    break;
                }
                _ = tokio::time::sleep(Duration::from_secs(5)) => {
                    if let Err(e) = channel_id.broadcast_typing(&http).await {
                        warn!(error = %e, "failed to send Discord typing indicator");
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_threshold_is_reasonable() {
        const { assert!(SPLIT_THRESHOLD < 2000) };
        const { assert!(SPLIT_THRESHOLD > 1500) };
    }
}
