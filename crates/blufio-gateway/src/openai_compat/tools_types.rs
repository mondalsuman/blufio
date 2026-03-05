// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Wire types for the Tools API (/v1/tools, /v1/tools/invoke).
//!
//! Provides tool listing in OpenAI function schema format with extended
//! metadata (source, version) and direct tool invocation bypassing the LLM.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Query / list types
// ---------------------------------------------------------------------------

/// Query parameters for GET /v1/tools.
#[derive(Debug, Deserialize)]
pub struct ToolsQueryParams {
    /// Filter by tool source: "builtin", "wasm", "mcp".
    #[serde(default)]
    pub source: Option<String>,
}

/// Response for GET /v1/tools.
#[derive(Debug, Serialize)]
pub struct ToolListResponse {
    /// Object type (always "list").
    pub object: String,
    /// Tool data.
    pub data: Vec<ToolInfo>,
}

/// Extended tool info in OpenAI function schema format.
#[derive(Debug, Serialize)]
pub struct ToolInfo {
    /// Tool type (always "function").
    #[serde(rename = "type")]
    pub tool_type: String,
    /// Function definition.
    pub function: ToolFunctionInfo,
    /// Extended: tool source ("builtin", "wasm", "mcp").
    pub source: String,
    /// Extended: tool version (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Extended: required permissions/capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_permissions: Option<Vec<String>>,
}

/// Function info within a ToolInfo.
#[derive(Debug, Serialize)]
pub struct ToolFunctionInfo {
    /// Function name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// JSON Schema for parameters.
    pub parameters: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Invoke types
// ---------------------------------------------------------------------------

/// Request body for POST /v1/tools/invoke.
#[derive(Debug, Deserialize)]
pub struct ToolInvokeRequest {
    /// Tool name to invoke.
    pub name: String,
    /// JSON input to pass to the tool.
    #[serde(default = "default_empty_object")]
    pub input: serde_json::Value,
}

fn default_empty_object() -> serde_json::Value {
    serde_json::json!({})
}

/// Response body for POST /v1/tools/invoke.
#[derive(Debug, Serialize)]
pub struct ToolInvokeResponse {
    /// Tool name that was invoked.
    pub name: String,
    /// Tool execution output.
    pub output: String,
    /// Whether the execution resulted in an error.
    pub is_error: bool,
    /// Execution time in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

// ---------------------------------------------------------------------------
// Helper: determine tool source from name pattern
// ---------------------------------------------------------------------------

/// Determine tool source from its registry name.
///
/// - Names containing `__` (e.g., `mcp_server__tool`) are namespaced
///   and assumed to be MCP or WASM tools.
/// - Simple names (e.g., `bash`, `http`) are built-in tools.
pub fn tool_source_from_name(name: &str) -> &str {
    if name.contains("__") {
        // Namespaced: could be MCP or WASM. Check prefix heuristics.
        if name.starts_with("mcp_") || name.starts_with("mcp__") {
            "mcp"
        } else {
            "wasm"
        }
    } else {
        "builtin"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_list_response_serializes() {
        let resp = ToolListResponse {
            object: "list".into(),
            data: vec![ToolInfo {
                tool_type: "function".into(),
                function: ToolFunctionInfo {
                    name: "bash".into(),
                    description: "Execute a bash command".into(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "command": { "type": "string" }
                        },
                        "required": ["command"]
                    }),
                },
                source: "builtin".into(),
                version: None,
                required_permissions: None,
            }],
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["object"], "list");
        assert_eq!(json["data"][0]["type"], "function");
        assert_eq!(json["data"][0]["function"]["name"], "bash");
        assert_eq!(json["data"][0]["source"], "builtin");
    }

    #[test]
    fn tool_info_with_metadata_serializes() {
        let info = ToolInfo {
            tool_type: "function".into(),
            function: ToolFunctionInfo {
                name: "http".into(),
                description: "Make HTTP requests".into(),
                parameters: serde_json::json!({"type": "object"}),
            },
            source: "builtin".into(),
            version: Some("0.1.0".into()),
            required_permissions: Some(vec!["network".into()]),
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["version"], "0.1.0");
        assert_eq!(json["required_permissions"][0], "network");
    }

    #[test]
    fn tool_invoke_request_deserializes() {
        let json = r#"{"name": "bash", "input": {"command": "echo hello"}}"#;
        let req: ToolInvokeRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "bash");
        assert_eq!(req.input["command"], "echo hello");
    }

    #[test]
    fn tool_invoke_request_defaults_empty_input() {
        let json = r#"{"name": "bash"}"#;
        let req: ToolInvokeRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.input, serde_json::json!({}));
    }

    #[test]
    fn tool_invoke_response_serializes() {
        let resp = ToolInvokeResponse {
            name: "bash".into(),
            output: "hello\n".into(),
            is_error: false,
            duration_ms: Some(42),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["name"], "bash");
        assert_eq!(json["output"], "hello\n");
        assert_eq!(json["is_error"], false);
        assert_eq!(json["duration_ms"], 42);
    }

    #[test]
    fn tool_invoke_response_omits_none_duration() {
        let resp = ToolInvokeResponse {
            name: "bash".into(),
            output: "error".into(),
            is_error: true,
            duration_ms: None,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json.get("duration_ms").is_none());
    }

    #[test]
    fn tool_source_builtin() {
        assert_eq!(tool_source_from_name("bash"), "builtin");
        assert_eq!(tool_source_from_name("http"), "builtin");
        assert_eq!(tool_source_from_name("file_read"), "builtin");
    }

    #[test]
    fn tool_source_namespaced() {
        assert_eq!(tool_source_from_name("mcp_server__search"), "mcp");
        assert_eq!(tool_source_from_name("github__create_issue"), "wasm");
        assert_eq!(tool_source_from_name("custom__tool"), "wasm");
    }

    #[test]
    fn tools_query_params_deserializes() {
        let json = r#"{"source": "builtin"}"#;
        let params: ToolsQueryParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.source.as_deref(), Some("builtin"));
    }

    #[test]
    fn tools_query_params_empty() {
        let json = r#"{}"#;
        let params: ToolsQueryParams = serde_json::from_str(json).unwrap();
        assert!(params.source.is_none());
    }
}
