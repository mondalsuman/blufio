// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! WhatsApp Cloud API webhook payload and API types.

use serde::{Deserialize, Serialize};

/// Top-level webhook payload from Meta WhatsApp Cloud API.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WhatsAppWebhookPayload {
    /// Object type (always "whatsapp_business_account").
    pub object: String,
    /// List of webhook entries.
    pub entry: Vec<WhatsAppEntry>,
}

/// A single entry in the webhook payload.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WhatsAppEntry {
    /// WhatsApp Business Account ID.
    pub id: String,
    /// List of changes in this entry.
    pub changes: Vec<WhatsAppChange>,
}

/// A change within a webhook entry.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WhatsAppChange {
    /// Field that changed (typically "messages").
    pub field: String,
    /// The change value containing message data.
    pub value: WhatsAppChangeValue,
}

/// The value of a webhook change, containing messages, statuses, contacts, etc.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct WhatsAppChangeValue {
    /// Messaging product (always "whatsapp").
    #[serde(default)]
    pub messaging_product: Option<String>,
    /// Metadata about the phone number receiving the webhook.
    #[serde(default)]
    pub metadata: Option<WhatsAppMetadata>,
    /// Contact information for message senders.
    #[serde(default)]
    pub contacts: Option<Vec<WhatsAppContact>>,
    /// Incoming messages.
    #[serde(default)]
    pub messages: Option<Vec<WhatsAppMessage>>,
    /// Message status updates (sent, delivered, read).
    #[serde(default)]
    pub statuses: Option<serde_json::Value>,
}

/// Metadata about the WhatsApp Business phone number.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WhatsAppMetadata {
    /// Display phone number.
    pub display_phone_number: String,
    /// Phone number ID used for API calls.
    pub phone_number_id: String,
}

/// Contact information from a webhook payload.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WhatsAppContact {
    /// Contact profile information.
    pub profile: WhatsAppProfile,
    /// WhatsApp ID of the contact.
    pub wa_id: String,
}

/// Contact profile.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WhatsAppProfile {
    /// Display name of the contact.
    pub name: String,
}

/// An incoming WhatsApp message.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WhatsAppMessage {
    /// Sender's phone number.
    pub from: String,
    /// Unique message ID.
    pub id: String,
    /// Unix timestamp of the message.
    pub timestamp: String,
    /// Message type (text, image, etc.).
    #[serde(rename = "type")]
    pub msg_type: String,
    /// Text message body (present when msg_type is "text").
    #[serde(default)]
    pub text: Option<WhatsAppTextBody>,
}

/// Text body of a WhatsApp message.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WhatsAppTextBody {
    /// The text content.
    pub body: String,
}

/// Query parameters for WhatsApp webhook verification (GET request).
#[derive(Debug, Clone, Deserialize)]
pub struct WhatsAppVerifyParams {
    /// Hub mode (should be "subscribe").
    #[serde(rename = "hub.mode")]
    pub hub_mode: String,
    /// Verification token to compare against configured value.
    #[serde(rename = "hub.verify_token")]
    pub hub_verify_token: String,
    /// Challenge string to echo back on success.
    #[serde(rename = "hub.challenge")]
    pub hub_challenge: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_text_message_payload() {
        let json = r#"{
            "object": "whatsapp_business_account",
            "entry": [{
                "id": "123456789",
                "changes": [{
                    "field": "messages",
                    "value": {
                        "messaging_product": "whatsapp",
                        "metadata": {
                            "display_phone_number": "+15551234567",
                            "phone_number_id": "987654321"
                        },
                        "contacts": [{
                            "profile": { "name": "Alice" },
                            "wa_id": "15551234567"
                        }],
                        "messages": [{
                            "from": "15551234567",
                            "id": "wamid.abc123",
                            "timestamp": "1709000000",
                            "type": "text",
                            "text": { "body": "Hello Blufio!" }
                        }]
                    }
                }]
            }]
        }"#;

        let payload: WhatsAppWebhookPayload = serde_json::from_str(json).unwrap();
        assert_eq!(payload.object, "whatsapp_business_account");
        assert_eq!(payload.entry.len(), 1);

        let change = &payload.entry[0].changes[0];
        assert_eq!(change.field, "messages");

        let messages = change.value.messages.as_ref().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].from, "15551234567");
        assert_eq!(messages[0].msg_type, "text");
        assert_eq!(messages[0].text.as_ref().unwrap().body, "Hello Blufio!");
    }

    #[test]
    fn deserialize_verify_params() {
        let json = r#"{
            "hub.mode": "subscribe",
            "hub.verify_token": "my-secret-token",
            "hub.challenge": "challenge-string-123"
        }"#;

        let params: WhatsAppVerifyParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.hub_mode, "subscribe");
        assert_eq!(params.hub_verify_token, "my-secret-token");
        assert_eq!(params.hub_challenge, "challenge-string-123");
    }
}
