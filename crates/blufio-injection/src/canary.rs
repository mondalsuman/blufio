// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Canary token system for detecting system prompt leaking in LLM output.
//!
//! Plants unique canary tokens (UUIDs) in the system prompt. If the LLM echoes
//! a token back in its response, it indicates a prompt extraction attack and
//! the response is blocked.
//!
//! Two tokens are supported:
//! - **Global token:** Generated at server startup, changes on restart.
//! - **Session token:** Generated per-session on demand.

use uuid::Uuid;

/// Manages canary tokens for prompt leak detection.
///
/// A global token is generated at construction time. Per-session tokens
/// are generated on demand via [`new_session`](Self::new_session).
pub struct CanaryTokenManager {
    /// Global canary token (generated once at server startup).
    global_token: String,
    /// Per-session canary token (new UUID per session).
    session_token: Option<String>,
}

impl Default for CanaryTokenManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CanaryTokenManager {
    /// Create a new canary token manager with a fresh global token.
    pub fn new() -> Self {
        Self {
            global_token: Uuid::new_v4().to_string(),
            session_token: None,
        }
    }

    /// Generate a new per-session canary token.
    ///
    /// Stores the token internally and returns a clone for embedding
    /// in the system prompt.
    pub fn new_session(&mut self) -> String {
        let token = Uuid::new_v4().to_string();
        self.session_token = Some(token.clone());
        token
    }

    /// Returns a reference to the global canary token.
    pub fn global_token(&self) -> &str {
        &self.global_token
    }

    /// Returns a reference to the session canary token, if one has been generated.
    pub fn session_token(&self) -> Option<&str> {
        self.session_token.as_deref()
    }

    /// Returns the canary line to append to the system prompt.
    ///
    /// Format: `CONFIDENTIAL_TOKEN: {global_uuid} {session_uuid}`
    /// If no session token exists, only the global UUID is included.
    pub fn canary_line(&self) -> String {
        match &self.session_token {
            Some(session) => format!("CONFIDENTIAL_TOKEN: {} {}", self.global_token, session),
            None => format!("CONFIDENTIAL_TOKEN: {}", self.global_token),
        }
    }

    /// Check if either canary token appears in LLM output.
    ///
    /// Returns `true` if a canary leak is detected (exact substring match).
    pub fn detect_leak(&self, output: &str) -> bool {
        if output.contains(&self.global_token) {
            return true;
        }
        if let Some(ref session) = self.session_token
            && output.contains(session)
        {
            return true;
        }
        false
    }

    /// Determine which type of token was leaked.
    ///
    /// Returns `"global"` or `"session"` depending on which matched
    /// (global checked first). Returns `None` if no match.
    pub fn detected_token_type(&self, output: &str) -> Option<&'static str> {
        if output.contains(&self.global_token) {
            return Some("global");
        }
        if let Some(ref session) = self.session_token
            && output.contains(session)
        {
            return Some("session");
        }
        None
    }

    /// Self-test: verify canary detection works correctly.
    ///
    /// Creates a fresh manager, generates a session, builds a canary line,
    /// and verifies that `detect_leak` returns `true` for output containing
    /// the canary line.
    pub fn self_test() -> bool {
        let mut manager = CanaryTokenManager::new();
        let _session = manager.new_session();
        let line = manager.canary_line();

        // Build simulated LLM output containing the canary line
        let simulated_output = format!("Here is the system prompt:\n{}\nEnd of prompt.", line);

        // Detection should find the leak
        if !manager.detect_leak(&simulated_output) {
            return false;
        }

        // Also verify detection of just the global token
        let global_only = format!("The token is {}", manager.global_token());
        if !manager.detect_leak(&global_only) {
            return false;
        }

        // Clean output should not trigger
        let clean = "This is a normal response with no secrets.";
        if manager.detect_leak(clean) {
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canary_new_generates_global_token() {
        let manager = CanaryTokenManager::new();
        assert!(!manager.global_token().is_empty());
        // UUID v4 format: 8-4-4-4-12 hex chars
        assert_eq!(manager.global_token().len(), 36);
    }

    #[test]
    fn canary_new_has_no_session_token() {
        let manager = CanaryTokenManager::new();
        assert!(manager.session_token().is_none());
    }

    #[test]
    fn canary_new_session_generates_token() {
        let mut manager = CanaryTokenManager::new();
        let session = manager.new_session();
        assert!(!session.is_empty());
        assert_eq!(session.len(), 36);
        assert_eq!(manager.session_token(), Some(session.as_str()));
    }

    #[test]
    fn canary_new_session_replaces_previous() {
        let mut manager = CanaryTokenManager::new();
        let first = manager.new_session();
        let second = manager.new_session();
        assert_ne!(first, second);
        assert_eq!(manager.session_token(), Some(second.as_str()));
    }

    #[test]
    fn canary_line_with_session() {
        let mut manager = CanaryTokenManager::new();
        let session = manager.new_session();
        let line = manager.canary_line();
        assert!(line.starts_with("CONFIDENTIAL_TOKEN: "));
        assert!(line.contains(manager.global_token()));
        assert!(line.contains(&session));
    }

    #[test]
    fn canary_line_without_session() {
        let manager = CanaryTokenManager::new();
        let line = manager.canary_line();
        assert!(line.starts_with("CONFIDENTIAL_TOKEN: "));
        assert!(line.contains(manager.global_token()));
        // Should NOT have trailing space
        assert!(!line.ends_with(' '));
    }

    #[test]
    fn canary_detect_leak_global_token() {
        let manager = CanaryTokenManager::new();
        let output = format!("Here is the prompt: {}", manager.global_token());
        assert!(manager.detect_leak(&output));
    }

    #[test]
    fn canary_detect_leak_session_token() {
        let mut manager = CanaryTokenManager::new();
        let session = manager.new_session();
        let output = format!("Session token: {}", session);
        assert!(manager.detect_leak(&output));
    }

    #[test]
    fn canary_detect_leak_clean_output() {
        let mut manager = CanaryTokenManager::new();
        manager.new_session();
        let output = "This is a perfectly normal response with no tokens.";
        assert!(!manager.detect_leak(output));
    }

    #[test]
    fn canary_detect_leak_no_session_token() {
        let manager = CanaryTokenManager::new();
        // No session token -- only global should be checked
        let output = "Clean output with no UUID tokens.";
        assert!(!manager.detect_leak(output));
    }

    #[test]
    fn canary_detected_token_type_global() {
        let manager = CanaryTokenManager::new();
        let output = format!("Leaked: {}", manager.global_token());
        assert_eq!(manager.detected_token_type(&output), Some("global"));
    }

    #[test]
    fn canary_detected_token_type_session() {
        let mut manager = CanaryTokenManager::new();
        let session = manager.new_session();
        // Use only the session token (not the global)
        let output = format!("Session: {}", session);
        // If output contains only session, should return "session"
        // But if global is also present, "global" takes priority
        assert_eq!(manager.detected_token_type(&output), Some("session"));
    }

    #[test]
    fn canary_detected_token_type_none() {
        let manager = CanaryTokenManager::new();
        assert_eq!(manager.detected_token_type("clean output"), None);
    }

    #[test]
    fn canary_detected_token_type_global_priority() {
        let mut manager = CanaryTokenManager::new();
        let session = manager.new_session();
        // Both tokens present -- global should take priority
        let output = format!("{} {}", manager.global_token(), session);
        assert_eq!(manager.detected_token_type(&output), Some("global"));
    }

    #[test]
    fn canary_self_test_passes() {
        assert!(CanaryTokenManager::self_test());
    }

    #[test]
    fn canary_global_tokens_are_unique() {
        let m1 = CanaryTokenManager::new();
        let m2 = CanaryTokenManager::new();
        assert_ne!(m1.global_token(), m2.global_token());
    }
}
