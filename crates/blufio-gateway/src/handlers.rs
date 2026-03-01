// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTP request handlers for the gateway REST API.
//!
//! Handles POST /v1/messages, GET /v1/health, GET /v1/sessions.

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use blufio_core::types::{InboundMessage, MessageContent};

use crate::server::GatewayState;
use crate::sse;

/// Request body for POST /v1/messages.
#[derive(Debug, Deserialize)]
pub struct MessageRequest {
    /// Message content text.
    pub content: String,
    /// Optional session ID to continue an existing session.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Optional sender identifier.
    #[serde(default)]
    pub sender_id: Option<String>,
}

/// Response body for POST /v1/messages.
#[derive(Debug, Serialize)]
pub struct MessageResponse {
    /// Request/message ID.
    pub id: String,
    /// Response content from the agent.
    pub content: String,
    /// Session ID (may be newly created).
    pub session_id: Option<String>,
    /// ISO 8601 timestamp.
    pub created_at: String,
}

/// Response body for GET /v1/health.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Health status string.
    pub status: String,
    /// Binary version.
    pub version: String,
    /// Uptime in seconds (placeholder).
    pub uptime_secs: u64,
}

/// Response body for GET /v1/sessions.
#[derive(Debug, Serialize)]
pub struct SessionListResponse {
    /// List of active sessions.
    pub sessions: Vec<SessionInfo>,
}

/// Information about a single session.
#[derive(Debug, Serialize)]
pub struct SessionInfo {
    /// Session identifier.
    pub id: String,
    /// Channel the session originates from.
    pub channel: String,
    /// Session state.
    pub state: String,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
}

/// Error response body.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Error description.
    pub error: String,
}

/// POST /v1/messages
///
/// Accepts a message, routes it through the agent loop, and returns the response.
/// If the Accept header contains "text/event-stream", routes to SSE streaming.
pub async fn post_messages(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Json(body): Json<MessageRequest>,
) -> Response {
    // Check for SSE streaming request.
    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if accept.contains("text/event-stream") {
        return sse::stream_messages(state, body).await.into_response();
    }

    let request_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let inbound = InboundMessage {
        id: request_id.clone(),
        session_id: body.session_id.clone(),
        channel: "api".to_string(),
        sender_id: body.sender_id.unwrap_or_else(|| "api-user".to_string()),
        content: MessageContent::Text(body.content),
        timestamp: now.clone(),
        metadata: Some(
            serde_json::json!({
                "request_id": request_id,
                "channel": "api"
            })
            .to_string(),
        ),
    };

    // Create oneshot channel for response routing.
    let (tx, rx) = oneshot::channel::<String>();
    state.response_map.insert(request_id.clone(), tx);

    // Send to inbound channel (with timeout).
    match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        state.inbound_tx.send(inbound),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(_)) => {
            state.response_map.remove(&request_id);
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "agent loop not accepting messages".to_string(),
                }),
            )
                .into_response();
        }
        Err(_) => {
            state.response_map.remove(&request_id);
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "inbound channel full".to_string(),
                }),
            )
                .into_response();
        }
    }

    // Wait for response (with timeout for LLM processing).
    match tokio::time::timeout(std::time::Duration::from_secs(120), rx).await {
        Ok(Ok(content)) => {
            let response = MessageResponse {
                id: request_id,
                content,
                session_id: body.session_id,
                created_at: now,
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Ok(Err(_)) => {
            // Sender dropped (agent loop crashed or disconnected).
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "response channel closed".to_string(),
                }),
            )
                .into_response()
        }
        Err(_) => {
            // Timeout waiting for LLM response.
            state.response_map.remove(&request_id);
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(ErrorResponse {
                    error: "response timeout (120s)".to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// GET /v1/health
///
/// Returns health status of the gateway.
pub async fn get_health(State(_state): State<GatewayState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs: 0, // TODO: track actual uptime
    })
}

/// GET /v1/sessions
///
/// Returns list of active sessions.
///
/// TODO: Wire StorageAdapter into GatewayState in Plan 03 integration
/// to provide actual session data.
pub async fn get_sessions(
    State(_state): State<GatewayState>,
) -> Json<SessionListResponse> {
    Json(SessionListResponse {
        sessions: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_request_deserializes_with_content() {
        let json = r#"{"content": "Hello, world!"}"#;
        let req: MessageRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.content, "Hello, world!");
        assert!(req.session_id.is_none());
        assert!(req.sender_id.is_none());
    }

    #[test]
    fn message_request_deserializes_with_all_fields() {
        let json = r#"{
            "content": "Hello",
            "session_id": "sess-123",
            "sender_id": "user-456"
        }"#;
        let req: MessageRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.content, "Hello");
        assert_eq!(req.session_id.as_deref(), Some("sess-123"));
        assert_eq!(req.sender_id.as_deref(), Some("user-456"));
    }

    #[test]
    fn health_response_serializes() {
        let resp = HealthResponse {
            status: "ok".to_string(),
            version: "0.1.0".to_string(),
            uptime_secs: 42,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"status\":\"ok\""));
        assert!(json.contains("\"version\":\"0.1.0\""));
        assert!(json.contains("\"uptime_secs\":42"));
    }

    #[test]
    fn error_response_serializes() {
        let resp = ErrorResponse {
            error: "something went wrong".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("something went wrong"));
    }

    #[test]
    fn session_list_response_serializes_empty() {
        let resp = SessionListResponse { sessions: vec![] };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"sessions\":[]"));
    }
}
