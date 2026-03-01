// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Built-in HTTP request tool.
//!
//! Makes HTTP requests using reqwest with SSRF prevention from blufio-security.
//! Response bodies are truncated to 50KB to prevent excessive token usage.

use async_trait::async_trait;
use blufio_core::BlufioError;

use crate::tool::{Tool, ToolOutput};

/// Maximum response body size in bytes (50KB).
const MAX_RESPONSE_SIZE: usize = 50 * 1024;

/// Makes HTTP requests and returns the response.
pub struct HttpTool {
    client: reqwest::Client,
}

impl HttpTool {
    /// Creates a new HttpTool with a default reqwest Client.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for HttpTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for HttpTool {
    fn name(&self) -> &str {
        "http"
    }

    fn description(&self) -> &str {
        "Make an HTTP request and return the response"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to request"
                },
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "DELETE", "PATCH"],
                    "default": "GET",
                    "description": "HTTP method to use"
                },
                "headers": {
                    "type": "object",
                    "description": "HTTP headers as key-value pairs"
                },
                "body": {
                    "type": "string",
                    "description": "Request body (for POST, PUT, PATCH)"
                }
            },
            "required": ["url"]
        })
    }

    async fn invoke(&self, input: serde_json::Value) -> Result<ToolOutput, BlufioError> {
        let url = input["url"].as_str().ok_or_else(|| BlufioError::Skill {
            message: "missing required 'url' parameter".to_string(),
            source: None,
        })?;

        // Validate URL scheme (http/https only).
        let parsed_url = reqwest::Url::parse(url).map_err(|e| BlufioError::Skill {
            message: format!("invalid URL: {e}"),
            source: Some(Box::new(e)),
        })?;

        let scheme = parsed_url.scheme();
        if scheme != "http" && scheme != "https" {
            return Ok(ToolOutput {
                content: format!("URL scheme '{scheme}' not allowed. Only http and https are supported."),
                is_error: true,
            });
        }

        // SSRF prevention: block private/internal IPs.
        if let Err(e) = blufio_security::ssrf::validate_url_host(url) {
            return Ok(ToolOutput {
                content: format!("SSRF prevention: {e}"),
                is_error: true,
            });
        }

        let method_str = input["method"].as_str().unwrap_or("GET");
        let method = method_str
            .parse::<reqwest::Method>()
            .map_err(|e| BlufioError::Skill {
                message: format!("invalid HTTP method '{method_str}': {e}"),
                source: Some(Box::new(e)),
            })?;

        let mut request_builder = self.client.request(method, url);

        // Add optional headers.
        if let Some(headers) = input["headers"].as_object() {
            for (key, value) in headers {
                if let Some(val_str) = value.as_str() {
                    request_builder = request_builder.header(key.as_str(), val_str);
                }
            }
        }

        // Add optional body.
        if let Some(body) = input["body"].as_str() {
            request_builder = request_builder.body(body.to_string());
        }

        let response = request_builder.send().await.map_err(|e| BlufioError::Skill {
            message: format!("HTTP request failed: {e}"),
            source: Some(Box::new(e)),
        })?;

        let status = response.status();
        let body = response.text().await.map_err(|e| BlufioError::Skill {
            message: format!("failed to read response body: {e}"),
            source: Some(Box::new(e)),
        })?;

        // Truncate response body if too large.
        let truncated = if body.len() > MAX_RESPONSE_SIZE {
            format!(
                "{}...\n\n[Response truncated from {} to {} bytes]",
                &body[..MAX_RESPONSE_SIZE],
                body.len(),
                MAX_RESPONSE_SIZE
            )
        } else {
            body
        };

        let content = format!("HTTP {status}\n\n{truncated}");
        let is_error = status.is_client_error() || status.is_server_error();

        Ok(ToolOutput { content, is_error })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_tool_parameters_schema_has_required_url() {
        let tool = HttpTool::new();
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "url"));
        assert!(schema["properties"]["url"].is_object());
    }

    #[test]
    fn http_tool_name_and_description() {
        let tool = HttpTool::new();
        assert_eq!(tool.name(), "http");
        assert!(!tool.description().is_empty());
    }

    #[tokio::test]
    async fn http_tool_missing_url_returns_error() {
        let tool = HttpTool::new();
        let input = serde_json::json!({});
        let result = tool.invoke(input).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn http_tool_invalid_scheme_returns_error() {
        let tool = HttpTool::new();
        let input = serde_json::json!({"url": "ftp://example.com/file"});
        let output = tool.invoke(input).await.unwrap();
        assert!(output.is_error);
        assert!(output.content.contains("not allowed"));
    }

    #[tokio::test]
    async fn http_tool_ssrf_blocks_private_ip() {
        let tool = HttpTool::new();
        let input = serde_json::json!({"url": "http://192.168.1.1/admin"});
        let output = tool.invoke(input).await.unwrap();
        assert!(output.is_error);
        assert!(output.content.contains("SSRF"));
    }
}
