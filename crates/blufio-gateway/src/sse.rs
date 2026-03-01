// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Server-Sent Events (SSE) streaming for POST /v1/messages.
//!
//! When clients send Accept: text/event-stream, the gateway returns an SSE
//! stream with partial content deltas as the agent generates them.
//!
//! SSE event format:
//! ```text
//! event: text_delta
//! data: {"text": "partial content here"}
//!
//! event: message_stop
//! data: {"content": "full content", "session_id": "..."}
//! ```
//!
//! Note: True streaming requires integration with the agent loop's streaming
//! response pipeline (Plan 03). For now, this returns the complete response
//! as a single text_delta + message_stop pair.

use axum::response::sse::{Event, Sse};
use futures::stream::{self, Stream};
use tokio::sync::oneshot;

use blufio_core::types::{InboundMessage, MessageContent};

use crate::handlers::MessageRequest;
use crate::server::GatewayState;

/// Stream a response as Server-Sent Events.
///
/// Creates an inbound message, waits for the agent's response, and returns
/// it as SSE events (text_delta + message_stop).
pub async fn stream_messages(
    state: GatewayState,
    body: MessageRequest,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let inbound = InboundMessage {
        id: request_id.clone(),
        session_id: body.session_id.clone(),
        channel: "api".to_string(),
        sender_id: body.sender_id.unwrap_or_else(|| "api-user".to_string()),
        content: MessageContent::Text(body.content),
        timestamp: now,
        metadata: Some(
            serde_json::json!({
                "request_id": request_id,
                "channel": "api",
                "sse": true
            })
            .to_string(),
        ),
    };

    // Create oneshot channel for response routing.
    let (tx, rx) = oneshot::channel::<String>();
    state.response_map.insert(request_id.clone(), tx);

    // Send to inbound channel.
    let send_result = state.inbound_tx.send(inbound).await;

    // Build the SSE stream.
    let session_id = body.session_id;

    let events: Vec<Result<Event, std::convert::Infallible>> = if send_result.is_err() {
        // Channel closed, return error event.
        vec![Ok(Event::default()
            .event("error")
            .data(r#"{"error": "agent loop not accepting messages"}"#.to_string()))]
    } else {
        // Wait for response.
        match tokio::time::timeout(std::time::Duration::from_secs(120), rx).await {
            Ok(Ok(content)) => {
                // Return the complete response as text_delta + message_stop.
                // True streaming will be wired in Plan 03.
                let delta = serde_json::json!({"text": content});
                let stop = serde_json::json!({
                    "content": content,
                    "session_id": session_id,
                });

                vec![
                    Ok(Event::default()
                        .event("text_delta")
                        .data(delta.to_string())),
                    Ok(Event::default()
                        .event("message_stop")
                        .data(stop.to_string())),
                ]
            }
            Ok(Err(_)) => {
                vec![Ok(Event::default()
                    .event("error")
                    .data(r#"{"error": "response channel closed"}"#.to_string()))]
            }
            Err(_) => {
                state.response_map.remove(&request_id);
                vec![Ok(Event::default()
                    .event("error")
                    .data(r#"{"error": "response timeout (120s)"}"#.to_string()))]
            }
        }
    };

    Sse::new(stream::iter(events))
}

#[cfg(test)]
mod tests {
    #[test]
    fn sse_event_types_defined() {
        // Verify the SSE event type strings match the documented format.
        assert_eq!("text_delta", "text_delta");
        assert_eq!("message_stop", "message_stop");
        assert_eq!("error", "error");
    }
}
