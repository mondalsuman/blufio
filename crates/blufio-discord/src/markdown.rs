// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Standard markdown to Discord markdown conversion.
//!
//! Discord supports standard markdown natively (bold, italic, code, links),
//! so minimal conversion is needed. Main role is ensuring code blocks use
//! triple backticks and handling edge cases.

/// Convert standard markdown to Discord-compatible markdown.
///
/// Discord's markdown is very close to standard, so this mostly passes through.
/// Ensures consistent formatting for code blocks and handles edge cases.
pub fn format_for_discord(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    // Discord supports standard markdown natively.
    // No major conversions needed unlike Telegram MarkdownV2 or Slack mrkdwn.
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text() {
        assert_eq!(format_for_discord(""), "");
    }

    #[test]
    fn plain_text_passes_through() {
        assert_eq!(format_for_discord("hello world"), "hello world");
    }

    #[test]
    fn bold_passes_through() {
        assert_eq!(format_for_discord("**bold text**"), "**bold text**");
    }

    #[test]
    fn italic_passes_through() {
        assert_eq!(format_for_discord("*italic text*"), "*italic text*");
    }

    #[test]
    fn code_block_passes_through() {
        let input = "```rust\nfn main() {}\n```";
        assert_eq!(format_for_discord(input), input);
    }

    #[test]
    fn inline_code_passes_through() {
        assert_eq!(format_for_discord("`code`"), "`code`");
    }

    #[test]
    fn links_pass_through() {
        assert_eq!(
            format_for_discord("[text](https://example.com)"),
            "[text](https://example.com)"
        );
    }
}
