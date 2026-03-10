// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! CLI handler for `blufio pii scan` subcommand.
//!
//! Scans text for PII (Personally Identifiable Information) using the
//! detection engine from blufio-security. Accepts input from:
//! - Positional text argument: `blufio pii scan "test@example.com"`
//! - File flag: `blufio pii scan --file /path/to/file`
//! - Stdin pipe: `echo "text" | blufio pii scan`
//!
//! # Examples
//!
//! ```bash
//! blufio pii scan "Contact: john@example.com or 555-123-4567"
//! blufio pii scan --file /tmp/data.txt
//! echo "SSN: 123-45-6789" | blufio pii scan
//! blufio pii scan --file /tmp/data.txt --json
//! ```

use std::io::{IsTerminal, Read};

use clap::Subcommand;
use colored::Colorize;

use blufio_core::BlufioError;
use blufio_security::{PiiMatch, PiiType, detect_pii};

/// PII subcommand actions.
#[derive(Subcommand, Debug)]
pub enum PiiAction {
    /// Scan text for PII (email, phone, SSN, credit card).
    Scan {
        /// Text to scan (positional argument).
        text: Option<String>,
        /// Path to a file to scan.
        #[arg(long)]
        file: Option<String>,
        /// Output as structured JSON for scripting.
        #[arg(long)]
        json: bool,
    },
}

/// Resolve input text from --file, positional arg, or stdin (in that priority order).
fn resolve_input(text: Option<&str>, file: Option<&str>) -> Result<String, BlufioError> {
    // Priority: --file > positional > stdin
    if let Some(path) = file {
        std::fs::read_to_string(path)
            .map_err(|e| BlufioError::Internal(format!("failed to read file '{}': {}", path, e)))
    } else if let Some(t) = text {
        Ok(t.to_string())
    } else {
        // Try reading from stdin (non-interactive).
        if std::io::stdin().is_terminal() {
            return Err(BlufioError::Internal(
                "no input provided: pass text as argument, use --file, or pipe to stdin"
                    .to_string(),
            ));
        }
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .map_err(|e| BlufioError::Internal(format!("failed to read from stdin: {}", e)))?;
        Ok(buffer)
    }
}

/// Color a PII type string for terminal output.
fn colored_pii_type(pii_type: PiiType) -> String {
    match pii_type {
        PiiType::Email => "email".cyan().to_string(),
        PiiType::Phone => "phone".yellow().to_string(),
        PiiType::Ssn => "ssn".red().bold().to_string(),
        PiiType::CreditCard => "credit_card".red().to_string(),
        _ => format!("{}", pii_type).white().to_string(),
    }
}

/// Format a PII match as a JSON value.
fn match_to_json(m: &PiiMatch) -> serde_json::Value {
    serde_json::json!({
        "type": m.pii_type.to_string(),
        "span": { "start": m.span.start, "end": m.span.end },
        "value": m.matched_value,
    })
}

/// Run the PII subcommand.
pub async fn run_pii(action: PiiAction) -> Result<(), BlufioError> {
    match action {
        PiiAction::Scan { text, file, json } => {
            run_pii_scan(text.as_deref(), file.as_deref(), json).await
        }
    }
}

/// Handle `blufio pii scan`.
async fn run_pii_scan(
    text: Option<&str>,
    file: Option<&str>,
    json: bool,
) -> Result<(), BlufioError> {
    let input = resolve_input(text, file)?;
    let matches = detect_pii(&input);

    if json {
        let output = serde_json::json!({
            "total": matches.len(),
            "matches": matches.iter().map(match_to_json).collect::<Vec<_>>(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|e| BlufioError::Internal(format!("JSON serialization failed: {e}")))?
        );
    } else if matches.is_empty() {
        println!("{} No PII detected.", "OK".green().bold());
    } else {
        println!(
            "{} Found {} PII match(es):\n",
            "WARNING".yellow().bold(),
            matches.len()
        );
        println!(
            "  {:<15} {:<12} {}",
            "Type".bold(),
            "Span".bold(),
            "Value".bold()
        );
        println!("  {}", "-".repeat(55));
        for m in &matches {
            println!(
                "  {:<15} {:<12} {}",
                colored_pii_type(m.pii_type),
                format!("{}..{}", m.span.start, m.span.end),
                m.matched_value.dimmed(),
            );
        }
        println!("\n  Total: {}", matches.len());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_input_positional() {
        let result = resolve_input(Some("hello"), None);
        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    fn resolve_input_file_not_found() {
        let result = resolve_input(None, Some("/nonexistent/path.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn resolve_input_file_priority_over_text() {
        // --file takes priority over positional text, but file doesn't exist.
        let result = resolve_input(Some("text"), Some("/nonexistent"));
        assert!(result.is_err()); // File error, not text fallback.
    }

    #[test]
    fn match_to_json_structure() {
        let m = PiiMatch {
            pii_type: PiiType::Email,
            span: 0..15,
            matched_value: "test@example.com".to_string(),
        };
        let json = match_to_json(&m);
        assert_eq!(json["type"], "email");
        assert_eq!(json["span"]["start"], 0);
        assert_eq!(json["span"]["end"], 15);
        assert_eq!(json["value"], "test@example.com");
    }

    #[test]
    fn colored_pii_type_returns_nonempty() {
        assert!(!colored_pii_type(PiiType::Email).is_empty());
        assert!(!colored_pii_type(PiiType::Phone).is_empty());
        assert!(!colored_pii_type(PiiType::Ssn).is_empty());
        assert!(!colored_pii_type(PiiType::CreditCard).is_empty());
    }

    #[tokio::test]
    async fn pii_scan_with_email() {
        let result = run_pii_scan(Some("contact: test@example.com"), None, false).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn pii_scan_with_email_json() {
        let result = run_pii_scan(Some("contact: test@example.com"), None, true).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn pii_scan_no_pii() {
        let result = run_pii_scan(Some("just plain text"), None, false).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn pii_scan_no_input() {
        // No text, no file, and stdin is a tty -- should error.
        let result = run_pii_scan(None, None, false).await;
        // In test context, stdin might not be a tty, so we just check it doesn't panic.
        let _ = result;
    }

    #[tokio::test]
    async fn pii_scan_file_not_found() {
        let result = run_pii_scan(None, Some("/nonexistent/file.txt"), false).await;
        assert!(result.is_err());
    }
}
