// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Message routing, authorization filtering, and content extraction.
//!
//! Determines whether an incoming Telegram message should be processed
//! based on authorization rules and chat type, then extracts the content
//! into a channel-agnostic [`InboundMessage`].

use blufio_core::error::BlufioError;
use blufio_core::types::{InboundMessage, MessageContent};
use teloxide::prelude::*;
use teloxide::types::ChatKind;
use tracing::debug;

use crate::media;

/// Checks whether the message sender is authorized.
///
/// Authorization passes if the sender's user ID (as string) or username
/// matches any entry in the `allowed_users` list. If `allowed_users` is
/// empty, all messages are rejected (secure default).
///
/// Messages without a sender (e.g., channel posts) always return `false`.
pub fn is_authorized(msg: &Message, allowed_users: &[String]) -> bool {
    if allowed_users.is_empty() {
        return false;
    }

    let user = match msg.from.as_ref() {
        Some(u) => u,
        None => return false,
    };

    let user_id_str = user.id.0.to_string();

    for allowed in allowed_users {
        // Match by user ID
        if *allowed == user_id_str {
            return true;
        }
        // Match by username (with or without @ prefix)
        if let Some(ref username) = user.username {
            let allowed_clean = allowed.strip_prefix('@').unwrap_or(allowed);
            if username.eq_ignore_ascii_case(allowed_clean) {
                return true;
            }
        }
    }

    false
}

/// Checks whether the message is from a private (DM) chat.
///
/// Group, supergroup, and channel messages return `false`.
pub fn is_dm(msg: &Message) -> bool {
    matches!(msg.chat.kind, ChatKind::Private(_))
}

/// Extracts content from a Telegram message.
///
/// Handles text, photo, document, and voice message types.
/// Returns `None` for unsupported message types (stickers, locations, etc.).
pub async fn extract_content(
    bot: &Bot,
    msg: &Message,
) -> Result<Option<MessageContent>, BlufioError> {
    // Text message
    if let Some(text) = msg.text() {
        return Ok(Some(MessageContent::Text(text.to_string())));
    }

    // Photo message
    if let Some(photos) = msg.photo() {
        let caption = msg.caption();
        let content = media::extract_photo_content(bot, photos, caption).await?;
        return Ok(Some(content));
    }

    // Document message
    if let Some(doc) = msg.document() {
        let content = media::extract_document_content(bot, doc).await?;
        return Ok(Some(content));
    }

    // Voice message
    if let Some(voice) = msg.voice() {
        let content = media::extract_voice_content(bot, voice).await?;
        return Ok(Some(content));
    }

    // Unsupported message type
    debug!(
        msg_id = msg.id.0,
        "ignoring unsupported message type"
    );
    Ok(None)
}

/// Converts a Telegram message and extracted content into an [`InboundMessage`].
pub fn to_inbound_message(msg: &Message, content: MessageContent) -> InboundMessage {
    let sender_id = msg
        .from
        .as_ref()
        .map(|u| u.id.0.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let timestamp = chrono::DateTime::to_rfc3339(&msg.date);

    // Store chat_id in metadata for routing responses back
    let metadata = Some(
        serde_json::json!({
            "chat_id": msg.chat.id.0.to_string(),
        })
        .to_string(),
    );

    InboundMessage {
        id: msg.id.0.to_string(),
        session_id: None, // Resolved by agent loop
        channel: "telegram".to_string(),
        sender_id,
        content,
        timestamp,
        metadata,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a mock private chat message from JSON, matching Telegram Bot API structure.
    fn make_private_message(user_id: u64, username: Option<&str>, text: &str) -> Message {
        let from = if let Some(uname) = username {
            serde_json::json!({
                "id": user_id,
                "is_bot": false,
                "first_name": "Test",
                "username": uname,
            })
        } else {
            serde_json::json!({
                "id": user_id,
                "is_bot": false,
                "first_name": "Test",
            })
        };

        let json = serde_json::json!({
            "message_id": 1,
            "date": 1700000000i64,
            "chat": {
                "id": user_id as i64,
                "type": "private",
                "first_name": "Test",
            },
            "from": from,
            "text": text,
        });

        serde_json::from_value(json).expect("failed to deserialize mock message")
    }

    /// Build a mock group chat message.
    fn make_group_message(user_id: u64, text: &str) -> Message {
        let json = serde_json::json!({
            "message_id": 1,
            "date": 1700000000i64,
            "chat": {
                "id": -100123i64,
                "type": "supergroup",
                "title": "Test Group",
            },
            "from": {
                "id": user_id,
                "is_bot": false,
                "first_name": "Test",
            },
            "text": text,
        });

        serde_json::from_value(json).expect("failed to deserialize mock group message")
    }

    /// Build a mock message without a sender.
    fn make_no_sender_message(text: &str) -> Message {
        let json = serde_json::json!({
            "message_id": 1,
            "date": 1700000000i64,
            "chat": {
                "id": 12345i64,
                "type": "private",
                "first_name": "Test",
            },
            "text": text,
        });

        serde_json::from_value(json).expect("failed to deserialize mock message")
    }

    #[test]
    fn authorized_by_user_id() {
        let msg = make_private_message(12345, None, "hello");
        assert!(is_authorized(&msg, &["12345".into()]));
    }

    #[test]
    fn authorized_by_username() {
        let msg = make_private_message(12345, Some("testuser"), "hello");
        assert!(is_authorized(&msg, &["testuser".into()]));
    }

    #[test]
    fn authorized_by_username_with_at() {
        let msg = make_private_message(12345, Some("testuser"), "hello");
        assert!(is_authorized(&msg, &["@testuser".into()]));
    }

    #[test]
    fn authorized_by_username_case_insensitive() {
        let msg = make_private_message(12345, Some("TestUser"), "hello");
        assert!(is_authorized(&msg, &["testuser".into()]));
    }

    #[test]
    fn not_authorized_wrong_user() {
        let msg = make_private_message(12345, Some("testuser"), "hello");
        assert!(!is_authorized(&msg, &["99999".into()]));
    }

    #[test]
    fn not_authorized_empty_list() {
        let msg = make_private_message(12345, Some("testuser"), "hello");
        assert!(!is_authorized(&msg, &[]));
    }

    #[test]
    fn not_authorized_no_sender() {
        let msg = make_no_sender_message("hello");
        assert!(!is_authorized(&msg, &["12345".into()]));
    }

    #[test]
    fn is_dm_private_chat() {
        let msg = make_private_message(12345, None, "hello");
        assert!(is_dm(&msg));
    }

    #[test]
    fn is_dm_group_chat() {
        let msg = make_group_message(12345, "hello");
        assert!(!is_dm(&msg));
    }

    #[test]
    fn to_inbound_message_maps_fields() {
        let msg = make_private_message(12345, Some("testuser"), "hello");
        let content = MessageContent::Text("hello".into());
        let inbound = to_inbound_message(&msg, content);

        assert_eq!(inbound.id, "1");
        assert_eq!(inbound.channel, "telegram");
        assert_eq!(inbound.sender_id, "12345");
        assert!(inbound.session_id.is_none());
        assert!(inbound.metadata.is_some());

        // Verify chat_id in metadata
        let meta: serde_json::Value =
            serde_json::from_str(inbound.metadata.as_ref().unwrap()).unwrap();
        assert_eq!(meta["chat_id"], "12345");
    }

    #[tokio::test]
    async fn extract_text_content() {
        let msg = make_private_message(12345, None, "hello world");
        let bot = Bot::new("test:token");
        let content = extract_content(&bot, &msg).await.unwrap();
        match content {
            Some(MessageContent::Text(t)) => assert_eq!(t, "hello world"),
            other => panic!("expected Some(Text), got {other:?}"),
        }
    }
}
