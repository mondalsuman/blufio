// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Message attribution formatting for bridged messages.
//!
//! Formats bridged messages with sender attribution so users on the target
//! channel can see who sent the original message and from which platform.

/// Format a bridged message with sender attribution.
///
/// Returns a string in the format `[Channel/Sender] content`.
///
/// # Arguments
/// - `source_channel`: The originating channel (e.g., "telegram", "discord").
/// - `sender_name`: The sender's display name.
/// - `content`: The original message text.
pub fn format_bridged_message(source_channel: &str, sender_name: &str, content: &str) -> String {
    let channel_display = capitalize_channel(source_channel);
    let name = if sender_name.is_empty() {
        "unknown"
    } else {
        sender_name
    };
    format!("[{channel_display}/{name}] {content}")
}

/// Capitalize the first character of a channel name.
///
/// "telegram" -> "Telegram", "discord" -> "Discord", "irc" -> "Irc".
pub fn capitalize_channel(channel: &str) -> String {
    let mut chars = channel.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let mut s = first.to_uppercase().to_string();
            s.extend(chars);
            s
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_with_all_fields() {
        let result = format_bridged_message("telegram", "Alice", "Hello!");
        assert_eq!(result, "[Telegram/Alice] Hello!");
    }

    #[test]
    fn format_with_empty_sender_name() {
        let result = format_bridged_message("discord", "", "Hi there");
        assert_eq!(result, "[Discord/unknown] Hi there");
    }

    #[test]
    fn capitalize_works() {
        assert_eq!(capitalize_channel("telegram"), "Telegram");
        assert_eq!(capitalize_channel("discord"), "Discord");
        assert_eq!(capitalize_channel("irc"), "Irc");
        assert_eq!(capitalize_channel("matrix"), "Matrix");
        assert_eq!(capitalize_channel(""), "");
    }

    #[test]
    fn format_preserves_content() {
        let content =
            "This is a longer message with special chars: @user #channel https://example.com";
        let result = format_bridged_message("slack", "Bob", content);
        assert_eq!(result, format!("[Slack/Bob] {content}"));
    }
}
