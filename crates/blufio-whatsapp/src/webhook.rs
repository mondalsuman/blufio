// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Axum route handlers for WhatsApp Cloud API webhooks.
//!
//! Provides GET (subscription verification) and POST (incoming messages)
//! handlers for the `/webhooks/whatsapp` route.

use axum::{
    Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
};
use blufio_core::types::{InboundMessage, MessageContent};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::types::{WhatsAppVerifyParams, WhatsAppWebhookPayload};

type HmacSha256 = Hmac<Sha256>;

/// Shared state for WhatsApp webhook handlers.
#[derive(Clone)]
pub struct WhatsAppWebhookState {
    /// Sender for forwarding parsed inbound messages to the adapter.
    pub inbound_tx: mpsc::Sender<InboundMessage>,
    /// Expected verification token for subscription validation.
    pub verify_token: String,
    /// App secret for HMAC-SHA256 webhook signature verification.
    pub app_secret: String,
}

/// GET handler for WhatsApp webhook subscription verification.
///
/// Meta sends a GET request with `hub.mode`, `hub.verify_token`, and
/// `hub.challenge` query parameters. Returns the challenge on success.
pub async fn whatsapp_verify(
    State(state): State<WhatsAppWebhookState>,
    Query(params): Query<WhatsAppVerifyParams>,
) -> impl IntoResponse {
    if params.hub_mode == "subscribe" && params.hub_verify_token == state.verify_token {
        debug!("WhatsApp webhook subscription verified");
        (StatusCode::OK, params.hub_challenge)
    } else {
        warn!("WhatsApp webhook verification failed: invalid token or mode");
        (StatusCode::FORBIDDEN, String::new())
    }
}

/// POST handler for incoming WhatsApp messages.
///
/// Verifies the `X-Hub-Signature-256` HMAC-SHA256 signature, parses the
/// payload, and forwards text messages as [`InboundMessage`]s.
/// Always returns 200 (Meta retries on non-200).
pub async fn whatsapp_webhook(
    State(state): State<WhatsAppWebhookState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Verify HMAC-SHA256 signature.
    let signature = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !verify_signature(&state.app_secret, &body, signature) {
        warn!("WhatsApp webhook signature verification failed");
        return StatusCode::UNAUTHORIZED;
    }

    // Parse payload.
    let payload: WhatsAppWebhookPayload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, "failed to parse WhatsApp webhook payload");
            return StatusCode::OK; // Still return 200 to prevent retries.
        }
    };

    // Process messages.
    for entry in &payload.entry {
        for change in &entry.changes {
            let Some(ref messages) = change.value.messages else {
                continue;
            };

            let phone_number_id = change
                .value
                .metadata
                .as_ref()
                .map(|m| m.phone_number_id.clone())
                .unwrap_or_default();

            // Build contact name lookup.
            let contacts = change.value.contacts.as_deref().unwrap_or(&[]);

            for msg in messages {
                // Only process text messages for now.
                if msg.msg_type != "text" {
                    debug!(msg_type = %msg.msg_type, "skipping non-text WhatsApp message");
                    continue;
                }

                let Some(ref text) = msg.text else {
                    continue;
                };

                // Look up sender name from contacts.
                let sender_name = contacts
                    .iter()
                    .find(|c| c.wa_id == msg.from)
                    .map(|c| c.profile.name.clone())
                    .unwrap_or_else(|| msg.from.clone());

                let metadata = serde_json::json!({
                    "chat_id": msg.from,
                    "phone_number_id": phone_number_id,
                    "sender_name": sender_name,
                });

                let inbound = InboundMessage {
                    id: msg.id.clone(),
                    session_id: None,
                    channel: "whatsapp".to_string(),
                    sender_id: msg.from.clone(),
                    content: MessageContent::Text(text.body.clone()),
                    metadata: Some(metadata.to_string()),
                    timestamp: msg.timestamp.clone(),
                };

                if state.inbound_tx.send(inbound).await.is_err() {
                    warn!("WhatsApp inbound channel full, dropping message");
                }
            }
        }
    }

    StatusCode::OK
}

/// Verify HMAC-SHA256 signature of the webhook payload.
///
/// The signature header is formatted as `sha256=<hex_digest>`.
fn verify_signature(app_secret: &str, body: &[u8], signature: &str) -> bool {
    let expected_prefix = "sha256=";
    let hex_digest = match signature.strip_prefix(expected_prefix) {
        Some(h) => h,
        None => return false,
    };

    let Ok(expected_bytes) = hex::decode(hex_digest) else {
        return false;
    };

    let mut mac =
        HmacSha256::new_from_slice(app_secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(body);

    mac.verify_slice(&expected_bytes).is_ok()
}

/// Build the WhatsApp webhook router.
///
/// Returns a `Router` with GET and POST handlers at `/webhooks/whatsapp`.
pub fn whatsapp_webhook_routes(state: WhatsAppWebhookState) -> Router {
    Router::new()
        .route(
            "/webhooks/whatsapp",
            get(whatsapp_verify).post(whatsapp_webhook),
        )
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_valid_signature() {
        let secret = "test-secret";
        let body = b"hello world";

        // Compute expected signature.
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let result = mac.finalize();
        let hex_sig = hex::encode(result.into_bytes());
        let signature = format!("sha256={hex_sig}");

        assert!(verify_signature(secret, body, &signature));
    }

    #[test]
    fn reject_tampered_body() {
        let secret = "test-secret";
        let body = b"hello world";

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let result = mac.finalize();
        let hex_sig = hex::encode(result.into_bytes());
        let signature = format!("sha256={hex_sig}");

        // Verify with tampered body.
        assert!(!verify_signature(secret, b"tampered body", &signature));
    }

    #[test]
    fn reject_missing_prefix() {
        assert!(!verify_signature("secret", b"body", "not-sha256=abc"));
    }

    #[test]
    fn reject_invalid_hex() {
        assert!(!verify_signature(
            "secret",
            b"body",
            "sha256=not-valid-hex!"
        ));
    }
}
