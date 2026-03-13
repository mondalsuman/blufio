// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTP handlers for webhook management endpoints.

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};

use super::{CreateWebhookRequest, CreateWebhookResponse, WebhookListItem};
use crate::api_keys::{AuthContext, require_scope};
use crate::server::GatewayState;

/// POST /v1/webhooks -- Register a new webhook.
///
/// Requires admin scope or master auth. The HMAC secret is returned once.
#[utoipa::path(
    post,
    path = "/v1/webhooks",
    tag = "Webhooks",
    request_body = CreateWebhookRequest,
    responses(
        (status = 201, description = "Webhook created", body = CreateWebhookResponse),
        (status = 400, description = "Invalid URL or empty events"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 500, description = "Internal server error"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn post_create_webhook(
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<GatewayState>,
    Json(req): Json<CreateWebhookRequest>,
) -> Result<(StatusCode, Json<CreateWebhookResponse>), StatusCode> {
    require_scope(&auth_ctx, "admin")?;

    // Validate URL: must be https:// or http://localhost (for dev).
    if !req.url.starts_with("https://")
        && !req.url.starts_with("http://localhost")
        && !req.url.starts_with("http://127.0.0.1")
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Validate events: must not be empty.
    if req.events.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let webhook_store = state.webhook_store.as_ref().ok_or_else(|| {
        tracing::error!("webhook store not configured");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let resp = webhook_store.create(&req).await.map_err(|e| {
        tracing::error!(error = %e, "failed to create webhook");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /v1/webhooks -- List all registered webhooks.
///
/// Requires admin scope or master auth. Never exposes secrets.
#[utoipa::path(
    get,
    path = "/v1/webhooks",
    tag = "Webhooks",
    responses(
        (status = 200, description = "List of webhooks", body = Vec<WebhookListItem>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 500, description = "Internal server error"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_list_webhooks(
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<GatewayState>,
) -> Result<Json<Vec<WebhookListItem>>, StatusCode> {
    require_scope(&auth_ctx, "admin")?;

    let webhook_store = state.webhook_store.as_ref().ok_or_else(|| {
        tracing::error!("webhook store not configured");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let items = webhook_store.list().await.map_err(|e| {
        tracing::error!(error = %e, "failed to list webhooks");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(items))
}

/// DELETE /v1/webhooks/:id -- Delete a webhook.
///
/// Requires admin scope or master auth.
#[utoipa::path(
    delete,
    path = "/v1/webhooks/{id}",
    tag = "Webhooks",
    params(("id" = String, Path, description = "Webhook ID to delete")),
    responses(
        (status = 204, description = "Webhook deleted"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 500, description = "Internal server error"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn delete_webhook(
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<GatewayState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    require_scope(&auth_ctx, "admin")?;

    let webhook_store = state.webhook_store.as_ref().ok_or_else(|| {
        tracing::error!("webhook store not configured");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    webhook_store.delete(&id).await.map_err(|e| {
        tracing::error!(error = %e, webhook_id = %id, "failed to delete webhook");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::NO_CONTENT)
}
