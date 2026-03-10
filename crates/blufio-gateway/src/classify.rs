// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! REST API endpoints for data classification management.
//!
//! Provides endpoints for setting, querying, and bulk-updating classification
//! levels on memories, messages, and sessions. All endpoints require the
//! `classify` scope.
//!
//! # Endpoints
//!
//! - `PUT /v1/classify/{type}/{id}` -- Set classification on an entity
//! - `GET /v1/classify/{type}/{id}` -- Get current classification
//! - `POST /v1/classify/bulk` -- Bulk update classifications with filters

use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{post, put},
};
use serde::{Deserialize, Serialize};

use crate::api_keys::AuthContext;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Request body for setting a classification level.
#[derive(Debug, Deserialize)]
pub struct SetClassificationRequest {
    /// Target classification level (public, internal, confidential, restricted).
    pub level: String,
    /// Force downgrade (required when lowering classification level).
    #[serde(default)]
    pub force: Option<bool>,
}

/// Response body for a successful classification set.
#[derive(Debug, Serialize)]
pub struct SetClassificationResponse {
    /// Operation status.
    pub status: String,
    /// Applied classification level.
    pub level: String,
}

/// Response body for a classification get.
#[derive(Debug, Serialize)]
pub struct GetClassificationResponse {
    /// Current classification level.
    pub level: String,
}

/// Filters for bulk classification operations.
#[derive(Debug, Deserialize)]
pub struct BulkFilters {
    /// Filter by session ID.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Filter by creation date (from, ISO 8601).
    #[serde(default)]
    pub from: Option<String>,
    /// Filter by creation date (to, ISO 8601).
    #[serde(default)]
    pub to: Option<String>,
    /// Filter by current classification level.
    #[serde(default)]
    pub current_level: Option<String>,
    /// Filter by content pattern (regex or substring).
    #[serde(default)]
    pub pattern: Option<String>,
}

/// Request body for bulk classification update.
#[derive(Debug, Deserialize)]
pub struct BulkClassificationRequest {
    /// Entity type: memory, message, or session.
    pub entity_type: String,
    /// Target classification level.
    pub level: String,
    /// Force downgrades.
    #[serde(default)]
    pub force: Option<bool>,
    /// Filters for selecting entities.
    #[serde(default)]
    pub filters: Option<BulkFilters>,
    /// Preview mode: show what would change without modifying.
    #[serde(default)]
    pub dry_run: Option<bool>,
}

/// Response body for bulk classification update.
#[derive(Debug, Serialize)]
pub struct BulkClassificationResponse {
    /// Total number of entities matching filters.
    pub total: usize,
    /// Number of successfully updated entities.
    pub succeeded: usize,
    /// Number of failed updates.
    pub failed: usize,
    /// Error messages for failed updates.
    pub errors: Vec<String>,
}

/// Error response body.
#[derive(Debug, Serialize)]
pub struct ClassifyErrorResponse {
    /// Error message.
    pub error: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Validate entity type string.
fn validate_entity_type(entity_type: &str) -> Result<(), (StatusCode, Json<ClassifyErrorResponse>)> {
    match entity_type {
        "memory" | "message" | "session" => Ok(()),
        _ => Err((
            StatusCode::BAD_REQUEST,
            Json(ClassifyErrorResponse {
                error: format!(
                    "invalid entity type '{}' (expected: memory, message, session)",
                    entity_type
                ),
            }),
        )),
    }
}

/// Parse and validate a classification level string.
fn validate_level(level: &str) -> Result<blufio_core::classification::DataClassification, (StatusCode, Json<ClassifyErrorResponse>)> {
    blufio_core::classification::DataClassification::from_str_value(level).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ClassifyErrorResponse {
                error: format!(
                    "invalid classification level '{}' (expected: public, internal, confidential, restricted)",
                    level
                ),
            }),
        )
    })
}

/// Require 'classify' scope from auth context.
fn require_classify_scope(
    auth: &AuthContext,
) -> Result<(), (StatusCode, Json<ClassifyErrorResponse>)> {
    if auth.has_scope("classify") {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(ClassifyErrorResponse {
                error: "insufficient scope: 'classify' required".to_string(),
            }),
        ))
    }
}

/// Extract the storage adapter from gateway state, returning 503 if unavailable.
fn require_storage(
    state: &crate::server::GatewayState,
) -> Result<Arc<dyn blufio_core::StorageAdapter + Send + Sync>, (StatusCode, Json<ClassifyErrorResponse>)>
{
    state.storage.clone().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ClassifyErrorResponse {
                error: "storage not available".to_string(),
            }),
        )
    })
}

/// PUT /v1/classify/{type}/{id}
///
/// Set classification level on an entity. Returns 409 if downgrade without force.
async fn put_classification(
    State(state): State<crate::server::GatewayState>,
    Path((entity_type, id)): Path<(String, String)>,
    Extension(auth): Extension<AuthContext>,
    Json(body): Json<SetClassificationRequest>,
) -> Result<Json<SetClassificationResponse>, (StatusCode, Json<ClassifyErrorResponse>)> {
    require_classify_scope(&auth)?;
    validate_entity_type(&entity_type)?;
    let new_level = validate_level(&body.level)?;
    let storage = require_storage(&state)?;

    // Fetch current level from DB.
    let current_str = storage
        .get_entity_classification(&entity_type, &id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ClassifyErrorResponse {
                    error: format!("storage error: {e}"),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ClassifyErrorResponse {
                    error: format!("{entity_type} not found: {id}"),
                }),
            )
        })?;

    let current_level = validate_level(&current_str)?;

    if new_level.is_downgrade_from(&current_level) && !body.force.unwrap_or(false) {
        return Err((
            StatusCode::CONFLICT,
            Json(ClassifyErrorResponse {
                error: format!(
                    "classification downgrade rejected: cannot change from {} to {} (use force: true to override)",
                    current_level.as_str(),
                    new_level.as_str()
                ),
            }),
        ));
    }

    // Persist the new classification level.
    storage
        .set_entity_classification(&entity_type, &id, new_level.as_str())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ClassifyErrorResponse {
                    error: format!("storage error: {e}"),
                }),
            )
        })?;

    // Emit event (fire-and-forget).
    let event = blufio_security::classification_changed_event(
        &entity_type,
        &id,
        current_level.as_str(),
        new_level.as_str(),
        auth.key_id().unwrap_or("master"),
    );

    // Publish event to EventBus if available.
    if let Some(ref bus) = state.event_bus {
        let _ = bus.publish(event).await;
    }

    tracing::info!(
        entity_type = %entity_type,
        entity_id = %id,
        level = %new_level.as_str(),
        "classification set via API"
    );

    Ok(Json(SetClassificationResponse {
        status: "ok".to_string(),
        level: new_level.as_str().to_string(),
    }))
}

/// GET /v1/classify/{type}/{id}
///
/// Get current classification level. Returns 404 if entity not found.
async fn get_classification(
    State(state): State<crate::server::GatewayState>,
    Path((entity_type, id)): Path<(String, String)>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<GetClassificationResponse>, (StatusCode, Json<ClassifyErrorResponse>)> {
    require_classify_scope(&auth)?;
    validate_entity_type(&entity_type)?;
    let storage = require_storage(&state)?;

    let level_str = storage
        .get_entity_classification(&entity_type, &id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ClassifyErrorResponse {
                    error: format!("storage error: {e}"),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ClassifyErrorResponse {
                    error: format!("{entity_type} not found: {id}"),
                }),
            )
        })?;

    Ok(Json(GetClassificationResponse { level: level_str }))
}

/// POST /v1/classify/bulk
///
/// Bulk update classifications with filters and optional dry-run.
async fn post_bulk_classification(
    State(state): State<crate::server::GatewayState>,
    Extension(auth): Extension<AuthContext>,
    Json(body): Json<BulkClassificationRequest>,
) -> Result<Json<BulkClassificationResponse>, (StatusCode, Json<ClassifyErrorResponse>)> {
    require_classify_scope(&auth)?;
    validate_entity_type(&body.entity_type)?;
    let new_level = validate_level(&body.level)?;

    // Validate current_level filter if provided.
    if let Some(ref filters) = body.filters
        && let Some(ref cl) = filters.current_level
    {
        validate_level(cl)?;
    }

    let dry_run = body.dry_run.unwrap_or(false);

    // Check downgrade protection when current_level filter is set.
    if let Some(ref filters) = body.filters
        && let Some(ref cl) = filters.current_level
    {
        let current = validate_level(cl)?;
        if new_level.is_downgrade_from(&current) && !body.force.unwrap_or(false) {
            return Err((
                StatusCode::CONFLICT,
                Json(ClassifyErrorResponse {
                    error: format!(
                        "bulk downgrade rejected: cannot change from {} to {} (use force: true)",
                        current.as_str(),
                        new_level.as_str()
                    ),
                }),
            ));
        }
    }

    let storage = require_storage(&state)?;

    // Extract filter parameters.
    let filters = body.filters.as_ref();
    let current_level = filters.and_then(|f| f.current_level.as_deref());
    let session_id = filters.and_then(|f| f.session_id.as_deref());
    let from_date = filters.and_then(|f| f.from.as_deref());
    let to_date = filters.and_then(|f| f.to.as_deref());
    let pattern = filters.and_then(|f| f.pattern.as_deref());

    let result = storage
        .bulk_update_classification(
            &body.entity_type,
            new_level.as_str(),
            current_level,
            session_id,
            from_date,
            to_date,
            pattern,
            dry_run,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ClassifyErrorResponse {
                    error: format!("storage error: {e}"),
                }),
            )
        })?;

    let (total, succeeded, failed, errors) = result;

    // Emit bulk event if any entities were updated.
    if succeeded > 0 {
        let event = blufio_security::bulk_classification_changed_event(
            &body.entity_type,
            succeeded,
            current_level.unwrap_or("mixed"),
            new_level.as_str(),
            auth.key_id().unwrap_or("master"),
        );
        if let Some(ref bus) = state.event_bus {
            let _ = bus.publish(event).await;
        }
    }

    Ok(Json(BulkClassificationResponse {
        total,
        succeeded,
        failed,
        errors,
    }))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Creates the classification API router.
///
/// All routes require authentication and the `classify` scope.
/// Mount on the gateway by merging with `classify_router()`.
pub fn classify_router() -> Router<crate::server::GatewayState> {
    Router::new()
        .route(
            "/v1/classify/{entity_type}/{id}",
            put(put_classification).get(get_classification),
        )
        .route("/v1/classify/bulk", post(post_bulk_classification))
}

#[cfg(test)]
mod tests {
    use super::*;
    use blufio_core::classification::DataClassification;

    #[test]
    fn validate_entity_type_valid() {
        assert!(validate_entity_type("memory").is_ok());
        assert!(validate_entity_type("message").is_ok());
        assert!(validate_entity_type("session").is_ok());
    }

    #[test]
    fn validate_entity_type_invalid() {
        let result = validate_entity_type("unknown");
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validate_level_valid() {
        assert_eq!(
            validate_level("public").unwrap(),
            DataClassification::Public
        );
        assert_eq!(
            validate_level("restricted").unwrap(),
            DataClassification::Restricted
        );
    }

    #[test]
    fn validate_level_invalid() {
        let result = validate_level("invalid");
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn require_classify_scope_master() {
        let auth = AuthContext::Master;
        assert!(require_classify_scope(&auth).is_ok());
    }

    #[test]
    fn require_classify_scope_with_scope() {
        let auth = AuthContext::Scoped {
            key_id: "k1".into(),
            scopes: vec!["classify".into()],
            rate_limit: 60,
        };
        assert!(require_classify_scope(&auth).is_ok());
    }

    #[test]
    fn require_classify_scope_without_scope() {
        let auth = AuthContext::Scoped {
            key_id: "k1".into(),
            scopes: vec!["chat.completions".into()],
            rate_limit: 60,
        };
        let result = require_classify_scope(&auth);
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[test]
    fn set_classification_request_deserialize() {
        let json = r#"{"level": "confidential"}"#;
        let req: SetClassificationRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.level, "confidential");
        assert!(req.force.is_none());
    }

    #[test]
    fn set_classification_request_with_force() {
        let json = r#"{"level": "public", "force": true}"#;
        let req: SetClassificationRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.level, "public");
        assert_eq!(req.force, Some(true));
    }

    #[test]
    fn bulk_request_deserialize() {
        let json = r#"{
            "entity_type": "memory",
            "level": "restricted",
            "dry_run": true,
            "filters": {
                "current_level": "internal",
                "session_id": "sess-1"
            }
        }"#;
        let req: BulkClassificationRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.entity_type, "memory");
        assert_eq!(req.level, "restricted");
        assert_eq!(req.dry_run, Some(true));
        let filters = req.filters.unwrap();
        assert_eq!(filters.current_level.unwrap(), "internal");
        assert_eq!(filters.session_id.unwrap(), "sess-1");
    }

    #[test]
    fn bulk_response_serialize() {
        let resp = BulkClassificationResponse {
            total: 10,
            succeeded: 8,
            failed: 2,
            errors: vec!["entity mem-1 not found".to_string()],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"total\":10"));
        assert!(json.contains("\"succeeded\":8"));
        assert!(json.contains("\"failed\":2"));
    }

    #[test]
    fn classify_router_creates_router() {
        let _router = classify_router();
    }
}
