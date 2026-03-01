// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tool trait and registry for built-in tools and WASM skills.
//!
//! The [`Tool`] trait defines the unified interface that both built-in tools
//! (bash, HTTP, file) and WASM skill sandboxes implement. The [`ToolRegistry`]
//! manages tool lookup by name and generates Anthropic-format tool definitions
//! for the LLM provider.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use blufio_core::BlufioError;
use serde::{Deserialize, Serialize};

/// Output from a tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// The content returned by the tool (text output, JSON, etc.).
    pub content: String,
    /// Whether the tool invocation resulted in an error.
    pub is_error: bool,
}

/// Unified trait for all tools (built-in and WASM skills).
///
/// Every tool provides a name, description, JSON Schema for its parameters,
/// and an async `invoke` method. The agent loop calls `invoke` with the
/// parsed JSON input from the LLM's `tool_use` content block.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Returns the tool's unique name (used for lookup and API serialization).
    fn name(&self) -> &str;

    /// Returns a human-readable description of what the tool does.
    fn description(&self) -> &str;

    /// Returns the JSON Schema describing the tool's input parameters.
    fn parameters_schema(&self) -> serde_json::Value;

    /// Invokes the tool with the given JSON input and returns the output.
    async fn invoke(&self, input: serde_json::Value) -> Result<ToolOutput, BlufioError>;
}

/// Registry of available tools, indexed by name.
///
/// The registry provides tool lookup for the agent loop and generates
/// Anthropic-format tool definition arrays for the provider request.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// Creates an empty tool registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Registers a tool. The tool is indexed by its `name()`.
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Looks up a tool by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// Returns (name, description) pairs for all registered tools.
    pub fn list(&self) -> Vec<(&str, &str)> {
        let mut entries: Vec<(&str, &str)> = self
            .tools
            .values()
            .map(|t| (t.name(), t.description()))
            .collect();
        entries.sort_by_key(|(name, _)| *name);
        entries
    }

    /// Returns Anthropic-format tool definitions for all registered tools.
    ///
    /// Each definition has the shape:
    /// ```json
    /// {
    ///   "name": "tool_name",
    ///   "description": "What the tool does",
    ///   "input_schema": { ... JSON Schema ... }
    /// }
    /// ```
    pub fn tool_definitions(&self) -> Vec<serde_json::Value> {
        let mut defs: Vec<serde_json::Value> = self
            .tools
            .values()
            .map(|t| {
                serde_json::json!({
                    "name": t.name(),
                    "description": t.description(),
                    "input_schema": t.parameters_schema(),
                })
            })
            .collect();
        defs.sort_by(|a, b| {
            a["name"]
                .as_str()
                .unwrap_or("")
                .cmp(b["name"].as_str().unwrap_or(""))
        });
        defs
    }

    /// Returns the number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Returns true if no tools are registered.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A simple test tool for registry tests.
    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }

        fn description(&self) -> &str {
            "Echoes the input back"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string", "description": "Message to echo" }
                },
                "required": ["message"]
            })
        }

        async fn invoke(&self, input: serde_json::Value) -> Result<ToolOutput, BlufioError> {
            let message = input["message"]
                .as_str()
                .unwrap_or("no message")
                .to_string();
            Ok(ToolOutput {
                content: message,
                is_error: false,
            })
        }
    }

    /// Another test tool to verify multiple registrations.
    struct AddTool;

    #[async_trait]
    impl Tool for AddTool {
        fn name(&self) -> &str {
            "add"
        }

        fn description(&self) -> &str {
            "Adds two numbers"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "a": { "type": "number" },
                    "b": { "type": "number" }
                },
                "required": ["a", "b"]
            })
        }

        async fn invoke(&self, input: serde_json::Value) -> Result<ToolOutput, BlufioError> {
            let a = input["a"].as_f64().unwrap_or(0.0);
            let b = input["b"].as_f64().unwrap_or(0.0);
            Ok(ToolOutput {
                content: format!("{}", a + b),
                is_error: false,
            })
        }
    }

    #[test]
    fn tool_registry_registers_and_retrieves_tools() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool));

        let tool = registry.get("echo");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name(), "echo");
    }

    #[test]
    fn tool_registry_returns_none_for_unknown_tools() {
        let registry = ToolRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn tool_registry_list_returns_all_tools() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool));
        registry.register(Arc::new(AddTool));

        let list = registry.list();
        assert_eq!(list.len(), 2);

        // Sorted alphabetically by name.
        assert_eq!(list[0], ("add", "Adds two numbers"));
        assert_eq!(list[1], ("echo", "Echoes the input back"));
    }

    #[test]
    fn tool_registry_tool_definitions_produces_valid_json() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool));

        let defs = registry.tool_definitions();
        assert_eq!(defs.len(), 1);

        let def = &defs[0];
        assert_eq!(def["name"], "echo");
        assert_eq!(def["description"], "Echoes the input back");
        assert!(def["input_schema"]["properties"]["message"].is_object());
        assert_eq!(def["input_schema"]["type"], "object");
    }

    #[test]
    fn tool_registry_tool_definitions_multiple_tools_sorted() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool));
        registry.register(Arc::new(AddTool));

        let defs = registry.tool_definitions();
        assert_eq!(defs.len(), 2);
        assert_eq!(defs[0]["name"], "add");
        assert_eq!(defs[1]["name"], "echo");
    }

    #[test]
    fn tool_registry_len_and_is_empty() {
        let mut registry = ToolRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);

        registry.register(Arc::new(EchoTool));
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
    }

    #[tokio::test]
    async fn tool_invoke_returns_correct_output() {
        let tool = EchoTool;
        let input = serde_json::json!({"message": "hello world"});
        let output = tool.invoke(input).await.unwrap();
        assert_eq!(output.content, "hello world");
        assert!(!output.is_error);
    }
}
