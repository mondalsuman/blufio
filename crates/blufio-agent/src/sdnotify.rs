// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! systemd sd_notify integration for process lifecycle signaling.
//!
//! Wraps the `sd-notify` crate to provide typed helpers for READY=1,
//! STOPPING=1, STATUS=, and WATCHDOG=1 notifications. All calls are
//! best-effort: failures are logged at debug level and never propagated.
//! On non-systemd platforms (macOS, Docker), all calls are silent no-ops
//! because the sd-notify crate returns Ok(()) when NOTIFY_SOCKET is absent.

use std::time::Duration;

use sd_notify::NotifyState;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

/// Notify systemd that the service is ready.
/// Sends READY=1 paired with a STATUS= summary.
pub fn notify_ready(status: &str) {
    debug!(status, "sd_notify: READY=1");
    if let Err(e) = sd_notify::notify(false, &[NotifyState::Ready, NotifyState::Status(status)]) {
        debug!(error = %e, "sd_notify: failed to send READY (best-effort)");
    }
}

/// Notify systemd that shutdown is beginning.
/// Sends STOPPING=1 paired with a STATUS= message.
pub fn notify_stopping(status: &str) {
    debug!(status, "sd_notify: STOPPING=1");
    if let Err(e) = sd_notify::notify(false, &[NotifyState::Stopping, NotifyState::Status(status)])
    {
        debug!(error = %e, "sd_notify: failed to send STOPPING (best-effort)");
    }
}

/// Send a STATUS= progress message to systemd.
/// Visible in `systemctl status blufio`.
pub fn notify_status(status: &str) {
    debug!(status, "sd_notify: STATUS=");
    if let Err(e) = sd_notify::notify(false, &[NotifyState::Status(status)]) {
        debug!(error = %e, "sd_notify: failed to send STATUS (best-effort)");
    }
}

/// Send WATCHDOG=1 keep-alive ping.
fn notify_watchdog() {
    if let Err(e) = sd_notify::notify(false, &[NotifyState::Watchdog]) {
        debug!(error = %e, "sd_notify: failed to send WATCHDOG (best-effort)");
    }
}

/// Spawn a watchdog ping background task if systemd watchdog is enabled.
///
/// Reads WATCHDOG_USEC from the environment (set by systemd when
/// WatchdogSec= is configured). Pings at half the timeout interval,
/// per systemd best practice.
///
/// Returns `Some(JoinHandle)` if watchdog is active, `None` otherwise.
pub fn spawn_watchdog(cancel: CancellationToken) -> Option<tokio::task::JoinHandle<()>> {
    let mut usec: u64 = 0;
    if !sd_notify::watchdog_enabled(false, &mut usec) || usec == 0 {
        debug!("sd_notify: watchdog not enabled, skipping");
        return None;
    }

    let interval = Duration::from_micros(usec / 2);
    info!(
        watchdog_timeout_sec = usec / 1_000_000,
        ping_interval_sec = interval.as_secs(),
        "watchdog enabled, spawning ping task"
    );

    let handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.tick().await; // skip immediate first tick

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    notify_watchdog();
                }
                _ = cancel.cancelled() => {
                    debug!("sd_notify: watchdog task shutting down");
                    break;
                }
            }
        }
    });

    Some(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notify_functions_no_panic() {
        // All calls should be silent no-ops when NOTIFY_SOCKET is absent.
        notify_ready("test");
        notify_stopping("test");
        notify_status("test");
    }

    #[tokio::test]
    async fn test_spawn_watchdog_returns_none_without_env() {
        // Without WATCHDOG_USEC set, spawn_watchdog should return None.
        let cancel = CancellationToken::new();
        let handle = spawn_watchdog(cancel);
        assert!(handle.is_none());
    }
}
