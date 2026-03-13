// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

//! Internal typed event bus for the Blufio agent framework.
//!
//! Provides a dual-channel pub/sub system:
//! - **Broadcast** (fire-and-forget): Subscribers may lag and miss events, with logged warnings.
//! - **Reliable (mpsc)**: Subscribers are guaranteed delivery; events are never silently dropped.
//!
//! # Usage
//!
//! ```rust
//! use blufio_bus::{EventBus, BusEvent, SessionEvent, new_event_id, now_timestamp};
//! use std::sync::Arc;
//!
//! # tokio_test::block_on(async {
//! let bus = Arc::new(EventBus::new(1024));
//!
//! // Fire-and-forget subscriber
//! let mut rx = bus.subscribe();
//!
//! // Reliable subscriber (guaranteed delivery)
//! let mut reliable_rx = bus.subscribe_reliable(256).await;
//!
//! bus.publish(BusEvent::Session(SessionEvent::Created {
//!     event_id: new_event_id(),
//!     timestamp: now_timestamp(),
//!     session_id: "sess-1".into(),
//!     channel: "telegram".into(),
//! })).await;
//! # });
//! ```

pub mod events;

pub use events::*;

use tokio::sync::{RwLock, broadcast, mpsc};
use tracing::error;

/// Internal event bus using tokio broadcast + mpsc channels.
///
/// The bus fans out published events to:
/// - All broadcast subscribers (fire-and-forget, may lag)
/// - All reliable mpsc subscribers (guaranteed delivery)
pub struct EventBus {
    /// Broadcast sender for fire-and-forget subscribers.
    broadcast_tx: broadcast::Sender<BusEvent>,
    /// Reliable mpsc senders for guaranteed-delivery subscribers.
    reliable_txs: RwLock<Vec<mpsc::Sender<BusEvent>>>,
}

impl EventBus {
    /// Create a new event bus with the given broadcast channel capacity.
    ///
    /// The capacity determines how many events can be buffered in the broadcast
    /// channel before slow subscribers start lagging.
    pub fn new(capacity: usize) -> Self {
        let (broadcast_tx, _) = broadcast::channel(capacity);
        Self {
            broadcast_tx,
            reliable_txs: RwLock::new(Vec::new()),
        }
    }

    /// Publish an event to all subscribers.
    ///
    /// Sends to the broadcast channel (fire-and-forget) and to each
    /// registered reliable mpsc sender. If a reliable sender's channel
    /// is full or closed, an error is logged but the publish continues.
    pub async fn publish(&self, event: BusEvent) {
        // Fire-and-forget: broadcast (ignore error = no subscribers)
        let _ = self.broadcast_tx.send(event.clone());

        // Reliable: each mpsc sender
        let txs = self.reliable_txs.read().await;
        for tx in txs.iter() {
            if tx.try_send(event.clone()).is_err() {
                error!("reliable subscriber dropped event — channel full or closed");
            }
        }
    }

    /// Subscribe for fire-and-forget event delivery.
    ///
    /// Returns a broadcast receiver. If the receiver falls behind, it will
    /// receive `RecvError::Lagged(n)` indicating `n` events were skipped.
    pub fn subscribe(&self) -> broadcast::Receiver<BusEvent> {
        self.broadcast_tx.subscribe()
    }

    /// Subscribe for reliable (guaranteed) event delivery.
    ///
    /// Returns an mpsc receiver with the given buffer size. Events are
    /// guaranteed to be delivered as long as the receiver is consumed
    /// before the buffer fills up.
    pub async fn subscribe_reliable(&self, buffer: usize) -> mpsc::Receiver<BusEvent> {
        let (tx, rx) = mpsc::channel(buffer);
        let mut txs = self.reliable_txs.write().await;
        txs.push(tx);
        rx
    }

    /// Returns the number of active broadcast subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.broadcast_tx.receiver_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_event() -> BusEvent {
        BusEvent::Session(SessionEvent::Created {
            event_id: "test-evt".into(),
            timestamp: "2026-03-05T00:00:00Z".into(),
            session_id: "sess-test".into(),
            channel: "test".into(),
        })
    }

    #[tokio::test]
    async fn test_publish_to_broadcast_subscriber() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        bus.publish(make_test_event()).await;

        let received = rx.recv().await.unwrap();
        match received {
            BusEvent::Session(SessionEvent::Created { session_id, .. }) => {
                assert_eq!(session_id, "sess-test");
            }
            _ => panic!("expected Session::Created"),
        }
    }

    #[tokio::test]
    async fn test_publish_to_reliable_subscriber() {
        let bus = EventBus::new(16);
        let mut reliable_rx = bus.subscribe_reliable(16).await;

        bus.publish(make_test_event()).await;

        let received = reliable_rx.recv().await.unwrap();
        match received {
            BusEvent::Session(SessionEvent::Created { session_id, .. }) => {
                assert_eq!(session_id, "sess-test");
            }
            _ => panic!("expected Session::Created"),
        }
    }

    #[tokio::test]
    async fn test_multiple_broadcast_subscribers() {
        let bus = EventBus::new(16);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.publish(make_test_event()).await;

        let r1 = rx1.recv().await.unwrap();
        let r2 = rx2.recv().await.unwrap();

        // Both should receive the same event.
        assert_eq!(format!("{:?}", r1), format!("{:?}", r2));
    }

    #[tokio::test]
    async fn test_reliable_and_broadcast_coexist() {
        let bus = EventBus::new(16);
        let mut broadcast_rx = bus.subscribe();
        let mut reliable_rx = bus.subscribe_reliable(16).await;

        bus.publish(make_test_event()).await;

        let b = broadcast_rx.recv().await.unwrap();
        let r = reliable_rx.recv().await.unwrap();

        assert_eq!(format!("{:?}", b), format!("{:?}", r));
    }

    #[test]
    fn test_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<EventBus>();
    }

    #[tokio::test]
    async fn test_subscriber_count() {
        let bus = EventBus::new(16);
        assert_eq!(bus.subscriber_count(), 0);

        let _rx1 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 1);

        let _rx2 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 2);
    }

    #[tokio::test]
    async fn test_publish_no_subscribers() {
        let bus = EventBus::new(16);
        // Should not panic even with no subscribers.
        bus.publish(make_test_event()).await;
    }
}
