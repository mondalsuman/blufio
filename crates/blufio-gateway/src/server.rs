// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Gateway HTTP server built on axum.
//!
//! Sets up routes, middleware, and shared state for the gateway.

use std::sync::Arc;

use axum::{
    Router, middleware as axum_middleware,
    routing::{delete, get, post},
};
use blufio_core::BlufioError;
use blufio_core::ProviderRegistry;
use blufio_core::StorageAdapter;
use blufio_core::types::InboundMessage;
use blufio_skill::ToolRegistry;
use dashmap::DashMap;
use tokio::sync::{RwLock, mpsc, oneshot};
use tower_http::cors::CorsLayer;

use crate::api_keys;
use crate::auth::{AuthConfig, auth_middleware};
use crate::batch;
use crate::handlers;
use crate::openai_compat;
use crate::rate_limit::rate_limit_middleware;
use crate::webhooks;
use crate::ws;

/// Health state for unauthenticated health/metrics endpoints.
#[derive(Clone)]
pub struct HealthState {
    /// Process start time for uptime calculation.
    pub start_time: std::time::Instant,
    /// Optional Prometheus metrics render function.
    pub prometheus_render: Option<Arc<dyn Fn() -> String + Send + Sync>>,
}

/// Shared state for axum request handlers.
#[derive(Clone)]
pub struct GatewayState {
    /// Channel for sending inbound messages to the agent loop.
    pub inbound_tx: mpsc::Sender<InboundMessage>,
    /// Map of request_id -> oneshot sender for HTTP response routing.
    pub response_map: Arc<DashMap<String, oneshot::Sender<String>>>,
    /// Map of ws_id -> mpsc sender for WebSocket response routing.
    pub ws_senders: Arc<DashMap<String, mpsc::Sender<String>>>,
    /// Authentication configuration.
    pub auth: AuthConfig,
    /// Health state for unauthenticated endpoints.
    pub health: HealthState,
    /// Storage adapter for querying sessions (DEBT-01).
    pub storage: Option<Arc<dyn StorageAdapter + Send + Sync>>,
    /// Provider registry for /v1/chat/completions and /v1/models (API-01).
    pub providers: Option<Arc<dyn ProviderRegistry + Send + Sync>>,
    /// Tool registry for /v1/tools and /v1/tools/invoke (API-09).
    pub tools: Option<Arc<RwLock<ToolRegistry>>>,
    /// Allowlist of tool names accessible via the Tools API (API-10).
    pub api_tools_allowlist: Vec<String>,
    /// Maximum batch size for POST /v1/batch (API-17).
    pub max_batch_size: usize,
    /// Webhook store for webhook CRUD (API-15).
    pub webhook_store: Option<Arc<webhooks::store::WebhookStore>>,
    /// Batch store for batch processing (API-17).
    pub batch_store: Option<Arc<batch::store::BatchStore>>,
    /// Event bus for publishing events to webhooks and batch processor.
    pub event_bus: Option<Arc<blufio_bus::EventBus>>,
}

/// Gateway server configuration (mirrors GatewayConfig from blufio-config).
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Host address to bind.
    pub host: String,
    /// Port to bind.
    pub port: u16,
    /// Bearer token for auth (None = auth disabled).
    pub bearer_token: Option<String>,
}

/// Start the gateway HTTP/WebSocket server.
///
/// Binds to the configured host:port and serves routes:
/// - POST /v1/messages (with auth)
/// - GET /v1/sessions (with auth)
/// - GET /v1/health (with auth)
/// - POST /v1/api-keys, GET /v1/api-keys, DELETE /v1/api-keys/:id (API-11 through API-14)
/// - GET /ws (auth via query params, not middleware)
/// - /mcp/* (MCP Streamable HTTP, if `mcp_router` is Some)
///
/// When an MCP router is provided, it is nested at `/mcp` with its own
/// restricted CORS and auth layers (applied internally by the MCP router).
/// The permissive CORS layer only applies to non-MCP routes.
pub async fn start_server(
    config: &ServerConfig,
    state: GatewayState,
    mcp_router: Option<Router>,
    mcp_max_connections: usize,
    extra_public_routes: Option<Router>,
) -> Result<(), BlufioError> {
    let auth_state = state.auth.clone();

    // Unauthenticated public routes (health + metrics for systemd and Prometheus).
    let public_routes = Router::new()
        .route("/health", get(handlers::get_public_health))
        .route("/metrics", get(handlers::get_public_metrics))
        .with_state(state.clone());

    // Routes requiring authentication.
    // Layer order matters: axum applies layers bottom-up, so rate_limit runs
    // AFTER auth (auth inserts AuthContext, rate_limit reads it).
    let api_routes = Router::new()
        .route("/v1/messages", post(handlers::post_messages))
        .route("/v1/sessions", get(handlers::get_sessions))
        .route("/v1/health", get(handlers::get_health))
        // OpenAI-compatible API endpoints (API-01 through API-10).
        .route(
            "/v1/chat/completions",
            post(openai_compat::handlers::post_chat_completions),
        )
        .route("/v1/models", get(openai_compat::handlers::get_models))
        .route(
            "/v1/responses",
            post(openai_compat::responses::post_responses),
        )
        .route("/v1/tools", get(openai_compat::tools::get_tools))
        .route(
            "/v1/tools/invoke",
            post(openai_compat::tools::post_tool_invoke),
        )
        // API key management endpoints (API-11 through API-14).
        .route(
            "/v1/api-keys",
            post(api_keys::handlers::post_create_api_key)
                .get(api_keys::handlers::get_list_api_keys),
        )
        .route(
            "/v1/api-keys/:id",
            delete(api_keys::handlers::delete_api_key),
        )
        // Webhook management endpoints (API-15, API-16).
        .route(
            "/v1/webhooks",
            post(webhooks::handlers::post_create_webhook)
                .get(webhooks::handlers::get_list_webhooks),
        )
        .route(
            "/v1/webhooks/:id",
            delete(webhooks::handlers::delete_webhook),
        )
        // Batch processing endpoints (API-17, API-18).
        .route("/v1/batch", post(batch::handlers::post_create_batch))
        .route("/v1/batch/:id", get(batch::handlers::get_batch_status))
        // Rate limiting middleware (runs after auth, reads AuthContext from extensions).
        .route_layer(axum_middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        // Auth middleware (runs first, inserts AuthContext into extensions).
        .route_layer(axum_middleware::from_fn_with_state(
            auth_state.clone(),
            auth_middleware,
        ))
        .with_state(state.clone());

    // WebSocket route (auth happens during handshake, not via middleware).
    let ws_routes = Router::new()
        .route("/ws", get(ws::ws_handler))
        .with_state(state);

    let mut app = Router::new()
        .merge(public_routes)
        .merge(api_routes)
        .merge(ws_routes);

    // Merge extra public routes (e.g., WhatsApp webhook routes).
    if let Some(extra) = extra_public_routes {
        app = app.merge(extra);
    }

    // Mount MCP Streamable HTTP routes at /mcp (if enabled).
    // The MCP router includes its own restricted CORS and auth layers,
    // so it must be nested BEFORE the permissive CORS layer.
    // Connection limit (INTG-05) enforces max concurrent MCP connections.
    if let Some(mcp) = mcp_router {
        let limited_mcp = mcp.layer(tower::limit::ConcurrencyLimitLayer::new(
            mcp_max_connections,
        ));
        app = app.nest("/mcp", limited_mcp);
        tracing::info!(
            max_connections = mcp_max_connections,
            "MCP connection limit enabled"
        );
    }

    // Permissive CORS for non-MCP routes.
    // NOTE: The MCP router already has its own restricted CORS layer applied internally.
    let app = app.layer(CorsLayer::permissive());

    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| BlufioError::channel_delivery_failed("gateway", e))?;

    tracing::info!("Gateway server listening on {addr}");

    axum::serve(listener, app)
        .await
        .map_err(|e| BlufioError::channel_delivery_failed("gateway", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_state_is_clone() {
        let (tx, _rx) = mpsc::channel(1);
        let state = GatewayState {
            inbound_tx: tx,
            response_map: Arc::new(DashMap::new()),
            ws_senders: Arc::new(DashMap::new()),
            auth: AuthConfig {
                bearer_token: None,
                keypair_public_key: None,
                key_store: None,
            },
            health: HealthState {
                start_time: std::time::Instant::now(),
                prometheus_render: None,
            },
            storage: None,
            providers: None,
            tools: None,
            api_tools_allowlist: vec![],
            max_batch_size: 100,
            webhook_store: None,
            batch_store: None,
            event_bus: None,
        };
        let _cloned = state.clone();
    }

    #[test]
    fn server_config_debug() {
        let config = ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 3000,
            bearer_token: None,
        };
        let debug = format!("{config:?}");
        assert!(debug.contains("127.0.0.1"));
    }
}
