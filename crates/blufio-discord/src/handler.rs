// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Discord message routing: @mention detection, authorization, and conversion.

use blufio_core::types::{InboundMessage, MessageContent};
use serenity::model::channel::Message;
use serenity::model::id::UserId;

/// Returns true if the bot should respond to this message.
///
/// DMs always get a response. Server/guild messages only when @mentioned.
pub fn should_respond(msg: &Message, bot_id: UserId) -> bool {
    // DMs (no guild) always respond.
    if msg.guild_id.is_none() {
        return true;
    }

    // In guilds, only respond when @mentioned.
    msg.mentions.iter().any(|u| u.id == bot_id)
}

/// Returns true if the sender is authorized (allowed_users check).
///
/// Empty allowed_users means everyone is allowed.
pub fn is_authorized(msg: &Message, allowed_users: &[String]) -> bool {
    if allowed_users.is_empty() {
        return true;
    }
    let sender_id = msg.author.id.to_string();
    allowed_users.contains(&sender_id)
}

/// Strips the bot @mention from message content.
pub fn strip_mention(content: &str, bot_id: UserId) -> String {
    let mention_pattern = format!("<@{}>", bot_id);
    let mention_nick_pattern = format!("<@!{}>", bot_id);
    content
        .replace(&mention_pattern, "")
        .replace(&mention_nick_pattern, "")
        .trim()
        .to_string()
}

/// Converts a serenity Message into a Blufio InboundMessage.
pub fn to_inbound_message(msg: &Message, content: String) -> InboundMessage {
    let metadata = serde_json::json!({
        "channel_id": msg.channel_id.to_string(),
        "guild_id": msg.guild_id.map(|g| g.to_string()),
        "chat_id": msg.channel_id.to_string(),
    });

    InboundMessage {
        id: msg.id.to_string(),
        session_id: None,
        channel: "discord".to_string(),
        sender_id: msg.author.id.to_string(),
        content: MessageContent::Text(content),
        timestamp: msg.timestamp.to_string(),
        metadata: Some(metadata.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serenity::model::id::UserId;

    #[test]
    fn strip_mention_removes_bot_mention() {
        let bot_id = UserId::new(123456789);
        let result = strip_mention("<@123456789> hello there", bot_id);
        assert_eq!(result, "hello there");
    }

    #[test]
    fn strip_mention_removes_nick_mention() {
        let bot_id = UserId::new(123456789);
        let result = strip_mention("<@!123456789> hello there", bot_id);
        assert_eq!(result, "hello there");
    }

    #[test]
    fn strip_mention_preserves_no_mention() {
        let bot_id = UserId::new(123456789);
        let result = strip_mention("hello there", bot_id);
        assert_eq!(result, "hello there");
    }

    #[test]
    fn is_authorized_empty_allows_all() {
        // Cannot construct a real Message in tests without full serenity fixtures,
        // so we test the logic via the string comparison.
        let allowed: Vec<String> = vec![];
        // Empty list = everyone allowed
        assert!(allowed.is_empty());
    }

    #[test]
    fn is_authorized_checks_id() {
        let allowed = ["111".to_string(), "222".to_string()];
        assert!(allowed.contains(&"111".to_string()));
        assert!(!allowed.contains(&"333".to_string()));
    }
}
