// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Webhook registration and event delivery for the gateway.
//!
//! Supports registering webhook endpoints with event filters and HMAC-SHA256
//! signed deliveries. Failed deliveries are retried with exponential backoff
//! and stored in a dead letter queue after exhausting all attempts.

pub mod delivery;
pub mod handlers;
pub mod store;

use serde::{Deserialize, Serialize};

/// A registered webhook with its secret (internal use only).
#[derive(Debug, Clone)]
pub struct Webhook {
    /// Unique webhook identifier.
    pub id: String,
    /// Delivery URL.
    pub url: String,
    /// HMAC secret (hex-encoded, 32 bytes).
    pub secret: String,
    /// Event types this webhook subscribes to.
    pub events: Vec<String>,
    /// Whether the webhook is active.
    pub active: bool,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last update timestamp.
    pub updated_at: String,
}

/// A webhook list item (never exposes the secret).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookListItem {
    /// Unique webhook identifier.
    pub id: String,
    /// Delivery URL.
    pub url: String,
    /// Event types this webhook subscribes to.
    pub events: Vec<String>,
    /// Whether the webhook is active.
    pub active: bool,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
}

/// Request body for creating a new webhook.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateWebhookRequest {
    /// Delivery URL (must be https:// or http://localhost for dev).
    pub url: String,
    /// Event types to subscribe to (e.g., "chat.completed", "tool.invoked").
    pub events: Vec<String>,
}

/// Response body after creating a webhook (shows secret once).
#[derive(Debug, Clone, Serialize)]
pub struct CreateWebhookResponse {
    /// Unique webhook identifier.
    pub id: String,
    /// Delivery URL.
    pub url: String,
    /// HMAC secret (shown once -- store securely).
    pub secret: String,
    /// Event types subscribed to.
    pub events: Vec<String>,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
}

/// A dead letter queue entry for failed webhook deliveries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterEntry {
    /// Auto-incremented ID.
    pub id: i64,
    /// Webhook that failed.
    pub webhook_id: String,
    /// Event type that triggered the delivery.
    pub event_type: String,
    /// Serialized payload that failed to deliver.
    pub payload: String,
    /// ISO 8601 timestamp of the last delivery attempt.
    pub last_attempt_at: String,
    /// Number of delivery attempts made.
    pub attempt_count: i64,
    /// Error message from the last attempt.
    pub last_error: Option<String>,
    /// ISO 8601 timestamp when the entry was created.
    pub created_at: String,
}

/// Payload sent to webhook endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    /// Event type (e.g., "chat.completed", "tool.invoked").
    pub event_type: String,
    /// ISO 8601 timestamp of the event.
    pub timestamp: String,
    /// Event-specific data.
    pub data: serde_json::Value,
}

/// Known webhook event types.
pub mod event_types {
    /// A chat completion finished.
    pub const CHAT_COMPLETED: &str = "chat.completed";
    /// A tool was invoked.
    pub const TOOL_INVOKED: &str = "tool.invoked";
    /// A batch completed processing.
    pub const BATCH_COMPLETED: &str = "batch.completed";
    /// A response completed.
    pub const RESPONSE_COMPLETED: &str = "response.completed";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_payload_serializes() {
        let payload = WebhookPayload {
            event_type: "chat.completed".into(),
            timestamp: "2026-03-06T12:00:00Z".into(),
            data: serde_json::json!({"model": "gpt-4o"}),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("chat.completed"));
        assert!(json.contains("gpt-4o"));
    }

    #[test]
    fn webhook_list_item_serializes() {
        let item = WebhookListItem {
            id: "wh-1".into(),
            url: "https://example.com/hook".into(),
            events: vec!["chat.completed".into()],
            active: true,
            created_at: "2026-03-06T12:00:00Z".into(),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("wh-1"));
        assert!(!json.contains("secret"));
    }

    #[test]
    fn create_webhook_response_includes_secret() {
        let resp = CreateWebhookResponse {
            id: "wh-1".into(),
            url: "https://example.com/hook".into(),
            secret: "abc123".into(),
            events: vec!["chat.completed".into()],
            created_at: "2026-03-06T12:00:00Z".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("abc123"));
    }

    #[test]
    fn event_type_constants() {
        assert_eq!(event_types::CHAT_COMPLETED, "chat.completed");
        assert_eq!(event_types::TOOL_INVOKED, "tool.invoked");
        assert_eq!(event_types::BATCH_COMPLETED, "batch.completed");
        assert_eq!(event_types::RESPONSE_COMPLETED, "response.completed");
    }
}
