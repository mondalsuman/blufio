// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Typed event definitions for the Blufio internal event bus.
//!
//! Each domain (session, channel, skill, node, webhook, batch) defines its own
//! event sub-enum. The top-level [`BusEvent`] wraps all domains.

use serde::{Deserialize, Serialize};

/// Generate a new unique event ID using UUID v4.
pub fn new_event_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Generate an ISO 8601 timestamp for the current time.
pub fn now_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Top-level event type for the Blufio internal event bus.
///
/// Each variant represents a domain of the system. Subscribers can pattern-match
/// on the variant to filter events by domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BusEvent {
    /// Session lifecycle events.
    Session(SessionEvent),
    /// Channel message events.
    Channel(ChannelEvent),
    /// Skill invocation events.
    Skill(SkillEvent),
    /// Node connection events.
    Node(NodeEvent),
    /// Webhook trigger and delivery events.
    Webhook(WebhookEvent),
    /// Batch processing events.
    Batch(BatchEvent),
}

impl BusEvent {
    /// Returns the dot-separated event type string for this event.
    ///
    /// Maps each leaf variant to a `"domain.action"` string matching the
    /// `broadcast_actions` TOML config format (e.g., `"skill.invoked"`,
    /// `"session.created"`, `"channel.message_received"`).
    ///
    /// The match is exhaustive, so the compiler will catch any future variants
    /// added to `BusEvent`.
    pub fn event_type_string(&self) -> &'static str {
        match self {
            BusEvent::Session(SessionEvent::Created { .. }) => "session.created",
            BusEvent::Session(SessionEvent::Closed { .. }) => "session.closed",
            BusEvent::Channel(ChannelEvent::MessageReceived { .. }) => "channel.message_received",
            BusEvent::Channel(ChannelEvent::MessageSent { .. }) => "channel.message_sent",
            BusEvent::Skill(SkillEvent::Invoked { .. }) => "skill.invoked",
            BusEvent::Skill(SkillEvent::Completed { .. }) => "skill.completed",
            BusEvent::Node(NodeEvent::Connected { .. }) => "node.connected",
            BusEvent::Node(NodeEvent::Disconnected { .. }) => "node.disconnected",
            BusEvent::Node(NodeEvent::Paired { .. }) => "node.paired",
            BusEvent::Node(NodeEvent::PairingFailed { .. }) => "node.pairing_failed",
            BusEvent::Node(NodeEvent::Stale { .. }) => "node.stale",
            BusEvent::Webhook(WebhookEvent::Triggered { .. }) => "webhook.triggered",
            BusEvent::Webhook(WebhookEvent::DeliveryAttempted { .. }) => "webhook.delivery_attempted",
            BusEvent::Batch(BatchEvent::Submitted { .. }) => "batch.submitted",
            BusEvent::Batch(BatchEvent::Completed { .. }) => "batch.completed",
        }
    }
}

// --- Session events ---

/// Events related to session lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionEvent {
    /// A new session was created.
    Created {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Session identifier.
        session_id: String,
        /// Channel the session was created on.
        channel: String,
    },
    /// A session was closed.
    Closed {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Session identifier.
        session_id: String,
    },
}

// --- Channel events ---

/// Events related to channel message flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelEvent {
    /// A message was received from a channel.
    MessageReceived {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Channel name (e.g., "telegram", "gateway").
        channel: String,
        /// Sender identifier from the channel.
        sender_id: String,
        /// Message content (for bridging). None if not applicable.
        #[serde(default)]
        content: Option<String>,
        /// Human-readable sender name (for bridging attribution).
        #[serde(default)]
        sender_name: Option<String>,
        /// Whether this message was forwarded by the bridge (loop prevention).
        #[serde(default)]
        is_bridged: bool,
    },
    /// A message was sent through a channel.
    MessageSent {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Channel name.
        channel: String,
    },
}

// --- Skill events ---

/// Events related to skill (WASM plugin) execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillEvent {
    /// A skill was invoked.
    Invoked {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Name of the skill.
        skill_name: String,
        /// Session that triggered the invocation.
        session_id: String,
    },
    /// A skill invocation completed.
    Completed {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Name of the skill.
        skill_name: String,
        /// Whether the skill returned an error.
        is_error: bool,
    },
}

// --- Node events ---

/// Events related to node connections in the fleet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeEvent {
    /// A node connected to the agent.
    Connected {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Node identifier.
        node_id: String,
    },
    /// A node disconnected from the agent.
    Disconnected {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Node identifier.
        node_id: String,
        /// Reason for disconnection.
        reason: String,
    },
    /// A new node was successfully paired.
    Paired {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Node identifier.
        node_id: String,
        /// Node display name.
        name: String,
    },
    /// A pairing attempt failed.
    PairingFailed {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Reason for failure.
        reason: String,
    },
    /// A node has become stale (missed heartbeats).
    Stale {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Node identifier.
        node_id: String,
        /// Seconds since last heartbeat.
        last_seen_secs_ago: u64,
    },
}

// --- Webhook events ---

/// Events related to webhook triggers and delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebhookEvent {
    /// A webhook was triggered.
    Triggered {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Webhook identifier.
        webhook_id: String,
        /// Type of event that triggered the webhook.
        event_type: String,
    },
    /// A webhook delivery was attempted.
    DeliveryAttempted {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Webhook identifier.
        webhook_id: String,
        /// HTTP status code from delivery attempt.
        status_code: u16,
        /// Whether delivery was successful.
        success: bool,
    },
}

// --- Batch events ---

/// Events related to batch processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BatchEvent {
    /// A batch was submitted for processing.
    Submitted {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Batch identifier.
        batch_id: String,
        /// Number of items in the batch.
        item_count: usize,
    },
    /// A batch completed processing.
    Completed {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Batch identifier.
        batch_id: String,
        /// Number of successfully processed items.
        success_count: usize,
        /// Number of items that failed.
        error_count: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_six_bus_event_variants_exist() {
        let _session = BusEvent::Session(SessionEvent::Created {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            session_id: "sess-1".into(),
            channel: "telegram".into(),
        });

        let _channel = BusEvent::Channel(ChannelEvent::MessageReceived {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            channel: "gateway".into(),
            sender_id: "user-1".into(),
            content: None,
            sender_name: None,
            is_bridged: false,
        });

        let _skill = BusEvent::Skill(SkillEvent::Invoked {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            skill_name: "weather".into(),
            session_id: "sess-1".into(),
        });

        let _node = BusEvent::Node(NodeEvent::Connected {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            node_id: "node-1".into(),
        });

        let _webhook = BusEvent::Webhook(WebhookEvent::Triggered {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            webhook_id: "wh-1".into(),
            event_type: "session.created".into(),
        });

        let _batch = BusEvent::Batch(BatchEvent::Submitted {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            batch_id: "batch-1".into(),
            item_count: 10,
        });
    }

    #[test]
    fn bus_event_implements_clone() {
        let event = BusEvent::Session(SessionEvent::Created {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            session_id: "sess-1".into(),
            channel: "telegram".into(),
        });
        let cloned = event.clone();
        // Verify clone is independent (Debug format should match).
        assert_eq!(format!("{:?}", event), format!("{:?}", cloned));
    }

    #[test]
    fn session_event_serialize_deserialize_roundtrip() {
        let event = BusEvent::Session(SessionEvent::Created {
            event_id: "evt-123".into(),
            timestamp: "2026-03-05T00:00:00Z".into(),
            session_id: "sess-abc".into(),
            channel: "telegram".into(),
        });

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: BusEvent = serde_json::from_str(&json).unwrap();

        match deserialized {
            BusEvent::Session(SessionEvent::Created {
                event_id,
                timestamp,
                session_id,
                channel,
            }) => {
                assert_eq!(event_id, "evt-123");
                assert_eq!(timestamp, "2026-03-05T00:00:00Z");
                assert_eq!(session_id, "sess-abc");
                assert_eq!(channel, "telegram");
            }
            _ => panic!("expected Session::Created"),
        }
    }

    #[test]
    fn new_event_id_is_unique() {
        let id1 = new_event_id();
        let id2 = new_event_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn now_timestamp_is_nonempty() {
        let ts = now_timestamp();
        assert!(!ts.is_empty());
    }

    #[test]
    fn event_type_string_all_variants() {
        let cases: Vec<(BusEvent, &str)> = vec![
            (
                BusEvent::Session(SessionEvent::Created {
                    event_id: String::new(),
                    timestamp: String::new(),
                    session_id: String::new(),
                    channel: String::new(),
                }),
                "session.created",
            ),
            (
                BusEvent::Session(SessionEvent::Closed {
                    event_id: String::new(),
                    timestamp: String::new(),
                    session_id: String::new(),
                }),
                "session.closed",
            ),
            (
                BusEvent::Channel(ChannelEvent::MessageReceived {
                    event_id: String::new(),
                    timestamp: String::new(),
                    channel: String::new(),
                    sender_id: String::new(),
                    content: None,
                    sender_name: None,
                    is_bridged: false,
                }),
                "channel.message_received",
            ),
            (
                BusEvent::Channel(ChannelEvent::MessageSent {
                    event_id: String::new(),
                    timestamp: String::new(),
                    channel: String::new(),
                }),
                "channel.message_sent",
            ),
            (
                BusEvent::Skill(SkillEvent::Invoked {
                    event_id: String::new(),
                    timestamp: String::new(),
                    skill_name: String::new(),
                    session_id: String::new(),
                }),
                "skill.invoked",
            ),
            (
                BusEvent::Skill(SkillEvent::Completed {
                    event_id: String::new(),
                    timestamp: String::new(),
                    skill_name: String::new(),
                    is_error: false,
                }),
                "skill.completed",
            ),
            (
                BusEvent::Node(NodeEvent::Connected {
                    event_id: String::new(),
                    timestamp: String::new(),
                    node_id: String::new(),
                }),
                "node.connected",
            ),
            (
                BusEvent::Node(NodeEvent::Disconnected {
                    event_id: String::new(),
                    timestamp: String::new(),
                    node_id: String::new(),
                    reason: String::new(),
                }),
                "node.disconnected",
            ),
            (
                BusEvent::Node(NodeEvent::Paired {
                    event_id: String::new(),
                    timestamp: String::new(),
                    node_id: String::new(),
                    name: String::new(),
                }),
                "node.paired",
            ),
            (
                BusEvent::Node(NodeEvent::PairingFailed {
                    event_id: String::new(),
                    timestamp: String::new(),
                    reason: String::new(),
                }),
                "node.pairing_failed",
            ),
            (
                BusEvent::Node(NodeEvent::Stale {
                    event_id: String::new(),
                    timestamp: String::new(),
                    node_id: String::new(),
                    last_seen_secs_ago: 0,
                }),
                "node.stale",
            ),
            (
                BusEvent::Webhook(WebhookEvent::Triggered {
                    event_id: String::new(),
                    timestamp: String::new(),
                    webhook_id: String::new(),
                    event_type: String::new(),
                }),
                "webhook.triggered",
            ),
            (
                BusEvent::Webhook(WebhookEvent::DeliveryAttempted {
                    event_id: String::new(),
                    timestamp: String::new(),
                    webhook_id: String::new(),
                    status_code: 0,
                    success: false,
                }),
                "webhook.delivery_attempted",
            ),
            (
                BusEvent::Batch(BatchEvent::Submitted {
                    event_id: String::new(),
                    timestamp: String::new(),
                    batch_id: String::new(),
                    item_count: 0,
                }),
                "batch.submitted",
            ),
            (
                BusEvent::Batch(BatchEvent::Completed {
                    event_id: String::new(),
                    timestamp: String::new(),
                    batch_id: String::new(),
                    success_count: 0,
                    error_count: 0,
                }),
                "batch.completed",
            ),
        ];

        for (event, expected) in &cases {
            assert_eq!(
                event.event_type_string(),
                *expected,
                "mismatch for {:?}",
                std::mem::discriminant(event)
            );
        }
    }
}
