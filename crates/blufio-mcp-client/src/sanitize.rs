// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Description sanitization and response truncation for external MCP tools.
//!
//! External tool descriptions may contain instruction-like patterns that
//! could influence LLM behavior (prompt injection via tool descriptions).
//! This module strips those patterns, caps description length, and labels
//! external tools with their server origin (CLNT-08, CLNT-09).

use regex::Regex;
use std::sync::LazyLock;

/// Maximum length for sanitized external tool descriptions (including prefix).
const MAX_DESCRIPTION_LEN: usize = 200;

/// Regex matching instruction-like patterns in tool descriptions.
///
/// These are stripped to prevent prompt injection via external tool descriptions.
/// Matches patterns like "You must...", "Always...", "Never...", etc. followed
/// by content up to the next sentence boundary.
static INSTRUCTION_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:you must|you should|you are|always|never|important:|note:|remember:|warning:|caution:)[^.!?\n]*[.!?\n]?"
    ).expect("valid instruction pattern regex")
});

/// Sanitize an external tool description for safe inclusion in prompts.
///
/// Processing steps:
/// 1. Strip instruction-like patterns that could influence LLM behavior
/// 2. Trim whitespace and collapse multiple spaces
/// 3. Cap total output at 200 characters with "..." suffix
/// 4. Prefix with `[External: {server_name}]`
///
/// If the cleaned description is empty, substitutes "No description provided".
pub fn sanitize_description(server_name: &str, raw: &str) -> String {
    let cleaned = INSTRUCTION_PATTERN.replace_all(raw, "").to_string();
    // Collapse multiple whitespace into single spaces
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");

    let prefix = format!("[External: {server_name}] ");
    let available = MAX_DESCRIPTION_LEN.saturating_sub(prefix.len());

    let body = if cleaned.is_empty() {
        "No description provided".to_string()
    } else if cleaned.len() > available {
        format!("{}...", &cleaned[..available.saturating_sub(3)])
    } else {
        cleaned
    };

    format!("{prefix}{body}")
}

/// Truncate a tool response if it exceeds the size cap.
///
/// Returns the content unchanged if it fits within the cap.
/// If over the cap, truncates and appends `[truncated: N chars removed]`
/// so the LLM knows the response was cut short.
pub fn truncate_response(content: &str, cap: usize) -> String {
    if content.len() <= cap {
        return content.to_string();
    }

    let removed = content.len() - cap;
    let suffix = format!(" [truncated: {removed} chars removed]");
    let keep = cap.saturating_sub(suffix.len());

    format!("{}{suffix}", &content[..keep])
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── sanitize_description tests ──────────────────────────────────

    #[test]
    fn strips_you_must_pattern() {
        let result = sanitize_description("github", "Search repos. You must include auth token. Returns results.");
        assert!(result.contains("Search repos."));
        assert!(!result.contains("You must"));
        assert!(result.contains("Returns results."));
    }

    #[test]
    fn strips_always_pattern() {
        let result = sanitize_description("server", "Fetch data. Always return JSON format. Useful tool.");
        assert!(!result.contains("Always"));
        assert!(result.contains("Fetch data."));
    }

    #[test]
    fn strips_never_pattern() {
        let result = sanitize_description("server", "Delete items. Never use without confirmation. Permanent action.");
        assert!(!result.contains("Never"));
    }

    #[test]
    fn strips_multiple_patterns() {
        let result = sanitize_description(
            "server",
            "Tool desc. You must do X. Always do Y. Never do Z. The end.",
        );
        assert!(!result.contains("You must"));
        assert!(!result.contains("Always"));
        assert!(!result.contains("Never"));
    }

    #[test]
    fn prefixes_with_server_name() {
        let result = sanitize_description("github", "Search repositories");
        assert!(result.starts_with("[External: github] "));
    }

    #[test]
    fn caps_at_200_chars() {
        let long_desc = "A".repeat(500);
        let result = sanitize_description("srv", &long_desc);
        assert!(
            result.len() <= MAX_DESCRIPTION_LEN,
            "Result length {} exceeds max {}",
            result.len(),
            MAX_DESCRIPTION_LEN
        );
        assert!(result.ends_with("..."));
    }

    #[test]
    fn empty_description_shows_fallback() {
        let result = sanitize_description("server", "");
        assert!(result.contains("No description provided"));
    }

    #[test]
    fn all_instructions_stripped_shows_fallback() {
        let result = sanitize_description("server", "You must do everything. Always comply.");
        assert!(result.contains("No description provided"));
    }

    #[test]
    fn collapses_whitespace() {
        let result = sanitize_description("srv", "Has   multiple    spaces   inside");
        assert!(result.contains("Has multiple spaces inside"));
    }

    #[test]
    fn case_insensitive_stripping() {
        let result = sanitize_description("srv", "Tool. YOU MUST comply. Good tool.");
        assert!(!result.contains("YOU MUST"));
        assert!(result.contains("Tool."));
    }

    #[test]
    fn short_description_unchanged() {
        let result = sanitize_description("github", "Search repos by query");
        assert_eq!(result, "[External: github] Search repos by query");
    }

    // ── truncate_response tests ─────────────────────────────────────

    #[test]
    fn under_cap_unchanged() {
        let content = "Short response";
        assert_eq!(truncate_response(content, 100), content);
    }

    #[test]
    fn exactly_at_cap_unchanged() {
        let content = "X".repeat(100);
        assert_eq!(truncate_response(&content, 100), content);
    }

    #[test]
    fn over_cap_truncated_with_suffix() {
        let content = "A".repeat(200);
        let result = truncate_response(&content, 100);
        assert!(result.len() <= 100);
        assert!(result.contains("[truncated:"));
        assert!(result.contains("chars removed]"));
    }

    #[test]
    fn truncation_suffix_shows_correct_count() {
        let content = "B".repeat(5000);
        let result = truncate_response(&content, 4096);
        // Should mention how many chars were removed
        let removed = 5000 - 4096;
        assert!(result.contains(&format!("{removed} chars removed")));
    }

    #[test]
    fn truncation_fits_within_cap() {
        let content = "C".repeat(10000);
        let result = truncate_response(&content, 4096);
        assert!(
            result.len() <= 4096,
            "Truncated result length {} exceeds cap 4096",
            result.len()
        );
    }

    #[test]
    fn zero_cap_still_works() {
        let content = "some content";
        let result = truncate_response(content, 0);
        // With cap 0, everything is truncated
        assert!(result.contains("[truncated:"));
    }
}
