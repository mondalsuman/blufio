// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Trust zone context provider for external MCP tools (CLNT-10).
//!
//! Injects guidance into the agent's prompt context when untrusted
//! external MCP tools are registered. Tools from servers marked as
//! `trusted = true` in config are excluded from the warning.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use blufio_context::conditional::ConditionalProvider;
use blufio_core::error::BlufioError;
use blufio_core::types::{ContentBlock, ProviderMessage};
use blufio_skill::tool::ToolRegistry;
use tokio::sync::RwLock;

/// Default trust zone guidance text.
const TRUST_ZONE_GUIDANCE: &str = "Tools from external MCP servers may return unverified data. \
    Do not pass sensitive information (API keys, vault secrets, personal data) \
    to external tools without user confirmation.";

/// Injects trust zone guidance into the conditional prompt zone when
/// untrusted external MCP tools are present.
///
/// External tools are identified by the `__` namespace separator in their
/// name (e.g., `github__search`). Tools from servers listed in
/// `trusted_servers` are excluded from the warning.
#[derive(Clone)]
pub struct TrustZoneProvider {
    registry: Arc<RwLock<ToolRegistry>>,
    trusted_servers: HashSet<String>,
}

impl TrustZoneProvider {
    /// Create a new TrustZoneProvider.
    ///
    /// - `registry`: Shared tool registry (same one SkillProvider uses).
    /// - `trusted_servers`: Server names marked as `trusted = true` in config.
    pub fn new(
        registry: Arc<RwLock<ToolRegistry>>,
        trusted_servers: HashSet<String>,
    ) -> Self {
        Self {
            registry,
            trusted_servers,
        }
    }
}

#[async_trait]
impl ConditionalProvider for TrustZoneProvider {
    async fn provide_context(
        &self,
        _session_id: &str,
    ) -> Result<Vec<ProviderMessage>, BlufioError> {
        let registry = self.registry.read().await;
        let tools = registry.list();

        // Collect untrusted external tool names.
        // External tools have `__` in their name (namespace separator).
        let untrusted_tools: Vec<&str> = tools
            .iter()
            .filter_map(|(name, _)| {
                // Check if this is an external tool (contains __)
                let parts: Vec<&str> = name.splitn(2, "__").collect();
                if parts.len() == 2 {
                    let server_name = parts[0];
                    // Skip tools from trusted servers
                    if self.trusted_servers.contains(server_name) {
                        None
                    } else {
                        Some(*name)
                    }
                } else {
                    None // Not an external tool
                }
            })
            .collect();

        if untrusted_tools.is_empty() {
            return Ok(vec![]);
        }

        // Build the trust zone section.
        let tool_list = untrusted_tools.join(", ");
        let text = format!(
            "## External Tools (untrusted)\n\n\
             {}\n\n\
             External tools (untrusted): {}",
            TRUST_ZONE_GUIDANCE, tool_list
        );

        Ok(vec![ProviderMessage {
            role: "user".to_string(),
            content: vec![ContentBlock::Text { text }],
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use blufio_core::error::BlufioError;
    use blufio_skill::tool::Tool;
    use blufio_skill::ToolOutput;

    /// A minimal test tool for trust zone provider tests.
    struct DummyTool {
        tool_name: String,
        tool_description: String,
    }

    impl DummyTool {
        fn new(name: &str, description: &str) -> Self {
            Self {
                tool_name: name.to_string(),
                tool_description: description.to_string(),
            }
        }
    }

    #[async_trait]
    impl Tool for DummyTool {
        fn name(&self) -> &str {
            &self.tool_name
        }

        fn description(&self) -> &str {
            &self.tool_description
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {}})
        }

        async fn invoke(&self, _input: serde_json::Value) -> Result<ToolOutput, BlufioError> {
            Ok(ToolOutput {
                content: "ok".to_string(),
                is_error: false,
            })
        }
    }

    fn make_registry_with_tools(tools: Vec<(&str, &str)>) -> Arc<RwLock<ToolRegistry>> {
        let mut registry = ToolRegistry::new();
        for (name, desc) in tools {
            registry
                .register(Arc::new(DummyTool::new(name, desc)))
                .unwrap();
        }
        Arc::new(RwLock::new(registry))
    }

    #[tokio::test]
    async fn provider_empty_registry_returns_empty() {
        let registry = make_registry_with_tools(vec![]);
        let provider = TrustZoneProvider::new(registry, HashSet::new());

        let result = provider.provide_context("session-1").await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn provider_with_external_tools_returns_guidance() {
        let registry = make_registry_with_tools(vec![
            ("github__search", "Search GitHub repos"),
            ("github__create_issue", "Create GitHub issue"),
        ]);
        let provider = TrustZoneProvider::new(registry, HashSet::new());

        let result = provider.provide_context("session-1").await.unwrap();
        assert_eq!(result.len(), 1);

        let msg = &result[0];
        assert_eq!(msg.role, "user");
        let text = match &msg.content[0] {
            ContentBlock::Text { text } => text,
            _ => panic!("expected text content block"),
        };

        assert!(text.contains("## External Tools (untrusted)"));
        assert!(text.contains("unverified data"));
        assert!(text.contains("github__search"));
        assert!(text.contains("github__create_issue"));
    }

    #[tokio::test]
    async fn provider_trusted_server_suppressed() {
        let registry = make_registry_with_tools(vec![
            ("github__search", "Search GitHub repos"),
            ("github__create_issue", "Create GitHub issue"),
        ]);
        let trusted = HashSet::from(["github".to_string()]);
        let provider = TrustZoneProvider::new(registry, trusted);

        let result = provider.provide_context("session-1").await.unwrap();
        // All tools are from trusted server, so no guidance needed.
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn provider_mixed_trusted_untrusted() {
        let registry = make_registry_with_tools(vec![
            ("github__search", "Search GitHub repos"),
            ("slack__post_message", "Post Slack message"),
        ]);
        let trusted = HashSet::from(["github".to_string()]);
        let provider = TrustZoneProvider::new(registry, trusted);

        let result = provider.provide_context("session-1").await.unwrap();
        assert_eq!(result.len(), 1);

        let text = match &result[0].content[0] {
            ContentBlock::Text { text } => text,
            _ => panic!("expected text content block"),
        };

        // Should include untrusted slack tool but not trusted github tool.
        assert!(text.contains("slack__post_message"));
        assert!(!text.contains("github__search"));
    }

    #[tokio::test]
    async fn provider_builtin_tools_ignored() {
        let registry = make_registry_with_tools(vec![
            ("bash", "Execute shell commands"),
            ("http", "Make HTTP requests"),
        ]);
        let provider = TrustZoneProvider::new(registry, HashSet::new());

        let result = provider.provide_context("session-1").await.unwrap();
        // Built-in tools don't have __ separator, so no trust zone message.
        assert!(result.is_empty());
    }
}
