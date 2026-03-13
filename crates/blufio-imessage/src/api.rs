// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! BlueBubbles REST API client.
//!
//! Provides methods for sending messages, registering webhooks, checking
//! server health, and sending typing/read-receipt indicators via the
//! BlueBubbles HTTP API.

use blufio_core::error::BlufioError;
use tracing::warn;

use crate::types::{BlueBubblesSendRequest, BlueBubblesSendResponse, BlueBubblesServerInfo};

/// Client for the BlueBubbles REST API.
///
/// BlueBubbles uses query-parameter authentication (`?password=...`) on every
/// request, not Authorization headers.
pub struct BlueBubblesClient {
    base_url: String,
    password: String,
    client: reqwest::Client,
}

impl BlueBubblesClient {
    /// Create a new BlueBubbles API client.
    pub fn new(base_url: &str, password: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            password: password.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Build a full URL with password query parameter.
    fn url(&self, path: &str) -> String {
        format!("{}{path}?password={}", self.base_url, self.password)
    }

    /// Fetch server info (health check / connectivity verification).
    pub async fn server_info(&self) -> Result<BlueBubblesServerInfo, BlufioError> {
        let url = self.url("/api/v1/server/info");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("imessage", e))?;

        let status = resp.status();
        if status.is_server_error() {
            // Single retry on 5xx.
            warn!(status = %status, "BlueBubbles server error, retrying once");
            let retry_resp = self
                .client
                .get(&url)
                .send()
                .await
                .map_err(|e| BlufioError::channel_delivery_failed("imessage", e))?;
            if !retry_resp.status().is_success() {
                return Err(BlufioError::Config(format!(
                    "BlueBubbles server error: {}",
                    retry_resp.status()
                )));
            }
            return retry_resp
                .json::<BlueBubblesServerInfo>()
                .await
                .map_err(|e| BlufioError::channel_delivery_failed("imessage", e));
        }

        if !status.is_success() {
            return Err(BlufioError::Config(format!(
                "BlueBubbles server error: {status}"
            )));
        }

        resp.json::<BlueBubblesServerInfo>()
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("imessage", e))
    }

    /// Send a text message to a chat.
    ///
    /// Returns the message GUID from the response.
    pub async fn send_message(&self, chat_guid: &str, text: &str) -> Result<String, BlufioError> {
        let url = self.url("/api/v1/message/text");
        let body = BlueBubblesSendRequest {
            chat_guid: chat_guid.to_string(),
            message: text.to_string(),
            method: "private-api".to_string(),
        };

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("imessage", e))?;

        let status = resp.status();
        if status.is_server_error() {
            // Single retry on 5xx.
            warn!(status = %status, "BlueBubbles send failed, retrying once");
            let retry_resp = self
                .client
                .post(&url)
                .json(&body)
                .send()
                .await
                .map_err(|e| BlufioError::channel_delivery_failed("imessage", e))?;
            let send_resp: BlueBubblesSendResponse = retry_resp
                .json()
                .await
                .map_err(|e| BlufioError::channel_delivery_failed("imessage", e))?;
            return extract_message_guid(send_resp);
        }

        if status.is_client_error() {
            return Err(BlufioError::Config(format!(
                "BlueBubbles client error sending message: {status}"
            )));
        }

        let send_resp: BlueBubblesSendResponse = resp
            .json()
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("imessage", e))?;

        extract_message_guid(send_resp)
    }

    /// Register a webhook URL with BlueBubbles for receiving events.
    pub async fn register_webhook(&self, url: &str) -> Result<(), BlufioError> {
        let api_url = self.url("/api/v1/webhook");
        let body = serde_json::json!({
            "url": url,
            "events": ["new-message"],
        });

        let resp = self
            .client
            .post(&api_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| BlufioError::channel_delivery_failed("imessage", e))?;

        if !resp.status().is_success() {
            warn!(
                status = %resp.status(),
                "Failed to register BlueBubbles webhook"
            );
        }

        Ok(())
    }

    /// Send a read receipt for a chat (best-effort).
    pub async fn send_read_receipt(&self, chat_guid: &str) -> Result<(), BlufioError> {
        let url = self.url(&format!("/api/v1/chat/{chat_guid}/read"));
        if let Err(e) = self.client.post(&url).send().await {
            warn!(error = %e, "Failed to send read receipt (best-effort)");
        }
        Ok(())
    }

    /// Send a typing indicator for a chat (best-effort, may not be supported).
    pub async fn send_typing(&self, chat_guid: &str) -> Result<(), BlufioError> {
        let url = self.url(&format!("/api/v1/chat/{chat_guid}/typing"));
        if let Err(e) = self.client.post(&url).send().await {
            warn!(error = %e, "Failed to send typing indicator (best-effort)");
        }
        Ok(())
    }
}

/// Extract the message GUID from a send response.
fn extract_message_guid(resp: BlueBubblesSendResponse) -> Result<String, BlufioError> {
    if let Some(data) = &resp.data
        && let Some(guid) = data.get("guid").and_then(|v| v.as_str())
    {
        return Ok(guid.to_string());
    }
    // Fall back to a generated UUID if the response doesn't contain a GUID.
    Ok(uuid::Uuid::new_v4().to_string())
}
