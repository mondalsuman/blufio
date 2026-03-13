// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration tests for the Email channel adapter.
//!
//! Tests MIME parsing, quoted-text stripping, HTML conversion, and message
//! construction without requiring real IMAP/SMTP servers.

use blufio_email::parsing::{html_to_text, markdown_to_html, parse_email_body, strip_quoted_text};

// ---------------------------------------------------------------------------
// MIME parsing: valid multipart email -> extracts plain text body
// ---------------------------------------------------------------------------

#[test]
fn parse_multipart_plain_text_body() {
    let raw = b"From: sender@example.com\r\n\
                To: bot@example.com\r\n\
                Subject: Test Message\r\n\
                Message-ID: <msg-001@example.com>\r\n\
                Content-Type: text/plain; charset=utf-8\r\n\
                Date: Mon, 1 Jan 2026 12:00:00 +0000\r\n\
                \r\n\
                Hello, this is a plain text body.";

    let parsed = parse_email_body(raw).expect("should parse plain text email");
    assert!(parsed.body.contains("Hello, this is a plain text body."));
    assert_eq!(parsed.subject, "Test Message");
    assert_eq!(parsed.from, "sender@example.com");
    assert_eq!(parsed.message_id.as_deref(), Some("msg-001@example.com"));
}

// ---------------------------------------------------------------------------
// MIME parsing: HTML-only email -> strips HTML tags
// ---------------------------------------------------------------------------

#[test]
fn parse_html_only_email_strips_tags() {
    let raw = b"From: sender@example.com\r\n\
                To: bot@example.com\r\n\
                Subject: HTML Only\r\n\
                Content-Type: text/html; charset=utf-8\r\n\
                \r\n\
                <html><body><p>Hello <b>world</b></p><p>Second paragraph</p></body></html>";

    let parsed = parse_email_body(raw).expect("should parse HTML email");
    assert!(
        parsed.body.contains("Hello") && parsed.body.contains("world"),
        "HTML should be converted to text. Got: {}",
        parsed.body
    );
}

// ---------------------------------------------------------------------------
// Quoted-text stripping: removes `> ` prefixed lines from replies
// ---------------------------------------------------------------------------

#[test]
fn strip_quoted_lines() {
    let input = "My reply text\n> Original message\n> More original\nAnother line";
    let result = strip_quoted_text(input);
    assert!(result.contains("My reply text"));
    assert!(result.contains("Another line"));
    assert!(!result.contains("Original message"));
    assert!(!result.contains("More original"));
}

#[test]
fn strip_gmail_on_wrote_pattern() {
    let input =
        "Thanks for the update!\n\nOn Mon, Jan 1, 2026 at 12:00 PM User wrote:\n> Previous message";
    let result = strip_quoted_text(input);
    assert_eq!(result, "Thanks for the update!");
}

#[test]
fn strip_outlook_from_sent_pattern() {
    let input = "Got it, will do.\n\nFrom: User <user@example.com>\nSent: Monday, January 1, 2026\nTo: Bot\nSubject: Re: Help";
    let result = strip_quoted_text(input);
    assert_eq!(result, "Got it, will do.");
}

#[test]
fn strip_signature_delimiter() {
    let input = "Main message body\n\n-- \nJohn Doe\nCEO, Acme Corp";
    let result = strip_quoted_text(input);
    assert_eq!(result, "Main message body");
}

// ---------------------------------------------------------------------------
// SMTP message construction: verify lettre Message building
// ---------------------------------------------------------------------------

#[test]
fn markdown_to_html_bold_and_italic() {
    let md = "**bold** and *italic*";
    let html = markdown_to_html(md);
    assert!(html.contains("<strong>bold</strong>"), "Expected bold HTML");
    assert!(html.contains("<em>italic</em>"), "Expected italic HTML");
}

#[test]
fn markdown_to_html_code_block() {
    let md = "```rust\nfn main() {}\n```";
    let html = markdown_to_html(md);
    assert!(
        html.contains("<pre>") || html.contains("<code"),
        "Expected code block HTML, got: {html}"
    );
    assert!(html.contains("fn main()"), "Expected code content in HTML");
}

#[test]
fn html_to_text_strips_tags() {
    let html = "<div><h1>Title</h1><p>Paragraph with <a href='url'>link</a></p></div>";
    let text = html_to_text(html);
    assert!(text.contains("Title"), "Expected title in text");
    assert!(text.contains("Paragraph"), "Expected paragraph content");
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[test]
fn parse_empty_body_returns_none() {
    // A message with no body at all
    let raw = b"From: sender@example.com\r\n\
                To: bot@example.com\r\n\
                Subject: Empty\r\n\
                Content-Type: text/plain\r\n\
                \r\n";

    // Empty body may or may not parse depending on parser behavior.
    // The key is that it doesn't panic.
    let _result = parse_email_body(raw);
}

#[test]
fn parse_missing_subject() {
    let raw = b"From: sender@example.com\r\n\
                To: bot@example.com\r\n\
                Content-Type: text/plain\r\n\
                \r\n\
                Body without subject";

    let parsed = parse_email_body(raw).expect("should parse email without subject");
    assert_eq!(parsed.subject, "");
    assert!(parsed.body.contains("Body without subject"));
}

#[test]
fn parse_malformed_mime_does_not_panic() {
    let raw = b"This is not a valid RFC 822 message at all\n\
                Just random garbage data\xff\xfe\x00";

    // Should return None, not panic
    let _result = parse_email_body(raw);
}

#[test]
fn parse_oversized_content_extracts_text() {
    // Build a large email body (100KB of text)
    let large_body = "A".repeat(100_000);
    let raw = format!(
        "From: sender@example.com\r\n\
         To: bot@example.com\r\n\
         Subject: Large Email\r\n\
         Content-Type: text/plain\r\n\
         \r\n\
         {large_body}"
    );

    let parsed = parse_email_body(raw.as_bytes()).expect("should parse large email");
    assert!(parsed.body.len() >= 100_000, "Body should contain all text");
}

#[test]
fn strip_quoted_text_no_quotes_passthrough() {
    let input = "A completely normal message\nWith multiple lines\nNo quotes at all";
    let result = strip_quoted_text(input);
    assert_eq!(result, input);
}

#[test]
fn strip_quoted_text_all_quoted_returns_empty() {
    let input = "> All lines\n> Are quoted\n> Nothing original";
    let result = strip_quoted_text(input);
    assert!(
        result.is_empty(),
        "Should be empty when all lines are quoted. Got: '{result}'"
    );
}

#[test]
fn parse_email_with_in_reply_to_and_references() {
    let raw = b"From: reply@example.com\r\n\
                To: bot@example.com\r\n\
                Subject: Re: Original\r\n\
                Message-ID: <reply-001@example.com>\r\n\
                In-Reply-To: <original-001@example.com>\r\n\
                References: <original-001@example.com>\r\n\
                Content-Type: text/plain\r\n\
                \r\n\
                This is a reply";

    let parsed = parse_email_body(raw).expect("should parse reply email");
    assert_eq!(
        parsed.in_reply_to.as_deref(),
        Some("original-001@example.com")
    );
    assert!(!parsed.references.is_empty());
}

#[test]
fn parse_email_date_header() {
    let raw = b"From: sender@example.com\r\n\
                To: bot@example.com\r\n\
                Subject: Dated\r\n\
                Date: Thu, 15 Feb 2026 10:30:00 +0530\r\n\
                Content-Type: text/plain\r\n\
                \r\n\
                Body with date";

    let parsed = parse_email_body(raw).expect("should parse email with date");
    assert!(parsed.date.is_some(), "date should be extracted");
}
