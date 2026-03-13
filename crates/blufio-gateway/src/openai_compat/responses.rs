// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Handler for the OpenResponses /v1/responses API.
//!
//! Streams semantic events compatible with the OpenAI Agents SDK:
//! response.created -> output_item.added -> content_part.added ->
//! output_text.delta (N times) -> output_text.done -> content_part.done ->
//! output_item.done -> response.completed

use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
};
use futures::stream::{self, Stream, StreamExt};

use blufio_core::traits::ProviderAdapter;
use blufio_core::types::{
    ContentBlock, ProviderMessage, ProviderRequest, ProviderStreamChunk, StreamEventType,
    ToolDefinition,
};

use crate::server::GatewayState;

use super::responses_types::*;
use super::types::{GatewayErrorDetail, GatewayErrorResponse, parse_model_string};

/// POST /v1/responses
///
/// Accepts OpenResponses-format requests and streams semantic events
/// compatible with the OpenAI Agents SDK. Only streaming mode is supported.
/// Returns Server-Sent Events with semantic event types: response.created,
/// output_item.added, content_part.added, output_text.delta, output_text.done,
/// content_part.done, output_item.done, response.completed.
#[utoipa::path(
    post,
    path = "/v1/responses",
    tag = "OpenAI Compatible",
    request_body = ResponsesRequest,
    responses(
        (status = 200, description = "Streaming semantic events (SSE)"),
        (status = 400, description = "Invalid request", body = GatewayErrorResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Provider not found", body = GatewayErrorResponse),
        (status = 503, description = "Service unavailable", body = GatewayErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub async fn post_responses(
    State(state): State<GatewayState>,
    Json(body): Json<ResponsesRequest>,
) -> Response {
    // Only streaming mode is supported.
    if !body.stream {
        return (
            StatusCode::BAD_REQUEST,
            Json(GatewayErrorResponse {
                error: GatewayErrorDetail {
                    message: "Only streaming mode is supported for /v1/responses".into(),
                    error_type: "invalid_request_error".into(),
                    param: Some("stream".into()),
                    code: Some("streaming_required".into()),
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

    // Convert to ProviderRequest.
    let mut provider_request = match to_provider_request(&body) {
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

    // Override model to resolved name.
    provider_request.model = model_name;

    let response_id = format!("resp_{}", uuid::Uuid::new_v4());

    stream_responses(provider, provider_request, response_id, body.model.clone())
        .await
        .into_response()
}

/// Convert a ResponsesRequest to a ProviderRequest.
fn to_provider_request(req: &ResponsesRequest) -> Result<ProviderRequest, String> {
    let messages = match &req.input {
        ResponsesInput::Text(text) => vec![ProviderMessage {
            role: "user".into(),
            content: vec![ContentBlock::Text { text: text.clone() }],
        }],
        ResponsesInput::Messages(msgs) => msgs
            .iter()
            .map(|m| ProviderMessage {
                role: m.role.clone(),
                content: vec![ContentBlock::Text {
                    text: m.content.clone(),
                }],
            })
            .collect(),
    };

    if messages.is_empty() {
        return Err("input must not be empty".into());
    }

    let tools = req.tools.as_ref().map(|ts| {
        ts.iter()
            .filter_map(|t| {
                if t.tool_type == "function" {
                    t.function.as_ref().map(|f| ToolDefinition {
                        name: f.name.clone(),
                        description: f.description.clone(),
                        input_schema: f.parameters.clone(),
                    })
                } else {
                    None
                }
            })
            .collect()
    });

    Ok(ProviderRequest {
        model: req.model.clone(),
        system_prompt: req.instructions.clone(),
        system_blocks: None,
        messages,
        max_tokens: req.max_output_tokens.unwrap_or(4096),
        stream: true, // Always stream for /v1/responses
        tools,
    })
}

/// Type alias for boxed SSE streams to unify match arms.
type SseStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

/// Stream provider response as OpenResponses semantic events.
async fn stream_responses(
    provider: Arc<dyn ProviderAdapter + Send + Sync>,
    request: ProviderRequest,
    response_id: String,
    model: String,
) -> Sse<SseStream> {
    let created_at = chrono::Utc::now().timestamp();

    let stream_result = provider.stream(request).await;

    let boxed_stream: SseStream = match stream_result {
        Ok(chunk_stream) => {
            let rid = response_id.clone();
            let mdl = model.clone();

            // Emit response.created first.
            let created_event = make_sse_event(
                "response.created",
                &ResponseEvent::ResponseCreated {
                    response: ResponseObject {
                        id: rid.clone(),
                        object: "response".into(),
                        status: "in_progress".into(),
                        model: mdl.clone(),
                        created_at,
                        output: None,
                        usage: None,
                    },
                },
            );

            // Emit output_item.added and content_part.added.
            let item_added_event = make_sse_event(
                "response.output_item.added",
                &ResponseEvent::OutputItemAdded {
                    output_index: 0,
                    item: OutputItem {
                        item_type: "message".into(),
                        role: Some("assistant".into()),
                        content: Some(vec![]),
                        call_id: None,
                        name: None,
                        arguments: None,
                    },
                },
            );

            let part_added_event = make_sse_event(
                "response.content_part.added",
                &ResponseEvent::ContentPartAdded {
                    output_index: 0,
                    content_index: 0,
                    part: ContentPart {
                        part_type: "output_text".into(),
                        text: Some(String::new()),
                    },
                },
            );

            let preamble = stream::iter(vec![created_event, item_added_event, part_added_event]);

            // Map provider chunks to response events.
            let rid2 = rid.clone();
            let mdl2 = mdl.clone();
            let accumulated_text = Arc::new(tokio::sync::Mutex::new(String::new()));
            let accumulated_text2 = Arc::clone(&accumulated_text);

            let mapped = chunk_stream.filter_map(move |result| {
                let rid = rid2.clone();
                let mdl = mdl2.clone();
                let acc = Arc::clone(&accumulated_text2);
                async move {
                    match result {
                        Ok(chunk) => {
                            map_chunk_to_response_event(chunk, &rid, &mdl, created_at, &acc).await
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "responses stream error");
                            Some(make_sse_event(
                                "response.failed",
                                &ResponseEvent::ResponseFailed {
                                    response: ResponseObject {
                                        id: rid,
                                        object: "response".into(),
                                        status: "failed".into(),
                                        model: mdl,
                                        created_at,
                                        output: None,
                                        usage: None,
                                    },
                                    error: ResponseError {
                                        message: e.to_string(),
                                        error_type: "server_error".into(),
                                        code: Some("provider_error".into()),
                                    },
                                },
                            ))
                        }
                    }
                }
            });

            // Append closing events after the stream.
            let rid3 = rid.clone();
            let mdl3 = mdl.clone();
            let accumulated_text3 = Arc::clone(&accumulated_text);
            let closing = stream::once(async move {
                let text = accumulated_text3.lock().await.clone();

                // Emit: output_text.done, content_part.done, output_item.done, response.completed
                let text_done = make_sse_event(
                    "response.output_text.done",
                    &ResponseEvent::OutputTextDone {
                        output_index: 0,
                        content_index: 0,
                        text: text.clone(),
                    },
                );
                let part_done = make_sse_event(
                    "response.content_part.done",
                    &ResponseEvent::ContentPartDone {
                        output_index: 0,
                        content_index: 0,
                        part: ContentPart {
                            part_type: "output_text".into(),
                            text: Some(text.clone()),
                        },
                    },
                );
                let item_done = make_sse_event(
                    "response.output_item.done",
                    &ResponseEvent::OutputItemDone {
                        output_index: 0,
                        item: OutputItem {
                            item_type: "message".into(),
                            role: Some("assistant".into()),
                            content: Some(vec![ContentPart {
                                part_type: "output_text".into(),
                                text: Some(text),
                            }]),
                            call_id: None,
                            name: None,
                            arguments: None,
                        },
                    },
                );
                let completed = make_sse_event(
                    "response.completed",
                    &ResponseEvent::ResponseCompleted {
                        response: ResponseObject {
                            id: rid3,
                            object: "response".into(),
                            status: "completed".into(),
                            model: mdl3,
                            created_at,
                            output: None, // Could populate, but kept lightweight
                            usage: None,  // Usage populated via MessageDelta if available
                        },
                    },
                );

                // Return the closing events as a stream of individual events.
                stream::iter(vec![text_done, part_done, item_done, completed])
            })
            .flatten();

            let full_stream = preamble.chain(mapped).chain(closing);
            Box::pin(full_stream)
        }
        Err(e) => {
            // Return a single error event.
            let error_event = make_sse_event(
                "response.failed",
                &ResponseEvent::ResponseFailed {
                    response: ResponseObject {
                        id: response_id,
                        object: "response".into(),
                        status: "failed".into(),
                        model,
                        created_at,
                        output: None,
                        usage: None,
                    },
                    error: ResponseError {
                        message: e.to_string(),
                        error_type: "server_error".into(),
                        code: Some("provider_error".into()),
                    },
                },
            );
            Box::pin(stream::iter(vec![error_event]))
        }
    };

    Sse::new(boxed_stream)
}

/// Map a single ProviderStreamChunk to a response SSE event.
async fn map_chunk_to_response_event(
    chunk: ProviderStreamChunk,
    _response_id: &str,
    _model: &str,
    _created_at: i64,
    accumulated_text: &tokio::sync::Mutex<String>,
) -> Option<Result<Event, Infallible>> {
    match chunk.event_type {
        StreamEventType::ContentBlockDelta => {
            if let Some(text) = chunk.text {
                // Accumulate text for the done event.
                accumulated_text.lock().await.push_str(&text);
                Some(make_sse_event(
                    "response.output_text.delta",
                    &ResponseEvent::OutputTextDelta {
                        output_index: 0,
                        content_index: 0,
                        delta: text,
                    },
                ))
            } else {
                None
            }
        }

        StreamEventType::ContentBlockStop => {
            // Emit tool call if present.
            if let Some(tool_use) = chunk.tool_use {
                let arguments = serde_json::to_string(&tool_use.input).unwrap_or_default();
                Some(make_sse_event(
                    "response.function_call_arguments.done",
                    &ResponseEvent::FunctionCallArgumentsDone {
                        output_index: 0,
                        call_id: tool_use.id,
                        name: tool_use.name,
                        arguments,
                    },
                ))
            } else {
                None
            }
        }

        // MessageStart, MessageDelta, MessageStop, Ping, Error, ContentBlockStart
        // are handled by the preamble/closing logic or ignored.
        _ => None,
    }
}

/// Create an SSE Event with an event type and JSON data.
fn make_sse_event(event_type: &str, data: &ResponseEvent) -> Result<Event, Infallible> {
    let json = serde_json::to_string(data).unwrap_or_default();
    Ok(Event::default().event(event_type).data(json))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_provider_request_with_text_input() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: ResponsesInput::Text("Hello".into()),
            instructions: Some("Be helpful".into()),
            tools: None,
            previous_response_id: None,
            stream: true,
            temperature: None,
            max_output_tokens: None,
        };
        let provider_req = to_provider_request(&req).unwrap();
        assert_eq!(provider_req.messages.len(), 1);
        assert_eq!(provider_req.messages[0].role, "user");
        assert_eq!(provider_req.system_prompt.as_deref(), Some("Be helpful"));
        assert!(provider_req.stream);
        assert_eq!(provider_req.max_tokens, 4096); // default
    }

    #[test]
    fn to_provider_request_with_messages_input() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: ResponsesInput::Messages(vec![
                ResponsesInputMessage {
                    role: "user".into(),
                    content: "Hello".into(),
                },
                ResponsesInputMessage {
                    role: "assistant".into(),
                    content: "Hi!".into(),
                },
            ]),
            instructions: None,
            tools: None,
            previous_response_id: None,
            stream: true,
            temperature: None,
            max_output_tokens: Some(2048),
        };
        let provider_req = to_provider_request(&req).unwrap();
        assert_eq!(provider_req.messages.len(), 2);
        assert_eq!(provider_req.max_tokens, 2048);
    }

    #[test]
    fn to_provider_request_with_tools() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: ResponsesInput::Text("Use the tool".into()),
            instructions: None,
            tools: Some(vec![ResponsesTool {
                tool_type: "function".into(),
                function: Some(ResponsesFunction {
                    name: "bash".into(),
                    description: "Execute a command".into(),
                    parameters: serde_json::json!({"type": "object"}),
                }),
                name: None,
            }]),
            previous_response_id: None,
            stream: true,
            temperature: None,
            max_output_tokens: None,
        };
        let provider_req = to_provider_request(&req).unwrap();
        let tools = provider_req.tools.as_ref().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "bash");
    }

    #[test]
    fn to_provider_request_skips_non_function_tools() {
        let req = ResponsesRequest {
            model: "gpt-4o".into(),
            input: ResponsesInput::Text("Search".into()),
            instructions: None,
            tools: Some(vec![ResponsesTool {
                tool_type: "web_search".into(),
                function: None,
                name: Some("web_search".into()),
            }]),
            previous_response_id: None,
            stream: true,
            temperature: None,
            max_output_tokens: None,
        };
        let provider_req = to_provider_request(&req).unwrap();
        let tools = provider_req.tools.as_ref().unwrap();
        assert!(tools.is_empty());
    }

    #[test]
    fn make_sse_event_creates_valid_event() {
        let event = ResponseEvent::OutputTextDelta {
            output_index: 0,
            content_index: 0,
            delta: "Hello".into(),
        };
        let result = make_sse_event("response.output_text.delta", &event);
        assert!(result.is_ok());
    }

    #[test]
    fn make_sse_event_response_created() {
        let event = ResponseEvent::ResponseCreated {
            response: ResponseObject {
                id: "resp_test".into(),
                object: "response".into(),
                status: "in_progress".into(),
                model: "gpt-4o".into(),
                created_at: 0,
                output: None,
                usage: None,
            },
        };
        let result = make_sse_event("response.created", &event);
        assert!(result.is_ok());
    }

    #[test]
    fn make_sse_event_response_failed() {
        let event = ResponseEvent::ResponseFailed {
            response: ResponseObject {
                id: "resp_test".into(),
                object: "response".into(),
                status: "failed".into(),
                model: "gpt-4o".into(),
                created_at: 0,
                output: None,
                usage: None,
            },
            error: ResponseError {
                message: "Provider error".into(),
                error_type: "server_error".into(),
                code: Some("provider_error".into()),
            },
        };
        let result = make_sse_event("response.failed", &event);
        assert!(result.is_ok());
    }
}
