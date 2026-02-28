// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Figment-to-miette error bridge with fuzzy match suggestions.
//!
//! Converts Figment deserialization errors into rich miette diagnostics
//! with source spans, valid key listings, and "did you mean?" suggestions
//! using Jaro-Winkler string similarity.

#![allow(unused_assignments)] // miette's Diagnostic derive generates code triggering this lint

use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

/// Minimum Jaro-Winkler similarity score to suggest a correction.
/// 0.75 is chosen to catch common typos like `naem` -> `name`,
/// `max_sesions` -> `max_sessions`, while filtering noise.
const SUGGESTION_THRESHOLD: f64 = 0.75;

/// A configuration error with rich diagnostic information.
///
/// Each variant carries enough context for miette to render an Elm-style
/// error message with source spans, suggestions, and valid key listings.
#[derive(Debug, Error, Diagnostic)]
pub enum ConfigError {
    /// An unknown key was found in the configuration.
    #[error("unknown configuration key `{key}`")]
    #[diagnostic(
        code(blufio::config::unknown_key),
        help("{}", format_unknown_key_help(suggestion.as_deref(), valid_keys))
    )]
    UnknownKey {
        /// The unrecognized key name.
        key: String,
        /// Suggested correction via fuzzy matching, if any.
        suggestion: Option<String>,
        /// List of valid keys for the section.
        valid_keys: String,
        /// Source span for the offending key.
        #[label("this key is not recognized")]
        span: Option<SourceSpan>,
        /// The source file content for context display.
        #[source_code]
        src: Option<NamedSource<String>>,
    },

    /// A configuration value has the wrong type.
    #[error("invalid type for key `{key}`: {detail}")]
    #[diagnostic(
        code(blufio::config::invalid_type),
        help("expected {expected}")
    )]
    InvalidType {
        /// The key with the wrong type.
        key: String,
        /// Description of the type mismatch.
        detail: String,
        /// What type was expected.
        expected: String,
        /// Source span for the offending value.
        #[label("wrong type here")]
        span: Option<SourceSpan>,
        /// The source file content.
        #[source_code]
        src: Option<NamedSource<String>>,
    },

    /// A required configuration key is missing.
    #[error("missing required key `{key}`")]
    #[diagnostic(
        code(blufio::config::missing_key),
        help("add `{key} = <value>` to your blufio.toml")
    )]
    MissingKey {
        /// The missing key name.
        key: String,
    },

    /// A validation error for a config value.
    #[error("validation error: {message}")]
    #[diagnostic(code(blufio::config::validation))]
    Validation {
        /// Description of the validation failure.
        message: String,
    },

    /// Catch-all for other configuration errors.
    #[error("configuration error: {0}")]
    #[diagnostic(code(blufio::config::other))]
    Other(String),
}

/// Format the help message for unknown key errors.
fn format_unknown_key_help(suggestion: Option<&str>, valid_keys: &str) -> String {
    match suggestion {
        Some(s) => format!("did you mean `{s}`? Valid keys: {valid_keys}"),
        None => format!("valid keys: {valid_keys}"),
    }
}

/// Convert a `figment::Error` into a list of `ConfigError` diagnostics.
///
/// Iterates through all errors in the figment error (which may contain multiple),
/// converting each to an appropriate `ConfigError` variant with fuzzy match
/// suggestions for unknown field errors.
pub fn figment_to_config_errors(
    err: figment::Error,
    toml_sources: &[(String, String)],
) -> Vec<ConfigError> {
    use figment::error::Kind;

    let mut errors = Vec::new();

    for error in err {
        let config_error = match &error.kind {
            Kind::UnknownField(field, expected) => {
                // expected is &'static [&'static str]
                let valid_keys: Vec<&str> = expected.to_vec();
                let suggestion = suggest_key(field, &valid_keys);
                let valid_keys_str = valid_keys.join(", ");

                // Try to find source span in TOML files
                let (span, src) = find_source_span(&error, field, toml_sources);

                ConfigError::UnknownKey {
                    key: field.clone(),
                    suggestion,
                    valid_keys: valid_keys_str,
                    span,
                    src,
                }
            }
            Kind::MissingField(field) => ConfigError::MissingKey {
                key: field.clone().into_owned(),
            },
            Kind::InvalidType(actual, expected) => {
                let key = error
                    .path
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(".");
                ConfigError::InvalidType {
                    key,
                    detail: format!("found {actual}, expected {expected}"),
                    expected: expected.to_string(),
                    span: None,
                    src: None,
                }
            }
            _ => ConfigError::Other(format!("{error}")),
        };

        errors.push(config_error);
    }

    errors
}

/// Find source span for an error in the TOML source files.
fn find_source_span(
    error: &figment::error::Error,
    field: &str,
    toml_sources: &[(String, String)],
) -> (Option<SourceSpan>, Option<NamedSource<String>>) {
    // Try to determine which source file the error came from
    let source_path = error
        .metadata
        .as_ref()
        .and_then(|m| m.source.as_ref())
        .and_then(|s| match s {
            figment::Source::File(path) => Some(path.display().to_string()),
            _ => None,
        });

    // Find matching source content
    let source = source_path.as_ref().and_then(|path| {
        toml_sources
            .iter()
            .find(|(p, _)| p == path)
            .map(|(p, content)| (p.as_str(), content.as_str()))
    });

    if let Some((path, content)) = source {
        // Extract section path (e.g., for "agent.naem", the section is "agent")
        let section: Vec<String> = error.path.iter().map(|s| s.to_string()).collect();

        if let Some(offset) = find_key_offset(content, &section, field) {
            let span = SourceSpan::new(offset.into(), field.len());
            let named = NamedSource::new(path, content.to_string());
            return (Some(span), Some(named));
        }
    }

    (None, None)
}

/// Find the byte offset of a key in TOML content, relative to a section path.
///
/// For `path = ["agent"]` and `field = "naem"`, finds `[agent]` header
/// then searches for `naem` after it. For top-level fields, searches from start.
pub fn find_key_offset(content: &str, path: &[String], field: &str) -> Option<usize> {
    let search_start = if path.is_empty() {
        0
    } else {
        // Find the section header, e.g., [agent]
        let section = &path[0];
        let header = format!("[{section}]");
        content.find(&header).map(|pos| pos + header.len())?
    };

    // Find the field after the section header
    let remaining = &content[search_start..];

    // Look for the field name at the start of a line (possibly with whitespace)
    let mut byte_offset = 0;
    for line in remaining.lines() {
        let trimmed = line.trim_start();
        if let Some(after) = trimmed.strip_prefix(field) {
            // Check that the character after the field name is whitespace or '='
            if after.starts_with(' ') || after.starts_with('=') || after.starts_with('\t') {
                // Find the exact position of the field name in the original content
                let field_start_in_line = line.len() - trimmed.len();
                return Some(search_start + byte_offset + field_start_in_line);
            }
        }
        byte_offset += line.len() + 1; // +1 for newline
    }

    None
}

/// Suggest a similar key name using Jaro-Winkler string similarity.
///
/// Returns the best match above the similarity threshold, or `None` if
/// no valid key is close enough to the unknown key.
pub fn suggest_key(unknown: &str, valid_keys: &[&str]) -> Option<String> {
    let mut best_score = SUGGESTION_THRESHOLD;
    let mut best_match = None;

    for &key in valid_keys {
        let score = strsim::jaro_winkler(unknown, key);
        if score > best_score {
            best_score = score;
            best_match = Some(key.to_string());
        }
    }

    best_match
}

/// Render a list of `ConfigError`s to stderr using miette's graphical handler.
pub fn render_errors(errors: &[ConfigError]) {
    use miette::GraphicalReportHandler;

    let handler = GraphicalReportHandler::new();
    for error in errors {
        let mut buf = String::new();
        let diagnostic: &dyn Diagnostic = error;
        if handler.render_report(&mut buf, diagnostic).is_ok() {
            eprint!("{buf}");
        } else {
            eprintln!("Error: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggest_naem_for_name() {
        let valid = &["name", "max_sessions", "log_level"];
        assert_eq!(suggest_key("naem", valid), Some("name".to_string()));
    }

    #[test]
    fn suggest_bot_tken_for_bot_token() {
        let valid = &["bot_token", "allowed_users"];
        assert_eq!(
            suggest_key("bot_tken", valid),
            Some("bot_token".to_string())
        );
    }

    #[test]
    fn no_suggestion_for_distant_typo() {
        let valid = &["name", "max_sessions", "log_level"];
        assert_eq!(suggest_key("zzzzzz", valid), None);
    }

    #[test]
    fn find_key_offset_in_section() {
        let content = "[agent]\nnaem = \"test\"\n";
        let path = vec!["agent".to_string()];
        let offset = find_key_offset(content, &path, "naem");
        assert!(offset.is_some());
        let o = offset.unwrap();
        assert_eq!(&content[o..o + 4], "naem");
    }
}
