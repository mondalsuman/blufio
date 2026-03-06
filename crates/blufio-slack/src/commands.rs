// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Slack slash command routing.
//!
//! Handles `/blufio` slash command with subcommands: status, help, chat.

use blufio_core::types::{InboundMessage, MessageContent};
use tokio::sync::mpsc;
use tracing::error;

use crate::blocks;

/// Response type for slash commands.
pub enum SlashCommandResponse {
    /// Respond with Block Kit blocks (ephemeral).
    Blocks(serde_json::Value),
    /// Respond with plain text acknowledgment.
    Text(String),
}

/// Handle a /blufio slash command.
///
/// Parses the text after `/blufio` and routes to the appropriate handler.
/// Returns a response to send back to the user.
pub async fn handle_slash_command(
    text: &str,
    user_id: &str,
    channel_id: &str,
    inbound_tx: &mpsc::Sender<InboundMessage>,
) -> SlashCommandResponse {
    let trimmed = text.trim();

    // Parse subcommand.
    let (subcommand, remainder) = match trimmed.split_once(' ') {
        Some((cmd, rest)) => (cmd, rest.trim()),
        None => (trimmed, ""),
    };

    match subcommand {
        "status" => SlashCommandResponse::Blocks(blocks::build_status_blocks()),
        "help" | "" => SlashCommandResponse::Blocks(blocks::build_help_blocks()),
        "chat" if !remainder.is_empty() => {
            send_chat_message(remainder, user_id, channel_id, inbound_tx).await
        }
        _ => {
            // Treat as a direct message to Blufio.
            let message = if remainder.is_empty() {
                subcommand
            } else {
                trimmed
            };
            send_chat_message(message, user_id, channel_id, inbound_tx).await
        }
    }
}

/// Converts the slash command text into an InboundMessage and sends it.
async fn send_chat_message(
    message: &str,
    user_id: &str,
    channel_id: &str,
    inbound_tx: &mpsc::Sender<InboundMessage>,
) -> SlashCommandResponse {
    let inbound = InboundMessage {
        id: uuid_v4(),
        session_id: None,
        channel: "slack".to_string(),
        sender_id: user_id.to_string(),
        content: MessageContent::Text(message.to_string()),
        timestamp: chrono_now(),
        metadata: Some(
            serde_json::json!({
                "channel_id": channel_id,
                "chat_id": channel_id,
                "from_slash_command": true,
            })
            .to_string(),
        ),
    };

    if inbound_tx.send(inbound).await.is_err() {
        error!("inbound channel closed, cannot forward slash command message");
        return SlashCommandResponse::Text("Internal error: message channel closed".to_string());
    }

    SlashCommandResponse::Text("Processing your message...".to_string())
}

/// Generate a simple UUID-like ID.
fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("slack-cmd-{ts}")
}

/// Get current timestamp as string.
fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    ts.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn status_returns_blocks() {
        let (tx, _rx) = mpsc::channel(1);
        let resp = handle_slash_command("status", "U123", "C456", &tx).await;
        match resp {
            SlashCommandResponse::Blocks(blocks) => {
                assert!(blocks.as_array().is_some());
            }
            _ => panic!("expected Blocks response for status command"),
        }
    }

    #[tokio::test]
    async fn help_returns_blocks() {
        let (tx, _rx) = mpsc::channel(1);
        let resp = handle_slash_command("help", "U123", "C456", &tx).await;
        match resp {
            SlashCommandResponse::Blocks(blocks) => {
                assert!(blocks.as_array().is_some());
            }
            _ => panic!("expected Blocks response for help command"),
        }
    }

    #[tokio::test]
    async fn empty_returns_help() {
        let (tx, _rx) = mpsc::channel(1);
        let resp = handle_slash_command("", "U123", "C456", &tx).await;
        match resp {
            SlashCommandResponse::Blocks(_) => {} // Help blocks
            _ => panic!("expected Blocks response for empty command"),
        }
    }

    #[tokio::test]
    async fn chat_sends_message() {
        let (tx, mut rx) = mpsc::channel(1);
        let resp = handle_slash_command("chat hello there", "U123", "C456", &tx).await;
        match resp {
            SlashCommandResponse::Text(t) => {
                assert!(t.contains("Processing"));
            }
            _ => panic!("expected Text response for chat command"),
        }

        // Verify the message was forwarded.
        let inbound = rx.recv().await.unwrap();
        assert_eq!(inbound.channel, "slack");
        assert_eq!(inbound.sender_id, "U123");
        match &inbound.content {
            MessageContent::Text(t) => assert_eq!(t, "hello there"),
            _ => panic!("expected text content"),
        }
    }

    #[tokio::test]
    async fn direct_message_sends() {
        let (tx, mut rx) = mpsc::channel(1);
        let resp = handle_slash_command("what is rust?", "U123", "C456", &tx).await;
        match resp {
            SlashCommandResponse::Text(t) => {
                assert!(t.contains("Processing"));
            }
            _ => panic!("expected Text response for direct message"),
        }

        let inbound = rx.recv().await.unwrap();
        match &inbound.content {
            MessageContent::Text(t) => assert_eq!(t, "what is rust?"),
            _ => panic!("expected text content"),
        }
    }
}
