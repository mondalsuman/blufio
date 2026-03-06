// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bridge event loop that subscribes to the event bus and routes messages
//! between channels in a bridge group.

use std::collections::HashMap;
use std::sync::Arc;

use blufio_bus::EventBus;
use blufio_bus::events::{BusEvent, ChannelEvent};
use blufio_config::model::BridgeGroupConfig;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::formatter;

/// An outbound bridged message ready to be sent to a target channel.
pub struct BridgedMessage {
    /// The channel to deliver this message to.
    pub target_channel: String,
    /// The formatted message content with sender attribution.
    pub content: String,
}

/// Run the bridge event loop.
///
/// Subscribes to the event bus, filters messages by bridge group membership,
/// formats with sender attribution, and sends to the `bridge_tx` channel.
/// The caller (serve.rs) reads from `bridge_rx` and dispatches via
/// ChannelMultiplexer.
pub async fn run_bridge_loop(
    bus: Arc<EventBus>,
    groups: HashMap<String, BridgeGroupConfig>,
    bridge_tx: mpsc::Sender<BridgedMessage>,
) {
    let manager = super::BridgeManager::new(groups);
    if manager.is_empty() {
        info!("no bridge groups configured, bridge loop not started");
        return;
    }

    let mut rx = bus.subscribe();
    info!("bridge loop started, listening for channel events");

    loop {
        match rx.recv().await {
            Ok(BusEvent::Channel(ChannelEvent::MessageReceived {
                channel,
                sender_id,
                content: Some(content),
                sender_name,
                is_bridged,
                ..
            })) => {
                // Determine target channels for this message.
                let targets = manager.should_bridge(&channel, &sender_id, is_bridged, false);

                if targets.is_empty() {
                    continue;
                }

                let display_name = sender_name.as_deref().unwrap_or(&sender_id);
                let formatted = formatter::format_bridged_message(&channel, display_name, &content);

                for (_group_name, target_channels) in targets {
                    for target in target_channels {
                        debug!(
                            source = %channel,
                            target = %target,
                            sender = %display_name,
                            "bridging message"
                        );
                        if bridge_tx
                            .send(BridgedMessage {
                                target_channel: target,
                                content: formatted.clone(),
                            })
                            .await
                            .is_err()
                        {
                            warn!("bridge outbound channel closed");
                            return;
                        }
                    }
                }
            }
            Ok(_) => {
                // Ignore non-channel events and events without content.
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                warn!(missed = n, "bridge subscriber lagged behind event bus");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                info!("event bus closed, bridge loop stopping");
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridged_message_fields() {
        let msg = BridgedMessage {
            target_channel: "discord".to_string(),
            content: "[Telegram/Alice] Hello!".to_string(),
        };
        assert_eq!(msg.target_channel, "discord");
        assert_eq!(msg.content, "[Telegram/Alice] Hello!");
    }
}
