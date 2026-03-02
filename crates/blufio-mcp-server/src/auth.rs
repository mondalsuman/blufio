// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! MCP-specific authentication middleware.
//!
//! Validates bearer token authentication on /mcp HTTP endpoints.
//! This is separate from the gateway's `auth_middleware` because MCP
//! uses its own `mcp.auth_token` (security isolation: MCP clients get
//! a different token than gateway API clients).

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};

/// Authentication configuration for MCP HTTP endpoints.
#[derive(Clone)]
pub struct McpAuthConfig {
    /// Expected bearer token for MCP authentication.
    pub auth_token: String,
}

/// Extracts and validates the bearer token from an Authorization header.
///
/// Returns `Some(token)` if the header is present and well-formed,
/// `None` otherwise.
fn extract_bearer_token(request: &Request) -> Option<&str> {
    request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

/// MCP authentication middleware.
///
/// Checks `Authorization: Bearer {token}` against `McpAuthConfig.auth_token`.
/// Returns 401 Unauthorized if the token is missing, malformed, or incorrect.
///
/// Simpler than the gateway's auth middleware (bearer only, no keypair).
pub async fn mcp_auth_middleware(
    State(auth): State<McpAuthConfig>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    match extract_bearer_token(&request) {
        Some(token) if token == auth.auth_token => Ok(next.run(request).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_bearer_token_valid() {
        let request = Request::builder()
            .header("authorization", "Bearer mcp-secret-123")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&request), Some("mcp-secret-123"));
    }

    #[test]
    fn extract_bearer_token_missing_header() {
        let request = Request::builder()
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&request), None);
    }

    #[test]
    fn extract_bearer_token_wrong_scheme() {
        let request = Request::builder()
            .header("authorization", "Basic abc123")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&request), None);
    }

    #[test]
    fn extract_bearer_token_empty_bearer() {
        let request = Request::builder()
            .header("authorization", "Bearer ")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&request), Some(""));
    }

    #[tokio::test]
    async fn middleware_accepts_correct_token() {
        use axum::{Router, body::Body, middleware, routing::get};
        use http::StatusCode;
        use tower::ServiceExt;

        let app = Router::new()
            .route("/test", get(|| async { "ok" }))
            .route_layer(middleware::from_fn_with_state(
                McpAuthConfig {
                    auth_token: "test-token".to_string(),
                },
                mcp_auth_middleware,
            ));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("authorization", "Bearer test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn middleware_rejects_wrong_token() {
        use axum::{Router, body::Body, middleware, routing::get};
        use http::StatusCode;
        use tower::ServiceExt;

        let app = Router::new()
            .route("/test", get(|| async { "ok" }))
            .route_layer(middleware::from_fn_with_state(
                McpAuthConfig {
                    auth_token: "correct-token".to_string(),
                },
                mcp_auth_middleware,
            ));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("authorization", "Bearer wrong-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn middleware_rejects_missing_token() {
        use axum::{Router, body::Body, middleware, routing::get};
        use http::StatusCode;
        use tower::ServiceExt;

        let app = Router::new()
            .route("/test", get(|| async { "ok" }))
            .route_layer(middleware::from_fn_with_state(
                McpAuthConfig {
                    auth_token: "test-token".to_string(),
                },
                mcp_auth_middleware,
            ));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
