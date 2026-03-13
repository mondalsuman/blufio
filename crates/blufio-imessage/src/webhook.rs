// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Webhook handler for incoming BlueBubbles events.
//!
//! Provides an Axum route at `/webhooks/imessage` that receives BlueBubbles
//! webhook payloads, validates the shared secret, filters tapbacks and
//! self-sent messages, and forwards valid messages as [`InboundMessage`]s.

use axum::{Router, extract::State, http::HeaderMap, http::StatusCode, response::IntoResponse};
use blufio_core::types::{InboundMessage, MessageContent};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::types::{BlueBubblesMessage, BlueBubblesWebhookPayload};

/// Shared state for the iMessage webhook handler.
#[derive(Clone)]
pub struct IMessageWebhookState {
    /// Sender for forwarding parsed inbound messages to the adapter.
    pub inbound_tx: mpsc::Sender<InboundMessage>,
    /// Shared secret for webhook validation (if set).
    pub webhook_secret: Option<String>,
    /// List of allowed contact addresses. Empty = accept all.
    pub allowed_contacts: Vec<String>,
    /// Trigger prefix for group chat messages (e.g., "Blufio").
    pub group_trigger: String,
}

/// Build the iMessage webhook router.
///
/// Returns a `Router` with a POST handler at `/webhooks/imessage`.
pub fn imessage_webhook_routes(state: IMessageWebhookState) -> Router {
    Router::new()
        .route(
            "/webhooks/imessage",
            axum::routing::post(imessage_webhook),
        )
        .with_state(state)
}

/// POST handler for incoming BlueBubbles webhook events.
///
/// Validates shared secret, filters non-message events and tapbacks,
/// and forwards valid messages via `inbound_tx`.
/// Always returns 200 OK (webhook best practice).
async fn imessage_webhook(
    State(state): State<IMessageWebhookState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Validate shared secret if configured.
    if let Some(ref expected_secret) = state.webhook_secret {
        let provided = headers
            .get("x-webhook-secret")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if provided != expected_secret.as_str() {
            warn!("iMessage webhook secret validation failed");
            return StatusCode::OK;
        }
    }

    // Parse the webhook payload.
    let payload: BlueBubblesWebhookPayload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            debug!(error = %e, "Failed to parse iMessage webhook payload");
            return StatusCode::OK;
        }
    };

    // Only process new-message events.
    if payload.type_field != "new-message" {
        debug!(event_type = %payload.type_field, "Ignoring non-message iMessage event");
        return StatusCode::OK;
    }

    // Parse the data field into a BlueBubblesMessage.
    let message: BlueBubblesMessage = match serde_json::from_value(payload.data) {
        Ok(m) => m,
        Err(e) => {
            debug!(error = %e, "Failed to parse BlueBubbles message data");
            return StatusCode::OK;
        }
    };

    // Ignore messages sent by ourselves.
    if message.is_from_me {
        debug!("Ignoring self-sent iMessage");
        return StatusCode::OK;
    }

    // Ignore tapback reactions (associated_message_type != 0).
    if let Some(amt) = message.associated_message_type {
        if amt != 0 {
            debug!(associated_message_type = amt, "Ignoring tapback reaction");
            return StatusCode::OK;
        }
    }

    // Extract sender address.
    let sender_address = match &message.handle {
        Some(handle) => handle.address.clone(),
        None => {
            debug!("Ignoring iMessage with no sender handle");
            return StatusCode::OK;
        }
    };

    // Check allowed contacts filter.
    if !state.allowed_contacts.is_empty()
        && !state.allowed_contacts.contains(&sender_address)
    {
        debug!(sender = %sender_address, "Ignoring message from non-allowed contact");
        return StatusCode::OK;
    }

    // Extract message text.
    let mut text = match &message.text {
        Some(t) if !t.trim().is_empty() => t.clone(),
        _ => {
            debug!("Ignoring iMessage with empty or missing text");
            return StatusCode::OK;
        }
    };

    let chat_guid = message.chat_guid.clone().unwrap_or_default();

    // Group chat handling: check for trigger prefix.
    let is_group = chat_guid.contains(";+;");
    if is_group {
        if !state.group_trigger.is_empty() {
            if let Some(stripped) = text.strip_prefix(&state.group_trigger) {
                text = stripped.trim_start().to_string();
            } else {
                debug!("Ignoring group chat message without trigger prefix");
                return StatusCode::OK;
            }
        }
    }

    // Build metadata JSON.
    let metadata = serde_json::json!({
        "chat_guid": chat_guid,
        "message_guid": message.guid,
        "is_group": is_group,
    });

    let timestamp = message
        .date_created
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let inbound = InboundMessage {
        id: message.guid.clone(),
        session_id: None,
        channel: "imessage".to_string(),
        sender_id: sender_address,
        content: MessageContent::Text(text),
        metadata: Some(metadata.to_string()),
        timestamp,
    };

    if state.inbound_tx.send(inbound).await.is_err() {
        warn!("iMessage inbound channel full, dropping message");
    }

    StatusCode::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_state_is_clone() {
        let (tx, _rx) = mpsc::channel(1);
        let state = IMessageWebhookState {
            inbound_tx: tx,
            webhook_secret: Some("secret".into()),
            allowed_contacts: vec![],
            group_trigger: "Blufio".into(),
        };
        let _cloned = state.clone();
    }
}
