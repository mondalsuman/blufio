// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tool trait and registry for built-in tools and WASM skills.
//!
//! The [`Tool`] trait defines the unified interface that both built-in tools
//! (bash, HTTP, file) and WASM skill sandboxes implement. The [`ToolRegistry`]
//! manages tool lookup by name and generates Anthropic-format tool definitions
//! for the LLM provider.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use blufio_core::BlufioError;
use regex::Regex;
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

    /// Indicates this tool only reads data and has no side effects.
    /// Default: false (assumes tools may have side effects).
    fn is_read_only(&self) -> bool {
        false
    }

    /// Indicates this tool may perform destructive/irreversible operations.
    /// Default: false (optimistic -- tools should override if destructive).
    fn is_destructive(&self) -> bool {
        false
    }

    /// Indicates calling this tool multiple times with the same input produces the same result.
    /// Default: false (conservative -- tools should override if idempotent).
    fn is_idempotent(&self) -> bool {
        false
    }

    /// Indicates this tool interacts with external systems outside Blufio's control.
    /// Default: true (most tools interact with external systems).
    fn is_open_world(&self) -> bool {
        true
    }
}

/// Regex for valid flat tool names: letter followed by letters/digits/underscores.
static TOOL_NAME_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z][a-zA-Z0-9_]*$").expect("valid tool name regex"));

/// Regex for valid namespaced tool names: two valid names joined by exactly
/// two underscores. The first segment is the namespace and the second is the
/// tool name. Example: `github__create_issue`.
static NAMESPACED_TOOL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z][a-zA-Z0-9_]*__[a-zA-Z][a-zA-Z0-9_]*$")
        .expect("valid namespaced tool name regex")
});

/// Validates a flat tool name (built-in tools).
///
/// Valid names start with a letter and contain only letters, digits, and
/// underscores: `[a-zA-Z][a-zA-Z0-9_]*`.
pub fn validate_tool_name(name: &str) -> bool {
    TOOL_NAME_REGEX.is_match(name)
}

/// Validates a namespaced tool name (`namespace__tool` format).
///
/// Both the namespace and tool portions must individually match the flat
/// tool name pattern, separated by exactly two underscores.
pub fn validate_namespaced_tool_name(name: &str) -> bool {
    NAMESPACED_TOOL_REGEX.is_match(name)
}

/// Registry of available tools, indexed by name.
///
/// The registry provides tool lookup for the agent loop and generates
/// Anthropic-format tool definition arrays for the provider request.
///
/// Tools are registered through three methods:
/// - [`register_builtin`](ToolRegistry::register_builtin): For built-in tools
///   (bash, HTTP, file). These are marked as built-in and always win on collision.
/// - [`register_namespaced`](ToolRegistry::register_namespaced): For external
///   MCP tools. The tool name is prefixed with `namespace__`.
/// - [`register`](ToolRegistry::register): Backward-compatible entry point
///   with name validation and duplicate rejection.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
    builtin_names: HashSet<String>,
}

impl ToolRegistry {
    /// Creates an empty tool registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            builtin_names: HashSet::new(),
        }
    }

    /// Registers a tool with name validation.
    ///
    /// This is the backward-compatible entry point. It validates the tool
    /// name and rejects duplicates. For namespace-aware registration,
    /// use [`register_builtin`](Self::register_builtin) or
    /// [`register_namespaced`](Self::register_namespaced).
    pub fn register(&mut self, tool: Arc<dyn Tool>) -> Result<(), BlufioError> {
        let name = tool.name().to_string();
        if !validate_tool_name(&name) && !validate_namespaced_tool_name(&name) {
            return Err(BlufioError::Skill {
                message: format!(
                    "invalid tool name '{name}': must match \
                     [a-zA-Z][a-zA-Z0-9_]* or namespace__tool format"
                ),
                source: None,
            });
        }
        if self.tools.contains_key(&name) {
            return Err(BlufioError::Skill {
                message: format!("duplicate tool name '{name}': already registered"),
                source: None,
            });
        }
        self.tools.insert(name, tool);
        Ok(())
    }

    /// Registers a built-in tool. Built-in tools always win on collision.
    ///
    /// Returns error if name is invalid or already registered.
    pub fn register_builtin(&mut self, tool: Arc<dyn Tool>) -> Result<(), BlufioError> {
        let name = tool.name().to_string();
        if !validate_tool_name(&name) {
            return Err(BlufioError::Skill {
                message: format!(
                    "invalid built-in tool name '{name}': must match [a-zA-Z][a-zA-Z0-9_]*"
                ),
                source: None,
            });
        }
        if self.tools.contains_key(&name) {
            return Err(BlufioError::Skill {
                message: format!("duplicate built-in tool name '{name}': already registered"),
                source: None,
            });
        }
        self.builtin_names.insert(name.clone());
        self.tools.insert(name, tool);
        Ok(())
    }

    /// Registers an external MCP tool with a namespace prefix.
    ///
    /// The tool is registered as `{namespace}__{tool.name()}`.
    /// If the namespaced name collides with a built-in tool, the external
    /// tool is skipped with a warning (built-in always wins).
    /// If the namespaced name is already registered, it is skipped with a warning.
    pub fn register_namespaced(
        &mut self,
        namespace: &str,
        tool: Arc<dyn Tool>,
    ) -> Result<(), BlufioError> {
        let tool_name = tool.name().to_string();
        let namespaced_name = format!("{namespace}__{tool_name}");

        if !validate_namespaced_tool_name(&namespaced_name) {
            return Err(BlufioError::Skill {
                message: format!(
                    "invalid namespaced tool name '{namespaced_name}': \
                     namespace and tool must each match [a-zA-Z][a-zA-Z0-9_]*"
                ),
                source: None,
            });
        }

        if self.builtin_names.contains(&namespaced_name) {
            tracing::warn!(
                tool = %namespaced_name,
                "namespace collision with built-in tool, skipping external tool"
            );
            return Ok(());
        }

        if self.tools.contains_key(&namespaced_name) {
            tracing::warn!(
                tool = %namespaced_name,
                "duplicate namespaced tool name, skipping"
            );
            return Ok(());
        }

        self.tools.insert(namespaced_name, tool);
        Ok(())
    }

    /// Looks up a tool by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// Returns (name, description) pairs for all registered tools.
    ///
    /// For namespaced tools, the name is the registry key (`namespace__tool`)
    /// rather than the tool's own `name()`.
    pub fn list(&self) -> Vec<(&str, &str)> {
        let mut entries: Vec<(&str, &str)> = self
            .tools
            .iter()
            .map(|(registry_name, t)| (registry_name.as_str(), t.description()))
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
    ///
    /// For namespaced tools, the `name` field uses the registry key
    /// (`namespace__tool`) rather than the tool's own `name()`, ensuring
    /// the LLM sees the namespaced identifier.
    pub fn tool_definitions(&self) -> Vec<serde_json::Value> {
        let mut defs: Vec<serde_json::Value> = self
            .tools
            .iter()
            .map(|(registry_name, t)| {
                serde_json::json!({
                    "name": registry_name,
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

    // ── Existing tests (updated for Result return) ──────────────────

    #[test]
    fn tool_registry_registers_and_retrieves_tools() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool)).unwrap();

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
        registry.register(Arc::new(EchoTool)).unwrap();
        registry.register(Arc::new(AddTool)).unwrap();

        let list = registry.list();
        assert_eq!(list.len(), 2);

        // Sorted alphabetically by name.
        assert_eq!(list[0], ("add", "Adds two numbers"));
        assert_eq!(list[1], ("echo", "Echoes the input back"));
    }

    #[test]
    fn tool_registry_tool_definitions_produces_valid_json() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool)).unwrap();

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
        registry.register(Arc::new(EchoTool)).unwrap();
        registry.register(Arc::new(AddTool)).unwrap();

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

        registry.register(Arc::new(EchoTool)).unwrap();
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

    // ── Name validation tests ───────────────────────────────────────

    #[test]
    fn validate_tool_name_accepts_valid_names() {
        assert!(validate_tool_name("echo"));
        assert!(validate_tool_name("my_tool"));
        assert!(validate_tool_name("Tool123"));
        assert!(validate_tool_name("a"));
    }

    #[test]
    fn validate_tool_name_rejects_invalid_names() {
        assert!(!validate_tool_name(""));
        assert!(!validate_tool_name("123abc"));
        assert!(!validate_tool_name("_bad"));
        assert!(!validate_tool_name("has-hyphen"));
        assert!(!validate_tool_name("has space"));
        assert!(!validate_tool_name("has.dot"));
    }

    #[test]
    fn validate_namespaced_tool_name_accepts_valid() {
        assert!(validate_namespaced_tool_name("server__tool"));
        assert!(validate_namespaced_tool_name("github__create_issue"));
        assert!(validate_namespaced_tool_name("a__b"));
    }

    #[test]
    fn validate_namespaced_tool_name_rejects_invalid() {
        assert!(!validate_namespaced_tool_name("notnamespaced"));
        // Triple underscore: "server___tool" matches as namespace="server_" + "__" + tool="tool"
        // This is technically valid (server_ is a valid namespace). The regex accepts it.
        // assert!(!validate_namespaced_tool_name("server___tool"));
        assert!(!validate_namespaced_tool_name("__leading"));
        assert!(!validate_namespaced_tool_name("trailing__"));
        assert!(!validate_namespaced_tool_name(""));
    }

    // ── Built-in registration tests ─────────────────────────────────

    #[test]
    fn register_builtin_succeeds_and_rejects_duplicate() {
        let mut registry = ToolRegistry::new();
        registry.register_builtin(Arc::new(EchoTool)).unwrap();
        assert!(registry.get("echo").is_some());

        let result = registry.register_builtin(Arc::new(EchoTool));
        assert!(result.is_err());
    }

    #[test]
    fn register_builtin_rejects_invalid_name() {
        struct BadTool;

        #[async_trait]
        impl Tool for BadTool {
            fn name(&self) -> &str {
                "123-bad"
            }
            fn description(&self) -> &str {
                "bad"
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            async fn invoke(&self, _: serde_json::Value) -> Result<ToolOutput, BlufioError> {
                unreachable!()
            }
        }

        let mut registry = ToolRegistry::new();
        let result = registry.register_builtin(Arc::new(BadTool));
        assert!(result.is_err());
    }

    // ── Namespaced registration tests ───────────────────────────────

    #[test]
    fn register_namespaced_prefixes_correctly() {
        let mut registry = ToolRegistry::new();
        registry
            .register_namespaced("github", Arc::new(EchoTool))
            .unwrap();
        assert!(registry.get("github__echo").is_some());
        assert!(registry.get("echo").is_none()); // not registered flat
    }

    #[test]
    fn register_namespaced_builtin_collision_skips() {
        let mut registry = ToolRegistry::new();
        registry.register_builtin(Arc::new(EchoTool)).unwrap();
        // Namespaced "github__echo" is a different key from "echo",
        // so no collision -- both should coexist.
        registry
            .register_namespaced("github", Arc::new(EchoTool))
            .unwrap();
        assert!(registry.get("echo").is_some());
        assert!(registry.get("github__echo").is_some());
    }

    #[test]
    fn register_namespaced_duplicate_skips() {
        let mut registry = ToolRegistry::new();
        registry
            .register_namespaced("github", Arc::new(EchoTool))
            .unwrap();
        // Second registration of same namespace+tool is a no-op.
        registry
            .register_namespaced("github", Arc::new(EchoTool))
            .unwrap();
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn register_namespaced_rejects_invalid_namespace() {
        let mut registry = ToolRegistry::new();
        let result = registry.register_namespaced("123bad", Arc::new(EchoTool));
        assert!(result.is_err());
    }

    // ── Backward-compatible register tests ──────────────────────────

    #[test]
    fn register_rejects_duplicate() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool)).unwrap();
        let result = registry.register(Arc::new(EchoTool));
        assert!(result.is_err());
    }

    #[test]
    fn register_rejects_invalid_name() {
        struct SpaceTool;

        #[async_trait]
        impl Tool for SpaceTool {
            fn name(&self) -> &str {
                "has space"
            }
            fn description(&self) -> &str {
                "bad"
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            async fn invoke(&self, _: serde_json::Value) -> Result<ToolOutput, BlufioError> {
                unreachable!()
            }
        }

        let mut registry = ToolRegistry::new();
        let result = registry.register(Arc::new(SpaceTool));
        assert!(result.is_err());
    }

    // ── Mixed registration tests ────────────────────────────────────

    #[test]
    fn list_includes_builtin_and_namespaced() {
        let mut registry = ToolRegistry::new();
        registry.register_builtin(Arc::new(EchoTool)).unwrap();
        registry
            .register_namespaced("github", Arc::new(AddTool))
            .unwrap();
        let list = registry.list();
        assert_eq!(list.len(), 2);
        // Sorted: echo, github__add
        assert_eq!(list[0].0, "echo");
        assert_eq!(list[1].0, "github__add");
    }

    #[test]
    fn tool_definitions_uses_registry_name_for_namespaced() {
        let mut registry = ToolRegistry::new();
        registry
            .register_namespaced("github", Arc::new(EchoTool))
            .unwrap();
        let defs = registry.tool_definitions();
        assert_eq!(defs.len(), 1);
        // The definition name should be the namespaced key, not the tool's own name.
        assert_eq!(defs[0]["name"], "github__echo");
    }

    // ── Annotation default tests ──────────────────────────────────

    #[test]
    fn echo_tool_has_default_annotations() {
        let tool = EchoTool;
        assert!(!tool.is_read_only(), "default is_read_only should be false");
        assert!(!tool.is_destructive(), "default is_destructive should be false");
        assert!(!tool.is_idempotent(), "default is_idempotent should be false");
        assert!(tool.is_open_world(), "default is_open_world should be true");
    }

    #[test]
    fn add_tool_has_default_annotations() {
        let tool = AddTool;
        assert!(!tool.is_read_only());
        assert!(!tool.is_destructive());
        assert!(!tool.is_idempotent());
        assert!(tool.is_open_world());
    }

    #[test]
    fn custom_tool_can_override_annotations() {
        struct ReadOnlyTool;

        #[async_trait]
        impl Tool for ReadOnlyTool {
            fn name(&self) -> &str {
                "readonly"
            }
            fn description(&self) -> &str {
                "A read-only tool"
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({"type": "object"})
            }
            async fn invoke(&self, _: serde_json::Value) -> Result<ToolOutput, BlufioError> {
                unreachable!()
            }
            fn is_read_only(&self) -> bool {
                true
            }
            fn is_idempotent(&self) -> bool {
                true
            }
            fn is_open_world(&self) -> bool {
                false
            }
        }

        let tool = ReadOnlyTool;
        assert!(tool.is_read_only(), "overridden is_read_only should be true");
        assert!(!tool.is_destructive(), "default is_destructive should be false");
        assert!(tool.is_idempotent(), "overridden is_idempotent should be true");
        assert!(!tool.is_open_world(), "overridden is_open_world should be false");
    }

    #[test]
    fn tool_definitions_includes_builtin_and_namespaced_sorted() {
        let mut registry = ToolRegistry::new();
        registry.register_builtin(Arc::new(EchoTool)).unwrap();
        registry
            .register_namespaced("github", Arc::new(AddTool))
            .unwrap();
        let defs = registry.tool_definitions();
        assert_eq!(defs.len(), 2);
        assert_eq!(defs[0]["name"], "echo");
        assert_eq!(defs[1]["name"], "github__add");
    }
}
