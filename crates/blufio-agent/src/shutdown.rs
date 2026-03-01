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
/// Polls session states at 100ms intervals until all sessions reach
/// [`Idle`](SessionState::Idle) or [`Draining`](SessionState::Draining),
/// or the timeout is exceeded.
///
/// Sessions in active states ([`Responding`](SessionState::Responding),
/// [`Processing`](SessionState::Processing), [`Receiving`](SessionState::Receiving),
/// [`ToolExecuting`](SessionState::ToolExecuting)) are given time to finish.
/// When the timeout is reached, each undrained session is logged with its
/// ID and current state for debugging.
pub async fn drain_sessions(
    sessions: &HashMap<String, SessionActor>,
    timeout: Duration,
) {
    // Count sessions that are NOT idle and NOT already draining (need draining).
    let active_count = sessions
        .values()
        .filter(|s| {
            let state = s.state();
            state != SessionState::Idle && state != SessionState::Draining
        })
        .count();

    if active_count == 0 {
        info!("no active sessions to drain");
        return;
    }

    info!(
        count = active_count,
        "waiting for active sessions to complete"
    );

    // Poll session states at short intervals until all are idle/draining or timeout.
    let poll_interval = Duration::from_millis(100);
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let still_active = sessions
            .values()
            .filter(|s| {
                let state = s.state();
                state != SessionState::Idle && state != SessionState::Draining
            })
            .count();

        if still_active == 0 {
            info!("all sessions drained successfully");
            return;
        }

        if tokio::time::Instant::now() >= deadline {
            // Log which sessions are still active.
            for (key, session) in sessions {
                let state = session.state();
                if state != SessionState::Idle && state != SessionState::Draining {
                    warn!(
                        session_key = key.as_str(),
                        session_id = session.session_id(),
                        state = %state,
                        "session did not drain within timeout"
                    );
                }
            }
            warn!(
                remaining = still_active,
                "timeout reached, some sessions did not complete"
            );
            return;
        }

        tokio::time::sleep(poll_interval).await;
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
