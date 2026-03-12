// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! L5 human-in-the-loop confirmation flow for high-risk tool operations.
//!
//! Provides the decision engine and state management for HITL confirmation:
//! - Auto-approves safe tools (configurable allowlist)
//! - Per-session trust caching (approve once per tool type per session)
//! - API/gateway bypass (programmatic trust)
//! - Non-interactive channel denial
//! - Max pending confirmation limit
//! - Timeout auto-denial
//! - Risk level categorization
//!
//! The actual async confirmation flow (sending messages to channels, waiting
//! for replies) is wired in Plan 04. This module defines the [`ConfirmationChannel`]
//! trait but does not implement it.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::config::HitlConfig;
use crate::events::{hitl_prompt_event, SecurityEvent};
use crate::metrics;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Decision from the HITL manager for a tool execution request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HitlDecision {
    /// Tool is auto-approved. Reason: `"safe_tool"`, `"session_trust"`,
    /// `"api_bypass"`, `"hitl_disabled"`, `"user_approved"`.
    AutoApproved(String),
    /// Tool requires user confirmation. Contains the request details.
    PendingConfirmation(HitlRequest),
    /// Tool execution denied. Reason: `"timeout"`, `"non_interactive"`,
    /// `"max_pending"`, `"user_denied"`.
    Denied(String),
    /// Dry-run mode: describes what would happen without taking action.
    DryRun(String),
}

/// Details for a pending HITL confirmation request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HitlRequest {
    /// Correlation ID for cross-layer tracing.
    pub correlation_id: String,
    /// Name of the tool requesting execution.
    pub tool_name: String,
    /// Truncated summary of tool arguments (not full JSON).
    pub args_summary: String,
    /// Risk level: `"low"`, `"medium"`, or `"high"`.
    pub risk_level: String,
    /// Formatted confirmation message for the user.
    pub confirmation_message: String,
}

/// Trait for channel adapters that can deliver HITL confirmation requests.
///
/// Implemented by Telegram, CLI, and other channel adapters in the wiring
/// plan. This trait is defined here but implementations live elsewhere.
#[async_trait::async_trait]
pub trait ConfirmationChannel: Send + Sync {
    /// Whether this channel supports interactive confirmation.
    fn supports_confirmation(&self) -> bool;

    /// Send a confirmation request to the user.
    async fn send_confirmation(
        &self,
        session_id: &str,
        message: &str,
    ) -> Result<(), blufio_core::BlufioError>;

    /// Wait for the user's response (true = approved, false = denied).
    async fn wait_for_response(
        &self,
        session_id: &str,
        timeout: Duration,
    ) -> Result<bool, blufio_core::BlufioError>;
}

// ---------------------------------------------------------------------------
// HitlManager
// ---------------------------------------------------------------------------

/// L5 human-in-the-loop confirmation manager.
///
/// Manages per-session tool approval state, enforces safe-tool allowlists,
/// and generates security events for denied/timed-out operations.
pub struct HitlManager {
    config: HitlConfig,
    dry_run: bool,
    /// Per-session approved tool types: session_id -> set of approved tool names.
    session_approvals: HashMap<String, HashSet<String>>,
    /// Number of pending (unresolved) confirmation requests.
    pending_count: u32,
}

impl HitlManager {
    /// Create a new HITL manager from configuration.
    pub fn new(config: &HitlConfig, dry_run: bool) -> Self {
        Self {
            config: config.clone(),
            dry_run,
            session_approvals: HashMap::new(),
            pending_count: 0,
        }
    }

    /// Check whether a tool execution requires HITL confirmation.
    ///
    /// Returns the decision based on the current state and configuration.
    /// This is the main entry point for L5 screening.
    #[allow(clippy::too_many_arguments)]
    pub fn check_tool(
        &mut self,
        tool_name: &str,
        tool_args: &serde_json::Value,
        session_id: &str,
        source_type: &str,
        channel_interactive: bool,
        correlation_id: &str,
    ) -> (HitlDecision, Vec<SecurityEvent>) {
        let risk_level = Self::categorize_risk(tool_name);

        // 1. HITL disabled: always auto-approve
        if !self.config.enabled {
            return (
                HitlDecision::AutoApproved("hitl_disabled".to_string()),
                vec![],
            );
        }

        // 2. Dry-run mode: report what would happen
        if self.dry_run {
            let msg = if self.config.safe_tools.iter().any(|s| s == tool_name) {
                format!("would auto-approve {} (safe_tool)", tool_name)
            } else {
                format!(
                    "would request confirmation for {} (risk: {})",
                    tool_name, risk_level
                )
            };
            return (HitlDecision::DryRun(msg), vec![]);
        }

        // 3. API/gateway bypass: programmatic trust
        if source_type == "api" || source_type == "gateway" {
            return (
                HitlDecision::AutoApproved("api_bypass".to_string()),
                vec![],
            );
        }

        // 4. Safe tool: auto-approve
        if self.config.safe_tools.iter().any(|s| s == tool_name) {
            return (
                HitlDecision::AutoApproved("safe_tool".to_string()),
                vec![],
            );
        }

        // 5. Session trust: already approved this tool type
        if let Some(approved) = self.session_approvals.get(session_id) {
            if approved.contains(tool_name) {
                return (
                    HitlDecision::AutoApproved("session_trust".to_string()),
                    vec![],
                );
            }
        }

        // 6. Non-interactive channel: auto-deny
        if !channel_interactive {
            let event = hitl_prompt_event(
                correlation_id,
                tool_name,
                risk_level,
                "denied",
                session_id,
            );
            metrics::record_hitl_denial();
            return (
                HitlDecision::Denied("non_interactive".to_string()),
                vec![event],
            );
        }

        // 7. Max pending limit reached: auto-deny
        if self.pending_count >= self.config.max_pending {
            let event = hitl_prompt_event(
                correlation_id,
                tool_name,
                risk_level,
                "denied",
                session_id,
            );
            metrics::record_hitl_denial();
            return (
                HitlDecision::Denied("max_pending".to_string()),
                vec![event],
            );
        }

        // 8. Require confirmation
        self.pending_count += 1;
        let confirmation_message = Self::format_confirmation_message(tool_name, tool_args);
        let args_summary = Self::summarize_args(tool_args);

        let request = HitlRequest {
            correlation_id: correlation_id.to_string(),
            tool_name: tool_name.to_string(),
            args_summary,
            risk_level: risk_level.to_string(),
            confirmation_message,
        };

        (HitlDecision::PendingConfirmation(request), vec![])
    }

    /// Resolve a pending confirmation (user approved or denied).
    ///
    /// If approved, adds the tool to the session trust set so subsequent
    /// calls to the same tool type are auto-approved.
    pub fn resolve_confirmation(
        &mut self,
        session_id: &str,
        tool_name: &str,
        approved: bool,
        correlation_id: &str,
    ) -> (HitlDecision, Vec<SecurityEvent>) {
        if self.pending_count > 0 {
            self.pending_count -= 1;
        }

        if approved {
            // Add to session trust
            self.session_approvals
                .entry(session_id.to_string())
                .or_default()
                .insert(tool_name.to_string());

            let event = hitl_prompt_event(
                correlation_id,
                tool_name,
                Self::categorize_risk(tool_name),
                "approved",
                session_id,
            );
            metrics::record_hitl_confirmation();

            (
                HitlDecision::AutoApproved("user_approved".to_string()),
                vec![event],
            )
        } else {
            let event = hitl_prompt_event(
                correlation_id,
                tool_name,
                Self::categorize_risk(tool_name),
                "denied",
                session_id,
            );
            metrics::record_hitl_denial();

            (
                HitlDecision::Denied("user_denied".to_string()),
                vec![event],
            )
        }
    }

    /// Handle a confirmation timeout (auto-deny after configured timeout).
    pub fn handle_timeout(
        &mut self,
        session_id: &str,
        tool_name: &str,
        correlation_id: &str,
    ) -> (HitlDecision, SecurityEvent) {
        if self.pending_count > 0 {
            self.pending_count -= 1;
        }

        let event = hitl_prompt_event(
            correlation_id,
            tool_name,
            Self::categorize_risk(tool_name),
            "timeout",
            session_id,
        );
        metrics::record_hitl_timeout();

        (HitlDecision::Denied("timeout".to_string()), event)
    }

    /// Clear session state (approvals + pending count).
    pub fn clear_session(&mut self, session_id: &str) {
        self.session_approvals.remove(session_id);
        self.pending_count = 0;
    }

    /// Get the configured timeout duration.
    pub fn timeout_duration(&self) -> Duration {
        Duration::from_secs(self.config.timeout_secs)
    }

    /// Get the current pending confirmation count.
    pub fn pending_count(&self) -> u32 {
        self.pending_count
    }

    // ── Static helpers ─────────────────────────────────────────────

    /// Format a confirmation message for the user.
    ///
    /// Produces: `"Approve [tool_name] with args [summary]? Reply YES/NO"`
    pub fn format_confirmation_message(tool_name: &str, args: &serde_json::Value) -> String {
        let summary = Self::summarize_args(args);
        format!(
            "Approve [{}] with args [{}]? Reply YES/NO",
            tool_name, summary
        )
    }

    /// Categorize tool risk level based on tool name.
    ///
    /// - `"high"`: tools containing "config", "export", "delete", "erase"
    /// - `"medium"`: tools starting with "mcp:" or containing "external"
    /// - `"low"`: everything else (WASM skills, etc.)
    pub fn categorize_risk(tool_name: &str) -> &'static str {
        let lower = tool_name.to_lowercase();
        if lower.contains("config")
            || lower.contains("export")
            || lower.contains("delete")
            || lower.contains("erase")
        {
            "high"
        } else if lower.starts_with("mcp:") || lower.contains("external") {
            "medium"
        } else {
            "low"
        }
    }

    /// Summarize tool arguments (first 200 chars of JSON, truncated with "...").
    fn summarize_args(args: &serde_json::Value) -> String {
        let json_str = serde_json::to_string(args).unwrap_or_else(|_| "{}".to_string());
        if json_str.len() > 200 {
            format!("{}...", &json_str[..200])
        } else {
            json_str
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn default_manager() -> HitlManager {
        let config = HitlConfig::default();
        // HITL is disabled by default; enable it for tests
        let config = HitlConfig {
            enabled: true,
            ..config
        };
        HitlManager::new(&config, false)
    }

    fn disabled_manager() -> HitlManager {
        let config = HitlConfig {
            enabled: false,
            ..HitlConfig::default()
        };
        HitlManager::new(&config, false)
    }

    fn dry_run_manager() -> HitlManager {
        let config = HitlConfig {
            enabled: true,
            ..HitlConfig::default()
        };
        HitlManager::new(&config, true)
    }

    // ── Safe tools ─────────────────────────────────────────────────

    #[test]
    fn safe_tool_auto_approved() {
        let mut m = default_manager();
        let (decision, events) = m.check_tool(
            "memory_search",
            &json!({}),
            "sess-1",
            "telegram",
            true,
            "corr-1",
        );
        assert_eq!(decision, HitlDecision::AutoApproved("safe_tool".to_string()));
        assert!(events.is_empty());
    }

    #[test]
    fn session_history_auto_approved() {
        let mut m = default_manager();
        let (decision, _) = m.check_tool(
            "session_history",
            &json!({}),
            "sess-1",
            "telegram",
            true,
            "corr-2",
        );
        assert_eq!(decision, HitlDecision::AutoApproved("safe_tool".to_string()));
    }

    // ── HITL disabled ──────────────────────────────────────────────

    #[test]
    fn hitl_disabled_returns_auto_approved() {
        let mut m = disabled_manager();
        let (decision, _) = m.check_tool(
            "mcp:dangerous_tool",
            &json!({}),
            "sess-1",
            "telegram",
            true,
            "corr-3",
        );
        assert_eq!(
            decision,
            HitlDecision::AutoApproved("hitl_disabled".to_string())
        );
    }

    // ── Non-interactive channel ────────────────────────────────────

    #[test]
    fn non_interactive_channel_returns_denied() {
        let mut m = default_manager();
        let (decision, events) = m.check_tool(
            "mcp:external_tool",
            &json!({}),
            "sess-1",
            "webhook",
            false, // non-interactive
            "corr-4",
        );
        assert_eq!(
            decision,
            HitlDecision::Denied("non_interactive".to_string())
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            SecurityEvent::HitlPrompt { action, .. } => {
                assert_eq!(action, "denied");
            }
            other => panic!("expected HitlPrompt, got {:?}", other),
        }
    }

    // ── Pending confirmation ───────────────────────────────────────

    #[test]
    fn external_tool_returns_pending_confirmation() {
        let mut m = default_manager();
        let (decision, _) = m.check_tool(
            "mcp:weather",
            &json!({"location": "London"}),
            "sess-1",
            "telegram",
            true,
            "corr-5",
        );
        match decision {
            HitlDecision::PendingConfirmation(req) => {
                assert_eq!(req.tool_name, "mcp:weather");
                assert_eq!(req.risk_level, "medium");
                assert!(req.confirmation_message.contains("mcp:weather"));
                assert!(req.confirmation_message.contains("YES/NO"));
            }
            other => panic!("expected PendingConfirmation, got {:?}", other),
        }
    }

    // ── Session trust ──────────────────────────────────────────────

    #[test]
    fn approved_tool_added_to_session_trust() {
        let mut m = default_manager();
        // First call: requires confirmation
        let (decision, _) = m.check_tool(
            "mcp:weather",
            &json!({}),
            "sess-1",
            "telegram",
            true,
            "corr-6",
        );
        assert!(matches!(decision, HitlDecision::PendingConfirmation(_)));

        // Resolve: user approves
        let (decision, events) =
            m.resolve_confirmation("sess-1", "mcp:weather", true, "corr-6");
        assert_eq!(
            decision,
            HitlDecision::AutoApproved("user_approved".to_string())
        );
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn session_trust_auto_approves_same_tool() {
        let mut m = default_manager();
        // First call + approve
        m.check_tool(
            "mcp:weather",
            &json!({}),
            "sess-1",
            "telegram",
            true,
            "corr-7a",
        );
        m.resolve_confirmation("sess-1", "mcp:weather", true, "corr-7a");

        // Second call: should be auto-approved via session trust
        let (decision, _) = m.check_tool(
            "mcp:weather",
            &json!({}),
            "sess-1",
            "telegram",
            true,
            "corr-7b",
        );
        assert_eq!(
            decision,
            HitlDecision::AutoApproved("session_trust".to_string())
        );
    }

    #[test]
    fn different_tool_still_requires_confirmation() {
        let mut m = default_manager();
        // Approve mcp:weather
        m.check_tool(
            "mcp:weather",
            &json!({}),
            "sess-1",
            "telegram",
            true,
            "corr-8a",
        );
        m.resolve_confirmation("sess-1", "mcp:weather", true, "corr-8a");

        // Different tool should still need confirmation
        let (decision, _) = m.check_tool(
            "mcp:calendar",
            &json!({}),
            "sess-1",
            "telegram",
            true,
            "corr-8b",
        );
        assert!(
            matches!(decision, HitlDecision::PendingConfirmation(_)),
            "different tool should require confirmation, got {:?}",
            decision
        );
    }

    // ── Max pending ────────────────────────────────────────────────

    #[test]
    fn max_pending_returns_denied() {
        let config = HitlConfig {
            enabled: true,
            max_pending: 2,
            ..HitlConfig::default()
        };
        let mut m = HitlManager::new(&config, false);

        // Fill up pending slots
        m.check_tool("tool_a", &json!({}), "s1", "telegram", true, "c1");
        m.check_tool("tool_b", &json!({}), "s1", "telegram", true, "c2");

        // Third should be denied
        let (decision, events) =
            m.check_tool("tool_c", &json!({}), "s1", "telegram", true, "c3");
        assert_eq!(decision, HitlDecision::Denied("max_pending".to_string()));
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn pending_count_increments_and_decrements() {
        let mut m = default_manager();
        assert_eq!(m.pending_count(), 0);

        // Pending increments
        m.check_tool("tool_a", &json!({}), "s1", "telegram", true, "c1");
        assert_eq!(m.pending_count(), 1);

        m.check_tool("tool_b", &json!({}), "s1", "telegram", true, "c2");
        assert_eq!(m.pending_count(), 2);

        // Resolve decrements
        m.resolve_confirmation("s1", "tool_a", true, "c1");
        assert_eq!(m.pending_count(), 1);

        m.resolve_confirmation("s1", "tool_b", false, "c2");
        assert_eq!(m.pending_count(), 0);
    }

    // ── Timeout ────────────────────────────────────────────────────

    #[test]
    fn timeout_produces_denied() {
        let mut m = default_manager();
        m.check_tool("mcp:slow", &json!({}), "s1", "telegram", true, "c-to");

        let (decision, event) = m.handle_timeout("s1", "mcp:slow", "c-to");
        assert_eq!(decision, HitlDecision::Denied("timeout".to_string()));
        match &event {
            SecurityEvent::HitlPrompt { action, .. } => {
                assert_eq!(action, "timeout");
            }
            other => panic!("expected HitlPrompt, got {:?}", other),
        }
        assert_eq!(m.pending_count(), 0);
    }

    // ── API bypass ─────────────────────────────────────────────────

    #[test]
    fn api_source_bypasses_hitl() {
        let mut m = default_manager();
        let (decision, _) = m.check_tool(
            "mcp:dangerous",
            &json!({}),
            "s1",
            "api", // API source
            false,
            "c-api",
        );
        assert_eq!(
            decision,
            HitlDecision::AutoApproved("api_bypass".to_string())
        );
    }

    // ── Risk categorization ────────────────────────────────────────

    #[test]
    fn risk_level_categorization() {
        assert_eq!(HitlManager::categorize_risk("export_data"), "high");
        assert_eq!(HitlManager::categorize_risk("delete_message"), "high");
        assert_eq!(HitlManager::categorize_risk("config_update"), "high");
        assert_eq!(HitlManager::categorize_risk("erase_all"), "high");
        assert_eq!(HitlManager::categorize_risk("mcp:weather"), "medium");
        assert_eq!(HitlManager::categorize_risk("external_api"), "medium");
        assert_eq!(HitlManager::categorize_risk("wasm_skill"), "low");
        assert_eq!(HitlManager::categorize_risk("random_tool"), "low");
    }

    // ── Denied event generation ────────────────────────────────────

    #[test]
    fn denied_generates_security_event() {
        let mut m = default_manager();
        // Deny via resolve
        m.check_tool("mcp:tool", &json!({}), "s1", "telegram", true, "c-deny");
        let (decision, events) =
            m.resolve_confirmation("s1", "mcp:tool", false, "c-deny");
        assert_eq!(decision, HitlDecision::Denied("user_denied".to_string()));
        assert_eq!(events.len(), 1);
        match &events[0] {
            SecurityEvent::HitlPrompt {
                action,
                tool_name,
                session_id,
                ..
            } => {
                assert_eq!(action, "denied");
                assert_eq!(tool_name, "mcp:tool");
                assert_eq!(session_id, "s1");
            }
            other => panic!("expected HitlPrompt, got {:?}", other),
        }
    }

    // ── Dry run ────────────────────────────────────────────────────

    #[test]
    fn dry_run_returns_dry_run() {
        let mut m = dry_run_manager();
        let (decision, _) = m.check_tool(
            "mcp:weather",
            &json!({}),
            "s1",
            "telegram",
            true,
            "c-dry",
        );
        match decision {
            HitlDecision::DryRun(msg) => {
                assert!(
                    msg.contains("would request confirmation"),
                    "msg: {}",
                    msg
                );
            }
            other => panic!("expected DryRun, got {:?}", other),
        }
    }

    // ── Confirmation message format ────────────────────────────────

    #[test]
    fn format_confirmation_message_correct() {
        let msg = HitlManager::format_confirmation_message(
            "mcp:weather",
            &json!({"location": "London"}),
        );
        assert!(msg.starts_with("Approve [mcp:weather]"));
        assert!(msg.contains("YES/NO"));
        assert!(msg.contains("London"));
    }

    // ── Clear session ──────────────────────────────────────────────

    #[test]
    fn clear_session_removes_trust_and_pending() {
        let mut m = default_manager();
        m.check_tool("mcp:weather", &json!({}), "s1", "telegram", true, "c1");
        m.resolve_confirmation("s1", "mcp:weather", true, "c1");

        // Verify session trust works
        let (d, _) = m.check_tool("mcp:weather", &json!({}), "s1", "telegram", true, "c2");
        assert_eq!(d, HitlDecision::AutoApproved("session_trust".to_string()));

        // Clear session
        m.clear_session("s1");

        // Should require confirmation again
        let (d, _) = m.check_tool("mcp:weather", &json!({}), "s1", "telegram", true, "c3");
        assert!(matches!(d, HitlDecision::PendingConfirmation(_)));
    }

    // ── Gateway bypass ─────────────────────────────────────────────

    #[test]
    fn gateway_source_bypasses_hitl() {
        let mut m = default_manager();
        let (decision, _) = m.check_tool(
            "mcp:tool",
            &json!({}),
            "s1",
            "gateway",
            false,
            "c-gw",
        );
        assert_eq!(
            decision,
            HitlDecision::AutoApproved("api_bypass".to_string())
        );
    }

    // ── Args summary truncation ────────────────────────────────────

    #[test]
    fn long_args_truncated_in_summary() {
        let long_value = "x".repeat(300);
        let msg = HitlManager::format_confirmation_message(
            "tool",
            &json!({"data": long_value}),
        );
        // The message should contain "..." indicating truncation
        assert!(msg.contains("..."), "should truncate long args: {}", msg);
    }
}
