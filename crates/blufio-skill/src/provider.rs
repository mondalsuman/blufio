// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Conditional context provider that injects skill one-liners into the prompt.
//!
//! [`SkillProvider`] implements [`ConditionalProvider`] to inject a summary of
//! available tools into the conditional zone of the LLM prompt. This enables
//! progressive skill discovery: the LLM learns about available tools from the
//! prompt context and can decide to invoke them via `tool_use` blocks.

use std::sync::Arc;

use async_trait::async_trait;
use blufio_context::conditional::ConditionalProvider;
use blufio_core::error::BlufioError;
use blufio_core::types::{ContentBlock, ProviderMessage};
use tokio::sync::RwLock;

use crate::tool::ToolRegistry;

/// Injects available tool one-liners into the conditional prompt zone.
///
/// On each call to [`provide_context`], reads the current tool registry and
/// builds a summary of available tools (name: description). The LLM sees this
/// in the conditional zone and can invoke tools via `tool_use` content blocks.
#[derive(Clone)]
pub struct SkillProvider {
    registry: Arc<RwLock<ToolRegistry>>,
    max_skills_in_prompt: usize,
}

impl SkillProvider {
    /// Creates a new `SkillProvider`.
    ///
    /// - `registry`: Shared tool registry containing built-in tools and WASM skills.
    /// - `max_skills_in_prompt`: Maximum number of tool one-liners to include in context.
    pub fn new(registry: Arc<RwLock<ToolRegistry>>, max_skills_in_prompt: usize) -> Self {
        Self {
            registry,
            max_skills_in_prompt,
        }
    }
}

#[async_trait]
impl ConditionalProvider for SkillProvider {
    async fn provide_context(
        &self,
        _session_id: &str,
    ) -> Result<Vec<ProviderMessage>, BlufioError> {
        let registry = self.registry.read().await;
        let tools = registry.list();

        if tools.is_empty() {
            return Ok(vec![]);
        }

        let total = tools.len();
        let shown = total.min(self.max_skills_in_prompt);

        let mut lines = Vec::with_capacity(shown + 2);
        lines.push("## Available Tools".to_string());

        for (name, description) in tools.iter().take(shown) {
            lines.push(format!("{name}: {description}"));
        }

        if total > shown {
            let remaining = total - shown;
            lines.push(format!("... and {remaining} more tools available"));
        }

        let text = lines.join("\n");

        Ok(vec![ProviderMessage {
            role: "user".to_string(),
            content: vec![ContentBlock::Text { text }],
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::tool::Tool;
    use crate::ToolOutput;

    /// A minimal test tool for provider tests.
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

    fn make_registry(tools: Vec<(&str, &str)>) -> Arc<RwLock<ToolRegistry>> {
        let mut registry = ToolRegistry::new();
        for (name, desc) in tools {
            registry.register(Arc::new(DummyTool::new(name, desc)));
        }
        Arc::new(RwLock::new(registry))
    }

    #[tokio::test]
    async fn provider_empty_registry_returns_empty_vec() {
        let registry = make_registry(vec![]);
        let provider = SkillProvider::new(registry, 20);

        let result = provider.provide_context("session-1").await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn provider_with_tools_returns_one_liners() {
        let registry = make_registry(vec![
            ("bash", "Execute shell commands"),
            ("http", "Make HTTP requests"),
        ]);
        let provider = SkillProvider::new(registry, 20);

        let result = provider.provide_context("session-1").await.unwrap();
        assert_eq!(result.len(), 1);

        let msg = &result[0];
        let text = match &msg.content[0] {
            ContentBlock::Text { text } => text,
            _ => panic!("expected text content block"),
        };

        assert!(text.contains("## Available Tools"));
        assert!(text.contains("bash: Execute shell commands"));
        assert!(text.contains("http: Make HTTP requests"));
        // No truncation message with only 2 tools and max 20.
        assert!(!text.contains("more tools available"));
    }

    #[tokio::test]
    async fn provider_truncates_at_max_skills_in_prompt() {
        // Create 25 tools.
        let tools: Vec<(&str, &str)> = vec![
            ("tool_01", "Description 01"),
            ("tool_02", "Description 02"),
            ("tool_03", "Description 03"),
            ("tool_04", "Description 04"),
            ("tool_05", "Description 05"),
            ("tool_06", "Description 06"),
            ("tool_07", "Description 07"),
            ("tool_08", "Description 08"),
            ("tool_09", "Description 09"),
            ("tool_10", "Description 10"),
            ("tool_11", "Description 11"),
            ("tool_12", "Description 12"),
            ("tool_13", "Description 13"),
            ("tool_14", "Description 14"),
            ("tool_15", "Description 15"),
            ("tool_16", "Description 16"),
            ("tool_17", "Description 17"),
            ("tool_18", "Description 18"),
            ("tool_19", "Description 19"),
            ("tool_20", "Description 20"),
            ("tool_21", "Description 21"),
            ("tool_22", "Description 22"),
            ("tool_23", "Description 23"),
            ("tool_24", "Description 24"),
            ("tool_25", "Description 25"),
        ];
        let registry = make_registry(tools);
        let provider = SkillProvider::new(registry, 20);

        let result = provider.provide_context("session-1").await.unwrap();
        assert_eq!(result.len(), 1);

        let text = match &result[0].content[0] {
            ContentBlock::Text { text } => text,
            _ => panic!("expected text content block"),
        };

        assert!(text.contains("## Available Tools"));
        assert!(text.contains("... and 5 more tools available"));

        // Count tool lines (lines that contain ": Description").
        let tool_lines = text.lines().filter(|l| l.contains(": Description")).count();
        assert_eq!(tool_lines, 20);
    }

    #[tokio::test]
    async fn provider_message_has_user_role() {
        let registry = make_registry(vec![("test", "A test tool")]);
        let provider = SkillProvider::new(registry, 20);

        let result = provider.provide_context("session-1").await.unwrap();
        assert_eq!(result[0].role, "user");
    }
}
