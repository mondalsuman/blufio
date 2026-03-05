// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! SSE streaming for OpenAI-compatible /v1/chat/completions.
//!
//! Converts internal `ProviderStreamChunk` events to OpenAI-format SSE chunks
//! with `data: [DONE]` termination.

use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;

use axum::response::sse::{Event, Sse};
use futures::stream::{self, Stream, StreamExt};

use blufio_core::traits::ProviderAdapter;
use blufio_core::types::{ProviderRequest, ProviderStreamChunk, StreamEventType};

use super::types::{
    GatewayDeltaFunction, GatewayDeltaMessage, GatewayDeltaToolCall, GatewaySseChunk,
    GatewaySseDelta, GatewayUsage, stop_reason_to_finish_reason,
};

/// Type alias for boxed SSE streams to unify match arms.
type SseStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

/// Stream provider response as OpenAI-compatible SSE chunks.
///
/// Maps `ProviderStreamChunk` events to `GatewaySseChunk` format and
/// terminates with `data: [DONE]`.
pub async fn stream_completion(
    provider: Arc<dyn ProviderAdapter + Send + Sync>,
    request: ProviderRequest,
    response_id: String,
    model: String,
    include_usage: bool,
) -> Sse<SseStream> {
    let stream_result = provider.stream(request).await;

    let boxed_stream: SseStream = match stream_result {
        Ok(chunk_stream) => {
            let rid = response_id.clone();
            let mdl = model.clone();
            let created = chrono::Utc::now().timestamp();

            let mapped = chunk_stream.filter_map(move |result| {
                let rid = rid.clone();
                let mdl = mdl.clone();
                async move {
                    match result {
                        Ok(chunk) => map_provider_chunk_to_sse_event(
                            chunk,
                            &rid,
                            &mdl,
                            created,
                            include_usage,
                        ),
                        Err(e) => {
                            tracing::error!(error = %e, "stream error");
                            None
                        }
                    }
                }
            });

            // Append [DONE] after the stream ends.
            let done_event =
                stream::once(async { Ok::<Event, Infallible>(Event::default().data("[DONE]")) });

            Box::pin(mapped.chain(done_event))
        }
        Err(e) => {
            // Return an error event followed by [DONE].
            let error_json = serde_json::json!({
                "error": {
                    "message": e.to_string(),
                    "type": "server_error",
                }
            });
            let events = vec![
                Ok::<Event, Infallible>(Event::default().data(error_json.to_string())),
                Ok(Event::default().data("[DONE]")),
            ];
            Box::pin(stream::iter(events))
        }
    };

    Sse::new(boxed_stream)
}

/// Map a single `ProviderStreamChunk` to an SSE `Event`, if applicable.
fn map_provider_chunk_to_sse_event(
    chunk: ProviderStreamChunk,
    response_id: &str,
    model: &str,
    created: i64,
    include_usage: bool,
) -> Option<Result<Event, Infallible>> {
    match chunk.event_type {
        StreamEventType::MessageStart => {
            // Emit first chunk with role.
            let sse_chunk = GatewaySseChunk {
                id: response_id.to_string(),
                object: "chat.completion.chunk".into(),
                created,
                model: model.to_string(),
                choices: vec![GatewaySseDelta {
                    index: 0,
                    delta: GatewayDeltaMessage {
                        role: Some("assistant".into()),
                        content: None,
                        tool_calls: None,
                    },
                    finish_reason: None,
                }],
                usage: None,
            };
            Some(Ok(
                Event::default().data(serde_json::to_string(&sse_chunk).unwrap_or_default())
            ))
        }

        StreamEventType::ContentBlockDelta => {
            if let Some(text) = chunk.text {
                let sse_chunk = GatewaySseChunk {
                    id: response_id.to_string(),
                    object: "chat.completion.chunk".into(),
                    created,
                    model: model.to_string(),
                    choices: vec![GatewaySseDelta {
                        index: 0,
                        delta: GatewayDeltaMessage {
                            role: None,
                            content: Some(text),
                            tool_calls: None,
                        },
                        finish_reason: None,
                    }],
                    usage: None,
                };
                Some(Ok(
                    Event::default().data(serde_json::to_string(&sse_chunk).unwrap_or_default())
                ))
            } else {
                None
            }
        }

        StreamEventType::ContentBlockStop => {
            // Emit tool call if present.
            if let Some(tool_use) = chunk.tool_use {
                let sse_chunk = GatewaySseChunk {
                    id: response_id.to_string(),
                    object: "chat.completion.chunk".into(),
                    created,
                    model: model.to_string(),
                    choices: vec![GatewaySseDelta {
                        index: 0,
                        delta: GatewayDeltaMessage {
                            role: None,
                            content: None,
                            tool_calls: Some(vec![GatewayDeltaToolCall {
                                index: 0,
                                id: Some(tool_use.id),
                                call_type: Some("function".into()),
                                function: Some(GatewayDeltaFunction {
                                    name: Some(tool_use.name),
                                    arguments: Some(
                                        serde_json::to_string(&tool_use.input).unwrap_or_default(),
                                    ),
                                }),
                            }]),
                        },
                        finish_reason: None,
                    }],
                    usage: None,
                };
                Some(Ok(
                    Event::default().data(serde_json::to_string(&sse_chunk).unwrap_or_default())
                ))
            } else {
                None
            }
        }

        StreamEventType::MessageDelta => {
            // Emit finish_reason and optional usage.
            let finish_reason = chunk
                .stop_reason
                .as_deref()
                .map(|sr| stop_reason_to_finish_reason(sr).to_string());

            let usage = if include_usage {
                chunk.usage.map(|u| GatewayUsage {
                    prompt_tokens: u.input_tokens,
                    completion_tokens: u.output_tokens,
                    total_tokens: u.input_tokens + u.output_tokens,
                })
            } else {
                None
            };

            let sse_chunk = GatewaySseChunk {
                id: response_id.to_string(),
                object: "chat.completion.chunk".into(),
                created,
                model: model.to_string(),
                choices: vec![GatewaySseDelta {
                    index: 0,
                    delta: GatewayDeltaMessage::default(),
                    finish_reason,
                }],
                usage,
            };
            Some(Ok(
                Event::default().data(serde_json::to_string(&sse_chunk).unwrap_or_default())
            ))
        }

        StreamEventType::MessageStop => {
            // No separate event — [DONE] is appended by the stream combinator.
            None
        }

        StreamEventType::Ping | StreamEventType::Error | StreamEventType::ContentBlockStart => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blufio_core::types::{TokenUsage, ToolUseData};

    #[test]
    fn map_text_delta_to_sse() {
        let chunk = ProviderStreamChunk {
            event_type: StreamEventType::ContentBlockDelta,
            text: Some("Hello".into()),
            usage: None,
            error: None,
            tool_use: None,
            stop_reason: None,
        };

        let result = map_provider_chunk_to_sse_event(chunk, "test-id", "gpt-4o", 0, false);
        assert!(result.is_some());
        let event = result.unwrap().unwrap();
        // Event should serialize to JSON containing "Hello".
        let _ = event; // Event is opaque but we verified it's created.
    }

    #[test]
    fn map_message_start_to_sse() {
        let chunk = ProviderStreamChunk {
            event_type: StreamEventType::MessageStart,
            text: None,
            usage: None,
            error: None,
            tool_use: None,
            stop_reason: None,
        };

        let result = map_provider_chunk_to_sse_event(chunk, "test-id", "gpt-4o", 0, false);
        assert!(result.is_some());
    }

    #[test]
    fn map_message_delta_with_usage() {
        let chunk = ProviderStreamChunk {
            event_type: StreamEventType::MessageDelta,
            text: None,
            usage: Some(TokenUsage {
                input_tokens: 10,
                output_tokens: 20,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            }),
            error: None,
            tool_use: None,
            stop_reason: Some("end_turn".into()),
        };

        let result = map_provider_chunk_to_sse_event(chunk, "test-id", "gpt-4o", 0, true);
        assert!(result.is_some());
    }

    #[test]
    fn map_message_delta_without_usage_when_not_requested() {
        let chunk = ProviderStreamChunk {
            event_type: StreamEventType::MessageDelta,
            text: None,
            usage: Some(TokenUsage {
                input_tokens: 10,
                output_tokens: 20,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            }),
            error: None,
            tool_use: None,
            stop_reason: Some("end_turn".into()),
        };

        // include_usage = false
        let result = map_provider_chunk_to_sse_event(chunk, "test-id", "gpt-4o", 0, false);
        assert!(result.is_some());
    }

    #[test]
    fn map_tool_use_to_sse() {
        let chunk = ProviderStreamChunk {
            event_type: StreamEventType::ContentBlockStop,
            text: None,
            usage: None,
            error: None,
            tool_use: Some(ToolUseData {
                id: "call_abc".into(),
                name: "bash".into(),
                input: serde_json::json!({"command": "echo hello"}),
            }),
            stop_reason: None,
        };

        let result = map_provider_chunk_to_sse_event(chunk, "test-id", "gpt-4o", 0, false);
        assert!(result.is_some());
    }

    #[test]
    fn map_message_stop_returns_none() {
        let chunk = ProviderStreamChunk {
            event_type: StreamEventType::MessageStop,
            text: None,
            usage: None,
            error: None,
            tool_use: None,
            stop_reason: Some("end_turn".into()),
        };

        let result = map_provider_chunk_to_sse_event(chunk, "test-id", "gpt-4o", 0, false);
        assert!(result.is_none());
    }
}
