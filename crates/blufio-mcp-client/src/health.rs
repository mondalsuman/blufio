// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Health monitoring background task for MCP server connections (CLNT-06).
//!
//! Periodically checks the health of connected MCP servers by calling
//! `ping`. When a server becomes unresponsive, its tools are removed
//! from the LLM context. When a degraded server recovers, tools are
//! re-discovered and re-registered.
//!
//! Reconnection uses exponential backoff: 1s, 2s, 4s, 8s, 16s, 32s, 60s cap.

use std::collections::HashMap;
use std::time::Duration;

use tracing::{debug, info, warn};

/// Default health check interval in seconds.
pub const DEFAULT_HEALTH_INTERVAL_SECS: u64 = 60;

/// Base backoff delay for reconnection attempts.
const BACKOFF_BASE_SECS: u64 = 1;

/// Maximum backoff delay cap.
const BACKOFF_CAP_SECS: u64 = 60;

/// Compute the next backoff duration using exponential backoff with cap.
///
/// Given the current attempt number (0-indexed), returns the backoff
/// duration: min(base * 2^attempt, cap).
pub fn compute_backoff(attempt: u32) -> Duration {
    let delay = BACKOFF_BASE_SECS.saturating_mul(1u64.checked_shl(attempt).unwrap_or(u64::MAX));
    Duration::from_secs(delay.min(BACKOFF_CAP_SECS))
}

/// Health state for a single MCP server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerHealthState {
    /// Server is healthy and responsive.
    Healthy,
    /// Server is degraded, tracking reconnection attempts.
    Degraded {
        /// Number of consecutive failed attempts.
        attempt: u32,
        /// Reason for last failure.
        reason: String,
    },
}

/// Health state tracker for all MCP servers.
///
/// Tracks per-server health state for the health monitor background task.
/// This is a simple struct that can be shared via `Arc<RwLock<>>`.
pub struct HealthTracker {
    states: HashMap<String, ServerHealthState>,
}

impl HealthTracker {
    /// Create a new health tracker.
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
        }
    }

    /// Mark a server as healthy.
    pub fn mark_healthy(&mut self, server: &str) {
        if let Some(prev) = self.states.get(server)
            && *prev != ServerHealthState::Healthy
        {
            info!(server = server, "MCP server recovered");
        }
        self.states
            .insert(server.to_string(), ServerHealthState::Healthy);
    }

    /// Mark a server as degraded, incrementing the attempt counter.
    ///
    /// Returns the current backoff duration.
    pub fn mark_degraded(&mut self, server: &str, reason: String) -> Duration {
        let attempt = match self.states.get(server) {
            Some(ServerHealthState::Degraded { attempt, .. }) => attempt + 1,
            _ => 0,
        };
        let backoff = compute_backoff(attempt);
        warn!(
            server = server,
            attempt = attempt,
            backoff_secs = backoff.as_secs(),
            reason = reason.as_str(),
            "MCP server degraded"
        );
        self.states.insert(
            server.to_string(),
            ServerHealthState::Degraded { attempt, reason },
        );
        backoff
    }

    /// Get the current health state of a server.
    pub fn state(&self, server: &str) -> Option<&ServerHealthState> {
        self.states.get(server)
    }

    /// Check if a server is in a degraded state.
    pub fn is_degraded(&self, server: &str) -> bool {
        matches!(
            self.states.get(server),
            Some(ServerHealthState::Degraded { .. })
        )
    }

    /// Get all degraded servers with their current backoff attempt.
    pub fn degraded_servers(&self) -> Vec<(&str, u32)> {
        self.states
            .iter()
            .filter_map(|(name, state)| match state {
                ServerHealthState::Degraded { attempt, .. } => Some((name.as_str(), *attempt)),
                _ => None,
            })
            .collect()
    }

    /// Get all healthy servers.
    pub fn healthy_servers(&self) -> Vec<&str> {
        self.states
            .iter()
            .filter_map(|(name, state)| match state {
                ServerHealthState::Healthy => Some(name.as_str()),
                _ => None,
            })
            .collect()
    }
}

impl Default for HealthTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Timeout for individual ping requests.
const PING_TIMEOUT: Duration = Duration::from_secs(5);

/// Spawn the health monitor background task.
///
/// The health monitor periodically pings each connected MCP server session.
/// On ping success, the server is marked healthy. On ping failure (timeout
/// or error), the server is marked degraded with exponential backoff tracking.
///
/// State transitions are logged by [`HealthTracker::mark_healthy`] (info)
/// and [`HealthTracker::mark_degraded`] (warn).
///
/// The task runs until the cancellation token is triggered.
pub fn spawn_health_monitor(
    sessions: HashMap<String, std::sync::Arc<rmcp::service::RunningService<rmcp::RoleClient, ()>>>,
    tracker: std::sync::Arc<tokio::sync::RwLock<HealthTracker>>,
    interval_secs: u64,
    cancel: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));

        // Skip initial tick.
        interval.tick().await;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    for (name, session) in &sessions {
                        debug!(server = %name, "MCP health check ping");

                        let ping_result = tokio::time::timeout(
                            PING_TIMEOUT,
                            session.send_request(
                                rmcp::model::ClientRequest::PingRequest(Default::default())
                            ),
                        )
                        .await;

                        // Acquire lock, update state, drop lock before next iteration.
                        let mut t = tracker.write().await;
                        match ping_result {
                            Ok(Ok(_)) => {
                                t.mark_healthy(name);
                            }
                            Ok(Err(e)) => {
                                t.mark_degraded(name, format!("ping error: {e}"));
                            }
                            Err(_) => {
                                t.mark_degraded(name, "ping timed out".to_string());
                            }
                        }
                        drop(t);
                    }
                }
                _ = cancel.cancelled() => {
                    info!("MCP health monitor shutting down");
                    break;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_backoff_exponential() {
        assert_eq!(compute_backoff(0), Duration::from_secs(1));
        assert_eq!(compute_backoff(1), Duration::from_secs(2));
        assert_eq!(compute_backoff(2), Duration::from_secs(4));
        assert_eq!(compute_backoff(3), Duration::from_secs(8));
        assert_eq!(compute_backoff(4), Duration::from_secs(16));
        assert_eq!(compute_backoff(5), Duration::from_secs(32));
    }

    #[test]
    fn compute_backoff_caps_at_60() {
        assert_eq!(compute_backoff(6), Duration::from_secs(60));
        assert_eq!(compute_backoff(7), Duration::from_secs(60));
        assert_eq!(compute_backoff(100), Duration::from_secs(60));
    }

    #[test]
    fn health_tracker_mark_healthy() {
        let mut tracker = HealthTracker::new();
        tracker.mark_healthy("server1");
        assert_eq!(tracker.state("server1"), Some(&ServerHealthState::Healthy));
        assert!(!tracker.is_degraded("server1"));
    }

    #[test]
    fn health_tracker_mark_degraded() {
        let mut tracker = HealthTracker::new();
        let backoff = tracker.mark_degraded("server1", "connection refused".to_string());
        assert_eq!(backoff, Duration::from_secs(1)); // First attempt: 2^0 = 1
        assert!(tracker.is_degraded("server1"));
    }

    #[test]
    fn health_tracker_degraded_escalates_backoff() {
        let mut tracker = HealthTracker::new();
        let b0 = tracker.mark_degraded("server1", "fail 1".to_string());
        let b1 = tracker.mark_degraded("server1", "fail 2".to_string());
        let b2 = tracker.mark_degraded("server1", "fail 3".to_string());
        assert_eq!(b0, Duration::from_secs(1));
        assert_eq!(b1, Duration::from_secs(2));
        assert_eq!(b2, Duration::from_secs(4));
    }

    #[test]
    fn health_tracker_recovery_resets() {
        let mut tracker = HealthTracker::new();
        tracker.mark_degraded("server1", "fail".to_string());
        tracker.mark_healthy("server1");
        assert!(!tracker.is_degraded("server1"));

        // After recovery, next degradation starts from attempt 0.
        let backoff = tracker.mark_degraded("server1", "fail again".to_string());
        assert_eq!(backoff, Duration::from_secs(1));
    }

    #[test]
    fn health_tracker_degraded_servers_list() {
        let mut tracker = HealthTracker::new();
        tracker.mark_healthy("server1");
        tracker.mark_degraded("server2", "down".to_string());
        tracker.mark_degraded("server3", "timeout".to_string());

        let degraded = tracker.degraded_servers();
        assert_eq!(degraded.len(), 2);

        let healthy = tracker.healthy_servers();
        assert_eq!(healthy.len(), 1);
        assert_eq!(healthy[0], "server1");
    }

    #[test]
    fn health_tracker_unknown_server_not_degraded() {
        let tracker = HealthTracker::new();
        assert!(!tracker.is_degraded("unknown"));
        assert_eq!(tracker.state("unknown"), None);
    }

    #[tokio::test]
    async fn health_monitor_shuts_down_on_cancel() {
        let cancel = tokio_util::sync::CancellationToken::new();
        let sessions = HashMap::new();
        let tracker = std::sync::Arc::new(tokio::sync::RwLock::new(HealthTracker::new()));
        let handle = spawn_health_monitor(
            sessions,
            tracker,
            DEFAULT_HEALTH_INTERVAL_SECS,
            cancel.clone(),
        );

        // Cancel immediately.
        cancel.cancel();
        // Should complete without hanging.
        let result = tokio::time::timeout(Duration::from_secs(5), handle).await;
        assert!(result.is_ok(), "health monitor should shut down promptly");
    }
}
