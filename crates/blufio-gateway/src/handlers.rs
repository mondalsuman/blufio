// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTP request handlers for the gateway REST API.
//!
//! Handles POST /v1/messages, GET /v1/health, GET /v1/sessions.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use std::collections::HashMap;

use blufio_core::types::{InboundMessage, MessageContent};

use crate::server::GatewayState;
use crate::sse;

/// Request body for POST /v1/messages.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MessageRequest {
    /// Message content text.
    #[schema(example = "Hello, how are you?")]
    pub content: String,
    /// Optional session ID to continue an existing session.
    #[serde(default)]
    #[schema(example = "sess-abc123")]
    pub session_id: Option<String>,
    /// Optional sender identifier.
    #[serde(default)]
    #[schema(example = "user-456")]
    pub sender_id: Option<String>,
}

/// Response body for POST /v1/messages.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MessageResponse {
    /// Request/message ID.
    #[schema(example = "msg-abc123")]
    pub id: String,
    /// Response content from the agent.
    #[schema(example = "I'm doing well, thank you!")]
    pub content: String,
    /// Session ID (may be newly created).
    #[schema(example = "sess-abc123")]
    pub session_id: Option<String>,
    /// ISO 8601 timestamp.
    #[schema(example = "2026-03-13T12:00:00Z")]
    pub created_at: String,
}

/// Response body for GET /v1/health.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct HealthResponse {
    /// Health status string.
    #[schema(example = "ok")]
    pub status: String,
    /// Binary version.
    #[schema(example = "0.1.0")]
    pub version: String,
    /// Uptime in seconds (placeholder).
    #[schema(example = 3600)]
    pub uptime_secs: u64,
    /// Current degradation level (e.g., "L0").
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(example = "L0")]
    pub degradation_level: Option<String>,
    /// Human-readable degradation level name (e.g., "FullyOperational").
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(example = "FullyOperational")]
    pub degradation_name: Option<String>,
    /// Per-dependency circuit breaker states (e.g., {"anthropic": "closed"}).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub circuit_breakers: Option<HashMap<String, String>>,
}

/// Response body for GET /v1/sessions.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SessionListResponse {
    /// List of active sessions.
    pub sessions: Vec<SessionInfo>,
}

/// Information about a single session.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SessionInfo {
    /// Session identifier.
    #[schema(example = "sess-abc123")]
    pub id: String,
    /// Channel the session originates from.
    #[schema(example = "api")]
    pub channel: String,
    /// Session state.
    #[schema(example = "active")]
    pub state: String,
    /// ISO 8601 creation timestamp.
    #[schema(example = "2026-03-13T12:00:00Z")]
    pub created_at: String,
}

/// Error response body.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ErrorResponse {
    /// Error description.
    #[schema(example = "Invalid request body")]
    pub error: String,
}

/// Response body for GET /health (unauthenticated).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PublicHealthResponse {
    /// Health status string.
    #[schema(example = "healthy")]
    pub status: String,
    /// Uptime in seconds.
    #[schema(example = 120)]
    pub uptime_secs: u64,
}

/// POST /v1/messages
///
/// Accepts a message, routes it through the agent loop, and returns the response.
/// If the Accept header contains "text/event-stream", routes to SSE streaming.
#[utoipa::path(
    post,
    path = "/v1/messages",
    tag = "Messages",
    request_body = MessageRequest,
    responses(
        (status = 200, description = "Message processed", body = MessageResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 401, description = "Unauthorized"),
        (status = 503, description = "Service unavailable", body = ErrorResponse),
        (status = 504, description = "Gateway timeout", body = ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
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
/// Returns health status of the gateway, including degradation state when
/// the resilience subsystem is wired in. Returns 503 for L4+ degradation.
#[utoipa::path(
    get,
    path = "/v1/health",
    tag = "Health",
    responses(
        (status = 200, description = "Service healthy", body = HealthResponse),
        (status = 401, description = "Unauthorized"),
        (status = 503, description = "Service degraded", body = HealthResponse),
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_health(State(state): State<GatewayState>) -> Response {
    let (degradation_level, degradation_name, circuit_breakers, level_val) =
        if let Some(dm) = &state.degradation_manager {
            let level = dm.current_level();
            let cb_map = state.circuit_breaker_registry.as_ref().map(|reg| {
                reg.all_snapshots()
                    .into_iter()
                    .map(|(name, snap)| (name, snap.state.to_string()))
                    .collect::<HashMap<String, String>>()
            });
            (
                Some(format!("L{}", level.as_u8())),
                Some(level.name().to_string()),
                cb_map,
                level.as_u8(),
            )
        } else {
            (None, None, None, 0)
        };

    let status = if level_val >= 4 { "degraded" } else { "ok" };

    let resp = HealthResponse {
        status: status.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs: 0, // TODO: track actual uptime
        degradation_level,
        degradation_name,
        circuit_breakers,
    };

    if level_val >= 4 {
        (StatusCode::SERVICE_UNAVAILABLE, Json(resp)).into_response()
    } else {
        (StatusCode::OK, Json(resp)).into_response()
    }
}

/// GET /health (unauthenticated)
///
/// Returns basic health status for systemd health checks and monitoring.
/// Does not require authentication. Returns minimal information.
#[utoipa::path(
    get,
    path = "/health",
    tag = "Health",
    responses(
        (status = 200, description = "Service healthy", body = PublicHealthResponse),
    )
)]
pub async fn get_public_health(State(state): State<GatewayState>) -> Json<PublicHealthResponse> {
    let uptime = state.health.start_time.elapsed().as_secs();
    Json(PublicHealthResponse {
        status: "healthy".to_string(),
        uptime_secs: uptime,
    })
}

/// GET /metrics (unauthenticated)
///
/// Returns Prometheus metrics in text format for scraping.
/// Does not require authentication.
#[utoipa::path(
    get,
    path = "/metrics",
    tag = "Health",
    responses(
        (status = 200, description = "Prometheus metrics", content_type = "text/plain"),
        (status = 503, description = "Metrics not available"),
    )
)]
pub async fn get_public_metrics(State(state): State<GatewayState>) -> Response {
    match &state.health.prometheus_render {
        Some(render_fn) => {
            let body = render_fn();
            (
                StatusCode::OK,
                [(
                    axum::http::header::CONTENT_TYPE,
                    "text/plain; version=0.0.4; charset=utf-8",
                )],
                body,
            )
                .into_response()
        }
        None => (StatusCode::SERVICE_UNAVAILABLE, "Metrics not available").into_response(),
    }
}

/// GET /v1/sessions
///
/// Returns list of active sessions from storage.
#[utoipa::path(
    get,
    path = "/v1/sessions",
    tag = "Sessions",
    responses(
        (status = 200, description = "Session list", body = SessionListResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_sessions(State(state): State<GatewayState>) -> Response {
    let Some(storage) = &state.storage else {
        return Json(SessionListResponse { sessions: vec![] }).into_response();
    };

    match storage.list_sessions(None).await {
        Ok(sessions) => {
            let infos: Vec<SessionInfo> = sessions
                .into_iter()
                .map(|s| SessionInfo {
                    id: s.id,
                    channel: s.channel,
                    state: s.state,
                    created_at: s.created_at,
                })
                .collect();
            Json(SessionListResponse { sessions: infos }).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to list sessions");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to retrieve sessions".to_string(),
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
            degradation_level: None,
            degradation_name: None,
            circuit_breakers: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"status\":\"ok\""));
        assert!(json.contains("\"version\":\"0.1.0\""));
        assert!(json.contains("\"uptime_secs\":42"));
        // Optional fields should be omitted when None
        assert!(!json.contains("degradation_level"));
    }

    #[test]
    fn health_response_with_degradation_fields() {
        let mut cb = HashMap::new();
        cb.insert("anthropic".to_string(), "closed".to_string());
        cb.insert("openai".to_string(), "open".to_string());
        let resp = HealthResponse {
            status: "ok".to_string(),
            version: "0.1.0".to_string(),
            uptime_secs: 100,
            degradation_level: Some("L1".to_string()),
            degradation_name: Some("MinorDegradation".to_string()),
            circuit_breakers: Some(cb),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"degradation_level\":\"L1\""));
        assert!(json.contains("\"degradation_name\":\"MinorDegradation\""));
        assert!(json.contains("\"circuit_breakers\""));
        assert!(json.contains("\"anthropic\":\"closed\""));
        assert!(json.contains("\"openai\":\"open\""));
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

    #[test]
    fn public_health_response_serializes() {
        let resp = PublicHealthResponse {
            status: "healthy".to_string(),
            uptime_secs: 120,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"status\":\"healthy\""));
        assert!(json.contains("\"uptime_secs\":120"));
    }
}
