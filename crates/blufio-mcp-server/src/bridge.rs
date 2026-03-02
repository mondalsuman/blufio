// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bridge between Blufio's [`ToolRegistry`] and MCP tool types.
//!
//! Converts Blufio tool definitions (Anthropic format) to rmcp [`Tool`]
//! structs, and filters the tool list based on the MCP export allowlist.
//! The `bash` tool is **permanently excluded** from MCP exports regardless
//! of configuration (SRVR-12).

use std::sync::Arc;

use blufio_skill::tool::{Tool as BlufioTool, ToolRegistry};

/// Returns the names of tools that should be exported via MCP.
///
/// Applies the export allowlist from [`McpConfig::export_tools`]:
/// - Empty allowlist: export all registered tools **except** `bash`
/// - Non-empty allowlist: export only listed tools that exist in the registry,
///   **except** `bash` (which is always excluded)
///
/// If `bash` appears in `export_tools`, it is silently ignored with a
/// warning log.
pub fn filtered_tool_names(registry: &ToolRegistry, export_tools: &[String]) -> Vec<String> {
    // Warn if bash is in the explicit allowlist.
    if export_tools.iter().any(|t| t == "bash") {
        tracing::warn!(
            "'bash' in mcp.export_tools is ignored (security: never exported via MCP)"
        );
    }

    let all_tools: Vec<(&str, &str)> = registry.list();

    if export_tools.is_empty() {
        // Export all non-bash tools.
        all_tools
            .into_iter()
            .filter(|(name, _)| *name != "bash")
            .map(|(name, _)| name.to_string())
            .collect()
    } else {
        // Export only explicitly listed tools (minus bash).
        all_tools
            .into_iter()
            .filter(|(name, _)| {
                *name != "bash" && export_tools.iter().any(|e| e == name)
            })
            .map(|(name, _)| name.to_string())
            .collect()
    }
}

/// Converts a Blufio tool to an rmcp [`Tool`] struct.
///
/// Maps from Blufio's Anthropic-format tool definition to MCP's tool
/// schema format:
/// - `name` -> `rmcp::model::Tool::name`
/// - `description` -> `rmcp::model::Tool::description`
/// - `parameters_schema()` -> `rmcp::model::Tool::input_schema` (as `JsonObject`)
///
/// The `parameters_schema()` JSON value is expected to be a JSON Schema
/// object with `type`, `properties`, and `required` fields. These are
/// extracted into an rmcp `JsonObject` (`serde_json::Map<String, Value>`).
pub fn to_mcp_tool(name: &str, tool: &dyn BlufioTool) -> rmcp::model::Tool {
    let schema = tool.parameters_schema();

    // Convert the serde_json::Value to a JsonObject (Map<String, Value>).
    // The schema should be a JSON object; if not, use an empty object.
    let json_object: serde_json::Map<String, serde_json::Value> = match schema {
        serde_json::Value::Object(map) => map,
        _ => {
            tracing::warn!(
                tool = name,
                "tool parameters_schema() did not return an object, using empty schema"
            );
            let mut map = serde_json::Map::new();
            map.insert(
                "type".to_string(),
                serde_json::Value::String("object".to_string()),
            );
            map
        }
    };

    rmcp::model::Tool::new(
        name.to_string(),
        tool.description().to_string(),
        Arc::new(json_object),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use blufio_core::BlufioError;
    use blufio_skill::tool::ToolOutput;

    /// Test tool that mimics bash (for filtering tests).
    struct BashTool;

    #[async_trait]
    impl BlufioTool for BashTool {
        fn name(&self) -> &str {
            "bash"
        }
        fn description(&self) -> &str {
            "Execute shell commands"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" }
                },
                "required": ["command"]
            })
        }
        async fn invoke(&self, _input: serde_json::Value) -> Result<ToolOutput, BlufioError> {
            unreachable!()
        }
    }

    /// Test HTTP tool.
    struct HttpTool;

    #[async_trait]
    impl BlufioTool for HttpTool {
        fn name(&self) -> &str {
            "http"
        }
        fn description(&self) -> &str {
            "Make HTTP requests"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "Request URL" },
                    "method": { "type": "string" }
                },
                "required": ["url"]
            })
        }
        async fn invoke(&self, _input: serde_json::Value) -> Result<ToolOutput, BlufioError> {
            unreachable!()
        }
    }

    /// Test file tool.
    struct FileTool;

    #[async_trait]
    impl BlufioTool for FileTool {
        fn name(&self) -> &str {
            "file"
        }
        fn description(&self) -> &str {
            "Read and write files"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            })
        }
        async fn invoke(&self, _input: serde_json::Value) -> Result<ToolOutput, BlufioError> {
            unreachable!()
        }
    }

    /// Helper: create a registry with bash, http, and file tools.
    fn registry_with_builtins() -> ToolRegistry {
        let mut registry = ToolRegistry::new();
        registry
            .register_builtin(Arc::new(BashTool))
            .expect("register bash");
        registry
            .register_builtin(Arc::new(HttpTool))
            .expect("register http");
        registry
            .register_builtin(Arc::new(FileTool))
            .expect("register file");
        registry
    }

    // ── filtered_tool_names tests ──────────────────────────────────

    #[test]
    fn empty_export_tools_returns_all_except_bash() {
        let registry = registry_with_builtins();
        let result = filtered_tool_names(&registry, &[]);
        assert!(!result.contains(&"bash".to_string()));
        assert!(result.contains(&"http".to_string()));
        assert!(result.contains(&"file".to_string()));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn explicit_allowlist_returns_only_listed() {
        let registry = registry_with_builtins();
        let export = vec!["http".to_string(), "file".to_string()];
        let result = filtered_tool_names(&registry, &export);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"http".to_string()));
        assert!(result.contains(&"file".to_string()));
    }

    #[test]
    fn bash_in_allowlist_is_excluded() {
        let registry = registry_with_builtins();
        let export = vec!["bash".to_string(), "http".to_string()];
        let result = filtered_tool_names(&registry, &export);
        assert!(!result.contains(&"bash".to_string()));
        assert!(result.contains(&"http".to_string()));
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn nonexistent_tool_in_allowlist_returns_empty() {
        let registry = registry_with_builtins();
        let export = vec!["nonexistent".to_string()];
        let result = filtered_tool_names(&registry, &export);
        assert!(result.is_empty());
    }

    #[test]
    fn allowlist_with_only_bash_returns_empty() {
        let registry = registry_with_builtins();
        let export = vec!["bash".to_string()];
        let result = filtered_tool_names(&registry, &export);
        assert!(result.is_empty());
    }

    #[test]
    fn empty_registry_returns_empty() {
        let registry = ToolRegistry::new();
        let result = filtered_tool_names(&registry, &[]);
        assert!(result.is_empty());
    }

    // ── to_mcp_tool tests ──────────────────────────────────────────

    #[test]
    fn to_mcp_tool_converts_name_and_description() {
        let tool = HttpTool;
        let mcp_tool = to_mcp_tool("http", &tool);
        assert_eq!(mcp_tool.name.as_ref(), "http");
        assert_eq!(
            mcp_tool.description.as_deref(),
            Some("Make HTTP requests")
        );
    }

    #[test]
    fn to_mcp_tool_preserves_properties_and_required() {
        let tool = HttpTool;
        let mcp_tool = to_mcp_tool("http", &tool);

        // The input_schema should contain the schema fields.
        let schema = &*mcp_tool.input_schema;
        assert_eq!(
            schema.get("type").and_then(|v| v.as_str()),
            Some("object")
        );

        let properties = schema.get("properties").expect("should have properties");
        assert!(properties.get("url").is_some());
        assert!(properties.get("method").is_some());

        let required = schema
            .get("required")
            .expect("should have required")
            .as_array()
            .expect("required should be array");
        assert_eq!(required.len(), 1);
        assert_eq!(required[0].as_str(), Some("url"));
    }

    #[test]
    fn to_mcp_tool_with_empty_schema() {
        struct MinimalTool;

        #[async_trait]
        impl BlufioTool for MinimalTool {
            fn name(&self) -> &str {
                "minimal"
            }
            fn description(&self) -> &str {
                "A minimal tool"
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({"type": "object"})
            }
            async fn invoke(
                &self,
                _input: serde_json::Value,
            ) -> Result<ToolOutput, BlufioError> {
                unreachable!()
            }
        }

        let tool = MinimalTool;
        let mcp_tool = to_mcp_tool("minimal", &tool);
        assert_eq!(mcp_tool.name.as_ref(), "minimal");
        let schema = &*mcp_tool.input_schema;
        assert_eq!(
            schema.get("type").and_then(|v| v.as_str()),
            Some("object")
        );
    }

    #[test]
    fn to_mcp_tool_uses_provided_name_not_tool_name() {
        // Verify that namespaced tools use the registry name, not tool.name().
        let tool = HttpTool;
        let mcp_tool = to_mcp_tool("github__http", &tool);
        assert_eq!(mcp_tool.name.as_ref(), "github__http");
    }
}
