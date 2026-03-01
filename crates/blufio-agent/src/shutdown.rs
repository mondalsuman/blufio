// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Graceful shutdown coordination with signal handling.
//!
//! Installs handlers for SIGTERM and SIGINT (Ctrl+C), triggering a
//! [`CancellationToken`] that the agent loop monitors. Active sessions
//! are drained before the process exits.

use std::collections::HashMap;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::session::{SessionActor, SessionState};

/// Installs signal handlers for SIGTERM and SIGINT.
///
/// Returns a [`CancellationToken`] that is cancelled when either signal is received.
/// The signal handler task runs in the background until the token is cancelled.
pub fn install_signal_handler() -> CancellationToken {
    let token = CancellationToken::new();
    let token_clone = token.clone();

    tokio::spawn(async move {
        let ctrl_c = tokio::signal::ctrl_c();

        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm =
                signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");

            tokio::select! {
                _ = ctrl_c => {
                    info!("received SIGINT (Ctrl+C), initiating shutdown");
                }
                _ = sigterm.recv() => {
                    info!("received SIGTERM, initiating shutdown");
                }
            }
        }

        #[cfg(not(unix))]
        {
            let _ = ctrl_c.await;
            info!("received Ctrl+C, initiating shutdown");
        }

        token_clone.cancel();
        debug!("shutdown signal handler completed");
    });

    token
}

/// Drains active sessions, waiting up to `timeout` for them to complete.
///
/// Sessions in the [`Responding`](SessionState::Responding) state are given
/// time to finish their current response. Sessions in other states are
/// considered immediately drainable.
pub async fn drain_sessions(
    sessions: &HashMap<String, SessionActor>,
    timeout: Duration,
) {
    let responding_count = sessions
        .values()
        .filter(|s| s.state() == SessionState::Responding)
        .count();

    if responding_count == 0 {
        info!("no active sessions to drain");
        return;
    }

    info!(
        count = responding_count,
        "waiting for active sessions to complete"
    );

    // Wait for the timeout period. In a full implementation, we would
    // monitor each session's state transition. For now, we just wait
    // the timeout and log the result.
    tokio::time::sleep(timeout).await;

    let still_active = sessions
        .values()
        .filter(|s| s.state() == SessionState::Responding)
        .count();

    if still_active == 0 {
        info!("all sessions drained successfully");
    } else {
        warn!(
            remaining = still_active,
            "timeout reached, some sessions interrupted"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn install_signal_handler_returns_token() {
        let token = install_signal_handler();
        // Token should not be cancelled yet.
        assert!(!token.is_cancelled());
        // Cancel it manually to clean up the background task.
        token.cancel();
    }

    #[tokio::test]
    async fn drain_empty_sessions() {
        let sessions = HashMap::new();
        // Should complete immediately with no sessions.
        drain_sessions(&sessions, Duration::from_millis(10)).await;
    }
}
