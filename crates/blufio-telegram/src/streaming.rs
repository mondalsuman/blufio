// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Edit-in-place message streaming with throttle for Telegram.
//!
//! Implements the streaming response pattern: send an initial message,
//! then edit it as tokens arrive, throttled to avoid Telegram rate limits.
//! Long responses are split at paragraph boundaries when exceeding 4096 chars.

use std::time::{Duration, Instant};

use blufio_core::error::BlufioError;
use teloxide::prelude::*;
use teloxide::types::{ChatAction, ChatId, MessageId, ParseMode};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::markdown;

/// Leave margin below the max (4096) to account for escaping overhead.
const SPLIT_THRESHOLD: usize = 3800;

/// Default throttle interval between message edits.
const DEFAULT_THROTTLE: Duration = Duration::from_millis(1500);

/// Manages streaming response delivery via edit-in-place.
///
/// Accumulates text chunks and periodically edits the Telegram message.
/// When the buffer exceeds [`SPLIT_THRESHOLD`] characters, the message is
/// split at a paragraph boundary and a new message is started.
pub struct StreamingEditor {
    bot: Bot,
    chat_id: ChatId,
    message_id: Option<MessageId>,
    buffer: String,
    last_edit: Instant,
    throttle: Duration,
    messages_sent: Vec<MessageId>,
}

impl StreamingEditor {
    /// Creates a new streaming editor for the given chat.
    pub fn new(bot: Bot, chat_id: ChatId) -> Self {
        Self {
            bot,
            chat_id,
            message_id: None,
            buffer: String::new(),
            last_edit: Instant::now() - DEFAULT_THROTTLE, // Allow immediate first send
            throttle: DEFAULT_THROTTLE,
            messages_sent: Vec::new(),
        }
    }

    /// Appends a text chunk and potentially sends/edits the message.
    ///
    /// If enough time has elapsed since the last edit, the accumulated
    /// buffer is sent to Telegram. If the buffer exceeds the split
    /// threshold, the message is finalized and a new one is started.
    pub async fn push_chunk(&mut self, text: &str) -> Result<(), BlufioError> {
        self.buffer.push_str(text);

        // Check if we need to split the message
        if self.buffer.len() > SPLIT_THRESHOLD {
            let (first, rest) = split_at_paragraph_boundary(&self.buffer, SPLIT_THRESHOLD);
            let first = first.to_string();
            let rest = rest.to_string();

            // Finalize the current message with the first part
            self.buffer = first;
            self.do_send_or_edit().await?;

            // Start a new message with the remainder
            if let Some(mid) = self.message_id.take() {
                self.messages_sent.push(mid);
            }
            self.buffer = rest;
            return Ok(());
        }

        // Check throttle
        if self.last_edit.elapsed() >= self.throttle {
            self.do_send_or_edit().await?;
        }

        Ok(())
    }

    /// Sends the final version of the accumulated text.
    ///
    /// Must be called after the last chunk to ensure all content is delivered.
    pub async fn finalize(&mut self) -> Result<(), BlufioError> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        self.do_send_or_edit().await?;

        if let Some(mid) = self.message_id.take() {
            self.messages_sent.push(mid);
        }

        Ok(())
    }

    /// Returns the IDs of all messages sent during this streaming session.
    pub fn message_ids(&self) -> &[MessageId] {
        &self.messages_sent
    }

    /// Internal: sends or edits the message with the current buffer content.
    async fn do_send_or_edit(&mut self) -> Result<(), BlufioError> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let escaped = markdown::format_for_telegram(&self.buffer);

        match self.message_id {
            None => {
                // First send
                let sent = self
                    .bot
                    .send_message(self.chat_id, &escaped)
                    .parse_mode(ParseMode::MarkdownV2)
                    .await
                    .map_err(|e| {
                        // If MarkdownV2 parsing fails, retry without parse mode
                        debug!(error = %e, "MarkdownV2 send failed, will retry as plain text");
                        BlufioError::Channel {
                            message: format!("failed to send message: {e}"),
                            source: Some(Box::new(e)),
                        }
                    });

                match sent {
                    Ok(msg) => {
                        self.message_id = Some(msg.id);
                        self.last_edit = Instant::now();
                    }
                    Err(_) => {
                        // Fallback: send as plain text
                        let sent = self
                            .bot
                            .send_message(self.chat_id, &self.buffer)
                            .await
                            .map_err(|e| BlufioError::Channel {
                                message: format!("failed to send plain text message: {e}"),
                                source: Some(Box::new(e)),
                            })?;
                        self.message_id = Some(sent.id);
                        self.last_edit = Instant::now();
                    }
                }
            }
            Some(msg_id) => {
                // Edit existing message
                let result = self
                    .bot
                    .edit_message_text(self.chat_id, msg_id, &escaped)
                    .parse_mode(ParseMode::MarkdownV2)
                    .await;

                match result {
                    Ok(_) => {
                        self.last_edit = Instant::now();
                    }
                    Err(e) => {
                        // Telegram may return "message is not modified" if content hasn't changed
                        let err_str = e.to_string();
                        if err_str.contains("message is not modified") {
                            debug!("message unchanged, skipping edit");
                        } else if err_str.contains("can't parse entities") {
                            // Fallback: edit as plain text
                            warn!(error = %e, "MarkdownV2 edit failed, retrying as plain text");
                            self.bot
                                .edit_message_text(self.chat_id, msg_id, &self.buffer)
                                .await
                                .map_err(|e| BlufioError::Channel {
                                    message: format!("failed to edit message: {e}"),
                                    source: Some(Box::new(e)),
                                })?;
                            self.last_edit = Instant::now();
                        } else {
                            return Err(BlufioError::Channel {
                                message: format!("failed to edit message: {e}"),
                                source: Some(Box::new(e)),
                            });
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

/// Splits text at a paragraph boundary before `max_len`.
///
/// Priority: double newline > single newline > space > hard split.
pub fn split_at_paragraph_boundary(text: &str, max_len: usize) -> (&str, &str) {
    if text.len() <= max_len {
        return (text, "");
    }

    let search_region = &text[..max_len];

    // Try to find last paragraph boundary (double newline)
    if let Some(pos) = search_region.rfind("\n\n") {
        return (&text[..pos], text[pos + 2..].trim_start());
    }

    // Try to find last single newline
    if let Some(pos) = search_region.rfind('\n') {
        return (&text[..pos], text[pos + 1..].trim_start());
    }

    // Try to find last space
    if let Some(pos) = search_region.rfind(' ') {
        return (&text[..pos], &text[pos + 1..]);
    }

    // Hard split at max_len
    (&text[..max_len], &text[max_len..])
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
