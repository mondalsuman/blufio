// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Matrix event handler callbacks for room messages and invites.
//!
//! These are registered with the matrix-sdk client via `add_event_handler`.
//! Context values (like `inbound_tx`) are provided through the event handler
//! context system using `Ctx<T>`.

use blufio_core::types::{InboundMessage, MessageContent};
use matrix_sdk::{
    Client, Room,
    ruma::events::room::{
        member::StrippedRoomMemberEvent,
        message::{MessageType, OriginalSyncRoomMessageEvent},
    },
};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Handle incoming room messages.
///
/// Registered as an event handler with the matrix-sdk client. Filters messages
/// from the bot itself, checks allowed users, and forwards valid messages to
/// the inbound channel.
pub async fn on_room_message(
    event: OriginalSyncRoomMessageEvent,
    room: Room,
    inbound_tx: matrix_sdk::event_handler::Ctx<mpsc::Sender<InboundMessage>>,
    bot_user_id: matrix_sdk::event_handler::Ctx<String>,
    allowed_users: matrix_sdk::event_handler::Ctx<Vec<String>>,
) {
    // Extract text content; skip non-text messages.
    let text = match &event.content.msgtype {
        MessageType::Text(content) => content.body.clone(),
        _ => return,
    };

    // Skip messages from the bot itself.
    if event.sender.as_str() == bot_user_id.as_str() {
        return;
    }

    // Check allowed users filter.
    if !allowed_users.is_empty() && !allowed_users.contains(&event.sender.to_string()) {
        debug!(
            sender = %event.sender,
            "skipping Matrix message from non-allowed user"
        );
        return;
    }

    let room_id = room.room_id().to_string();
    let sender_id = event.sender.to_string();
    let sender_name = event.sender.localpart().to_string();

    let metadata = serde_json::json!({
        "chat_id": room_id,
        "sender_name": sender_name,
    });

    let inbound = InboundMessage {
        id: event.event_id.to_string(),
        session_id: None,
        channel: "matrix".to_string(),
        sender_id,
        content: MessageContent::Text(text),
        metadata: Some(metadata.to_string()),
        timestamp: event.origin_server_ts.as_secs().to_string(),
    };

    if inbound_tx.send(inbound).await.is_err() {
        warn!("Matrix inbound channel closed");
    }
}

/// Handle room invites by auto-joining.
///
/// When the bot receives a room invite, it automatically attempts to join
/// with up to 3 retries.
pub async fn on_room_invite(event: StrippedRoomMemberEvent, client: Client, room: Room) {
    // Only process invites for our own user.
    let our_user_id = match client.user_id() {
        Some(id) => id.to_owned(),
        None => return,
    };

    if event.state_key != our_user_id {
        return;
    }

    let room_id = room.room_id().to_owned();
    info!(room_id = %room_id, "received room invite, attempting to join");

    tokio::spawn(async move {
        for attempt in 1..=3 {
            match client.join_room_by_id(&room_id).await {
                Ok(_) => {
                    info!(room_id = %room_id, "successfully joined room on attempt {attempt}");
                    return;
                }
                Err(e) => {
                    warn!(
                        room_id = %room_id,
                        attempt = attempt,
                        error = %e,
                        "failed to join room"
                    );
                    if attempt < 3 {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    }
                }
            }
        }
        warn!(room_id = %room_id, "gave up joining room after 3 attempts");
    });
}
