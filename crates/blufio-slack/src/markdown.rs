// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Standard markdown to Slack mrkdwn conversion.
//!
//! Slack's mrkdwn format differs significantly from standard markdown:
//! - Bold: `**text**` -> `*text*`
//! - Italic: `*text*` -> `_text_`
//! - Strikethrough: `~~text~~` -> `~text~`
//! - Links: `[text](url)` -> `<url|text>`
//! - Headers: `# text` -> `*text*` (bold, no native headers)
//! - Code blocks and inline code pass through unchanged.

use regex::Regex;

/// Unique markers that won't appear in normal text.
const BOLD_OPEN: &str = "\x01BOLD_OPEN\x01";
const BOLD_CLOSE: &str = "\x01BOLD_CLOSE\x01";
const CODE_BLOCK_MARKER: &str = "\x01CODE_BLOCK_";
const INLINE_CODE_MARKER: &str = "\x01INLINE_CODE_";
const MARKER_END: &str = "\x01";

/// Convert standard markdown to Slack mrkdwn syntax.
///
/// Handles bold, italic, strikethrough, links, and headers while
/// preserving code blocks and inline code unchanged.
pub fn markdown_to_mrkdwn(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    let mut result = text.to_string();
    let mut code_blocks: Vec<String> = Vec::new();

    // Phase 1: Protect code blocks and inline code with placeholders.

    // Protect fenced code blocks (```...```)
    let code_block_re = Regex::new(r"(?s)```[^\n]*\n.*?```").expect("valid regex: code_block");
    loop {
        let current = result.clone();
        if let Some(m) = code_block_re.find(&current) {
            let idx = code_blocks.len();
            let placeholder = format!("{CODE_BLOCK_MARKER}{idx}{MARKER_END}");
            code_blocks.push(m.as_str().to_string());
            result = format!(
                "{}{}{}",
                &current[..m.start()],
                placeholder,
                &current[m.end()..]
            );
        } else {
            break;
        }
    }

    // Protect inline code (`...`)
    let inline_code_re = Regex::new(r"`[^`]+`").expect("valid regex: inline_code");
    let mut inline_codes: Vec<String> = Vec::new();
    loop {
        let current = result.clone();
        if let Some(m) = inline_code_re.find(&current) {
            let idx = inline_codes.len();
            let placeholder = format!("{INLINE_CODE_MARKER}{idx}{MARKER_END}");
            inline_codes.push(m.as_str().to_string());
            result = format!(
                "{}{}{}",
                &current[..m.start()],
                placeholder,
                &current[m.end()..]
            );
        } else {
            break;
        }
    }

    // Phase 2: Convert markdown syntax to mrkdwn.

    // Convert links: [text](url) -> <url|text>
    let link_re = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").expect("valid regex: link");
    result = link_re.replace_all(&result, "<$2|$1>").to_string();

    // Convert bold: **text** -> temporary markers
    let bold_re = Regex::new(r"\*\*(.+?)\*\*").expect("valid regex: bold");
    result = bold_re
        .replace_all(&result, |caps: &regex::Captures| {
            format!("{BOLD_OPEN}{}{BOLD_CLOSE}", &caps[1])
        })
        .to_string();

    // Convert italic: remaining *text* -> _text_
    // Note: Must use ${1} not $1_ because the regex crate interprets $1_ as
    // a capture group named "1_" (underscore is a valid identifier char).
    let italic_re = Regex::new(r"\*([^*]+?)\*").expect("valid regex: italic");
    result = italic_re.replace_all(&result, "_${1}_").to_string();

    // Restore bold markers to Slack bold (*text*)
    result = result.replace(BOLD_OPEN, "*").replace(BOLD_CLOSE, "*");

    // Convert strikethrough: ~~text~~ -> ~text~
    let strike_re = Regex::new(r"~~(.+?)~~").expect("valid regex: strikethrough");
    result = strike_re.replace_all(&result, "~$1~").to_string();

    // Convert headers: # text -> *text* (bold)
    let header_re = Regex::new(r"(?m)^#{1,6}\s+(.+)$").expect("valid regex: header");
    result = header_re.replace_all(&result, "*$1*").to_string();

    // Phase 3: Restore protected code blocks and inline code.
    for (i, original) in code_blocks.iter().enumerate() {
        let placeholder = format!("{CODE_BLOCK_MARKER}{i}{MARKER_END}");
        result = result.replace(&placeholder, original);
    }
    for (i, original) in inline_codes.iter().enumerate() {
        let placeholder = format!("{INLINE_CODE_MARKER}{i}{MARKER_END}");
        result = result.replace(&placeholder, original);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text() {
        assert_eq!(markdown_to_mrkdwn(""), "");
    }

    #[test]
    fn plain_text_passes_through() {
        assert_eq!(markdown_to_mrkdwn("hello world"), "hello world");
    }

    #[test]
    fn bold_converts() {
        assert_eq!(markdown_to_mrkdwn("**bold text**"), "*bold text*");
    }

    #[test]
    fn italic_converts() {
        assert_eq!(markdown_to_mrkdwn("*italic text*"), "_italic text_");
    }

    #[test]
    fn bold_and_italic() {
        let input = "**bold** and *italic*";
        let output = markdown_to_mrkdwn(input);
        assert_eq!(output, "*bold* and _italic_");
    }

    #[test]
    fn strikethrough_converts() {
        assert_eq!(markdown_to_mrkdwn("~~deleted~~"), "~deleted~");
    }

    #[test]
    fn link_converts() {
        assert_eq!(
            markdown_to_mrkdwn("[click here](https://example.com)"),
            "<https://example.com|click here>"
        );
    }

    #[test]
    fn header_converts_to_bold() {
        assert_eq!(markdown_to_mrkdwn("# Title"), "*Title*");
        assert_eq!(markdown_to_mrkdwn("## Subtitle"), "*Subtitle*");
        assert_eq!(markdown_to_mrkdwn("### Section"), "*Section*");
    }

    #[test]
    fn code_block_preserved() {
        let input = "```rust\nfn main() {}\n```";
        assert_eq!(markdown_to_mrkdwn(input), input);
    }

    #[test]
    fn inline_code_preserved() {
        assert_eq!(markdown_to_mrkdwn("`code`"), "`code`");
    }

    #[test]
    fn code_block_with_bold_inside_preserved() {
        let input = "before ```\n**not bold**\n``` after";
        let result = markdown_to_mrkdwn(input);
        assert!(result.contains("**not bold**"));
    }

    #[test]
    fn multiple_links() {
        let input = "[a](http://a.com) and [b](http://b.com)";
        let output = markdown_to_mrkdwn(input);
        assert_eq!(output, "<http://a.com|a> and <http://b.com|b>");
    }
}
