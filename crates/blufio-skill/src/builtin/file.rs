// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Built-in file I/O tool.
//!
//! Reads and writes files on the filesystem. Full filesystem access is
//! permitted -- consistent with bash access for the personal agent.
//! Read contents are truncated to 100KB to prevent excessive token usage.

use async_trait::async_trait;
use blufio_core::BlufioError;

use crate::tool::{Tool, ToolOutput};

/// Maximum file read size in bytes (100KB).
const MAX_READ_SIZE: usize = 100 * 1024;

/// Reads and writes files on the filesystem.
pub struct FileTool;

#[async_trait]
impl Tool for FileTool {
    fn name(&self) -> &str {
        "file"
    }

    fn description(&self) -> &str {
        "Read or write files on the filesystem"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["read", "write"],
                    "description": "Whether to read or write the file"
                },
                "path": {
                    "type": "string",
                    "description": "The file path to read from or write to"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write (required for write action)"
                }
            },
            "required": ["action", "path"]
        })
    }

    async fn invoke(&self, input: serde_json::Value) -> Result<ToolOutput, BlufioError> {
        let action = input["action"]
            .as_str()
            .ok_or_else(|| BlufioError::Skill {
                message: "missing required 'action' parameter".to_string(),
                source: None,
            })?;

        let path = input["path"].as_str().ok_or_else(|| BlufioError::Skill {
            message: "missing required 'path' parameter".to_string(),
            source: None,
        })?;

        match action {
            "read" => {
                let contents =
                    tokio::fs::read_to_string(path)
                        .await
                        .map_err(|e| BlufioError::Skill {
                            message: format!("failed to read file '{path}': {e}"),
                            source: Some(Box::new(e)),
                        })?;

                // Truncate if too large.
                let output = if contents.len() > MAX_READ_SIZE {
                    format!(
                        "{}...\n\n[File truncated from {} to {} bytes]",
                        &contents[..MAX_READ_SIZE],
                        contents.len(),
                        MAX_READ_SIZE
                    )
                } else {
                    contents
                };

                Ok(ToolOutput {
                    content: output,
                    is_error: false,
                })
            }
            "write" => {
                let content =
                    input["content"]
                        .as_str()
                        .ok_or_else(|| BlufioError::Skill {
                            message: "missing required 'content' parameter for write action"
                                .to_string(),
                            source: None,
                        })?;

                tokio::fs::write(path, content)
                    .await
                    .map_err(|e| BlufioError::Skill {
                        message: format!("failed to write file '{path}': {e}"),
                        source: Some(Box::new(e)),
                    })?;

                Ok(ToolOutput {
                    content: format!("Successfully wrote {} bytes to '{path}'", content.len()),
                    is_error: false,
                })
            }
            other => Ok(ToolOutput {
                content: format!(
                    "Unknown action '{other}'. Supported actions: 'read', 'write'."
                ),
                is_error: true,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn file_tool_read_nonexistent_returns_error() {
        let tool = FileTool;
        let input = serde_json::json!({
            "action": "read",
            "path": "/tmp/blufio-test-nonexistent-file-xyz-12345"
        });
        let result = tool.invoke(input).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn file_tool_write_and_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let path_str = file_path.to_str().unwrap();

        let tool = FileTool;

        // Write.
        let write_input = serde_json::json!({
            "action": "write",
            "path": path_str,
            "content": "hello from blufio"
        });
        let write_output = tool.invoke(write_input).await.unwrap();
        assert!(!write_output.is_error);
        assert!(write_output.content.contains("Successfully wrote"));

        // Read.
        let read_input = serde_json::json!({
            "action": "read",
            "path": path_str
        });
        let read_output = tool.invoke(read_input).await.unwrap();
        assert!(!read_output.is_error);
        assert_eq!(read_output.content, "hello from blufio");
    }

    #[tokio::test]
    async fn file_tool_unknown_action_returns_error() {
        let tool = FileTool;
        let input = serde_json::json!({
            "action": "delete",
            "path": "/tmp/test"
        });
        let output = tool.invoke(input).await.unwrap();
        assert!(output.is_error);
        assert!(output.content.contains("Unknown action"));
    }

    #[test]
    fn file_tool_parameters_schema_has_required_fields() {
        let tool = FileTool;
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "action"));
        assert!(required.iter().any(|v| v == "path"));
    }
}
