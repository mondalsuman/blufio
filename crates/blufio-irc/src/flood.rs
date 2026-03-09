// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Rate-limited message sending for IRC to prevent flood-based bans.
//!
//! IRC servers aggressively ban clients that send too many messages in a short
//! period ("flooding"). This module provides a queue-based sender that enforces
//! a configurable interval between outbound messages.

use std::sync::Arc;
use std::time::Duration;

use blufio_core::error::BlufioError;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::splitter;

/// A rate-limited IRC message sender.
///
/// Messages are queued and sent at a controlled rate to prevent IRC flood bans.
/// Long messages are automatically split at word boundaries before sending.
pub struct FloodProtectedSender {
    queue_tx: mpsc::Sender<(String, String)>,
}

impl FloodProtectedSender {
    /// Creates a new flood-protected sender.
    ///
    /// Spawns a background task that dequeues messages and sends them via the
    /// IRC client with the configured rate limit between each send.
    ///
    /// # Arguments
    /// - `client`: Arc-wrapped IRC client for sending messages.
    /// - `rate_limit_ms`: Minimum milliseconds between consecutive sends.
    /// - `nick`: The bot's nickname (used for message splitting calculations).
    pub fn new(client: Arc<irc::client::Client>, rate_limit_ms: u64, nick: String) -> Self {
        let (queue_tx, mut queue_rx) = mpsc::channel::<(String, String)>(256);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(rate_limit_ms));
            // The first tick completes immediately; consume it.
            interval.tick().await;

            while let Some((target, message)) = queue_rx.recv().await {
                let chunks = splitter::split_message(&target, &nick, &message, 512);

                for chunk in chunks {
                    // Wait for the rate limit tick before sending.
                    interval.tick().await;

                    if let Err(e) = client.send_privmsg(&target, &chunk) {
                        warn!(
                            target = %target,
                            error = %e,
                            "failed to send IRC PRIVMSG"
                        );
                    } else {
                        debug!(target = %target, len = chunk.len(), "sent IRC PRIVMSG chunk");
                    }
                }
            }
        });

        Self { queue_tx }
    }

    /// Queue a message for rate-limited delivery.
    ///
    /// Returns immediately after placing the message in the send queue.
    /// The message will be split at word boundaries and sent at the configured
    /// rate limit.
    pub async fn send(&self, target: &str, message: &str) -> Result<(), BlufioError> {
        self.queue_tx
            .send((target.to_string(), message.to_string()))
            .await
            .map_err(|_| BlufioError::channel_connection_lost("irc"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn queue_accepts_messages() {
        // We cannot easily create a real irc::client::Client in tests,
        // but we can verify the queue side works by creating a channel directly.
        let (tx, mut rx) = mpsc::channel::<(String, String)>(16);

        let sender = FloodProtectedSender { queue_tx: tx };

        sender.send("#test", "hello").await.unwrap();
        sender.send("#test", "world").await.unwrap();

        let (target1, msg1) = rx.recv().await.unwrap();
        assert_eq!(target1, "#test");
        assert_eq!(msg1, "hello");

        let (target2, msg2) = rx.recv().await.unwrap();
        assert_eq!(target2, "#test");
        assert_eq!(msg2, "world");
    }
}
