// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Slack message routing: @mention detection, authorization, and conversion.
//!
//! Provides event handler logic for Socket Mode events including message
//! filtering, authorization checks, and InboundMessage conversion.

use blufio_core::types::{InboundMessage, MessageContent};
use regex::Regex;

/// Returns true if the bot should respond to this message.
///
/// DMs (im channels) always get a response. Channels only when @mentioned.
pub fn should_respond(text: &str, channel_type: &str, bot_user_id: &str) -> bool {
    // DMs always respond.
    if channel_type == "im" {
        return true;
    }

    // In channels/groups, only respond when @mentioned.
    let mention_pattern = format!("<@{bot_user_id}>");
    text.contains(&mention_pattern)
}

/// Returns true if the sender is authorized (allowed_users check).
///
/// Empty allowed_users means everyone is allowed.
pub fn is_authorized(user_id: &str, allowed_users: &[String]) -> bool {
    if allowed_users.is_empty() {
        return true;
    }
    allowed_users.iter().any(|u| u == user_id)
}

/// Strips the bot @mention from message text.
pub fn strip_mention(text: &str, bot_user_id: &str) -> String {
    let mention_re =
        Regex::new(&format!(r"<@{}>", regex::escape(bot_user_id))).expect("valid regex: mention");
    mention_re.replace_all(text, "").trim().to_string()
}

/// Converts Slack event data into a Blufio InboundMessage.
pub fn to_inbound_message(
    event_ts: &str,
    user_id: &str,
    channel_id: &str,
    team_id: Option<&str>,
    content: String,
) -> InboundMessage {
    let metadata = serde_json::json!({
        "channel_id": channel_id,
        "chat_id": channel_id,
        "team_id": team_id,
    });

    InboundMessage {
        id: event_ts.to_string(),
        session_id: None,
        channel: "slack".to_string(),
        sender_id: user_id.to_string(),
        content: MessageContent::Text(content),
        timestamp: event_ts.to_string(),
        metadata: Some(metadata.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_respond_dm_always() {
        assert!(should_respond("hello", "im", "U123BOT"));
    }

    #[test]
    fn should_respond_channel_with_mention() {
        assert!(should_respond("<@U123BOT> hello", "channel", "U123BOT"));
    }

    #[test]
    fn should_respond_channel_without_mention() {
        assert!(!should_respond("hello", "channel", "U123BOT"));
    }

    #[test]
    fn should_respond_group_with_mention() {
        assert!(should_respond("<@U123BOT> hello", "group", "U123BOT"));
    }

    #[test]
    fn is_authorized_empty_allows_all() {
        let allowed: Vec<String> = vec![];
        assert!(is_authorized("U123", &allowed));
    }

    #[test]
    fn is_authorized_checks_id() {
        let allowed = vec!["U111".to_string(), "U222".to_string()];
        assert!(is_authorized("U111", &allowed));
        assert!(!is_authorized("U333", &allowed));
    }

    #[test]
    fn strip_mention_removes_bot() {
        let result = strip_mention("<@U123BOT> hello there", "U123BOT");
        assert_eq!(result, "hello there");
    }

    #[test]
    fn strip_mention_preserves_no_mention() {
        let result = strip_mention("hello there", "U123BOT");
        assert_eq!(result, "hello there");
    }

    #[test]
    fn strip_mention_multiple() {
        let result = strip_mention("<@U123BOT> hello <@U123BOT>", "U123BOT");
        assert_eq!(result, "hello");
    }

    #[test]
    fn to_inbound_message_correct_fields() {
        let msg = to_inbound_message(
            "1234567890.123456",
            "U123",
            "C456",
            Some("T789"),
            "hello".to_string(),
        );
        assert_eq!(msg.id, "1234567890.123456");
        assert_eq!(msg.channel, "slack");
        assert_eq!(msg.sender_id, "U123");
        match &msg.content {
            MessageContent::Text(t) => assert_eq!(t, "hello"),
            _ => panic!("expected text"),
        }
        let meta: serde_json::Value = serde_json::from_str(msg.metadata.as_ref().unwrap()).unwrap();
        assert_eq!(meta["channel_id"], "C456");
        assert_eq!(meta["chat_id"], "C456");
        assert_eq!(meta["team_id"], "T789");
    }
}
