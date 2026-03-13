// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Email MIME parsing and quoted-text stripping utilities.
//!
//! Handles RFC 822 message parsing via `mail-parser`, quoted-text stripping
//! for Gmail, Outlook, and Apple Mail reply patterns, and HTML-to-text /
//! Markdown-to-HTML conversions.

use mail_parser::MessageParser;

/// Parsed email extracted from raw RFC 822 bytes.
#[derive(Debug, Clone)]
pub struct ParsedEmail {
    /// Email subject line.
    pub subject: String,
    /// Cleaned body text (quoted text stripped, HTML converted if needed).
    pub body: String,
    /// Sender email address.
    pub from: String,
    /// Message-ID header value.
    pub message_id: Option<String>,
    /// In-Reply-To header value (first entry).
    pub in_reply_to: Option<String>,
    /// References header values.
    pub references: Vec<String>,
    /// Date header as string.
    pub date: Option<String>,
}

/// Parse a raw RFC 822 email message into a [`ParsedEmail`].
///
/// Extracts subject, body (text preferred, HTML fallback converted via
/// `html2text`), sender, message-id, in-reply-to, and references headers.
/// Prepends subject to body and strips quoted text before returning.
pub fn parse_email_body(raw: &[u8]) -> Option<ParsedEmail> {
    let message = MessageParser::default().parse(raw)?;

    // Extract text body; fall back to HTML converted to text.
    let body = if let Some(text) = message.body_text(0) {
        text.to_string()
    } else if let Some(html_body) = message.body_html(0) {
        html_to_text(&html_body)
    } else {
        return None;
    };

    let subject = message.subject().unwrap_or("").to_string();

    // Extract sender from the From header.
    let from = message
        .from()
        .and_then(|addr| match addr {
            mail_parser::Address::List(list) => list
                .first()
                .and_then(|a| a.address.as_ref())
                .map(|a| a.to_string()),
            mail_parser::Address::Group(groups) => groups
                .first()
                .and_then(|g| g.addresses.first())
                .and_then(|a| a.address.as_ref())
                .map(|a| a.to_string()),
        })
        .unwrap_or_default();

    let message_id = message.message_id().map(|s| s.to_string());

    // In-Reply-To can be Text or TextList.
    let in_reply_to = {
        let hv = message.in_reply_to();
        if let Some(text) = hv.as_text() {
            Some(text.to_string())
        } else {
            hv.as_text_list()
                .and_then(|list| list.first())
                .map(|s| s.to_string())
        }
    };

    // References can be Text or TextList.
    let references = {
        let hv = message.references();
        if let Some(list) = hv.as_text_list() {
            list.iter().map(|s| s.to_string()).collect()
        } else if let Some(text) = hv.as_text() {
            vec![text.to_string()]
        } else {
            vec![]
        }
    };

    let date = message.date().map(|d| {
        let sign = if d.tz_before_gmt { '-' } else { '+' };
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}{:02}:{:02}",
            d.year, d.month, d.day, d.hour, d.minute, d.second, sign, d.tz_hour, d.tz_minute
        )
    });

    // Prepend subject to body per CONTEXT.md.
    let full_body = if subject.is_empty() {
        body
    } else {
        format!("Subject: {subject}\n\n{body}")
    };

    let cleaned_body = strip_quoted_text(&full_body);

    Some(ParsedEmail {
        subject,
        body: cleaned_body,
        from,
        message_id,
        in_reply_to,
        references,
        date,
    })
}

/// Strip quoted text from an email body.
///
/// Handles the following patterns:
/// - Lines starting with `>` or `> ` (inline quotes)
/// - Gmail/Apple Mail: lines starting with `"On "` and ending with `" wrote:"`
/// - Outlook: lines starting with `"From: "` followed by `"Sent: "`
/// - Signature delimiters: `"-- "` or `"--"`
///
/// Processing stops at the first stop pattern encountered; inline `>` lines
/// are simply skipped.
pub fn strip_quoted_text(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut result = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_end();

        // Signature delimiter: stop processing.
        if trimmed == "-- " || trimmed == "--" {
            break;
        }

        // Gmail/Apple Mail pattern: "On ... wrote:"
        if trimmed.starts_with("On ") && trimmed.ends_with(" wrote:") {
            break;
        }

        // Outlook pattern: "From: ..." followed by "Sent: ..."
        if trimmed.starts_with("From: ") {
            if i + 1 < lines.len() && lines[i + 1].trim_end().starts_with("Sent: ") {
                break;
            }
        }

        // Skip inline quoted lines (starting with >).
        if trimmed.starts_with('>') {
            i += 1;
            continue;
        }

        result.push(line);
        i += 1;
    }

    // Trim trailing empty lines.
    while result.last().is_some_and(|l| l.trim().is_empty()) {
        result.pop();
    }

    result.join("\n")
}

/// Convert HTML to plaintext at 80-column width.
pub fn html_to_text(html: &str) -> String {
    html2text::from_read(html.as_bytes(), 80)
        .unwrap_or_else(|_| html.to_string())
        .trim()
        .to_string()
}

/// Convert Markdown to HTML using comrak.
pub fn markdown_to_html(markdown: &str) -> String {
    comrak::markdown_to_html(markdown, &comrak::Options::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_quoted_text_gmail() {
        let input = "Hello\n\nOn Mon, Jan 1, 2026 at 12:00 PM User wrote:\n> old text";
        assert_eq!(strip_quoted_text(input), "Hello");
    }

    #[test]
    fn test_strip_quoted_text_outlook() {
        let input =
            "Hello\n\nFrom: User\nSent: Monday\nTo: Bot\nSubject: Re: Help";
        assert_eq!(strip_quoted_text(input), "Hello");
    }

    #[test]
    fn test_strip_quoted_text_apple_mail() {
        let input =
            "Hello\n\nOn Jan 1, 2026, at 12:00 PM, User <user@example.com> wrote:\n> old";
        assert_eq!(strip_quoted_text(input), "Hello");
    }

    #[test]
    fn test_strip_quoted_text_signature() {
        let input = "Hello\n\n-- \nJohn Doe\nCEO";
        assert_eq!(strip_quoted_text(input), "Hello");
    }

    #[test]
    fn test_strip_quoted_text_no_quotes() {
        let input = "Just a plain message";
        assert_eq!(strip_quoted_text(input), "Just a plain message");
    }

    #[test]
    fn test_strip_quoted_text_inline_quotes() {
        let input = "> quoted\nnot quoted\n> more quoted";
        assert_eq!(strip_quoted_text(input), "not quoted");
    }

    #[test]
    fn test_html_to_text() {
        let html = "<p>Hello <b>world</b></p>";
        let result = html_to_text(html);
        assert!(
            result.contains("Hello") && result.contains("world"),
            "Expected 'Hello world', got: {result}"
        );
    }

    #[test]
    fn test_markdown_to_html() {
        let md = "**bold**";
        let result = markdown_to_html(md);
        assert!(
            result.contains("<strong>bold</strong>"),
            "Expected <strong>bold</strong>, got: {result}"
        );
    }

    #[test]
    fn test_parse_email_basic() {
        let raw = b"From: sender@example.com\r\n\
                     To: recipient@example.com\r\n\
                     Subject: Test Subject\r\n\
                     Message-ID: <abc123@example.com>\r\n\
                     Date: Mon, 1 Jan 2026 12:00:00 +0000\r\n\
                     \r\n\
                     Hello, this is the body.";

        let parsed = parse_email_body(raw).expect("should parse");
        assert_eq!(parsed.subject, "Test Subject");
        assert!(parsed.body.contains("Hello, this is the body."));
        assert!(parsed.body.contains("Subject: Test Subject"));
        assert_eq!(parsed.from, "sender@example.com");
        assert_eq!(
            parsed.message_id.as_deref(),
            Some("abc123@example.com")
        );
    }
}
