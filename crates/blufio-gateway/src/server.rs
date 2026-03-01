// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Gateway HTTP server built on axum.
//!
//! Sets up routes, middleware, and shared state for the gateway.

use std::sync::Arc;

use axum::{
    middleware as axum_middleware,
    routing::{get, post},
    Router,
};
use blufio_core::types::InboundMessage;
use blufio_core::BlufioError;
use dashmap::DashMap;
use tokio::sync::{mpsc, oneshot};
use tower_http::cors::CorsLayer;

use crate::auth::{auth_middleware, AuthConfig};
use crate::handlers;
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
/// - GET /ws (auth via query params, not middleware)
pub async fn start_server(
    config: &ServerConfig,
    state: GatewayState,
) -> Result<(), BlufioError> {
    let auth_state = state.auth.clone();

    // Unauthenticated public routes (health + metrics for systemd and Prometheus).
    let public_routes = Router::new()
        .route("/health", get(handlers::get_public_health))
        .route("/metrics", get(handlers::get_public_metrics))
        .with_state(state.clone());

    // Routes requiring authentication.
    let api_routes = Router::new()
        .route("/v1/messages", post(handlers::post_messages))
        .route("/v1/sessions", get(handlers::get_sessions))
        .route("/v1/health", get(handlers::get_health))
        .route_layer(axum_middleware::from_fn_with_state(
            auth_state.clone(),
            auth_middleware,
        ))
        .with_state(state.clone());

    // WebSocket route (auth happens during handshake, not via middleware).
    let ws_routes = Router::new()
        .route("/ws", get(ws::ws_handler))
        .with_state(state);

    let app = Router::new()
        .merge(public_routes)
        .merge(api_routes)
        .merge(ws_routes)
        .layer(CorsLayer::permissive());

    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| BlufioError::Channel {
            message: format!("failed to bind gateway to {addr}: {e}"),
            source: Some(Box::new(e)),
        })?;

    tracing::info!("Gateway server listening on {addr}");

    axum::serve(listener, app)
        .await
        .map_err(|e| BlufioError::Channel {
            message: format!("gateway server error: {e}"),
            source: Some(Box::new(e)),
        })?;

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
            },
            health: HealthState {
                start_time: std::time::Instant::now(),
                prometheus_render: None,
            },
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
