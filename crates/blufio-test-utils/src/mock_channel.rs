// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Mock channel adapter for deterministic testing.
//!
//! `MockChannel` implements `ChannelAdapter` with injectable inbound messages
//! and captured outbound messages for assertion in tests.

use std::collections::VecDeque;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{Mutex, Notify};

use blufio_core::traits::adapter::PluginAdapter;
use blufio_core::traits::channel::ChannelAdapter;
use blufio_core::types::{
    AdapterType, ChannelCapabilities, HealthStatus, InboundMessage, MessageId, OutboundMessage,
};
use blufio_core::BlufioError;

/// A mock messaging channel for testing.
///
/// Provides two queues:
/// - **inbound**: Messages injected via `inject_message()` are returned by `receive()`
/// - **sent**: Messages passed to `send()` are captured and retrievable via `sent_messages()`
pub struct MockChannel {
    inbound: Arc<Mutex<VecDeque<InboundMessage>>>,
    sent: Arc<Mutex<Vec<OutboundMessage>>>,
    notify: Arc<Notify>,
}

impl MockChannel {
    /// Create a new mock channel with empty queues.
    pub fn new() -> Self {
        Self {
            inbound: Arc::new(Mutex::new(VecDeque::new())),
            sent: Arc::new(Mutex::new(Vec::new())),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Inject an inbound message into the receive queue.
    ///
    /// The next call to `receive()` will return this message.
    pub async fn inject_message(&self, msg: InboundMessage) {
        self.inbound.lock().await.push_back(msg);
        self.notify.notify_one();
    }

    /// Get all messages that were sent through `send()`.
    pub async fn sent_messages(&self) -> Vec<OutboundMessage> {
        self.sent.lock().await.clone()
    }

    /// Get the count of sent messages.
    pub async fn sent_count(&self) -> usize {
        self.sent.lock().await.len()
    }

    /// Clear all sent messages.
    pub async fn clear_sent(&self) {
        self.sent.lock().await.clear();
    }
}

impl Default for MockChannel {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PluginAdapter for MockChannel {
    fn name(&self) -> &str {
        "mock-channel"
    }

    fn version(&self) -> semver::Version {
        semver::Version::new(0, 1, 0)
    }

    fn adapter_type(&self) -> AdapterType {
        AdapterType::Channel
    }

    async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
        Ok(HealthStatus::Healthy)
    }

    async fn shutdown(&self) -> Result<(), BlufioError> {
        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for MockChannel {
    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_edit: false,
            supports_typing: false,
            supports_images: false,
            supports_documents: false,
            supports_voice: false,
            max_message_length: None,
        }
    }

    async fn connect(&mut self) -> Result<(), BlufioError> {
        Ok(())
    }

    async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> {
        let id = format!("mock-msg-{}", uuid::Uuid::new_v4());
        self.sent.lock().await.push(msg);
        Ok(MessageId(id))
    }

    async fn receive(&self) -> Result<InboundMessage, BlufioError> {
        loop {
            // Try to pop from queue
            {
                let mut queue = self.inbound.lock().await;
                if let Some(msg) = queue.pop_front() {
                    return Ok(msg);
                }
            }
            // Wait for notification that a new message was injected
            self.notify.notified().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blufio_core::types::MessageContent;

    fn make_inbound(text: &str) -> InboundMessage {
        InboundMessage {
            id: format!("test-{}", uuid::Uuid::new_v4()),
            session_id: None,
            channel: "mock".to_string(),
            sender_id: "test-user".to_string(),
            content: MessageContent::Text(text.to_string()),
            timestamp: chrono::Utc::now().to_rfc3339(),
            metadata: None,
        }
    }

    #[tokio::test]
    async fn receive_returns_injected_messages() {
        let channel = MockChannel::new();
        let msg = make_inbound("hello");
        channel.inject_message(msg).await;

        let received = channel.receive().await.unwrap();
        assert_eq!(received.sender_id, "test-user");
        match &received.content {
            MessageContent::Text(t) => assert_eq!(t, "hello"),
            _ => panic!("expected text content"),
        }
    }

    #[tokio::test]
    async fn send_captures_outbound_messages() {
        let channel = MockChannel::new();
        let msg = OutboundMessage {
            session_id: Some("sess-1".to_string()),
            channel: "mock".to_string(),
            content: "response text".to_string(),
            reply_to: None,
            parse_mode: None,
            metadata: None,
        };

        let msg_id = channel.send(msg).await.unwrap();
        assert!(msg_id.0.starts_with("mock-msg-"));

        let sent = channel.sent_messages().await;
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].content, "response text");
        assert_eq!(sent[0].session_id.as_deref(), Some("sess-1"));
    }

    #[tokio::test]
    async fn capabilities_returns_all_false() {
        let channel = MockChannel::new();
        let caps = channel.capabilities();
        assert!(!caps.supports_edit);
        assert!(!caps.supports_typing);
        assert!(!caps.supports_images);
        assert!(!caps.supports_documents);
        assert!(!caps.supports_voice);
        assert!(caps.max_message_length.is_none());
    }

    #[tokio::test]
    async fn connect_succeeds() {
        let mut channel = MockChannel::new();
        assert!(channel.connect().await.is_ok());
    }

    #[tokio::test]
    async fn multiple_messages_in_order() {
        let channel = MockChannel::new();
        channel.inject_message(make_inbound("first")).await;
        channel.inject_message(make_inbound("second")).await;

        let msg1 = channel.receive().await.unwrap();
        let msg2 = channel.receive().await.unwrap();

        match (&msg1.content, &msg2.content) {
            (MessageContent::Text(t1), MessageContent::Text(t2)) => {
                assert_eq!(t1, "first");
                assert_eq!(t2, "second");
            }
            _ => panic!("expected text content"),
        }
    }

    #[tokio::test]
    async fn receive_waits_for_injection() {
        let channel = Arc::new(MockChannel::new());
        let channel_clone = channel.clone();

        // Spawn a task that will inject a message after a short delay
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            channel_clone.inject_message(make_inbound("delayed")).await;
        });

        // receive() should block until the message is injected
        let received = tokio::time::timeout(
            tokio::time::Duration::from_secs(2),
            channel.receive(),
        )
        .await
        .expect("receive timed out")
        .unwrap();

        match &received.content {
            MessageContent::Text(t) => assert_eq!(t, "delayed"),
            _ => panic!("expected text content"),
        }
    }

    #[tokio::test]
    async fn sent_count_and_clear() {
        let channel = MockChannel::new();
        assert_eq!(channel.sent_count().await, 0);

        let msg = OutboundMessage {
            session_id: None,
            channel: "mock".to_string(),
            content: "test".to_string(),
            reply_to: None,
            parse_mode: None,
            metadata: None,
        };

        channel.send(msg.clone()).await.unwrap();
        channel.send(msg).await.unwrap();
        assert_eq!(channel.sent_count().await, 2);

        channel.clear_sent().await;
        assert_eq!(channel.sent_count().await, 0);
    }
}
