// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Handlers for the Tools API (/v1/tools, /v1/tools/invoke).
//!
//! Provides tool discovery in OpenAI function schema format and direct
//! tool execution bypassing the LLM. Tool access is controlled by a
//! config-based allowlist.

use std::time::Instant;

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::server::GatewayState;

use super::tools_types::*;
use super::types::{GatewayErrorDetail, GatewayErrorResponse};

/// GET /v1/tools
///
/// Returns a list of available tools in OpenAI function schema format.
/// Only tools in the config allowlist are returned.
pub async fn get_tools(
    State(state): State<GatewayState>,
    Query(params): Query<ToolsQueryParams>,
) -> Response {
    let tools = match &state.tools {
        Some(t) => t,
        None => {
            return (
                StatusCode::OK,
                Json(ToolListResponse {
                    object: "list".into(),
                    data: vec![],
                }),
            )
                .into_response();
        }
    };

    let registry = tools.read().await;
    let definitions = registry.tool_definitions();

    let tool_infos: Vec<ToolInfo> = definitions
        .into_iter()
        .filter(|td| state.api_tools_allowlist.contains(&td.name))
        .filter(|td| {
            if let Some(source_filter) = &params.source {
                tool_source_from_name(&td.name) == source_filter.as_str()
            } else {
                true
            }
        })
        .map(|td| {
            let source = tool_source_from_name(&td.name).to_string();
            ToolInfo {
                tool_type: "function".into(),
                function: ToolFunctionInfo {
                    name: td.name,
                    description: td.description,
                    parameters: td.input_schema,
                },
                source,
                version: None,
                required_permissions: None,
            }
        })
        .collect();

    (
        StatusCode::OK,
        Json(ToolListResponse {
            object: "list".into(),
            data: tool_infos,
        }),
    )
        .into_response()
}

/// POST /v1/tools/invoke
///
/// Executes a tool directly (bypassing the LLM) and returns the result.
/// Only tools in the config allowlist can be invoked.
pub async fn post_tool_invoke(
    State(state): State<GatewayState>,
    Json(body): Json<ToolInvokeRequest>,
) -> Response {
    let tools = match &state.tools {
        Some(t) => t,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(GatewayErrorResponse {
                    error: GatewayErrorDetail {
                        message: "Tools not configured".into(),
                        error_type: "server_error".into(),
                        param: None,
                        code: Some("tools_not_configured".into()),
                        provider: None,
                        retry_after: None,
                        category: None,
                        retryable: None,
                        failure_mode: None,
                    },
                }),
            )
                .into_response();
        }
    };

    // Check allowlist first (403 if not allowed).
    if !state.api_tools_allowlist.contains(&body.name) {
        return (
            StatusCode::FORBIDDEN,
            Json(GatewayErrorResponse {
                error: GatewayErrorDetail {
                    message: format!("Tool '{}' is not in the API tools allowlist", body.name),
                    error_type: "permission_denied".into(),
                    param: Some("name".into()),
                    code: Some("tool_not_allowed".into()),
                    provider: None,
                    retry_after: None,
                    category: None,
                    retryable: None,
                    failure_mode: None,
                },
            }),
        )
            .into_response();
    }

    // Look up tool in registry (404 if not found).
    let registry = tools.read().await;
    let tool = match registry.get(&body.name) {
        Some(t) => t,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(GatewayErrorResponse {
                    error: GatewayErrorDetail {
                        message: format!("Tool '{}' not found", body.name),
                        error_type: "not_found".into(),
                        param: Some("name".into()),
                        code: Some("tool_not_found".into()),
                        provider: None,
                        retry_after: None,
                        category: None,
                        retryable: None,
                        failure_mode: None,
                    },
                }),
            )
                .into_response();
        }
    };

    // Execute the tool.
    let start = Instant::now();
    let result = tool.invoke(body.input).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(output) => (
            StatusCode::OK,
            Json(ToolInvokeResponse {
                name: body.name,
                output: output.content,
                is_error: output.is_error,
                duration_ms: Some(duration_ms),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::OK,
            Json(ToolInvokeResponse {
                name: body.name,
                output: e.to_string(),
                is_error: true,
                duration_ms: Some(duration_ms),
            }),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_source_identifies_builtins() {
        assert_eq!(tool_source_from_name("bash"), "builtin");
        assert_eq!(tool_source_from_name("http"), "builtin");
    }

    #[test]
    fn tool_source_identifies_namespaced() {
        assert_eq!(tool_source_from_name("mcp_server__tool"), "mcp");
        assert_eq!(tool_source_from_name("github__issues"), "wasm");
    }
}
