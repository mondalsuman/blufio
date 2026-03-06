// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Word-boundary message splitting for IRC PRIVMSG.
//!
//! IRC RFC 2812 limits a single message line to 512 bytes (including `\r\n`).
//! The actual payload available depends on the protocol prefix overhead:
//! `:nick!~nick@host PRIVMSG target :`
//!
//! This module splits long messages at word boundaries to fit within the limit.

/// Split a message into chunks that fit within the IRC PRIVMSG line length limit.
///
/// The IRC protocol line limit is 512 bytes including `\r\n`. The overhead is:
/// `:nick!~nick@host PRIVMSG target :` plus `\r\n` (2 bytes).
///
/// We estimate the prefix overhead conservatively as:
/// `nick.len() * 2 + 15 + target.len() + 4 + 2`
///
/// Arguments:
/// - `target`: The channel or nick to send to.
/// - `nick`: The bot's nickname.
/// - `message`: The message text to split.
/// - `max_line_bytes`: Total line byte limit (typically 512).
///
/// Returns a vector of message chunks that each fit within the available payload.
pub fn split_message(
    target: &str,
    nick: &str,
    message: &str,
    max_line_bytes: usize,
) -> Vec<String> {
    if message.is_empty() {
        return vec![];
    }

    // Calculate protocol overhead.
    // Format: `:nick!~nick@host PRIVMSG target :message\r\n`
    // Overhead = 1(:) + nick + 2(!~) + nick + 6(@host ) + 8(PRIVMSG ) + target + 2( :) + 2(\r\n)
    let overhead = 1 + nick.len() + 2 + nick.len() + 6 + 8 + target.len() + 2 + 2;

    let available = if max_line_bytes > overhead {
        max_line_bytes - overhead
    } else {
        // Extremely unlikely, but fall back to a safe minimum.
        50
    };

    let mut chunks = Vec::new();
    let mut remaining = message;

    while !remaining.is_empty() {
        if remaining.len() <= available {
            chunks.push(remaining.to_string());
            break;
        }

        // Find the last space within the available range for word-boundary split.
        let split_at = if let Some(pos) = remaining[..available].rfind(' ') {
            pos
        } else {
            // No space found; force split at byte boundary.
            available
        };

        let (chunk, rest) = remaining.split_at(split_at);
        chunks.push(chunk.to_string());

        // Skip the space separator if we split at a word boundary.
        remaining = rest.strip_prefix(' ').unwrap_or(rest);
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_message_single_chunk() {
        let chunks = split_message("#test", "bot", "Hello, world!", 512);
        assert_eq!(chunks, vec!["Hello, world!"]);
    }

    #[test]
    fn long_message_splits_at_word_boundary() {
        let long_msg = "word ".repeat(100); // ~500 chars
        let chunks = split_message("#test", "bot", long_msg.trim(), 512);
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            // Each chunk + overhead should fit in 512 bytes.
            let overhead = 1 + 3 + 2 + 3 + 6 + 8 + 5 + 2 + 2; // nick="bot", target="#test"
            assert!(
                chunk.len() + overhead <= 512,
                "chunk too long: {}",
                chunk.len()
            );
        }
    }

    #[test]
    fn no_space_string_splits_at_byte_boundary() {
        let long_word = "a".repeat(600);
        let chunks = split_message("#test", "bot", &long_word, 512);
        assert!(chunks.len() > 1);
        // Reassemble should equal original.
        let reassembled: String = chunks.join("");
        assert_eq!(reassembled, long_word);
    }

    #[test]
    fn empty_input_returns_empty() {
        let chunks = split_message("#test", "bot", "", 512);
        assert!(chunks.is_empty());
    }

    #[test]
    fn reassembled_content_matches_original() {
        let msg = "The quick brown fox jumps over the lazy dog. ".repeat(20);
        let msg = msg.trim();
        let chunks = split_message("#channel", "mybot", msg, 512);
        // Reassemble with spaces.
        let reassembled = chunks.join(" ");
        assert_eq!(reassembled, msg);
    }
}
