// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Shared types for iMessage adapter.
//!
//! BlueBubbles webhook payload and REST API request/response types.

use serde::{Deserialize, Serialize};

/// BlueBubbles webhook event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesWebhookPayload {
    /// Event type (e.g., "new-message", "updated-message").
    #[serde(rename = "type")]
    pub type_field: String,
    /// Event data (structure varies by event type).
    pub data: serde_json::Value,
}

/// A message from the BlueBubbles server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesMessage {
    /// Unique message GUID.
    pub guid: String,
    /// Message text content.
    pub text: Option<String>,
    /// Sender handle (phone number or email).
    pub handle: Option<BlueBubblesHandle>,
    /// Chat GUID the message belongs to.
    #[serde(rename = "chatGuid")]
    pub chat_guid: Option<String>,
    /// Whether this message was sent by the local user.
    #[serde(rename = "isFromMe")]
    pub is_from_me: bool,
    /// ISO 8601 creation timestamp.
    #[serde(rename = "dateCreated")]
    pub date_created: Option<String>,
    /// Non-zero indicates a tapback/reaction (not a regular message).
    #[serde(rename = "associatedMessageType")]
    pub associated_message_type: Option<i32>,
}

/// A BlueBubbles contact handle (sender/receiver identity).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesHandle {
    /// Phone number or email address.
    pub address: String,
}

/// BlueBubbles server info response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesServerInfo {
    /// macOS version on the server host.
    pub os_version: Option<String>,
    /// BlueBubbles server version.
    pub server_version: Option<String>,
    /// Whether the Private API is available.
    pub private_api: Option<bool>,
}

/// Request body for sending a message via BlueBubbles REST API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesSendRequest {
    /// Target chat GUID (e.g., "iMessage;-;+1234567890").
    #[serde(rename = "chatGuid")]
    pub chat_guid: String,
    /// Message text to send.
    pub message: String,
    /// Send method: "private-api" or "apple-script".
    pub method: String,
}

/// Response from BlueBubbles after sending a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesSendResponse {
    /// HTTP-like status code.
    pub status: i32,
    /// Human-readable message.
    pub message: Option<String>,
    /// Response data (may contain the sent message).
    pub data: Option<serde_json::Value>,
}
