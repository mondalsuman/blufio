// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Webhook handler for incoming Twilio SMS events.
//!
//! Provides an Axum route at `/webhooks/sms` that receives Twilio webhook
//! payloads, validates X-Twilio-Signature HMAC-SHA1, filters STOP keywords
//! and MMS, and forwards valid messages as [`InboundMessage`]s.
//!
//! CRITICAL: Twilio uses HMAC-SHA1 with Base64 encoding -- NOT SHA256/hex
//! like the WhatsApp webhook handler.

use axum::{Router, extract::State, http::HeaderMap, http::StatusCode, response::IntoResponse};
use base64::Engine;
use blufio_core::types::{InboundMessage, MessageContent};
use hmac::{Hmac, Mac};
use sha1::Sha1;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

type HmacSha1 = Hmac<Sha1>;

/// Shared state for the SMS webhook handler.
#[derive(Clone)]
pub struct SmsWebhookState {
    /// Sender for forwarding parsed inbound messages to the adapter.
    pub inbound_tx: mpsc::Sender<InboundMessage>,
    /// Twilio Auth Token for HMAC-SHA1 signature validation.
    pub auth_token: String,
    /// Full webhook URL for signature computation.
    pub webhook_url: String,
    /// List of allowed phone numbers. Empty = accept all.
    pub allowed_numbers: Vec<String>,
}

/// Build the SMS webhook router.
///
/// Returns a `Router` with a POST handler at `/webhooks/sms`.
pub fn sms_webhook_routes(state: SmsWebhookState) -> Router {
    Router::new()
        .route("/webhooks/sms", axum::routing::post(sms_webhook))
        .with_state(state)
}

/// Validate a Twilio webhook signature using HMAC-SHA1 + Base64.
///
/// Algorithm (per Twilio docs):
/// 1. Start with the full URL (including scheme and query params)
/// 2. Sort POST parameters alphabetically by key
/// 3. Append each key+value to the URL string
/// 4. HMAC-SHA1 with auth_token as key
/// 5. Base64-encode the result
/// 6. Compare with X-Twilio-Signature header
pub fn validate_twilio_signature(
    auth_token: &str,
    url: &str,
    params: &[(String, String)],
    signature: &str,
) -> bool {
    let mut data = url.to_string();

    // Sort parameters alphabetically by key.
    let mut sorted_params = params.to_vec();
    sorted_params.sort_by(|a, b| a.0.cmp(&b.0));

    // Append key+value pairs.
    for (key, value) in &sorted_params {
        data.push_str(key);
        data.push_str(value);
    }

    let mut mac =
        HmacSha1::new_from_slice(auth_token.as_bytes()).expect("HMAC accepts any key length");
    mac.update(data.as_bytes());
    let result = mac.finalize();

    let computed = base64::engine::general_purpose::STANDARD.encode(result.into_bytes());

    computed == signature
}

/// STOP/UNSUBSCRIBE keywords that Twilio recognizes.
const STOP_KEYWORDS: &[&str] = &["STOP", "STOPALL", "UNSUBSCRIBE", "CANCEL", "END", "QUIT"];

/// Check if a message body contains a STOP/unsubscribe keyword.
fn is_stop_keyword(body: &str) -> bool {
    let normalized = body.trim().to_uppercase();
    STOP_KEYWORDS.contains(&normalized.as_str())
}

/// POST handler for incoming Twilio SMS webhooks.
///
/// Validates HMAC-SHA1 signature, filters STOP keywords and MMS,
/// and forwards valid messages via `inbound_tx`.
///
/// CRITICAL: Returns empty 200 OK body. Non-empty body is interpreted
/// as TwiML by Twilio and would send an unexpected reply to the user.
async fn sms_webhook(
    State(state): State<SmsWebhookState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Extract X-Twilio-Signature header.
    let signature = headers
        .get("x-twilio-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // Parse body as application/x-www-form-urlencoded.
    let params: Vec<(String, String)> = match serde_urlencoded::from_bytes(&body) {
        Ok(p) => p,
        Err(e) => {
            debug!(error = %e, "Failed to parse SMS webhook form body");
            return StatusCode::OK;
        }
    };

    // Validate Twilio signature.
    if !validate_twilio_signature(&state.auth_token, &state.webhook_url, &params, signature) {
        warn!("Twilio webhook signature validation failed");
        return StatusCode::OK;
    }

    // Extract standard Twilio fields from params.
    let get_param = |key: &str| -> Option<String> {
        params
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    };

    let message_sid = match get_param("MessageSid") {
        Some(s) => s,
        None => {
            debug!("SMS webhook missing MessageSid");
            return StatusCode::OK;
        }
    };

    let from = match get_param("From") {
        Some(f) => f,
        None => {
            debug!("SMS webhook missing From");
            return StatusCode::OK;
        }
    };

    let to = get_param("To").unwrap_or_default();

    let body_text = match get_param("Body") {
        Some(b) => b,
        None => {
            debug!("SMS webhook missing Body");
            return StatusCode::OK;
        }
    };

    let num_media = get_param("NumMedia").unwrap_or_else(|| "0".to_string());

    // Check STOP/UNSUBSCRIBE keywords.
    if is_stop_keyword(&body_text) {
        info!(from = %from, keyword = %body_text.trim(), "STOP keyword received, skipping processing");
        return StatusCode::OK;
    }

    // Ignore MMS (messages with media).
    if num_media != "0" {
        debug!(num_media = %num_media, "Ignoring MMS message");
        return StatusCode::OK;
    }

    // Check allowed numbers filter.
    if !state.allowed_numbers.is_empty() && !state.allowed_numbers.contains(&from) {
        debug!(from = %from, "Ignoring SMS from non-allowed number");
        return StatusCode::OK;
    }

    // Build metadata JSON.
    let metadata = serde_json::json!({
        "MessageSid": message_sid,
        "From": from,
        "To": to,
    });

    let timestamp = chrono::Utc::now().to_rfc3339();

    let inbound = InboundMessage {
        id: message_sid,
        session_id: None,
        channel: "sms".to_string(),
        sender_id: from,
        content: MessageContent::Text(body_text),
        metadata: Some(metadata.to_string()),
        timestamp,
    };

    if state.inbound_tx.send(inbound).await.is_err() {
        warn!("SMS inbound channel full, dropping message");
    }

    StatusCode::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_twilio_signature_valid() {
        // Known test vector from Twilio documentation.
        // Auth token: "12345"
        // URL: "https://mycompany.com/myapp.php?foo=1&bar=2"
        // POST params (alphabetically sorted for data construction):
        //   CallSid=CA1234567890ABCDE, Caller=+14158675310, Digits=1234,
        //   From=+14158675310, To=+18005551212
        let auth_token = "12345";
        let url = "https://mycompany.com/myapp.php?foo=1&bar=2";
        let params = vec![
            ("CallSid".to_string(), "CA1234567890ABCDE".to_string()),
            ("Caller".to_string(), "+14158675310".to_string()),
            ("Digits".to_string(), "1234".to_string()),
            ("From".to_string(), "+14158675310".to_string()),
            ("To".to_string(), "+18005551212".to_string()),
        ];

        // Compute expected signature manually.
        let mut data = url.to_string();
        let mut sorted = params.clone();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        for (key, value) in &sorted {
            data.push_str(key);
            data.push_str(value);
        }
        let mut mac =
            HmacSha1::new_from_slice(auth_token.as_bytes()).expect("HMAC accepts any key length");
        mac.update(data.as_bytes());
        let result = mac.finalize();
        let expected_signature =
            base64::engine::general_purpose::STANDARD.encode(result.into_bytes());

        assert!(validate_twilio_signature(
            auth_token,
            url,
            &params,
            &expected_signature,
        ));
    }

    #[test]
    fn test_validate_twilio_signature_invalid() {
        let auth_token = "12345";
        let url = "https://mycompany.com/myapp.php?foo=1&bar=2";
        let params = vec![
            ("CallSid".to_string(), "CA1234567890ABCDE".to_string()),
            ("From".to_string(), "+14158675310".to_string()),
        ];

        // Tampered params: use original signature but different params.
        let tampered_params = vec![
            ("CallSid".to_string(), "TAMPERED_VALUE".to_string()),
            ("From".to_string(), "+14158675310".to_string()),
        ];

        // Compute signature with original params.
        let mut data = url.to_string();
        let mut sorted = params.clone();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        for (key, value) in &sorted {
            data.push_str(key);
            data.push_str(value);
        }
        let mut mac =
            HmacSha1::new_from_slice(auth_token.as_bytes()).expect("HMAC accepts any key length");
        mac.update(data.as_bytes());
        let result = mac.finalize();
        let signature = base64::engine::general_purpose::STANDARD.encode(result.into_bytes());

        // Validation with tampered params should fail.
        assert!(!validate_twilio_signature(
            auth_token,
            url,
            &tampered_params,
            &signature,
        ));
    }

    #[test]
    fn test_validate_twilio_signature_wrong_token() {
        let url = "https://example.com/webhook";
        let params = vec![("Body".to_string(), "Hello".to_string())];

        // Compute signature with "correct_token".
        let mut data = url.to_string();
        let mut sorted = params.clone();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        for (key, value) in &sorted {
            data.push_str(key);
            data.push_str(value);
        }
        let mut mac = HmacSha1::new_from_slice(b"correct_token").unwrap();
        mac.update(data.as_bytes());
        let result = mac.finalize();
        let signature = base64::engine::general_purpose::STANDARD.encode(result.into_bytes());

        // Validation with wrong token should fail.
        assert!(!validate_twilio_signature(
            "wrong_token",
            url,
            &params,
            &signature,
        ));
    }

    #[test]
    fn test_stop_keyword_detection() {
        assert!(is_stop_keyword("STOP"));
        assert!(is_stop_keyword("stop"));
        assert!(is_stop_keyword("Stop"));
        assert!(is_stop_keyword("UNSUBSCRIBE"));
        assert!(is_stop_keyword("  STOP  ")); // with whitespace
        assert!(is_stop_keyword("CANCEL"));
        assert!(is_stop_keyword("END"));
        assert!(is_stop_keyword("QUIT"));
        assert!(is_stop_keyword("STOPALL"));

        // Not stop keywords.
        assert!(!is_stop_keyword("Hello"));
        assert!(!is_stop_keyword("STOP ME"));
        assert!(!is_stop_keyword("Please stop"));
    }

    #[test]
    fn webhook_state_is_clone() {
        let (tx, _rx) = mpsc::channel(1);
        let state = SmsWebhookState {
            inbound_tx: tx,
            auth_token: "token".into(),
            webhook_url: "https://example.com/webhooks/sms".into(),
            allowed_numbers: vec![],
        };
        let _cloned = state.clone();
    }
}
