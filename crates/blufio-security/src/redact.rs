// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Secret redaction for log output and error messages.
//!
//! Two complementary mechanisms:
//! 1. **Regex-based**: Catches known secret formats (API keys, Bearer tokens, etc.)
//! 2. **Exact-match**: Catches vault-stored values loaded at runtime.

use std::io::Write;
use std::sync::{Arc, LazyLock, RwLock};

use regex::Regex;

/// Known secret patterns to redact from output.
static REDACTION_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        // Anthropic API keys: sk-ant-api03-...
        Regex::new(r"sk-ant-[a-zA-Z0-9_\-]{20,}").unwrap(),
        // Generic secret keys: sk-... (OpenAI style)
        Regex::new(r"sk-[a-zA-Z0-9]{20,}").unwrap(),
        // Bearer tokens in headers
        Regex::new(r"Bearer\s+[a-zA-Z0-9._\-]{10,}").unwrap(),
        // Telegram bot tokens: 123456789:ABCdefGHI-zyx57W2v1u123ew11
        Regex::new(r"\d{8,10}:[a-zA-Z0-9_\-]{35}").unwrap(),
    ]
});

/// The redaction placeholder.
const REDACTED: &str = "[REDACTED]";

/// Redact secrets from a string using regex patterns and optional exact-match values.
///
/// This is a standalone function for use outside the logging pipeline (e.g.,
/// error messages, debug output).
pub fn redact(input: &str, vault_values: &[String]) -> String {
    let mut result = input.to_string();

    // Apply regex patterns.
    for pattern in REDACTION_PATTERNS.iter() {
        result = pattern.replace_all(&result, REDACTED).to_string();
    }

    // Apply exact-match vault values (longest first to avoid partial matches).
    let mut sorted_values: Vec<&String> = vault_values.iter().collect();
    sorted_values.sort_by_key(|v| std::cmp::Reverse(v.len()));
    for value in sorted_values {
        if !value.is_empty() {
            result = result.replace(value.as_str(), REDACTED);
        }
    }

    result
}

/// A writer wrapper that redacts secrets from output.
///
/// Wraps any `Write` implementor and replaces known secret patterns and
/// exact vault-stored values with `[REDACTED]`.
pub struct RedactingWriter<W> {
    inner: W,
    vault_values: Arc<RwLock<Vec<String>>>,
}

impl<W: Write> RedactingWriter<W> {
    /// Create a new redacting writer.
    pub fn new(inner: W, vault_values: Arc<RwLock<Vec<String>>>) -> Self {
        Self {
            inner,
            vault_values,
        }
    }

    /// Add a new vault value to the redaction list.
    pub fn add_vault_value(vault_values: &Arc<RwLock<Vec<String>>>, value: String) {
        if let Ok(mut values) = vault_values.write()
            && !values.contains(&value)
        {
            values.push(value);
        }
    }
}

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let input = String::from_utf8_lossy(buf);
        let vault_vals = self
            .vault_values
            .read()
            .map(|v| v.clone())
            .unwrap_or_default();
        let redacted = redact(&input, &vault_vals);
        self.inner.write_all(redacted.as_bytes())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_anthropic_api_key() {
        let input = "Using key sk-ant-api03-abcdefghijklmnopqrstuvwxyz for request";
        let result = redact(input, &[]);
        assert!(result.contains(REDACTED));
        assert!(!result.contains("sk-ant-api03"));
    }

    #[test]
    fn redacts_generic_sk_key() {
        let input = "key=sk-abcdefghijklmnopqrstuvwxyz1234";
        let result = redact(input, &[]);
        assert!(result.contains(REDACTED));
        assert!(!result.contains("sk-abcdefghij"));
    }

    #[test]
    fn redacts_bearer_token() {
        let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature";
        let result = redact(input, &[]);
        assert!(result.contains(REDACTED));
        assert!(!result.contains("eyJhbGci"));
    }

    #[test]
    fn redacts_telegram_bot_token() {
        let input = "Bot token: 123456789:ABCdefGHI-jklMNOpqrSTUvwxyz12345678";
        let result = redact(input, &[]);
        assert!(result.contains(REDACTED));
        assert!(!result.contains("123456789:ABC"));
    }

    #[test]
    fn redacts_exact_vault_values() {
        let vault_values = vec!["my-secret-value-123".to_string()];
        let input = "The value is my-secret-value-123 and more text";
        let result = redact(input, &vault_values);
        assert_eq!(result, "The value is [REDACTED] and more text");
    }

    #[test]
    fn passes_through_non_sensitive_text() {
        let input = "This is a normal log message with no secrets";
        let result = redact(input, &[]);
        assert_eq!(result, input);
    }

    #[test]
    fn redacts_multiple_patterns_in_one_string() {
        let input = "key1=sk-ant-api03-abcdefghijklmnopqrstuvwxyz token=Bearer eyJhbGciOiJIUzI1NiIsInR5c";
        let result = redact(input, &[]);
        // Both should be redacted.
        assert!(!result.contains("sk-ant-api03"));
        assert!(!result.contains("eyJhbGci"));
    }

    #[test]
    fn redacting_writer_works() {
        let vault_values = Arc::new(RwLock::new(vec!["secret123".to_string()]));
        let mut buf = Vec::new();
        {
            let mut writer = RedactingWriter::new(&mut buf, vault_values);
            write!(writer, "API response: secret123 received").unwrap();
        }
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains(REDACTED));
        assert!(!output.contains("secret123"));
    }

    #[test]
    fn add_vault_value_prevents_duplicates() {
        let values = Arc::new(RwLock::new(vec![]));
        RedactingWriter::<Vec<u8>>::add_vault_value(&values, "test".to_string());
        RedactingWriter::<Vec<u8>>::add_vault_value(&values, "test".to_string());
        assert_eq!(values.read().unwrap().len(), 1);
    }

    #[test]
    fn exact_match_longest_first() {
        let vault_values = vec!["short".to_string(), "short-longer".to_string()];
        let input = "prefix short-longer suffix";
        let result = redact(input, &vault_values);
        // "short-longer" should be replaced first (it's longer), not "short" within it.
        assert_eq!(result, "prefix [REDACTED] suffix");
    }
}
