// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Notification infrastructure for the MCP server.
//!
//! Two notification types are supported:
//!
//! - **tools/list_changed** (SRVR-13): Emitted when the tool set changes
//!   (e.g., skill install/discovery). Uses a [`tokio::sync::watch`] channel
//!   with a generation counter.
//!
//! - **Progress** (SRVR-14): Plumbing for reporting progress on long-running
//!   tool invocations. Currently logs progress; will be wired to the MCP
//!   transport when WASM tools support progress callbacks.

use tokio::sync::watch;

// ── tools/list_changed ──────────────────────────────────────────────

/// Creates a channel pair for signaling tool list changes.
///
/// The [`ToolsChangedSender`] should be held by serve.rs (or wherever
/// tool registration happens). Call [`ToolsChangedSender::notify`] when
/// tools are added or removed.
///
/// The [`ToolsChangedReceiver`] is given to the MCP handler so it can
/// detect changes and forward `notifications/tools/list_changed` to
/// connected MCP clients.
pub fn tools_changed_channel() -> (ToolsChangedSender, ToolsChangedReceiver) {
    let (tx, rx) = watch::channel(0u64);
    (ToolsChangedSender(tx), ToolsChangedReceiver(rx))
}

/// Sender half of the tools-changed notification channel.
///
/// Call [`notify`](Self::notify) to signal that the tool set has changed.
/// Each call increments the internal generation counter.
pub struct ToolsChangedSender(watch::Sender<u64>);

impl ToolsChangedSender {
    /// Signals that the tool set has changed.
    ///
    /// Increments the generation counter, which wakes any waiting receiver.
    pub fn notify(&self) {
        self.0.send_modify(|v| *v += 1);
    }

    /// Returns the current generation counter value.
    pub fn generation(&self) -> u64 {
        *self.0.borrow()
    }
}

/// Receiver half of the tools-changed notification channel.
///
/// Call [`changed`](Self::changed) to wait for the next change signal.
pub struct ToolsChangedReceiver(watch::Receiver<u64>);

impl ToolsChangedReceiver {
    /// Waits until the tool set changes.
    ///
    /// Returns `Ok(())` when a new generation is signaled, or `Err` if
    /// the sender has been dropped.
    pub async fn changed(&mut self) -> Result<(), watch::error::RecvError> {
        self.0.changed().await
    }

    /// Returns the current generation counter value.
    pub fn generation(&self) -> u64 {
        *self.0.borrow()
    }
}

// ── Progress notifications ──────────────────────────────────────────

/// Progress reporter for long-running tool invocations.
///
/// Created per tool invocation with the `progress_token` from the MCP
/// request (if any). When WASM tools support progress callbacks, this
/// reporter will be passed into the invocation.
///
/// Currently logs progress via tracing. The actual MCP transport wiring
/// requires access to the session peer, which will be connected in a
/// future phase when `BlufioTool::invoke` accepts a progress callback.
pub struct ProgressReporter {
    progress_token: Option<String>,
}

impl ProgressReporter {
    /// Creates a new progress reporter.
    ///
    /// If `progress_token` is `None`, all [`report`](Self::report) calls
    /// are no-ops.
    pub fn new(progress_token: Option<String>) -> Self {
        Self { progress_token }
    }

    /// Returns the progress token, if any.
    pub fn token(&self) -> Option<&str> {
        self.progress_token.as_deref()
    }

    /// Reports progress for a long-running operation.
    ///
    /// No-op if no progress token was provided in the request.
    pub async fn report(&self, progress: u32, total: u32, message: &str) {
        if let Some(ref token) = self.progress_token {
            tracing::debug!(
                progress_token = token,
                progress,
                total,
                message,
                "progress notification (not yet wired to MCP transport)"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── tools_changed_channel tests ──────────────────────────────

    #[test]
    fn channel_starts_at_generation_zero() {
        let (tx, rx) = tools_changed_channel();
        assert_eq!(tx.generation(), 0);
        assert_eq!(rx.generation(), 0);
    }

    #[test]
    fn notify_increments_generation() {
        let (tx, _rx) = tools_changed_channel();
        tx.notify();
        assert_eq!(tx.generation(), 1);
        tx.notify();
        assert_eq!(tx.generation(), 2);
    }

    #[tokio::test]
    async fn receiver_detects_change() {
        let (tx, mut rx) = tools_changed_channel();
        tx.notify();
        let result = rx.changed().await;
        assert!(result.is_ok());
        assert_eq!(rx.generation(), 1);
    }

    #[tokio::test]
    async fn receiver_detects_multiple_changes() {
        let (tx, mut rx) = tools_changed_channel();
        tx.notify();
        tx.notify();
        tx.notify();
        let result = rx.changed().await;
        assert!(result.is_ok());
        // After changed(), the receiver sees the latest generation.
        assert_eq!(rx.generation(), 3);
    }

    #[tokio::test]
    async fn receiver_errors_when_sender_dropped() {
        let (tx, mut rx) = tools_changed_channel();
        drop(tx);
        let result = rx.changed().await;
        assert!(result.is_err());
    }

    // ── ProgressReporter tests ──────────────────────────────────

    #[test]
    fn progress_reporter_with_token() {
        let reporter = ProgressReporter::new(Some("tok-1".to_string()));
        assert_eq!(reporter.token(), Some("tok-1"));
    }

    #[test]
    fn progress_reporter_without_token() {
        let reporter = ProgressReporter::new(None);
        assert!(reporter.token().is_none());
    }

    #[tokio::test]
    async fn progress_report_with_token_does_not_panic() {
        let reporter = ProgressReporter::new(Some("tok-1".to_string()));
        // Should not panic; just logs.
        reporter.report(50, 100, "halfway done").await;
    }

    #[tokio::test]
    async fn progress_report_without_token_is_noop() {
        let reporter = ProgressReporter::new(None);
        // No token = no-op, should not panic.
        reporter.report(0, 100, "start").await;
    }
}
