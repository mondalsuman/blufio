// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Mock LLM provider adapter for deterministic testing.
//!
//! `MockProvider` implements `ProviderAdapter` with pre-configured responses,
//! enabling fast, CI-runnable tests without external API calls.

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::stream;
use tokio::sync::Mutex;

use blufio_core::traits::adapter::PluginAdapter;
use blufio_core::traits::provider::ProviderAdapter;
use blufio_core::types::{
    AdapterType, HealthStatus, ProviderRequest, ProviderResponse, ProviderStreamChunk,
    StreamEventType, TokenUsage,
};
use blufio_core::BlufioError;

/// A mock LLM provider that returns pre-configured responses.
///
/// Responses are popped from a FIFO queue. When the queue is empty,
/// a default "mock response" text is returned.
pub struct MockProvider {
    responses: Arc<Mutex<VecDeque<String>>>,
}

impl MockProvider {
    /// Create a new mock provider with an empty response queue.
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Create a mock provider pre-loaded with the given responses.
    pub fn with_responses(responses: Vec<String>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(VecDeque::from(responses))),
        }
    }

    /// Add a response to the end of the queue.
    pub async fn add_response(&self, text: String) {
        self.responses.lock().await.push_back(text);
    }

    /// Pop the next response, or return the default.
    async fn next_response(&self) -> String {
        self.responses
            .lock()
            .await
            .pop_front()
            .unwrap_or_else(|| "mock response".to_string())
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PluginAdapter for MockProvider {
    fn name(&self) -> &str {
        "mock-provider"
    }

    fn version(&self) -> semver::Version {
        semver::Version::new(0, 1, 0)
    }

    fn adapter_type(&self) -> AdapterType {
        AdapterType::Provider
    }

    async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
        Ok(HealthStatus::Healthy)
    }

    async fn shutdown(&self) -> Result<(), BlufioError> {
        Ok(())
    }
}

#[async_trait]
impl ProviderAdapter for MockProvider {
    async fn complete(
        &self,
        request: ProviderRequest,
    ) -> Result<ProviderResponse, BlufioError> {
        let text = self.next_response().await;
        Ok(ProviderResponse {
            id: format!("mock-resp-{}", uuid::Uuid::new_v4()),
            content: text,
            model: request.model,
            stop_reason: Some("end_turn".to_string()),
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 20,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            },
        })
    }

    async fn stream(
        &self,
        request: ProviderRequest,
    ) -> Result<
        Pin<
            Box<
                dyn futures_core::Stream<Item = Result<ProviderStreamChunk, BlufioError>>
                    + Send,
            >,
        >,
        BlufioError,
    > {
        let text = self.next_response().await;
        let model = request.model.clone();

        // Produce a realistic SSE event sequence:
        // MessageStart -> ContentBlockDelta (text) -> MessageDelta (usage + stop) -> MessageStop
        let chunks = vec![
            Ok(ProviderStreamChunk {
                event_type: StreamEventType::MessageStart,
                text: None,
                usage: None,
                error: None,
                tool_use: None,
                stop_reason: None,
            }),
            Ok(ProviderStreamChunk {
                event_type: StreamEventType::ContentBlockDelta,
                text: Some(text),
                usage: None,
                error: None,
                tool_use: None,
                stop_reason: None,
            }),
            Ok(ProviderStreamChunk {
                event_type: StreamEventType::MessageDelta,
                text: None,
                usage: Some(TokenUsage {
                    input_tokens: 10,
                    output_tokens: 20,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
                }),
                error: None,
                tool_use: None,
                stop_reason: Some("end_turn".to_string()),
            }),
            Ok(ProviderStreamChunk {
                event_type: StreamEventType::MessageStop,
                text: None,
                usage: None,
                error: None,
                tool_use: None,
                stop_reason: None,
            }),
        ];

        let _ = model; // Used in real provider for MessageStart metadata
        Ok(Box::pin(stream::iter(chunks)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn default_response_when_queue_empty() {
        let provider = MockProvider::new();
        let request = ProviderRequest {
            model: "test-model".to_string(),
            system_prompt: None,
            system_blocks: None,
            messages: vec![],
            max_tokens: 100,
            stream: false,
            tools: None,
        };
        let resp = provider.complete(request).await.unwrap();
        assert_eq!(resp.content, "mock response");
    }

    #[tokio::test]
    async fn queued_responses_returned_in_order() {
        let provider = MockProvider::with_responses(vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string(),
        ]);
        let req = || ProviderRequest {
            model: "test-model".to_string(),
            system_prompt: None,
            system_blocks: None,
            messages: vec![],
            max_tokens: 100,
            stream: false,
            tools: None,
        };

        assert_eq!(provider.complete(req()).await.unwrap().content, "first");
        assert_eq!(provider.complete(req()).await.unwrap().content, "second");
        assert_eq!(provider.complete(req()).await.unwrap().content, "third");
        // Queue exhausted, falls back to default
        assert_eq!(
            provider.complete(req()).await.unwrap().content,
            "mock response"
        );
    }

    #[tokio::test]
    async fn stream_produces_correct_event_sequence() {
        let provider = MockProvider::with_responses(vec!["streamed text".to_string()]);
        let request = ProviderRequest {
            model: "test-model".to_string(),
            system_prompt: None,
            system_blocks: None,
            messages: vec![],
            max_tokens: 100,
            stream: true,
            tools: None,
        };

        let mut stream = provider.stream(request).await.unwrap();
        let mut events = Vec::new();
        while let Some(chunk) = stream.next().await {
            events.push(chunk.unwrap());
        }

        assert_eq!(events.len(), 4);
        assert_eq!(events[0].event_type, StreamEventType::MessageStart);
        assert_eq!(events[1].event_type, StreamEventType::ContentBlockDelta);
        assert_eq!(events[1].text.as_deref(), Some("streamed text"));
        assert_eq!(events[2].event_type, StreamEventType::MessageDelta);
        assert!(events[2].usage.is_some());
        assert_eq!(events[2].stop_reason.as_deref(), Some("end_turn"));
        assert_eq!(events[3].event_type, StreamEventType::MessageStop);
    }

    #[tokio::test]
    async fn complete_returns_provider_response_with_usage() {
        let provider = MockProvider::with_responses(vec!["test output".to_string()]);
        let request = ProviderRequest {
            model: "claude-test".to_string(),
            system_prompt: None,
            system_blocks: None,
            messages: vec![],
            max_tokens: 100,
            stream: false,
            tools: None,
        };
        let resp = provider.complete(request).await.unwrap();
        assert_eq!(resp.content, "test output");
        assert_eq!(resp.model, "claude-test");
        assert_eq!(resp.stop_reason.as_deref(), Some("end_turn"));
        assert_eq!(resp.usage.input_tokens, 10);
        assert_eq!(resp.usage.output_tokens, 20);
    }

    #[tokio::test]
    async fn add_response_after_construction() {
        let provider = MockProvider::new();
        provider.add_response("dynamic response".to_string()).await;
        let request = ProviderRequest {
            model: "test-model".to_string(),
            system_prompt: None,
            system_blocks: None,
            messages: vec![],
            max_tokens: 100,
            stream: false,
            tools: None,
        };
        assert_eq!(
            provider.complete(request).await.unwrap().content,
            "dynamic response"
        );
    }
}
