// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Streaming response parser for Gemini's streamGenerateContent endpoint.
//!
//! Gemini streams chunked JSON (not SSE). The response is a stream of
//! `GenerateContentResponse` objects. We use a JSON depth counter to
//! detect complete objects from the byte stream.

use std::pin::Pin;
use std::task::{Context, Poll};

use blufio_core::{BlufioError, ErrorContext, ProviderErrorKind};
use bytes::BytesMut;
use futures::stream::Stream;

use crate::types::GenerateContentResponse;

/// Parses a Gemini streaming response into individual `GenerateContentResponse` objects.
///
/// Gemini's streamGenerateContent returns a chunked JSON response that may be:
/// - A JSON array of objects `[{...},{...}]`
/// - Newline-delimited JSON objects
/// - Partial objects split across HTTP chunks
///
/// This parser tracks `{` and `}` depth to detect complete JSON objects,
/// properly handling strings (with escaped characters).
pub fn parse_gemini_stream(
    response: reqwest::Response,
) -> Pin<Box<dyn Stream<Item = Result<GenerateContentResponse, BlufioError>> + Send>> {
    let byte_stream = response.bytes_stream();
    Box::pin(GeminiStreamParser {
        inner: Box::pin(byte_stream),
        buffer: BytesMut::new(),
        depth: 0,
        in_string: false,
        escape_next: false,
        obj_start: None,
    })
}

/// Stateful parser that accumulates bytes and emits complete JSON objects.
struct GeminiStreamParser {
    inner: Pin<Box<dyn Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send>>,
    buffer: BytesMut,
    depth: i32,
    in_string: bool,
    escape_next: bool,
    /// Byte offset within `buffer` where the current `{` object started.
    obj_start: Option<usize>,
}

impl Stream for GeminiStreamParser {
    type Item = Result<GenerateContentResponse, BlufioError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        loop {
            // Try to extract a complete JSON object from the buffer first.
            if let Some(result) = this.try_extract_object() {
                return Poll::Ready(Some(result));
            }

            // Need more data -- poll the inner stream.
            match this.inner.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => {
                    this.buffer.extend_from_slice(&bytes);
                    // Loop back to try extracting again.
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(BlufioError::Provider {
                        kind: ProviderErrorKind::ServerError,
                        context: ErrorContext {
                            provider_name: Some("gemini".into()),
                            ..Default::default()
                        },
                        source: Some(Box::new(e)),
                    })));
                }
                Poll::Ready(None) => {
                    // Stream ended. Try one more extraction then done.
                    if let Some(result) = this.try_extract_object() {
                        return Poll::Ready(Some(result));
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }
    }
}

impl GeminiStreamParser {
    /// Scans the buffer for a complete JSON object using brace depth tracking.
    ///
    /// Returns `Some(result)` if a complete object was found and parsed,
    /// `None` if more data is needed.
    fn try_extract_object(&mut self) -> Option<Result<GenerateContentResponse, BlufioError>> {
        let buf = &self.buffer[..];
        let len = buf.len();
        let mut pos = self.obj_start.map_or(0, |s| {
            // If we already started tracking an object, resume from where we left off.
            // We need to find where we were. We'll re-scan from obj_start for simplicity
            // after new data arrives. Actually, we track position differently.
            s
        });

        // If we haven't found the start of an object yet, skip to the first `{`.
        if self.obj_start.is_none() {
            while pos < len {
                let ch = buf[pos] as char;
                if ch == '{' {
                    self.obj_start = Some(pos);
                    self.depth = 0;
                    self.in_string = false;
                    self.escape_next = false;
                    break;
                }
                pos += 1;
            }
            if self.obj_start.is_none() {
                // No `{` found -- discard scanned non-object bytes.
                if pos > 0 {
                    let _ = self.buffer.split_to(pos);
                }
                return None;
            }
        }

        // Scan from the current position to find a complete object.
        let start = self.obj_start.expect("obj_start checked above");
        let mut scan_pos = if pos > start { pos } else { start };

        while scan_pos < len {
            let ch = buf[scan_pos] as char;

            if self.escape_next {
                self.escape_next = false;
                scan_pos += 1;
                continue;
            }

            if self.in_string {
                match ch {
                    '\\' => self.escape_next = true,
                    '"' => self.in_string = false,
                    _ => {}
                }
                scan_pos += 1;
                continue;
            }

            match ch {
                '"' => self.in_string = true,
                '{' => self.depth += 1,
                '}' => {
                    self.depth -= 1;
                    if self.depth == 0 {
                        // Found complete object from `start` to `scan_pos` inclusive.
                        let obj_bytes = &buf[start..=scan_pos];
                        let result = serde_json::from_slice::<GenerateContentResponse>(obj_bytes)
                            .map_err(|e| BlufioError::Provider {
                                kind: ProviderErrorKind::ServerError,
                                context: ErrorContext {
                                    provider_name: Some("gemini".into()),
                                    ..Default::default()
                                },
                                source: Some(Box::new(e)),
                            });

                        // Consume processed bytes from buffer.
                        let consumed = scan_pos + 1;
                        let _ = self.buffer.split_to(consumed);
                        self.obj_start = None;
                        self.depth = 0;
                        self.in_string = false;
                        self.escape_next = false;

                        return Some(result);
                    }
                }
                _ => {}
            }

            scan_pos += 1;
        }

        // Incomplete object -- need more data.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GeminiPart;
    use futures::StreamExt;

    /// Helper: create a mock streaming response from raw body text.
    async fn mock_stream_response(body: &str) -> reqwest::Response {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_string(body.to_string()),
            )
            .mount(&server)
            .await;

        reqwest::get(&server.uri()).await.unwrap()
    }

    #[tokio::test]
    async fn parse_single_response_object() {
        let body = r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"Hello"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":3,"totalTokenCount":8}}"#;
        let response = mock_stream_response(body).await;
        let mut stream = parse_gemini_stream(response);

        let chunk = stream.next().await.unwrap().unwrap();
        assert_eq!(chunk.candidates.len(), 1);
        match &chunk.candidates[0].content.parts[0] {
            GeminiPart::Text(tp) => assert_eq!(tp.text, "Hello"),
            other => panic!("expected Text, got {other:?}"),
        }
        assert_eq!(chunk.candidates[0].finish_reason.as_deref(), Some("STOP"));

        // Stream should end.
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn parse_json_array_of_responses() {
        let body = r#"[{"candidates":[{"content":{"role":"model","parts":[{"text":"Hello"}]}}]},{"candidates":[{"content":{"role":"model","parts":[{"text":" world"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":3,"totalTokenCount":8}}]"#;
        let response = mock_stream_response(body).await;
        let mut stream = parse_gemini_stream(response);

        let chunk1 = stream.next().await.unwrap().unwrap();
        match &chunk1.candidates[0].content.parts[0] {
            GeminiPart::Text(tp) => assert_eq!(tp.text, "Hello"),
            other => panic!("expected Text, got {other:?}"),
        }

        let chunk2 = stream.next().await.unwrap().unwrap();
        match &chunk2.candidates[0].content.parts[0] {
            GeminiPart::Text(tp) => assert_eq!(tp.text, " world"),
            other => panic!("expected Text, got {other:?}"),
        }
        assert_eq!(chunk2.candidates[0].finish_reason.as_deref(), Some("STOP"));

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn parse_response_with_function_call() {
        let body = r#"{"candidates":[{"content":{"role":"model","parts":[{"functionCall":{"name":"bash","args":{"command":"echo hello"}}}]},"finishReason":"STOP"}]}"#;
        let response = mock_stream_response(body).await;
        let mut stream = parse_gemini_stream(response);

        let chunk = stream.next().await.unwrap().unwrap();
        match &chunk.candidates[0].content.parts[0] {
            GeminiPart::FunctionCall(fc) => {
                assert_eq!(fc.function_call.name, "bash");
                assert_eq!(fc.function_call.args["command"], "echo hello");
            }
            other => panic!("expected FunctionCall, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn handles_strings_with_braces() {
        // Ensure the parser doesn't get confused by braces inside strings.
        let body = r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"function() { return {}; }"}]}}]}"#;
        let response = mock_stream_response(body).await;
        let mut stream = parse_gemini_stream(response);

        let chunk = stream.next().await.unwrap().unwrap();
        match &chunk.candidates[0].content.parts[0] {
            GeminiPart::Text(tp) => {
                assert_eq!(tp.text, "function() { return {}; }");
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn handles_escaped_quotes_in_strings() {
        let body = r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"He said \"hello\""}]}}]}"#;
        let response = mock_stream_response(body).await;
        let mut stream = parse_gemini_stream(response);

        let chunk = stream.next().await.unwrap().unwrap();
        match &chunk.candidates[0].content.parts[0] {
            GeminiPart::Text(tp) => {
                assert_eq!(tp.text, "He said \"hello\"");
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }
}
