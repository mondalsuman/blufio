// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Cross-channel message bridge for the Blufio agent framework.
//!
//! Forwards messages between configurable groups of channels with sender
//! attribution and infinite loop prevention.

pub mod formatter;
pub mod router;

use std::collections::HashMap;
use std::sync::Arc;

use blufio_bus::EventBus;
use blufio_config::model::BridgeGroupConfig;
use tokio::sync::mpsc;
use tracing::info;

use crate::router::BridgedMessage;

/// Manages cross-channel bridge groups and their routing rules.
pub struct BridgeManager {
    groups: HashMap<String, BridgeGroupConfig>,
}

impl BridgeManager {
    /// Creates a new bridge manager from the configured groups.
    pub fn new(groups: HashMap<String, BridgeGroupConfig>) -> Self {
        Self { groups }
    }

    /// Returns true if no bridge groups are configured.
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// Determine which channels a message should be bridged to.
    ///
    /// Returns a list of (group_name, target_channels) tuples.
    /// Returns empty if:
    /// - The message is already bridged (loop prevention)
    /// - The source channel is not in any bridge group
    /// - The message is from a bot and exclude_bots is true
    /// - The sender is not in the include_users list (if non-empty)
    pub fn should_bridge(
        &self,
        channel: &str,
        sender_id: &str,
        is_bridged: bool,
        is_bot: bool,
    ) -> Vec<(String, Vec<String>)> {
        if is_bridged {
            return vec![];
        }

        let mut targets = vec![];

        for (group_name, group) in &self.groups {
            if !group.channels.contains(&channel.to_string()) {
                continue;
            }
            if group.exclude_bots && is_bot {
                continue;
            }
            if !group.include_users.is_empty()
                && !group.include_users.contains(&sender_id.to_string())
            {
                continue;
            }
            let target_channels: Vec<String> = group
                .channels
                .iter()
                .filter(|c| c.as_str() != channel)
                .cloned()
                .collect();
            if !target_channels.is_empty() {
                targets.push((group_name.clone(), target_channels));
            }
        }

        targets
    }
}

/// Spawn the bridge loop as a background task.
///
/// Returns a receiver for bridged messages that the caller dispatches
/// via ChannelMultiplexer, and a JoinHandle for the background task.
pub fn spawn_bridge(
    bus: Arc<EventBus>,
    groups: HashMap<String, BridgeGroupConfig>,
) -> (mpsc::Receiver<BridgedMessage>, tokio::task::JoinHandle<()>) {
    let (bridge_tx, bridge_rx) = mpsc::channel(256);
    let handle = tokio::spawn(async move {
        router::run_bridge_loop(bus, groups, bridge_tx).await;
    });
    info!("bridge background task spawned");
    (bridge_rx, handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_bridge_returns_targets() {
        let mut groups = HashMap::new();
        groups.insert(
            "team".to_string(),
            BridgeGroupConfig {
                channels: vec![
                    "telegram".to_string(),
                    "discord".to_string(),
                    "slack".to_string(),
                ],
                exclude_bots: true,
                include_users: vec![],
            },
        );
        let manager = BridgeManager::new(groups);

        let targets = manager.should_bridge("telegram", "alice", false, false);
        assert_eq!(targets.len(), 1);
        let (_, chans) = &targets[0];
        assert!(chans.contains(&"discord".to_string()));
        assert!(chans.contains(&"slack".to_string()));
        assert!(!chans.contains(&"telegram".to_string()));
    }

    #[test]
    fn should_bridge_skips_already_bridged() {
        let mut groups = HashMap::new();
        groups.insert(
            "team".to_string(),
            BridgeGroupConfig {
                channels: vec!["telegram".to_string(), "discord".to_string()],
                exclude_bots: true,
                include_users: vec![],
            },
        );
        let manager = BridgeManager::new(groups);

        let targets = manager.should_bridge("telegram", "alice", true, false);
        assert!(targets.is_empty());
    }

    #[test]
    fn should_bridge_respects_exclude_bots() {
        let mut groups = HashMap::new();
        groups.insert(
            "team".to_string(),
            BridgeGroupConfig {
                channels: vec!["telegram".to_string(), "discord".to_string()],
                exclude_bots: true,
                include_users: vec![],
            },
        );
        let manager = BridgeManager::new(groups);

        let targets = manager.should_bridge("telegram", "bot", false, true);
        assert!(targets.is_empty());
    }

    #[test]
    fn should_bridge_respects_include_users() {
        let mut groups = HashMap::new();
        groups.insert(
            "team".to_string(),
            BridgeGroupConfig {
                channels: vec!["telegram".to_string(), "discord".to_string()],
                exclude_bots: false,
                include_users: vec!["alice".to_string()],
            },
        );
        let manager = BridgeManager::new(groups);

        // Allowed user.
        let targets = manager.should_bridge("telegram", "alice", false, false);
        assert_eq!(targets.len(), 1);

        // Non-allowed user.
        let targets = manager.should_bridge("telegram", "bob", false, false);
        assert!(targets.is_empty());
    }

    #[test]
    fn should_bridge_ignores_unrelated_channel() {
        let mut groups = HashMap::new();
        groups.insert(
            "team".to_string(),
            BridgeGroupConfig {
                channels: vec!["telegram".to_string(), "discord".to_string()],
                exclude_bots: true,
                include_users: vec![],
            },
        );
        let manager = BridgeManager::new(groups);

        let targets = manager.should_bridge("irc", "alice", false, false);
        assert!(targets.is_empty());
    }

    #[test]
    fn is_empty_when_no_groups() {
        let manager = BridgeManager::new(HashMap::new());
        assert!(manager.is_empty());
    }
}
