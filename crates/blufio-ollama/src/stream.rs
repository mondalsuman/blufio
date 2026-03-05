// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! NDJSON (newline-delimited JSON) stream parser for Ollama responses.
//!
//! Ollama streams responses as NDJSON: one JSON object per line, separated
//! by newlines. This differs from SSE (Server-Sent Events) used by OpenAI
//! and Anthropic.

use std::pin::Pin;

use blufio_core::BlufioError;
use bytes::BytesMut;
use futures::StreamExt;
use futures::stream::Stream;

use crate::types::OllamaResponse;

/// Parses a streaming HTTP response body as NDJSON, yielding one
/// `OllamaResponse` per complete JSON line.
///
/// Handles:
/// - Partial lines across byte chunks (buffered until newline)
/// - Empty lines (skipped)
/// - Invalid JSON (emitted as errors)
pub fn parse_ndjson_stream(
    response: reqwest::Response,
) -> Pin<Box<dyn Stream<Item = Result<OllamaResponse, BlufioError>> + Send>> {
    let byte_stream = response.bytes_stream();
    let mut buffer = BytesMut::new();

    let stream = byte_stream.flat_map(move |chunk_result| {
        let items = match chunk_result {
            Ok(chunk) => {
                buffer.extend_from_slice(&chunk);
                parse_complete_lines(&mut buffer)
            }
            Err(e) => vec![Err(BlufioError::Provider {
                message: format!("NDJSON stream read error: {e}"),
                source: Some(Box::new(e)),
            })],
        };
        futures::stream::iter(items)
    });

    Box::pin(stream)
}

/// Extracts and parses all complete lines from the buffer.
///
/// A complete line ends with `\n`. Partial lines remain in the buffer
/// for the next chunk. Empty lines are skipped.
fn parse_complete_lines(buffer: &mut BytesMut) -> Vec<Result<OllamaResponse, BlufioError>> {
    let mut results = Vec::new();

    loop {
        // Find the next newline in the buffer.
        let newline_pos = buffer.iter().position(|&b| b == b'\n');
        match newline_pos {
            Some(pos) => {
                // Extract the line (excluding the newline).
                let line_bytes = buffer.split_to(pos + 1);
                let line = String::from_utf8_lossy(&line_bytes[..pos]);
                let trimmed = line.trim();

                // Skip empty lines.
                if trimmed.is_empty() {
                    continue;
                }

                // Parse the JSON line.
                match serde_json::from_str::<OllamaResponse>(trimmed) {
                    Ok(resp) => results.push(Ok(resp)),
                    Err(e) => results.push(Err(BlufioError::Provider {
                        message: format!("NDJSON parse error: {e} (line: {trimmed})"),
                        source: Some(Box::new(e)),
                    })),
                }
            }
            None => break,
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_complete_line() {
        let mut buffer = BytesMut::from(
            r#"{"model":"llama3.2","message":{"role":"assistant","content":"Hi"},"done":false}
"#,
        );
        let results = parse_complete_lines(&mut buffer);
        assert_eq!(results.len(), 1);
        let resp = results[0].as_ref().unwrap();
        assert_eq!(resp.model, "llama3.2");
        assert_eq!(resp.message.content, "Hi");
        assert!(!resp.done);
        assert!(buffer.is_empty());
    }

    #[test]
    fn parse_multiple_lines() {
        let mut buffer = BytesMut::from(
            r#"{"model":"llama3.2","message":{"role":"assistant","content":"Hi"},"done":false}
{"model":"llama3.2","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop"}
"#,
        );
        let results = parse_complete_lines(&mut buffer);
        assert_eq!(results.len(), 2);
        assert!(!results[0].as_ref().unwrap().done);
        assert!(results[1].as_ref().unwrap().done);
        assert!(buffer.is_empty());
    }

    #[test]
    fn partial_line_stays_in_buffer() {
        let mut buffer = BytesMut::from(r#"{"model":"llama3.2","message":{"role":"#);
        let results = parse_complete_lines(&mut buffer);
        assert!(results.is_empty());
        assert!(!buffer.is_empty()); // Partial line preserved.
    }

    #[test]
    fn partial_lines_across_chunks() {
        // First chunk: partial line (split mid-JSON, before "content" key)
        let mut buffer = BytesMut::from(r#"{"model":"llama3.2","message":{"role":"assistant","#);
        let results1 = parse_complete_lines(&mut buffer);
        assert!(results1.is_empty());

        // Second chunk: completes the line
        buffer.extend_from_slice(
            br#""content":"Hi"},"done":false}
"#,
        );
        let results2 = parse_complete_lines(&mut buffer);
        assert_eq!(results2.len(), 1);
        let resp = results2[0].as_ref().unwrap();
        assert_eq!(resp.message.content, "Hi");
        assert!(buffer.is_empty());
    }

    #[test]
    fn empty_lines_skipped() {
        let mut buffer = BytesMut::from(concat!(
            "\n",
            r#"{"model":"llama3.2","message":{"role":"assistant","content":"Hi"},"done":false}"#,
            "\n",
            "\n",
        ));
        let results = parse_complete_lines(&mut buffer);
        assert_eq!(results.len(), 1);
        let resp = results[0].as_ref().unwrap();
        assert_eq!(resp.message.content, "Hi");
    }

    #[test]
    fn invalid_json_returns_error() {
        let mut buffer = BytesMut::from("not valid json\n");
        let results = parse_complete_lines(&mut buffer);
        assert_eq!(results.len(), 1);
        assert!(results[0].is_err());
        let err = results[0].as_ref().unwrap_err().to_string();
        assert!(err.contains("NDJSON parse error"), "got: {err}");
    }

    #[test]
    fn mixed_valid_and_empty_lines() {
        let mut buffer = BytesMut::from(concat!(
            "\n",
            r#"{"model":"m","message":{"role":"assistant","content":"A"},"done":false}"#,
            "\n",
            "\n",
            r#"{"model":"m","message":{"role":"assistant","content":"B"},"done":false}"#,
            "\n",
            "   \n",
        ));
        let results = parse_complete_lines(&mut buffer);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].as_ref().unwrap().message.content, "A");
        assert_eq!(results[1].as_ref().unwrap().message.content, "B");
    }
}
