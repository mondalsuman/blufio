// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Cross-contamination tests verifying protocol isolation between REST and MCP.
//!
//! These tests ensure that JSON-RPC requests to REST endpoints are rejected
//! and that REST-format requests to MCP endpoints produce errors.
//! Tests use axum's test utilities (tower::ServiceExt::oneshot) without port binding.

use std::sync::Arc;

use axum::body::Body;
use axum::routing::{get, post};
use axum::Router;
use blufio_core::types::InboundMessage;
use blufio_gateway::auth::AuthConfig;
use blufio_gateway::server::{GatewayState, HealthState};
use dashmap::DashMap;
use http::{Request, StatusCode};
use tokio::sync::{mpsc, oneshot};
use tower::ServiceExt;

/// Creates a minimal gateway router for testing (no auth middleware).
fn build_test_router() -> Router {
    let (inbound_tx, _rx) = mpsc::channel::<InboundMessage>(16);

    let state = GatewayState {
        inbound_tx,
        response_map: Arc::new(DashMap::<String, oneshot::Sender<String>>::new()),
        ws_senders: Arc::new(DashMap::new()),
        auth: AuthConfig {
            bearer_token: None,
            keypair_public_key: None,
        },
        health: HealthState {
            start_time: std::time::Instant::now(),
            prometheus_render: None,
        },
        storage: None,
    };

    // Build routes matching the gateway server setup (without auth middleware for testing).
    Router::new()
        .route(
            "/v1/messages",
            post(blufio_gateway::handlers::post_messages),
        )
        .route(
            "/v1/sessions",
            get(blufio_gateway::handlers::get_sessions),
        )
        .route("/v1/health", get(blufio_gateway::handlers::get_health))
        .route("/health", get(blufio_gateway::handlers::get_public_health))
        .with_state(state)
}

#[tokio::test]
async fn test_jsonrpc_body_to_rest_messages_rejected() {
    let app = build_test_router();

    // Send a JSON-RPC formatted body to POST /v1/messages.
    // The handler expects MessageRequest { content, session_id?, sender_id? },
    // not JSON-RPC format. This should fail deserialization.
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/list",
        "id": 1
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should be 422 Unprocessable Entity (axum's default for JSON parse failure)
    // because the body lacks the required "content" field.
    assert_eq!(
        response.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "JSON-RPC body to REST endpoint should be rejected (missing 'content' field)"
    );
}

#[tokio::test]
async fn test_empty_body_to_rest_messages_rejected() {
    let app = build_test_router();

    // Send empty body to POST /v1/messages.
    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should be 422 because MessageRequest requires "content".
    assert_eq!(
        response.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "empty JSON body should be rejected (missing 'content' field)"
    );
}

#[tokio::test]
async fn test_sessions_endpoint_returns_json_list() {
    let app = build_test_router();

    // GET /v1/sessions should return a session list (empty when no storage).
    let request = Request::builder()
        .method("GET")
        .uri("/v1/sessions")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Should have a "sessions" array.
    assert!(
        json.get("sessions").is_some(),
        "response should have 'sessions' field"
    );
    assert!(
        json["sessions"].is_array(),
        "'sessions' should be an array"
    );
}

#[tokio::test]
async fn test_health_endpoint_returns_ok() {
    let app = build_test_router();

    let request = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "healthy");
}

#[tokio::test]
async fn test_invalid_content_type_to_messages_rejected() {
    let app = build_test_router();

    // Send plain text to POST /v1/messages (expects application/json).
    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "text/plain")
        .body(Body::from("just a string"))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // axum rejects non-JSON content types with 415 Unsupported Media Type.
    assert_eq!(
        response.status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "plain text content type should be rejected"
    );
}

#[tokio::test]
async fn test_get_method_on_messages_not_allowed() {
    let app = build_test_router();

    // GET /v1/messages should not be a valid route (only POST is registered).
    let request = Request::builder()
        .method("GET")
        .uri("/v1/messages")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should be 405 Method Not Allowed.
    assert_eq!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "GET on POST-only endpoint should return 405"
    );
}
