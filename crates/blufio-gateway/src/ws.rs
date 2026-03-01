// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! WebSocket handler for bidirectional messaging.
//!
//! Client -> Server (JSON):
//! ```json
//! {"content": "Hello, what's the weather?", "session_id": "optional-session-id"}
//! ```
//!
//! Server -> Client (JSON):
//! ```json
//! {"type": "typing"}
//! {"type": "text_delta", "text": "partial..."}
//! {"type": "message_complete", "content": "full response", "session_id": "..."}
//! ```

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;

use blufio_core::types::{InboundMessage, MessageContent};

use crate::server::GatewayState;

/// WebSocket message from client.
#[derive(Debug, Deserialize)]
struct WsIncoming {
    content: String,
    #[serde(default)]
    session_id: Option<String>,
}

/// WebSocket upgrade handler.
///
/// Upgrades the HTTP connection to WebSocket and spawns a handler task.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<GatewayState>,
) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Handle an individual WebSocket connection.
///
/// Spawns two tasks:
/// 1. Sender task: forwards responses from the agent to the WebSocket client
/// 2. Receiver loop: reads messages from client and forwards to agent loop
async fn handle_socket(socket: WebSocket, state: GatewayState) {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let ws_id = uuid::Uuid::new_v4().to_string();

    // Create mpsc channel for sending responses back to this WebSocket.
    let (tx, mut rx) = mpsc::channel::<String>(64);
    state.ws_senders.insert(ws_id.clone(), tx);

    // Spawn task to forward responses to WebSocket.
    let sender_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Read messages from WebSocket client.
    while let Some(Ok(msg)) = ws_receiver.next().await {
        match msg {
            Message::Text(text) => {
                let text_str: &str = &text;
                let incoming: WsIncoming = match serde_json::from_str(text_str) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("invalid WebSocket message: {e}");
                        continue;
                    }
                };

                let request_id = uuid::Uuid::new_v4().to_string();
                let now = chrono::Utc::now().to_rfc3339();

                let inbound = InboundMessage {
                    id: request_id.clone(),
                    session_id: incoming.session_id.clone(),
                    channel: "ws".to_string(),
                    sender_id: ws_id.clone(),
                    content: MessageContent::Text(incoming.content),
                    timestamp: now,
                    metadata: Some(
                        serde_json::json!({
                            "request_id": request_id,
                            "channel": "ws",
                            "ws_id": ws_id
                        })
                        .to_string(),
                    ),
                };

                if state.inbound_tx.send(inbound).await.is_err() {
                    tracing::error!("failed to send WebSocket message to agent loop");
                    break;
                }
            }
            Message::Close(_) => break,
            _ => {} // Ignore binary, ping (handled by tungstenite layer)
        }
    }

    // Cleanup.
    state.ws_senders.remove(&ws_id);
    sender_task.abort();
}

/// WebSocket message type constants for server -> client messages.
pub mod message_types {
    /// Typing indicator.
    pub const TYPING: &str = "typing";
    /// Partial text content.
    pub const TEXT_DELTA: &str = "text_delta";
    /// Complete message.
    pub const MESSAGE_COMPLETE: &str = "message_complete";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_incoming_deserializes_minimal() {
        let json = r#"{"content": "hello"}"#;
        let msg: WsIncoming = serde_json::from_str(json).unwrap();
        assert_eq!(msg.content, "hello");
        assert!(msg.session_id.is_none());
    }

    #[test]
    fn ws_incoming_deserializes_with_session() {
        let json = r#"{"content": "hello", "session_id": "sess-1"}"#;
        let msg: WsIncoming = serde_json::from_str(json).unwrap();
        assert_eq!(msg.content, "hello");
        assert_eq!(msg.session_id.as_deref(), Some("sess-1"));
    }

    #[test]
    fn message_type_constants() {
        assert_eq!(message_types::TYPING, "typing");
        assert_eq!(message_types::TEXT_DELTA, "text_delta");
        assert_eq!(message_types::MESSAGE_COMPLETE, "message_complete");
    }
}
