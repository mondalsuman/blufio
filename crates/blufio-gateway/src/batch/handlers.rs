// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTP handlers for batch processing endpoints.

use std::sync::Arc;

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};

use super::processor::{self, DEFAULT_CONCURRENCY};
use super::{BatchRequest, BatchResponse, BatchSubmitResponse};
use crate::api_keys::{AuthContext, require_scope};
use crate::server::GatewayState;

/// POST /v1/batch -- Submit a batch of chat completion requests.
///
/// Requires chat.completions, admin scope, or master auth.
/// Returns 202 Accepted with batch_id for status polling.
pub async fn post_create_batch(
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<GatewayState>,
    Json(req): Json<BatchRequest>,
) -> Result<(StatusCode, Json<BatchSubmitResponse>), (StatusCode, Json<serde_json::Value>)> {
    require_scope(&auth_ctx, "chat.completions").map_err(|status| {
        (
            status,
            Json(serde_json::json!({"error": "insufficient scope: chat.completions required"})),
        )
    })?;

    // Validate batch size.
    if req.items.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "batch must contain at least one item"})),
        ));
    }

    if req.items.len() > state.max_batch_size {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("batch exceeds maximum size of {} items", state.max_batch_size)
            })),
        ));
    }

    let batch_store = state.batch_store.as_ref().ok_or_else(|| {
        tracing::error!("batch store not configured");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "batch processing not configured"})),
        )
    })?;

    let providers = state.providers.as_ref().ok_or_else(|| {
        tracing::error!("providers not configured for batch processing");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "no providers available"})),
        )
    })?;

    // Create batch in store.
    let batch_id = batch_store
        .create_batch(&req.items, auth_ctx.key_id())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to create batch");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "failed to create batch"})),
            )
        })?;

    let now = chrono::Utc::now().to_rfc3339();
    let total_items = req.items.len();

    // Spawn background processing task.
    let batch_id_c = batch_id.clone();
    let store = Arc::clone(batch_store);
    let providers = Arc::clone(providers);
    let bus = state.event_bus.clone();

    tokio::spawn(async move {
        processor::process_batch(
            batch_id_c,
            req.items,
            providers,
            store,
            bus,
            auth_ctx,
            DEFAULT_CONCURRENCY,
        )
        .await;
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(BatchSubmitResponse {
            id: batch_id,
            status: "processing".into(),
            total_items,
            created_at: now,
        }),
    ))
}

/// GET /v1/batch/:id -- Get batch status and results.
///
/// For scoped keys, verifies the batch was submitted by the same key.
pub async fn get_batch_status(
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<GatewayState>,
    Path(batch_id): Path<String>,
) -> Result<Json<BatchResponse>, StatusCode> {
    let batch_store = state.batch_store.as_ref().ok_or_else(|| {
        tracing::error!("batch store not configured");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let batch = batch_store
        .get_batch(&batch_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to get batch");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // For scoped keys, verify the batch was submitted by the same key.
    if let Some(caller_key_id) = auth_ctx.key_id()
        && let Some(ref batch_key_id) = batch.api_key_id
        && caller_key_id != batch_key_id
    {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(Json(batch))
}
