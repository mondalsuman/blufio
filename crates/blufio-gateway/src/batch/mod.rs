// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Batch request processing for the gateway.
//!
//! Allows submitting multiple chat completion requests in a single API call,
//! executed in parallel with configurable concurrency control.

pub mod handlers;
pub mod processor;
pub mod store;

use serde::{Deserialize, Serialize};

/// Request body for submitting a batch of chat completion requests.
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct BatchRequest {
    /// Individual chat completion requests to process.
    pub items: Vec<serde_json::Value>,
}

/// Response returned immediately when a batch is submitted (202 Accepted).
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct BatchSubmitResponse {
    /// Unique batch identifier for polling status.
    pub id: String,
    /// Status (always "processing" on submit).
    pub status: String,
    /// Total number of items in the batch.
    pub total_items: usize,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
}

/// Full batch status response (returned by GET /v1/batch/:id).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BatchResponse {
    /// Unique batch identifier.
    pub id: String,
    /// Batch status: "processing", "completed", or "failed".
    pub status: String,
    /// Total number of items.
    pub total_items: usize,
    /// Number of successfully completed items.
    pub completed_items: usize,
    /// Number of failed items.
    pub failed_items: usize,
    /// Per-item results (populated when batch is complete).
    pub items: Option<Vec<BatchItemResult>>,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 completion timestamp (None if still processing).
    pub completed_at: Option<String>,
    /// API key ID that submitted the batch (None for master tokens).
    pub api_key_id: Option<String>,
}

/// Result of a single batch item.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BatchItemResult {
    /// Index of the item in the original request array.
    pub index: usize,
    /// Item status: "completed" or "failed".
    pub status: String,
    /// Response data (if completed).
    pub response: Option<serde_json::Value>,
    /// Error message (if failed).
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_submit_response_serializes() {
        let resp = BatchSubmitResponse {
            id: "batch-1".into(),
            status: "processing".into(),
            total_items: 5,
            created_at: "2026-03-06T12:00:00Z".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("batch-1"));
        assert!(json.contains("processing"));
    }

    #[test]
    fn batch_response_serializes() {
        let resp = BatchResponse {
            id: "batch-1".into(),
            status: "completed".into(),
            total_items: 2,
            completed_items: 1,
            failed_items: 1,
            items: Some(vec![
                BatchItemResult {
                    index: 0,
                    status: "completed".into(),
                    response: Some(serde_json::json!({"choices": []})),
                    error: None,
                },
                BatchItemResult {
                    index: 1,
                    status: "failed".into(),
                    response: None,
                    error: Some("model not found".into()),
                },
            ]),
            created_at: "2026-03-06T12:00:00Z".into(),
            completed_at: Some("2026-03-06T12:01:00Z".into()),
            api_key_id: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("completed"));
        assert!(json.contains("model not found"));
    }

    #[test]
    fn batch_request_deserializes() {
        let json = r#"{"items": [{"model": "gpt-4o", "messages": []}]}"#;
        let req: BatchRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.items.len(), 1);
    }

    #[test]
    fn batch_item_result_completed() {
        let item = BatchItemResult {
            index: 0,
            status: "completed".into(),
            response: Some(serde_json::json!({"id": "cmpl-1"})),
            error: None,
        };
        assert_eq!(item.status, "completed");
        assert!(item.error.is_none());
    }

    #[test]
    fn batch_item_result_failed() {
        let item = BatchItemResult {
            index: 1,
            status: "failed".into(),
            response: None,
            error: Some("rate limit exceeded".into()),
        };
        assert_eq!(item.status, "failed");
        assert!(item.response.is_none());
    }
}
