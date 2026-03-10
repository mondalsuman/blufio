// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Audit middleware for the gateway that emits [`ApiEvent`] for mutating HTTP requests.
//!
//! Only fires for POST, PUT, and DELETE methods (not GET, not health/status paths).
//! Emits events after the response is generated so the HTTP status code is available.

use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::Request,
    middleware::Next,
    response::Response,
};

use blufio_bus::EventBus;
use blufio_bus::events::{ApiEvent, BusEvent, new_event_id, now_timestamp};

use crate::api_keys::AuthContext;

/// Audit middleware that emits [`BusEvent::Api`] for mutating HTTP requests.
///
/// Must run AFTER `auth_middleware` (which inserts `AuthContext` into extensions).
/// Only emits events for POST, PUT, and DELETE methods.
/// Skips health and metrics endpoints.
pub async fn audit_middleware(
    State(event_bus): State<Option<Arc<EventBus>>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let bus = match event_bus {
        Some(ref bus) => bus.clone(),
        None => return next.run(request).await,
    };

    let method = request.method().clone();

    // Only audit mutating requests.
    if method == axum::http::Method::GET
        || method == axum::http::Method::HEAD
        || method == axum::http::Method::OPTIONS
    {
        return next.run(request).await;
    }

    let path = request.uri().path().to_string();

    // Skip health and metrics endpoints.
    if path == "/health" || path == "/metrics" {
        return next.run(request).await;
    }

    // Extract actor from AuthContext (inserted by auth middleware).
    let actor = request
        .extensions()
        .get::<AuthContext>()
        .map(|ctx| match ctx {
            AuthContext::Master => "user:master".to_string(),
            AuthContext::Scoped { key_id, .. } => format!("api-key:{key_id}"),
        })
        .unwrap_or_else(|| "anonymous".to_string());

    let response = next.run(request).await;

    let status = response.status().as_u16();

    // Fire-and-forget event emission.
    tokio::spawn(async move {
        bus.publish(BusEvent::Api(ApiEvent::Request {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            method: method.to_string(),
            path,
            status,
            actor,
        }))
        .await;
    });

    response
}
