// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Meta Graph API client for sending WhatsApp messages.

use blufio_core::error::BlufioError;
use serde_json::json;

/// Send a text message via the WhatsApp Cloud API (Meta Graph API v21.0).
///
/// POSTs to `https://graph.facebook.com/v21.0/{phone_number_id}/messages`
/// with bearer authentication.
pub async fn send_whatsapp_message(
    client: &reqwest::Client,
    phone_number_id: &str,
    access_token: &str,
    to: &str,
    text: &str,
) -> Result<String, BlufioError> {
    let url = format!("https://graph.facebook.com/v21.0/{phone_number_id}/messages");

    let body = json!({
        "messaging_product": "whatsapp",
        "to": to,
        "type": "text",
        "text": { "body": text }
    });

    let resp = client
        .post(&url)
        .bearer_auth(access_token)
        .json(&body)
        .send()
        .await
        .map_err(|e| BlufioError::Channel {
            message: format!("WhatsApp API request failed: {e}"),
            source: Some(Box::new(e)),
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(BlufioError::Channel {
            message: format!("WhatsApp API returned {status}: {body}"),
            source: None,
        });
    }

    let resp_json: serde_json::Value = resp.json().await.map_err(|e| BlufioError::Channel {
        message: format!("WhatsApp API response parse error: {e}"),
        source: Some(Box::new(e)),
    })?;

    // Extract message ID from response: { "messages": [{ "id": "wamid.xxx" }] }
    let message_id = resp_json
        .get("messages")
        .and_then(|m| m.as_array())
        .and_then(|a| a.first())
        .and_then(|m| m.get("id"))
        .and_then(|id| id.as_str())
        .unwrap_or("unknown")
        .to_string();

    Ok(message_id)
}
