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
