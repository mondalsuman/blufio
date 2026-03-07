// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Heartbeat monitoring for the node mesh.
//!
//! Periodically sends heartbeat messages with system metrics to all connected
//! peers, and checks for stale nodes that have missed heartbeats.

use std::sync::Arc;
use std::time::Duration;

use blufio_bus::{
    EventBus,
    events::{BusEvent, NodeEvent, new_event_id, now_timestamp},
};
use tracing::{debug, warn};

use crate::config::NodeConfig;
use crate::connection::ConnectionManager;
use crate::types::NodeMessage;

/// System metrics collected for heartbeat messages.
#[derive(Debug, Clone)]
pub struct SystemMetrics {
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub battery_percent: Option<u8>,
    pub uptime_secs: u64,
}

/// Collect system metrics in a blocking context (sysinfo reads from /proc).
pub async fn collect_metrics() -> SystemMetrics {
    tokio::task::spawn_blocking(|| {
        use sysinfo::System;

        let mut sys = System::new();
        sys.refresh_memory();

        SystemMetrics {
            memory_used_mb: sys.used_memory() / (1024 * 1024),
            memory_total_mb: sys.total_memory() / (1024 * 1024),
            battery_percent: None, // Battery reporting is optional; skip for now
            uptime_secs: System::uptime(),
        }
    })
    .await
    .unwrap_or(SystemMetrics {
        memory_used_mb: 0,
        memory_total_mb: 0,
        battery_percent: None,
        uptime_secs: 0,
    })
}

/// Heartbeat monitor that runs as a background task.
pub struct HeartbeatMonitor {
    connection_manager: Arc<ConnectionManager>,
    event_bus: Arc<EventBus>,
    config: NodeConfig,
}

impl HeartbeatMonitor {
    /// Create a new heartbeat monitor.
    pub fn new(
        connection_manager: Arc<ConnectionManager>,
        event_bus: Arc<EventBus>,
        config: NodeConfig,
    ) -> Self {
        Self {
            connection_manager,
            event_bus,
            config,
        }
    }

    /// Start the heartbeat loop (send heartbeats + check for stale nodes).
    ///
    /// This runs forever; spawn it as a background task.
    pub async fn run(&self) {
        let interval = Duration::from_secs(self.config.heartbeat.interval_secs);
        let stale_threshold = Duration::from_secs(self.config.heartbeat.stale_threshold_secs);
        let mut ticker = tokio::time::interval(interval);

        loop {
            ticker.tick().await;

            // Collect metrics (in spawn_blocking)
            let metrics = collect_metrics().await;

            // Send heartbeat to all connected nodes
            let heartbeat = NodeMessage::Heartbeat {
                node_id: self.config.node_id.clone(),
                battery_percent: metrics.battery_percent,
                memory_used_mb: metrics.memory_used_mb,
                memory_total_mb: metrics.memory_total_mb,
                uptime_secs: metrics.uptime_secs,
            };

            self.connection_manager.broadcast(heartbeat).await;
            debug!("heartbeat sent to all connected nodes");

            // Check for stale nodes
            for entry in self.connection_manager.node_states().iter() {
                let node_id = entry.key();
                let state = entry.value();
                let elapsed = state.last_heartbeat.elapsed();

                if elapsed > stale_threshold && state.status != crate::types::NodeStatus::Stale {
                    warn!(
                        node_id = %node_id,
                        last_seen_secs_ago = elapsed.as_secs(),
                        "node is stale"
                    );
                    self.event_bus
                        .publish(BusEvent::Node(NodeEvent::Stale {
                            event_id: new_event_id(),
                            timestamp: now_timestamp(),
                            node_id: node_id.clone(),
                            last_seen_secs_ago: elapsed.as_secs(),
                        }))
                        .await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn collect_metrics_does_not_panic() {
        let metrics = collect_metrics().await;
        // Verify we got some metrics back (uptime always > 0 on a running system)
        assert!(metrics.uptime_secs > 0);
    }
}
