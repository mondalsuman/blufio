// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! MarkdownV2 escaping for Telegram Bot API.
//!
//! Telegram's MarkdownV2 parse mode requires escaping 18 special characters
//! outside of code blocks. Characters inside inline code (`` ` ``) or fenced
//! code blocks (`` ``` ``) must NOT be escaped.

/// Characters that must be escaped in MarkdownV2 outside code blocks.
const SPECIAL_CHARS: &[char] = &[
    '_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!',
];

/// Escapes text for Telegram MarkdownV2 parse mode.
///
/// Splits the input into code and non-code segments, escaping only the
/// non-code segments. Fenced code blocks (`` ``` ``) and inline code (`` ` ``)
/// are preserved without internal escaping.
pub fn escape_markdown_v2(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    let mut result = String::with_capacity(text.len() * 2);
    let mut chars = text.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch == '`' {
            // Check for fenced code block (```)
            let mut backtick_count = 0;
            let mut temp = String::new();
            while chars.peek() == Some(&'`') {
                temp.push(chars.next().unwrap());
                backtick_count += 1;
            }

            if backtick_count >= 3 {
                // Fenced code block: find closing ```
                result.push_str(&temp);
                let mut found_close = false;
                let mut close_count = 0;
                for c in chars.by_ref() {
                    result.push(c);
                    if c == '`' {
                        close_count += 1;
                        if close_count >= 3 {
                            found_close = true;
                            break;
                        }
                    } else {
                        close_count = 0;
                    }
                }
                if !found_close {
                    // Unclosed code block -- just leave as-is
                }
            } else if backtick_count == 1 {
                // Inline code: find closing `
                result.push('`');
                let mut found_close = false;
                for c in chars.by_ref() {
                    result.push(c);
                    if c == '`' {
                        found_close = true;
                        break;
                    }
                }
                if !found_close {
                    // Unclosed inline code -- just leave as-is
                }
            } else {
                // Two backticks -- not standard markdown, escape them
                for _ in 0..backtick_count {
                    result.push('\\');
                    result.push('`');
                }
            }
        } else if SPECIAL_CHARS.contains(&ch) {
            result.push('\\');
            result.push(chars.next().unwrap());
        } else {
            result.push(chars.next().unwrap());
        }
    }

    result
}

/// High-level formatting function for Telegram output.
///
/// Applies MarkdownV2 escaping. Returns empty string for empty input.
pub fn format_for_telegram(text: &str) -> String {
    escape_markdown_v2(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string() {
        assert_eq!(escape_markdown_v2(""), "");
    }

    #[test]
    fn plain_text_no_special_chars() {
        assert_eq!(escape_markdown_v2("Hello world"), "Hello world");
    }

    #[test]
    fn escapes_dots_and_exclamation() {
        assert_eq!(escape_markdown_v2("Hello."), "Hello\\.");
        assert_eq!(escape_markdown_v2("Hello!"), "Hello\\!");
    }

    #[test]
    fn escapes_all_special_characters_without_backtick() {
        // Test all special chars except backtick (which triggers inline code mode).
        let input = "_*[]()~>#+-=|{}.!";
        let expected = "\\_\\*\\[\\]\\(\\)\\~\\>\\#\\+\\-\\=\\|\\{\\}\\.\\!";
        assert_eq!(escape_markdown_v2(input), expected);
    }

    #[test]
    fn lone_backtick_starts_inline_code() {
        // A single backtick without a closing one is treated as unclosed inline code.
        // Characters after it are preserved unescaped (inside "code").
        let input = "before `after.end";
        let result = escape_markdown_v2(input);
        assert!(result.contains("`after.end"));
        assert!(result.starts_with("before "));
    }

    #[test]
    fn preserves_inline_code() {
        let input = "Use `println!()` to print.";
        let result = escape_markdown_v2(input);
        // The backtick-delimited content should NOT be escaped.
        assert!(result.contains("`println!()`"));
        // The period outside should be escaped.
        assert!(result.ends_with("\\."));
    }

    #[test]
    fn preserves_fenced_code_block() {
        let input = "Example:\n```rust\nfn main() {\n    println!(\"Hello!\");\n}\n```\nDone.";
        let result = escape_markdown_v2(input);
        // Code block content should be preserved.
        assert!(result.contains("println!(\"Hello!\")"));
        // "Done." outside code block should have escaped period.
        assert!(result.ends_with("Done\\."));
    }

    #[test]
    fn mixed_text_and_code() {
        let input = "Call `foo()` then run `bar()`.";
        let result = escape_markdown_v2(input);
        assert!(result.contains("`foo()`"));
        assert!(result.contains("`bar()`"));
        assert!(result.ends_with("\\."));
    }

    #[test]
    fn escapes_markdown_formatting_chars() {
        let input = "This is *bold* and _italic_.";
        let expected = "This is \\*bold\\* and \\_italic\\_\\.";
        assert_eq!(escape_markdown_v2(input), expected);
    }

    #[test]
    fn escapes_brackets_and_parens() {
        let input = "See [link](https://example.com)";
        let expected = "See \\[link\\]\\(https://example\\.com\\)";
        assert_eq!(escape_markdown_v2(input), expected);
    }

    #[test]
    fn format_for_telegram_delegates() {
        let input = "Hello!";
        assert_eq!(format_for_telegram(input), "Hello\\!");
    }

    #[test]
    fn handles_unclosed_inline_code() {
        // Unclosed backtick should not panic
        let input = "Use `foo to print";
        let result = escape_markdown_v2(input);
        assert!(result.contains("`foo to print"));
    }

    #[test]
    fn handles_unclosed_fenced_code() {
        let input = "```\nsome code without closing";
        let result = escape_markdown_v2(input);
        assert!(result.contains("some code without closing"));
    }

    #[test]
    fn escapes_hash_and_tilde() {
        let input = "# Heading ~strikethrough~";
        let expected = "\\# Heading \\~strikethrough\\~";
        assert_eq!(escape_markdown_v2(input), expected);
    }

    #[test]
    fn escapes_pipe_and_equals() {
        let input = "a = b | c";
        let expected = "a \\= b \\| c";
        assert_eq!(escape_markdown_v2(input), expected);
    }

    #[test]
    fn escapes_plus_and_minus() {
        let input = "1 + 2 - 3";
        let expected = "1 \\+ 2 \\- 3";
        assert_eq!(escape_markdown_v2(input), expected);
    }

    #[test]
    fn escapes_curly_braces() {
        let input = "map{key}";
        let expected = "map\\{key\\}";
        assert_eq!(escape_markdown_v2(input), expected);
    }
}
