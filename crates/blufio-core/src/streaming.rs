// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Shared streaming message editing infrastructure.
//!
//! Provides the [`StreamingEditorOps`] trait for platform-specific send/edit
//! operations and [`StreamingBuffer`] for shared buffering, throttling, and
//! paragraph-boundary splitting across all channel adapters.

use std::time::{Duration, Instant};

use async_trait::async_trait;

use crate::error::BlufioError;

/// Platform-specific operations for streaming message editing.
///
/// Each channel adapter implements this trait with its platform-specific
/// send/edit API calls. The shared [`StreamingBuffer`] handles buffering,
/// throttling, and paragraph-boundary splitting.
#[async_trait]
pub trait StreamingEditorOps: Send {
    /// Send the initial message, returning a platform-specific message ID string.
    async fn send_initial(&mut self, text: &str) -> Result<String, BlufioError>;

    /// Edit an existing message identified by `msg_id` with updated text.
    async fn edit_message(&mut self, msg_id: &str, text: &str) -> Result<(), BlufioError>;

    /// Platform-specific maximum message length in characters.
    fn max_message_length(&self) -> usize;

    /// Platform-specific throttle interval between edits.
    fn throttle_interval(&self) -> Duration;
}

/// Shared streaming buffer with throttle and paragraph-boundary splitting.
///
/// Works with any [`StreamingEditorOps`] implementation to provide consistent
/// streaming behavior across all channel adapters.
pub struct StreamingBuffer {
    buffer: String,
    last_edit: Instant,
    current_message_id: Option<String>,
    messages_sent: Vec<String>,
    split_threshold: usize,
}

impl StreamingBuffer {
    /// Create a new streaming buffer with the given split threshold.
    ///
    /// The split threshold should be ~90% of the platform's max message length
    /// to leave room for formatting overhead.
    pub fn new(split_threshold: usize) -> Self {
        Self {
            buffer: String::new(),
            last_edit: Instant::now() - Duration::from_secs(10), // Allow immediate first send
            current_message_id: None,
            messages_sent: Vec::new(),
            split_threshold,
        }
    }

    /// Append a text chunk and potentially send/edit via the editor.
    pub async fn push_chunk<E: StreamingEditorOps>(
        &mut self,
        editor: &mut E,
        text: &str,
    ) -> Result<(), BlufioError> {
        self.buffer.push_str(text);

        // Check if we need to split the message
        if self.buffer.len() > self.split_threshold {
            let (first, rest) =
                split_at_paragraph_boundary(&self.buffer, self.split_threshold);
            let first = first.to_string();
            let rest = rest.to_string();

            // Finalize the current message with the first part
            self.buffer = first;
            self.do_send_or_edit(editor).await?;

            // Start a new message with the remainder
            if let Some(mid) = self.current_message_id.take() {
                self.messages_sent.push(mid);
            }
            self.buffer = rest;
            return Ok(());
        }

        // Check throttle
        if self.last_edit.elapsed() >= editor.throttle_interval() {
            self.do_send_or_edit(editor).await?;
        }

        Ok(())
    }

    /// Send the final version of the accumulated text.
    pub async fn finalize<E: StreamingEditorOps>(
        &mut self,
        editor: &mut E,
    ) -> Result<(), BlufioError> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        self.do_send_or_edit(editor).await?;

        if let Some(mid) = self.current_message_id.take() {
            self.messages_sent.push(mid);
        }

        Ok(())
    }

    /// Returns the IDs of all messages sent during this streaming session.
    pub fn message_ids(&self) -> &[String] {
        &self.messages_sent
    }

    /// Internal: sends or edits the message with the current buffer content.
    async fn do_send_or_edit<E: StreamingEditorOps>(
        &mut self,
        editor: &mut E,
    ) -> Result<(), BlufioError> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        match &self.current_message_id {
            None => {
                let msg_id = editor.send_initial(&self.buffer).await?;
                self.current_message_id = Some(msg_id);
                self.last_edit = Instant::now();
            }
            Some(msg_id) => {
                editor.edit_message(msg_id, &self.buffer).await?;
                self.last_edit = Instant::now();
            }
        }

        Ok(())
    }
}

/// Splits text at a paragraph boundary before `max_len`.
///
/// Priority: double newline > single newline > space > hard split.
/// Shared across all channel adapters for consistent message splitting.
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

    #[test]
    fn streaming_buffer_new() {
        let buf = StreamingBuffer::new(1800);
        assert_eq!(buf.split_threshold, 1800);
        assert!(buf.buffer.is_empty());
        assert!(buf.current_message_id.is_none());
        assert!(buf.messages_sent.is_empty());
    }

    #[test]
    fn streaming_buffer_message_ids_empty() {
        let buf = StreamingBuffer::new(1800);
        assert!(buf.message_ids().is_empty());
    }
}
