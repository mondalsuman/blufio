// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! SSE stream parser for OpenRouter streaming responses.
//!
//! Converts a reqwest response byte stream into typed [`SseChunk`] variants
//! using the `eventsource-stream` crate for SSE protocol compliance.
//! Handles the `data: [DONE]` terminator that signals end of stream.
//!
//! OpenRouter uses the same SSE format as OpenAI:
//! - No named event types (all events are unnamed `data:` lines)
//! - `data: [DONE]` signals end of stream
//! - Each `data:` line is a JSON `SseChunk`

use std::pin::Pin;

use blufio_core::BlufioError;
use eventsource_stream::Eventsource;
use futures::stream::{Stream, StreamExt};

use crate::types::SseChunk;

/// Parses a reqwest streaming response into a stream of typed [`SseChunk`]s.
///
/// The response body is parsed as Server-Sent Events. Each SSE data line is
/// parsed as a JSON [`SseChunk`], except for `[DONE]` which terminates the stream.
pub fn parse_openrouter_sse_stream(
    response: reqwest::Response,
) -> Pin<Box<dyn Stream<Item = Result<SseChunk, BlufioError>> + Send>> {
    let byte_stream = response.bytes_stream();
    let event_stream = byte_stream.eventsource();

    let mapped = event_stream.filter_map(|result| async move {
        match result {
            Ok(event) => {
                let data = event.data.trim().to_string();

                // [DONE] signals stream end -- stop producing items.
                if data == "[DONE]" {
                    return None;
                }

                // Empty data lines are ignored.
                if data.is_empty() {
                    return None;
                }

                // Parse as SseChunk.
                match serde_json::from_str::<SseChunk>(&data) {
                    Ok(chunk) => Some(Ok(chunk)),
                    Err(e) => Some(Err(BlufioError::Provider {
                        message: format!("failed to parse OpenRouter SSE chunk: {e}"),
                        source: Some(Box::new(e)),
                    })),
                }
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

    /// Helper: create a mock SSE response from raw SSE text.
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
    async fn parse_text_delta() {
        let sse = "data: {\"id\":\"chatcmpl-test\",\"choices\":[{\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null,\"index\":0}],\"model\":\"anthropic/claude-sonnet-4\"}\n\n";
        let response = mock_sse_response(sse).await;
        let mut stream = parse_openrouter_sse_stream(response);

        let chunk = stream.next().await.unwrap().unwrap();
        assert_eq!(chunk.choices.len(), 1);
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("Hello"));
    }

    #[tokio::test]
    async fn parse_done_terminates_stream() {
        let sse = "data: {\"id\":\"chatcmpl-test\",\"choices\":[{\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null,\"index\":0}],\"model\":\"anthropic/claude-sonnet-4\"}\n\ndata: [DONE]\n\n";
        let response = mock_sse_response(sse).await;
        let mut stream = parse_openrouter_sse_stream(response);

        // First item: text delta
        let chunk = stream.next().await.unwrap().unwrap();
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("Hi"));

        // Second item: [DONE] causes stream to end
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn parse_tool_call_delta() {
        let sse = "data: {\"id\":\"chatcmpl-test\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_abc\",\"type\":\"function\",\"function\":{\"name\":\"bash\",\"arguments\":\"{\\\"cmd\\\":\"}}]},\"finish_reason\":null,\"index\":0}],\"model\":\"anthropic/claude-sonnet-4\"}\n\n";
        let response = mock_sse_response(sse).await;
        let mut stream = parse_openrouter_sse_stream(response);

        let chunk = stream.next().await.unwrap().unwrap();
        let tool_calls = chunk.choices[0].delta.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls[0].id.as_deref(), Some("call_abc"));
        let func = tool_calls[0].function.as_ref().unwrap();
        assert_eq!(func.name.as_deref(), Some("bash"));
    }

    #[tokio::test]
    async fn parse_finish_reason_with_usage() {
        let sse = "data: {\"id\":\"chatcmpl-test\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\",\"index\":0}],\"model\":\"anthropic/claude-sonnet-4\",\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":20,\"total_tokens\":30}}\n\n";
        let response = mock_sse_response(sse).await;
        let mut stream = parse_openrouter_sse_stream(response);

        let chunk = stream.next().await.unwrap().unwrap();
        assert_eq!(chunk.choices[0].finish_reason.as_deref(), Some("stop"));
        let usage = chunk.usage.as_ref().unwrap();
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 20);
    }
}
