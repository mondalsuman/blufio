// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Shared types for SMS adapter.
//!
//! Twilio webhook inbound payload and REST API request/response types.

use serde::{Deserialize, Serialize};

/// Twilio inbound SMS webhook payload (form-urlencoded POST parameters).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwilioInboundSms {
    /// Unique message SID from Twilio.
    #[serde(rename = "MessageSid")]
    pub message_sid: String,
    /// Sender phone number (E.164 format).
    #[serde(rename = "From")]
    pub from: String,
    /// Recipient phone number (E.164 format).
    #[serde(rename = "To")]
    pub to: String,
    /// Message body text.
    #[serde(rename = "Body")]
    pub body: String,
    /// Number of media attachments (MMS). "0" for pure SMS.
    #[serde(rename = "NumMedia")]
    pub num_media: Option<String>,
}

/// Request body for sending an SMS via Twilio REST API (form-urlencoded).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwilioSendRequest {
    /// Destination phone number (E.164 format).
    #[serde(rename = "To")]
    pub to: String,
    /// Source phone number (E.164 format).
    #[serde(rename = "From")]
    pub from: String,
    /// Message body text.
    #[serde(rename = "Body")]
    pub body: String,
    /// Optional status callback URL.
    #[serde(rename = "StatusCallback", skip_serializing_if = "Option::is_none")]
    pub status_callback: Option<String>,
}

/// Response from Twilio after sending an SMS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwilioSendResponse {
    /// Unique message SID.
    pub sid: Option<String>,
    /// Message status (e.g., "queued", "sent").
    pub status: Option<String>,
    /// Error code (if any).
    pub error_code: Option<i32>,
    /// Error message (if any).
    pub error_message: Option<String>,
}

/// Response from Twilio Account API (health check).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwilioAccountInfo {
    /// Account SID.
    pub sid: String,
    /// Account status (e.g., "active", "suspended").
    pub status: String,
}
