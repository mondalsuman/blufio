// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Slack streaming message editing via the shared StreamingEditorOps trait.

use std::time::Duration;

use crate::markdown;
use async_trait::async_trait;
use blufio_core::error::BlufioError;
use blufio_core::streaming::{StreamingBuffer, StreamingEditorOps};
use slack_morphism::prelude::*;

/// Slack-specific split threshold.
/// Slack's limit is 40000, but individual text sections in blocks have
/// a 3000-char limit. Use 4000 as a practical threshold for plain text.
const SPLIT_THRESHOLD: usize = 4000;

/// Slack-specific streaming operations.
///
/// Wraps the Slack API client, channel ID, and bot token for send/edit.
pub(crate) struct SlackStreamOps {
    client: SlackClient<SlackClientHyperHttpsConnector>,
    token: SlackApiToken,
    channel_id: SlackChannelId,
}

#[async_trait]
impl StreamingEditorOps for SlackStreamOps {
    async fn send_initial(&mut self, text: &str) -> Result<String, BlufioError> {
        let formatted = markdown::markdown_to_mrkdwn(text);
        let session = self.client.open_session(&self.token);

        let req = SlackApiChatPostMessageRequest::new(
            self.channel_id.clone(),
            SlackMessageContent::new().with_text(formatted),
        );

        let resp = session
            .chat_post_message(&req)
            .await
            .map_err(|e| {
                use blufio_core::error::{ChannelErrorKind, ErrorContext};
                BlufioError::Channel {
                    kind: ChannelErrorKind::DeliveryFailed,
                    context: ErrorContext {
                        channel_name: Some("slack".to_string()),
                        ..Default::default()
                    },
                    source: None,
                }
            })?;

        // Return the ts (timestamp) as the message ID.
        Ok(resp.ts.to_string())
    }

    async fn edit_message(&mut self, msg_id: &str, text: &str) -> Result<(), BlufioError> {
        let formatted = markdown::markdown_to_mrkdwn(text);
        let session = self.client.open_session(&self.token);

        let ts: SlackTs = msg_id.to_string().into();

        let req = SlackApiChatUpdateRequest::new(
            self.channel_id.clone(),
            SlackMessageContent::new().with_text(formatted),
            ts,
        );

        session
            .chat_update(&req)
            .await
            .map_err(|e| {
                use blufio_core::error::{ChannelErrorKind, ErrorContext};
                BlufioError::Channel {
                    kind: ChannelErrorKind::DeliveryFailed,
                    context: ErrorContext {
                        channel_name: Some("slack".to_string()),
                        ..Default::default()
                    },
                    source: None,
                }
            })?;

        Ok(())
    }

    fn max_message_length(&self) -> usize {
        40000
    }

    fn throttle_interval(&self) -> Duration {
        // Slack chat.update is Tier 3 (~50+/min).
        // Use 3000ms for conservative rate limiting.
        Duration::from_millis(3000)
    }
}

/// Slack streaming editor wrapping the shared StreamingBuffer.
pub struct SlackStreamingEditor {
    ops: SlackStreamOps,
    buffer: StreamingBuffer,
}

impl SlackStreamingEditor {
    /// Create a new Slack streaming editor.
    pub fn new(
        client: SlackClient<SlackClientHyperHttpsConnector>,
        token: SlackApiToken,
        channel_id: SlackChannelId,
    ) -> Self {
        Self {
            ops: SlackStreamOps {
                client,
                token,
                channel_id,
            },
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

    /// Returns the IDs (timestamps) of all messages sent.
    pub fn message_ids(&self) -> &[String] {
        self.buffer.message_ids()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_threshold_is_reasonable() {
        const { assert!(SPLIT_THRESHOLD < 40000) };
        const { assert!(SPLIT_THRESHOLD > 2000) };
    }
}
