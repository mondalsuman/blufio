// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Edit-in-place message streaming with throttle for Telegram.
//!
//! Implements the streaming response pattern: send an initial message,
//! then edit it as tokens arrive, throttled to avoid Telegram rate limits.
//! Long responses are split at paragraph boundaries when exceeding 4096 chars.
//!
//! Uses the shared [`StreamingEditorOps`] trait and [`StreamingBuffer`] from
//! `blufio-core` for cross-adapter consistency.

use std::time::Duration;

use async_trait::async_trait;
use blufio_core::error::BlufioError;
use blufio_core::streaming::{StreamingBuffer, StreamingEditorOps};
use teloxide::prelude::*;
use teloxide::types::{ChatAction, ChatId, ParseMode};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::markdown;

// Re-export the shared split function so existing users can still find it here.
pub use blufio_core::streaming::split_at_paragraph_boundary;

/// Leave margin below the max (4096) to account for escaping overhead.
const SPLIT_THRESHOLD: usize = 3800;

/// Default throttle interval between message edits.
const DEFAULT_THROTTLE: Duration = Duration::from_millis(1500);

/// Telegram-specific streaming operations.
///
/// Wraps a teloxide `Bot` and `ChatId` to provide MarkdownV2 send/edit
/// with plain text fallback.
struct TelegramStreamOps {
    bot: Bot,
    chat_id: ChatId,
}

#[async_trait]
impl StreamingEditorOps for TelegramStreamOps {
    async fn send_initial(&mut self, text: &str) -> Result<String, BlufioError> {
        let escaped = markdown::format_for_telegram(text);

        // Try MarkdownV2 first
        let sent = self
            .bot
            .send_message(self.chat_id, &escaped)
            .parse_mode(ParseMode::MarkdownV2)
            .await
            .map_err(|e| {
                debug!(error = %e, "MarkdownV2 send failed, will retry as plain text");
                BlufioError::Channel {
                    message: format!("failed to send message: {e}"),
                    source: Some(Box::new(e)),
                }
            });

        match sent {
            Ok(msg) => Ok(msg.id.0.to_string()),
            Err(_) => {
                // Fallback: send as plain text
                let sent = self
                    .bot
                    .send_message(self.chat_id, text)
                    .await
                    .map_err(|e| BlufioError::Channel {
                        message: format!("failed to send plain text message: {e}"),
                        source: Some(Box::new(e)),
                    })?;
                Ok(sent.id.0.to_string())
            }
        }
    }

    async fn edit_message(&mut self, msg_id: &str, text: &str) -> Result<(), BlufioError> {
        let msg_id: i32 = msg_id.parse().map_err(|e| BlufioError::Channel {
            message: format!("invalid message_id: {e}"),
            source: None,
        })?;
        let msg_id = teloxide::types::MessageId(msg_id);

        let escaped = markdown::format_for_telegram(text);

        let result = self
            .bot
            .edit_message_text(self.chat_id, msg_id, &escaped)
            .parse_mode(ParseMode::MarkdownV2)
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("message is not modified") {
                    debug!("message unchanged, skipping edit");
                    Ok(())
                } else if err_str.contains("can't parse entities") {
                    // Fallback: edit as plain text
                    warn!(error = %e, "MarkdownV2 edit failed, retrying as plain text");
                    self.bot
                        .edit_message_text(self.chat_id, msg_id, text)
                        .await
                        .map_err(|e| BlufioError::Channel {
                            message: format!("failed to edit message: {e}"),
                            source: Some(Box::new(e)),
                        })?;
                    Ok(())
                } else {
                    Err(BlufioError::Channel {
                        message: format!("failed to edit message: {e}"),
                        source: Some(Box::new(e)),
                    })
                }
            }
        }
    }

    fn max_message_length(&self) -> usize {
        4096
    }

    fn throttle_interval(&self) -> Duration {
        DEFAULT_THROTTLE
    }
}

/// Manages streaming response delivery via edit-in-place.
///
/// Accumulates text chunks and periodically edits the Telegram message.
/// When the buffer exceeds [`SPLIT_THRESHOLD`] characters, the message is
/// split at a paragraph boundary and a new message is started.
///
/// Uses [`StreamingBuffer`] from `blufio-core` for shared buffering logic.
pub struct StreamingEditor {
    ops: TelegramStreamOps,
    buffer: StreamingBuffer,
}

impl StreamingEditor {
    /// Creates a new streaming editor for the given chat.
    pub fn new(bot: Bot, chat_id: ChatId) -> Self {
        Self {
            ops: TelegramStreamOps { bot, chat_id },
            buffer: StreamingBuffer::new(SPLIT_THRESHOLD),
        }
    }

    /// Appends a text chunk and potentially sends/edits the message.
    ///
    /// If enough time has elapsed since the last edit, the accumulated
    /// buffer is sent to Telegram. If the buffer exceeds the split
    /// threshold, the message is finalized and a new one is started.
    pub async fn push_chunk(&mut self, text: &str) -> Result<(), BlufioError> {
        self.buffer.push_chunk(&mut self.ops, text).await
    }

    /// Sends the final version of the accumulated text.
    ///
    /// Must be called after the last chunk to ensure all content is delivered.
    pub async fn finalize(&mut self) -> Result<(), BlufioError> {
        self.buffer.finalize(&mut self.ops).await
    }

    /// Returns the IDs of all messages sent during this streaming session.
    pub fn message_ids(&self) -> &[String] {
        self.buffer.message_ids()
    }
}

/// Starts a background task that sends typing indicators every 5 seconds.
///
/// The task continues until the `cancel` token is triggered.
pub fn start_typing_indicator(
    bot: Bot,
    chat_id: ChatId,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    debug!("typing indicator cancelled");
                    break;
                }
                _ = tokio::time::sleep(Duration::from_secs(5)) => {
                    if let Err(e) = bot.send_chat_action(chat_id, ChatAction::Typing).await {
                        warn!(error = %e, "failed to send typing indicator");
                        // Don't break on error -- typing indicators are best-effort
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
    fn split_at_paragraph_boundary_double_newline() {
        let text = "First paragraph.\n\nSecond paragraph that is longer.";
        let (first, rest) = split_at_paragraph_boundary(text, 30);
        assert_eq!(first, "First paragraph.");
        assert_eq!(rest, "Second paragraph that is longer.");
    }

    #[test]
    fn split_at_paragraph_boundary_single_newline() {
        let text = "First line\nSecond line that is longer";
        let (first, rest) = split_at_paragraph_boundary(text, 20);
        assert_eq!(first, "First line");
        assert_eq!(rest, "Second line that is longer");
    }

    #[test]
    fn split_at_paragraph_boundary_space() {
        let text = "OneLongWordThen another word";
        let (first, rest) = split_at_paragraph_boundary(text, 20);
        assert_eq!(first, "OneLongWordThen");
        assert_eq!(rest, "another word");
    }

    #[test]
    fn split_at_paragraph_boundary_hard_split() {
        let text = "abcdefghijklmnopqrstuvwxyz";
        let (first, rest) = split_at_paragraph_boundary(text, 10);
        assert_eq!(first, "abcdefghij");
        assert_eq!(rest, "klmnopqrstuvwxyz");
    }

    #[test]
    fn split_at_paragraph_boundary_short_text() {
        let text = "Short text";
        let (first, rest) = split_at_paragraph_boundary(text, 100);
        assert_eq!(first, "Short text");
        assert_eq!(rest, "");
    }

    #[test]
    fn split_prefers_double_newline_over_single() {
        let text = "A\nB\n\nC\nD";
        let (first, _rest) = split_at_paragraph_boundary(text, 6);
        // Should split at \n\n (position 3), not at the \n at position 1
        assert_eq!(first, "A\nB");
    }

    #[test]
    fn split_at_paragraph_boundary_multiple_paragraphs() {
        let text = "Para 1.\n\nPara 2.\n\nPara 3 is very long and should cause a split.";
        let (first, rest) = split_at_paragraph_boundary(text, 20);
        assert_eq!(first, "Para 1.\n\nPara 2.");
        assert_eq!(rest, "Para 3 is very long and should cause a split.");
    }
}
