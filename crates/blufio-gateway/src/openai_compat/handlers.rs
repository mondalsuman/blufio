// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTP request handlers for OpenAI-compatible gateway endpoints.
//!
//! Handles POST /v1/chat/completions and GET /v1/models.

use std::time::Instant;

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::server::GatewayState;

use super::stream::stream_completion;
use super::types::{
    GatewayChoice, GatewayCompletionRequest, GatewayCompletionResponse, GatewayErrorDetail,
    GatewayErrorResponse, GatewayResponseMessage, GatewayUsage, ModelsListResponse,
    ModelsQueryParams, gateway_request_to_provider_request, parse_model_string,
    stop_reason_to_finish_reason,
};

/// POST /v1/chat/completions
///
/// Accepts OpenAI-compatible chat completion requests and returns responses
/// in the same format. Supports both streaming (SSE) and non-streaming modes.
/// When `stream: true`, returns Server-Sent Events with `data: [JSON]` chunks.
#[utoipa::path(
    post,
    path = "/v1/chat/completions",
    tag = "OpenAI Compatible",
    request_body = GatewayCompletionRequest,
    responses(
        (status = 200, description = "Chat completion response", body = GatewayCompletionResponse),
        (status = 400, description = "Invalid request", body = GatewayErrorResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Provider not found", body = GatewayErrorResponse),
        (status = 502, description = "Provider error", body = GatewayErrorResponse),
        (status = 503, description = "Service unavailable", body = GatewayErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub async fn post_chat_completions(
    State(state): State<GatewayState>,
    Json(body): Json<GatewayCompletionRequest>,
) -> Response {
    // Check provider registry is configured.
    let providers = match &state.providers {
        Some(p) => p,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(GatewayErrorResponse {
                    error: GatewayErrorDetail {
                        message: "API not configured: no providers available".into(),
                        error_type: "server_error".into(),
                        param: None,
                        code: Some("api_not_configured".into()),
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

    // Parse model string.
    let (provider_name, model_name) = parse_model_string(&body.model, providers.default_provider());

    // Get provider adapter.
    let provider = match providers.get_provider(&provider_name) {
        Some(p) => p,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(GatewayErrorResponse {
                    error: GatewayErrorDetail {
                        message: format!("Provider '{}' not found", provider_name),
                        error_type: "not_found".into(),
                        param: Some("model".into()),
                        code: Some("provider_not_found".into()),
                        provider: Some(provider_name),
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

    // Convert gateway request to provider request.
    let mut provider_request = match gateway_request_to_provider_request(&body) {
        Ok(req) => req,
        Err(err_msg) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(GatewayErrorResponse {
                    error: GatewayErrorDetail {
                        message: err_msg,
                        error_type: "invalid_request_error".into(),
                        param: None,
                        code: None,
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

    // Override model to the resolved model name (without provider prefix).
    provider_request.model = model_name;

    let response_id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
    let include_usage = body
        .stream_options
        .as_ref()
        .map(|so| so.include_usage)
        .unwrap_or(false);

    // Streaming mode.
    if body.stream {
        return stream_completion(
            provider,
            provider_request,
            response_id,
            body.model.clone(),
            include_usage,
        )
        .await
        .into_response();
    }

    // Non-streaming mode.
    let start = Instant::now();
    match provider.complete(provider_request).await {
        Ok(response) => {
            let latency_ms = start.elapsed().as_millis() as u64;

            // Map stop_reason to finish_reason.
            let finish_reason = response
                .stop_reason
                .as_deref()
                .map(|sr| stop_reason_to_finish_reason(sr).to_string());

            let resp = GatewayCompletionResponse {
                id: response_id,
                object: "chat.completion".into(),
                created: chrono::Utc::now().timestamp(),
                model: response.model,
                choices: vec![GatewayChoice {
                    index: 0,
                    message: GatewayResponseMessage {
                        role: "assistant".into(),
                        content: if response.content.is_empty() {
                            None
                        } else {
                            Some(response.content)
                        },
                        tool_calls: None, // Tool calls are in content blocks for non-streaming
                    },
                    finish_reason,
                }],
                usage: GatewayUsage {
                    prompt_tokens: response.usage.input_tokens,
                    completion_tokens: response.usage.output_tokens,
                    total_tokens: response.usage.input_tokens + response.usage.output_tokens,
                },
                x_provider: Some(provider_name),
                x_latency_ms: Some(latency_ms),
            };

            (StatusCode::OK, Json(resp)).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, provider = %provider_name, "provider completion error");

            // Populate classification fields from BlufioError.
            let category = Some(e.category().to_string());
            let retryable = Some(e.is_retryable());
            let failure_mode = Some(e.failure_mode().to_string());
            let retry_after = e.suggested_backoff().map(|d| d.as_secs());

            (
                StatusCode::BAD_GATEWAY,
                Json(GatewayErrorResponse {
                    error: GatewayErrorDetail {
                        message: e.to_string(),
                        error_type: "server_error".into(),
                        param: None,
                        code: Some("provider_error".into()),
                        provider: Some(provider_name),
                        retry_after,
                        category,
                        retryable,
                        failure_mode,
                    },
                }),
            )
                .into_response()
        }
    }
}

/// GET /v1/models
///
/// Returns a list of available models across all configured providers.
#[utoipa::path(
    get,
    path = "/v1/models",
    tag = "OpenAI Compatible",
    params(ModelsQueryParams),
    responses(
        (status = 200, description = "Model list", body = ModelsListResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error", body = GatewayErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_models(
    State(state): State<GatewayState>,
    Query(params): Query<ModelsQueryParams>,
) -> Response {
    let providers = match &state.providers {
        Some(p) => p,
        None => {
            return (
                StatusCode::OK,
                Json(ModelsListResponse {
                    object: "list".into(),
                    data: vec![],
                }),
            )
                .into_response();
        }
    };

    match providers.list_models(params.provider.as_deref()).await {
        Ok(models) => (
            StatusCode::OK,
            Json(ModelsListResponse {
                object: "list".into(),
                data: models,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "failed to list models");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(GatewayErrorResponse {
                    error: GatewayErrorDetail {
                        message: "Failed to list models".into(),
                        error_type: "server_error".into(),
                        param: None,
                        code: None,
                        provider: None,
                        retry_after: None,
                        category: None,
                        retryable: None,
                        failure_mode: None,
                    },
                }),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_model_string_splits_provider() {
        let (prov, model) = parse_model_string("openai/gpt-4o", "anthropic");
        assert_eq!(prov, "openai");
        assert_eq!(model, "gpt-4o");
    }

    #[test]
    fn parse_model_string_uses_default() {
        let (prov, model) = parse_model_string("gpt-4o", "anthropic");
        assert_eq!(prov, "anthropic");
        assert_eq!(model, "gpt-4o");
    }
}
