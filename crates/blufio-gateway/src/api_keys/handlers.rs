// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTP handlers for API key management endpoints.

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};

use super::{AuthContext, CreateKeyRequest, CreateKeyResponse, require_scope};
use crate::server::GatewayState;

/// POST /v1/api-keys -- Create a new scoped API key.
///
/// Requires admin scope or master auth. Returns the raw key once.
#[utoipa::path(
    post,
    path = "/v1/api-keys",
    tag = "API Keys",
    request_body = CreateKeyRequest,
    responses(
        (status = 201, description = "API key created", body = CreateKeyResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 500, description = "Internal server error"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn post_create_api_key(
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<GatewayState>,
    Json(req): Json<CreateKeyRequest>,
) -> Result<(StatusCode, Json<CreateKeyResponse>), StatusCode> {
    require_scope(&auth_ctx, "admin")?;

    let key_store = state.auth.key_store.as_ref().ok_or_else(|| {
        tracing::error!("API key store not configured");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let resp = key_store.create(&req).await.map_err(|e| {
        tracing::error!(error = %e, "failed to create API key");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /v1/api-keys -- List all API keys.
///
/// Requires admin scope or master auth. Never exposes key hashes.
#[utoipa::path(
    get,
    path = "/v1/api-keys",
    tag = "API Keys",
    responses(
        (status = 200, description = "List of API keys", body = Vec<super::ApiKey>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 500, description = "Internal server error"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_list_api_keys(
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<GatewayState>,
) -> Result<Json<Vec<super::ApiKey>>, StatusCode> {
    require_scope(&auth_ctx, "admin")?;

    let key_store = state.auth.key_store.as_ref().ok_or_else(|| {
        tracing::error!("API key store not configured");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let keys = key_store.list().await.map_err(|e| {
        tracing::error!(error = %e, "failed to list API keys");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(keys))
}

/// DELETE /v1/api-keys/:id -- Revoke an API key.
///
/// Requires admin scope or master auth. Revokes rather than deletes
/// so the key is immediately rejected on all endpoints.
#[utoipa::path(
    delete,
    path = "/v1/api-keys/{id}",
    tag = "API Keys",
    params(("id" = String, Path, description = "API key ID to revoke")),
    responses(
        (status = 204, description = "Key revoked"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 500, description = "Internal server error"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn delete_api_key(
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<GatewayState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    require_scope(&auth_ctx, "admin")?;

    let key_store = state.auth.key_store.as_ref().ok_or_else(|| {
        tracing::error!("API key store not configured");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    key_store.revoke(&id).await.map_err(|e| {
        tracing::error!(error = %e, key_id = %id, "failed to revoke API key");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::NO_CONTENT)
}
