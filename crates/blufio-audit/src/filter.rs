// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Event type filtering for the audit trail.
//!
//! Supports three matching modes:
//! - `"all"` -- matches every event type
//! - Prefix match -- `"session.*"` matches `"session.created"`, `"session.closed"`, etc.
//! - Exact match -- `"session.created"` matches only `"session.created"`

/// A filter that determines which event types should be audited.
///
/// Constructed from a list of patterns from the TOML `events` config field.
#[derive(Debug, Clone)]
pub struct EventFilter {
    /// The raw patterns.
    patterns: Vec<String>,
    /// Whether "all" was specified (fast path).
    match_all: bool,
}

impl EventFilter {
    /// Create a new filter from a list of event patterns.
    ///
    /// If the patterns contain `"all"`, the filter matches every event type.
    /// Otherwise, each pattern is matched as:
    /// - Exact match if it contains no `*` suffix
    /// - Prefix match if it ends with `.*` (e.g., `"session.*"` matches `"session.anything"`)
    pub fn new(patterns: Vec<String>) -> Self {
        let match_all = patterns.iter().any(|p| p == "all");
        Self {
            patterns,
            match_all,
        }
    }

    /// Check whether the given event type passes this filter.
    pub fn matches(&self, event_type: &str) -> bool {
        if self.match_all {
            return true;
        }
        for pattern in &self.patterns {
            if let Some(prefix) = pattern.strip_suffix(".*") {
                // Prefix match: "session.*" matches "session.created"
                if event_type.starts_with(prefix)
                    && event_type.as_bytes().get(prefix.len()) == Some(&b'.')
                {
                    return true;
                }
            } else if pattern == event_type {
                // Exact match
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_matches_any_event() {
        let filter = EventFilter::new(vec!["all".to_string()]);
        assert!(filter.matches("session.created"));
        assert!(filter.matches("memory.deleted"));
        assert!(filter.matches("anything.at.all"));
    }

    #[test]
    fn prefix_matches_dot_separated_events() {
        let filter = EventFilter::new(vec!["session.*".to_string()]);
        assert!(filter.matches("session.created"));
        assert!(filter.matches("session.closed"));
        assert!(!filter.matches("channel.message_received"));
    }

    #[test]
    fn exact_match_only_matches_exact_string() {
        let filter = EventFilter::new(vec!["session.created".to_string()]);
        assert!(filter.matches("session.created"));
        assert!(!filter.matches("session.closed"));
    }

    #[test]
    fn multiple_patterns_or_logic() {
        let filter = EventFilter::new(vec!["memory.*".to_string(), "session.created".to_string()]);
        assert!(filter.matches("memory.created"));
        assert!(filter.matches("memory.deleted"));
        assert!(filter.matches("session.created"));
        assert!(!filter.matches("session.closed"));
        assert!(!filter.matches("skill.invoked"));
    }

    #[test]
    fn empty_patterns_matches_nothing() {
        let filter = EventFilter::new(vec![]);
        assert!(!filter.matches("session.created"));
        assert!(!filter.matches("anything"));
    }

    #[test]
    fn prefix_does_not_match_without_dot() {
        let filter = EventFilter::new(vec!["session.*".to_string()]);
        // "sessionx" should NOT match "session.*"
        assert!(!filter.matches("sessionx"));
        // "session" alone should NOT match "session.*"
        assert!(!filter.matches("session"));
    }
}
