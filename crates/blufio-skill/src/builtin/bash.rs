// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Built-in bash command execution tool.
//!
//! Executes shell commands via `bash -c` and returns stdout/stderr.
//! No restrictions on bash access -- this is a personal agent on a single-user VPS.

use async_trait::async_trait;
use blufio_core::BlufioError;

use crate::tool::{Tool, ToolOutput};

/// Executes bash commands and returns stdout/stderr.
pub struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute a bash command and return stdout/stderr"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute"
                }
            },
            "required": ["command"]
        })
    }

    async fn invoke(&self, input: serde_json::Value) -> Result<ToolOutput, BlufioError> {
        let command = input["command"]
            .as_str()
            .ok_or_else(|| BlufioError::Skill {
                message: "missing required 'command' parameter".to_string(),
                source: None,
            })?;

        let output = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(command)
            .output()
            .await
            .map_err(|e| BlufioError::Skill {
                message: format!("failed to execute bash command: {e}"),
                source: Some(Box::new(e)),
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let is_error = !output.status.success();
        let content = if is_error {
            let exit_code = output.status.code().unwrap_or(-1);
            format!(
                "Exit code: {exit_code}\nstdout:\n{stdout}\nstderr:\n{stderr}"
            )
        } else if stderr.is_empty() {
            stdout.to_string()
        } else {
            format!("{stdout}\nstderr:\n{stderr}")
        };

        Ok(ToolOutput { content, is_error })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bash_tool_echo_hello() {
        let tool = BashTool;
        let input = serde_json::json!({"command": "echo hello"});
        let output = tool.invoke(input).await.unwrap();
        assert_eq!(output.content.trim(), "hello");
        assert!(!output.is_error);
    }

    #[tokio::test]
    async fn bash_tool_exit_nonzero_returns_error() {
        let tool = BashTool;
        let input = serde_json::json!({"command": "exit 1"});
        let output = tool.invoke(input).await.unwrap();
        assert!(output.is_error);
        assert!(output.content.contains("Exit code: 1"));
    }

    #[tokio::test]
    async fn bash_tool_missing_command_returns_error() {
        let tool = BashTool;
        let input = serde_json::json!({});
        let result = tool.invoke(input).await;
        assert!(result.is_err());
    }

    #[test]
    fn bash_tool_parameters_schema_has_required_command() {
        let tool = BashTool;
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "command"));
        assert!(schema["properties"]["command"].is_object());
    }
}
