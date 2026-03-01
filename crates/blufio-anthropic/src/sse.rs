// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! SSE stream parser for Anthropic Messages API streaming responses.
//!
//! Converts a reqwest response byte stream into typed [`StreamEvent`] variants
//! using the `eventsource-stream` crate for SSE protocol compliance.

use std::pin::Pin;

use blufio_core::BlufioError;
use eventsource_stream::Eventsource;
use futures::stream::{Stream, StreamExt};

use crate::types::{
    SseContentBlockDelta, SseContentBlockStart, SseContentBlockStop, SseError, SseMessageDelta,
    SseMessageStart,
};

/// Typed SSE events from the Anthropic streaming protocol.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Initial message metadata (id, model, usage).
    MessageStart(SseMessageStart),
    /// A new content block begins.
    ContentBlockStart(SseContentBlockStart),
    /// Incremental update to a content block (text delta, JSON delta).
    ContentBlockDelta(SseContentBlockDelta),
    /// A content block has finished.
    ContentBlockStop(SseContentBlockStop),
    /// Message-level delta (stop_reason, usage update).
    MessageDelta(SseMessageDelta),
    /// The message is complete.
    MessageStop,
    /// Keep-alive ping.
    Ping,
    /// API error during streaming.
    Error(SseError),
}

/// Parses a reqwest streaming response into a stream of typed [`StreamEvent`]s.
///
/// The response body is parsed as Server-Sent Events. Each SSE event is
/// deserialized into the appropriate [`StreamEvent`] variant based on the
/// event name. Unknown event types are silently skipped per Anthropic's
/// API versioning policy.
pub fn parse_sse_stream(
    response: reqwest::Response,
) -> Pin<Box<dyn Stream<Item = Result<StreamEvent, BlufioError>> + Send>> {
    let byte_stream = response.bytes_stream();
    let event_stream = byte_stream.eventsource();

    let mapped = event_stream.filter_map(|result| async move {
        match result {
            Ok(event) => {
                let parsed = match event.event.as_str() {
                    "message_start" => {
                        serde_json::from_str::<SseMessageStart>(&event.data)
                            .map(StreamEvent::MessageStart)
                            .map_err(|e| BlufioError::Provider {
                                message: format!("failed to parse message_start: {e}"),
                                source: Some(Box::new(e)),
                            })
                    }
                    "content_block_start" => {
                        serde_json::from_str::<SseContentBlockStart>(&event.data)
                            .map(StreamEvent::ContentBlockStart)
                            .map_err(|e| BlufioError::Provider {
                                message: format!("failed to parse content_block_start: {e}"),
                                source: Some(Box::new(e)),
                            })
                    }
                    "content_block_delta" => {
                        serde_json::from_str::<SseContentBlockDelta>(&event.data)
                            .map(StreamEvent::ContentBlockDelta)
                            .map_err(|e| BlufioError::Provider {
                                message: format!("failed to parse content_block_delta: {e}"),
                                source: Some(Box::new(e)),
                            })
                    }
                    "content_block_stop" => {
                        serde_json::from_str::<SseContentBlockStop>(&event.data)
                            .map(StreamEvent::ContentBlockStop)
                            .map_err(|e| BlufioError::Provider {
                                message: format!("failed to parse content_block_stop: {e}"),
                                source: Some(Box::new(e)),
                            })
                    }
                    "message_delta" => {
                        serde_json::from_str::<SseMessageDelta>(&event.data)
                            .map(StreamEvent::MessageDelta)
                            .map_err(|e| BlufioError::Provider {
                                message: format!("failed to parse message_delta: {e}"),
                                source: Some(Box::new(e)),
                            })
                    }
                    "message_stop" => Ok(StreamEvent::MessageStop),
                    "ping" => Ok(StreamEvent::Ping),
                    "error" => {
                        serde_json::from_str::<SseError>(&event.data)
                            .map(StreamEvent::Error)
                            .map_err(|e| BlufioError::Provider {
                                message: format!("failed to parse error event: {e}"),
                                source: Some(Box::new(e)),
                            })
                    }
                    // Unknown event types are silently ignored per Anthropic versioning policy.
                    _ => return None,
                };
                Some(parsed)
            }
            Err(e) => Some(Err(BlufioError::Provider {
                message: format!("SSE stream error: {e}"),
                source: None,
            })),
        }
    });

    Box::pin(mapped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    /// Helper: create a mock SSE byte stream from raw SSE text.
    ///
    /// Uses wiremock to serve the SSE response to get a real reqwest::Response.
    async fn mock_sse_response(sse_text: &str) -> reqwest::Response {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_text.to_string()),
            )
            .mount(&server)
            .await;

        reqwest::get(&server.uri()).await.unwrap()
    }

    #[tokio::test]
    async fn parse_content_block_delta() {
        let sse = "event: content_block_delta\ndata: {\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n";
        let response = mock_sse_response(sse).await;
        let mut stream = parse_sse_stream(response);

        let event = stream.next().await.unwrap().unwrap();
        match event {
            StreamEvent::ContentBlockDelta(delta) => {
                assert_eq!(delta.index, 0);
                match delta.delta {
                    crate::types::SseDelta::TextDelta { ref text } => {
                        assert_eq!(text, "Hello");
                    }
                    _ => panic!("expected TextDelta"),
                }
            }
            other => panic!("expected ContentBlockDelta, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn parse_message_stop() {
        let sse = "event: message_stop\ndata: {}\n\n";
        let response = mock_sse_response(sse).await;
        let mut stream = parse_sse_stream(response);

        let event = stream.next().await.unwrap().unwrap();
        assert!(matches!(event, StreamEvent::MessageStop));
    }

    #[tokio::test]
    async fn parse_ping() {
        let sse = "event: ping\ndata: {}\n\n";
        let response = mock_sse_response(sse).await;
        let mut stream = parse_sse_stream(response);

        let event = stream.next().await.unwrap().unwrap();
        assert!(matches!(event, StreamEvent::Ping));
    }

    #[tokio::test]
    async fn unknown_events_are_skipped() {
        let sse = "event: unknown_future_event\ndata: {\"foo\":\"bar\"}\n\nevent: message_stop\ndata: {}\n\n";
        let response = mock_sse_response(sse).await;
        let mut stream = parse_sse_stream(response);

        // The unknown event should be skipped; first item should be message_stop.
        let event = stream.next().await.unwrap().unwrap();
        assert!(matches!(event, StreamEvent::MessageStop));
    }

    #[tokio::test]
    async fn parse_message_delta_with_usage() {
        let sse = "event: message_delta\ndata: {\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"input_tokens\":10,\"output_tokens\":25}}\n\n";
        let response = mock_sse_response(sse).await;
        let mut stream = parse_sse_stream(response);

        let event = stream.next().await.unwrap().unwrap();
        match event {
            StreamEvent::MessageDelta(md) => {
                assert_eq!(md.delta.stop_reason, Some("end_turn".into()));
                assert_eq!(md.usage.as_ref().unwrap().output_tokens, 25);
            }
            other => panic!("expected MessageDelta, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn parse_error_event() {
        let sse = "event: error\ndata: {\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n";
        let response = mock_sse_response(sse).await;
        let mut stream = parse_sse_stream(response);

        let event = stream.next().await.unwrap().unwrap();
        match event {
            StreamEvent::Error(err) => {
                assert_eq!(err.error.type_, "overloaded_error");
                assert_eq!(err.error.message, "Overloaded");
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }
}
