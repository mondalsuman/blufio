// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! External MCP tool wrapper implementing the Blufio [`Tool`] trait.
//!
//! Each discovered tool from a remote MCP server is wrapped in an
//! [`ExternalTool`] that translates between Blufio's tool interface
//! and the rmcp client API. The wrapper handles:
//!
//! - Namespaced naming (`server__tool` format)
//! - Sanitized descriptions (instruction stripping, length capping)
//! - Response truncation per server size cap (CLNT-09)
//! - Error mapping from rmcp errors to `ToolOutput { is_error: true }`

use std::borrow::Cow;
use std::sync::Arc;

use async_trait::async_trait;
use blufio_core::BlufioError;
use blufio_skill::tool::{Tool as BlufioTool, ToolOutput};
use rmcp::RoleClient;
use rmcp::model::{CallToolRequestParams, CallToolResult, RawContent};
use rmcp::service::RunningService;

use crate::sanitize::truncate_response;

/// An external MCP tool discovered from a remote server.
///
/// Implements the Blufio [`Tool`] trait so it can be registered in the
/// [`ToolRegistry`](blufio_skill::tool::ToolRegistry) and invoked by
/// the agent loop like any built-in tool.
///
/// All rmcp types are internal; the public API uses only Blufio types.
pub struct ExternalTool {
    /// Server name for cost attribution.
    server_name: String,
    /// Original tool name from the MCP server.
    tool_name: String,
    /// Namespaced name: `server__tool`.
    namespaced_name: String,
    /// Sanitized description (already processed by `sanitize_description`).
    description: String,
    /// Tool input schema as JSON Value.
    schema: serde_json::Value,
    /// Reference to the running MCP client session.
    session: Arc<RunningService<RoleClient, ()>>,
    /// Maximum response size in characters.
    response_size_cap: usize,
    /// Optional L1 injection classifier for scanning tool output.
    /// When present, tool output is scanned before returning to the LLM.
    injection_classifier: Option<Arc<blufio_injection::classifier::InjectionClassifier>>,
    /// Whether this server is trusted (skip injection scanning).
    trusted: bool,
}

impl ExternalTool {
    /// Create a new external tool wrapper.
    ///
    /// The `description` should already be sanitized via
    /// [`sanitize_description`](crate::sanitize::sanitize_description).
    pub fn new(
        server_name: &str,
        tool_name: String,
        description: String,
        schema: serde_json::Value,
        session: Arc<RunningService<RoleClient, ()>>,
        response_size_cap: usize,
    ) -> Self {
        let namespaced_name = format!("{server_name}__{tool_name}");
        Self {
            server_name: server_name.to_string(),
            tool_name,
            namespaced_name,
            description,
            schema,
            session,
            response_size_cap,
            injection_classifier: None,
            trusted: false,
        }
    }

    /// Set the injection classifier for output scanning.
    pub fn set_injection_classifier(
        &mut self,
        classifier: Arc<blufio_injection::classifier::InjectionClassifier>,
    ) {
        self.injection_classifier = Some(classifier);
    }

    /// Mark this tool's server as trusted (skip injection scanning).
    pub fn set_trusted(&mut self, trusted: bool) {
        self.trusted = trusted;
    }

    /// Get the server name for cost attribution.
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// Extract text content from an MCP `CallToolResult`.
    ///
    /// Concatenates all text content blocks, ignoring image/audio content.
    /// Returns a fallback message if no text content is present.
    fn extract_text(result: &CallToolResult) -> String {
        let mut text_parts = Vec::new();
        for content in &result.content {
            if let RawContent::Text(text_content) = &content.raw {
                text_parts.push(text_content.text.as_str());
            }
        }

        if text_parts.is_empty() {
            "[no text content in response]".to_string()
        } else {
            text_parts.join("\n")
        }
    }
}

#[async_trait]
impl BlufioTool for ExternalTool {
    fn name(&self) -> &str {
        &self.namespaced_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.schema.clone()
    }

    async fn invoke(&self, input: serde_json::Value) -> Result<ToolOutput, BlufioError> {
        let result = self
            .session
            .call_tool(CallToolRequestParams {
                meta: None,
                name: Cow::Owned(self.tool_name.clone()),
                arguments: input.as_object().cloned(),
                task: None,
            })
            .await
            .map_err(BlufioError::mcp_tool_failed)?;

        // Extract text content from MCP response.
        let content = Self::extract_text(&result);

        // INTG-04: Record tool response size metric.
        let response_bytes = content.len() as f64;
        blufio_prometheus::recording::record_mcp_tool_response_size(response_bytes);

        // CLNT-09: Truncate if over per-server cap.
        let content = truncate_response(&content, self.response_size_cap);

        // INJC-06: Scan tool output with L1 classifier (if enabled and not trusted).
        if !self.trusted {
            if let Some(ref classifier) = self.injection_classifier {
                let scan = classifier.classify(&content, "mcp");
                if scan.score > 0.0 {
                    tracing::warn!(
                        server = %self.server_name,
                        tool = %self.tool_name,
                        score = scan.score,
                        action = %scan.action,
                        "injection pattern detected in MCP tool output"
                    );
                    blufio_injection::metrics::record_input_detection("mcp", &scan.action);

                    // Block if score exceeds MCP blocking threshold (0.98).
                    if scan.score >= 0.98 {
                        return Ok(ToolOutput {
                            content: "[Tool output blocked by injection defense]".to_string(),
                            is_error: true,
                        });
                    }
                }
            }
        }

        Ok(ToolOutput {
            content,
            is_error: result.is_error.unwrap_or(false),
        })
    }

    /// External tools are never read-only by default (we cannot verify).
    fn is_read_only(&self) -> bool {
        false
    }

    /// External tools interact with systems outside Blufio's control.
    fn is_open_world(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::{Annotated, Content, RawTextContent};

    fn make_text_content(text: &str) -> Content {
        Annotated {
            raw: RawContent::Text(RawTextContent {
                text: text.to_string(),
                meta: None,
            }),
            annotations: None,
        }
    }

    #[test]
    fn extract_text_single_block() {
        let result = CallToolResult {
            content: vec![make_text_content("hello world")],
            structured_content: None,
            is_error: None,
            meta: None,
        };
        assert_eq!(ExternalTool::extract_text(&result), "hello world");
    }

    #[test]
    fn extract_text_multiple_blocks() {
        let result = CallToolResult {
            content: vec![make_text_content("line 1"), make_text_content("line 2")],
            structured_content: None,
            is_error: None,
            meta: None,
        };
        assert_eq!(ExternalTool::extract_text(&result), "line 1\nline 2");
    }

    #[test]
    fn extract_text_empty_content() {
        let result = CallToolResult {
            content: vec![],
            structured_content: None,
            is_error: None,
            meta: None,
        };
        assert_eq!(
            ExternalTool::extract_text(&result),
            "[no text content in response]"
        );
    }

    #[test]
    fn namespaced_name_format() {
        // Verify the double-underscore naming convention.
        let namespaced = format!("{}__{}", "github", "search");
        assert_eq!(namespaced, "github__search");

        // Single underscore is a different name.
        let single = format!("{}_{}", "github", "search");
        assert_ne!(single, namespaced);
    }
}
