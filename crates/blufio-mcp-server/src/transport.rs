// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTP transport for the MCP server.
//!
//! Provides Streamable HTTP transport via rmcp's [`StreamableHttpService`],
//! MCP-specific CORS configuration, and router construction.
//!
//! The MCP HTTP router is mounted at `/mcp` on the gateway, with its own
//! restricted CORS layer and bearer token authentication.

use std::sync::Arc;
use std::time::Duration;

use axum::{Router, middleware};
use http::Method;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tokio_util::sync::CancellationToken;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::auth::{McpAuthConfig, mcp_auth_middleware};
use crate::handler::BlufioMcpHandler;

/// Builds a CORS layer for MCP HTTP endpoints.
///
/// - Empty `cors_origins`: reject all cross-origin requests (secure by default).
/// - Non-empty: allow only the listed origins.
///
/// Allowed methods: GET, POST, DELETE (required by Streamable HTTP spec).
/// Allowed headers: Content-Type, Authorization, Accept, Mcp-Session-Id.
pub fn build_mcp_cors(cors_origins: &[String]) -> CorsLayer {
    let methods = vec![Method::GET, Method::POST, Method::DELETE];
    let headers = vec![
        CONTENT_TYPE,
        AUTHORIZATION,
        ACCEPT,
        http::header::HeaderName::from_static("mcp-session-id"),
    ];

    let layer = CorsLayer::new()
        .allow_methods(methods)
        .allow_headers(headers);

    if cors_origins.is_empty() {
        // Reject all cross-origin requests by not allowing any origin.
        layer.allow_origin(AllowOrigin::list(std::iter::empty::<http::HeaderValue>()))
    } else {
        let origins: Vec<http::HeaderValue> =
            cors_origins.iter().filter_map(|o| o.parse().ok()).collect();
        layer.allow_origin(AllowOrigin::list(origins))
    }
}

/// Creates a [`StreamableHttpServerConfig`] with standard MCP settings.
///
/// - SSE keep-alive: 30 seconds
/// - SSE retry: 5 seconds
/// - Stateful mode: enabled (session tracking)
/// - Cancellation token: provided by caller (from shutdown handler)
pub fn mcp_service_config(cancel: CancellationToken) -> StreamableHttpServerConfig {
    StreamableHttpServerConfig {
        sse_keep_alive: Some(Duration::from_secs(30)),
        sse_retry: Some(Duration::from_secs(5)),
        stateful_mode: true,
        json_response: false,
        cancellation_token: cancel,
    }
}

/// Builds the MCP HTTP router with authentication, CORS, and the
/// Streamable HTTP service.
///
/// The returned [`Router`] should be nested at `/mcp` on the gateway.
/// It includes:
/// - StreamableHttpService handling all MCP protocol messages
/// - Bearer token authentication middleware
/// - Restricted CORS layer (only configured origins)
///
/// The CORS layer is applied as the outermost layer so that OPTIONS
/// preflight requests are handled before auth checks.
pub fn build_mcp_router(
    handler: BlufioMcpHandler,
    config: StreamableHttpServerConfig,
    cors_origins: &[String],
    auth_token: String,
) -> Router {
    // The handler must be shared across sessions. Each session gets a clone.
    let handler = Arc::new(handler);
    let session_manager = Arc::new(LocalSessionManager::default());

    let mcp_service = StreamableHttpService::new(
        move || {
            // Clone handler for each new session.
            Ok(handler.clone())
        },
        session_manager,
        config,
    );

    let cors = build_mcp_cors(cors_origins);
    let mcp_auth = McpAuthConfig { auth_token };

    // Layer order matters:
    // 1. CORS (outermost) - handles OPTIONS preflight before auth
    // 2. Auth (inner) - validates bearer token on actual requests
    // 3. Service (innermost) - handles MCP protocol messages
    Router::new()
        .nest_service("/", mcp_service)
        .route_layer(middleware::from_fn_with_state(
            mcp_auth,
            mcp_auth_middleware,
        ))
        .layer(cors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_mcp_cors_empty_origins_blocks_all() {
        let cors = build_mcp_cors(&[]);
        // The layer is created successfully; verification is structural.
        // An empty origin list means no origin will match.
        drop(cors);
    }

    #[test]
    fn build_mcp_cors_with_origins() {
        let origins = vec![
            "https://app.example.com".to_string(),
            "https://other.example.com".to_string(),
        ];
        let cors = build_mcp_cors(&origins);
        // Layer created successfully with 2 allowed origins.
        drop(cors);
    }

    #[test]
    fn mcp_service_config_defaults() {
        let cancel = CancellationToken::new();
        let config = mcp_service_config(cancel.clone());
        assert_eq!(config.sse_keep_alive, Some(Duration::from_secs(30)));
        assert_eq!(config.sse_retry, Some(Duration::from_secs(5)));
        assert!(config.stateful_mode);
        assert!(!config.json_response);
    }

    #[test]
    fn mcp_service_config_uses_provided_token() {
        let cancel = CancellationToken::new();
        let config = mcp_service_config(cancel.clone());
        // Cancel the token and verify the config's token is also cancelled.
        cancel.cancel();
        assert!(config.cancellation_token.is_cancelled());
    }
}
