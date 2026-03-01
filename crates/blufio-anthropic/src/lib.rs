// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Anthropic Claude provider adapter for the Blufio agent framework.
//!
//! This crate implements [`ProviderAdapter`] for the Anthropic Messages API,
//! providing both single-shot completion and streaming SSE responses.

pub mod client;
pub mod sse;
pub mod types;

use std::pin::Pin;

use async_trait::async_trait;
use blufio_config::BlufioConfig;
use blufio_core::error::BlufioError;
use blufio_core::traits::{PluginAdapter, ProviderAdapter};
use blufio_core::types::{
    AdapterType, ContentBlock, HealthStatus, ProviderRequest, ProviderResponse,
    ProviderStreamChunk, StreamEventType, TokenUsage,
};
use futures::stream::{Stream, StreamExt};
use tracing::{debug, info};

use crate::client::AnthropicClient;
use crate::sse::StreamEvent;
use crate::types::{ApiContent, ApiContentBlock, ApiMessage, ImageSource, MessageRequest};

/// Anthropic Claude provider implementing [`ProviderAdapter`].
///
/// Supports both synchronous completion and streaming responses via SSE.
/// API key resolution order: config -> `ANTHROPIC_API_KEY` env var -> error.
pub struct AnthropicProvider {
    client: AnthropicClient,
    system_prompt: String,
}

impl AnthropicProvider {
    /// Creates a new Anthropic provider from the given configuration.
    ///
    /// # API Key Resolution
    /// 1. `config.anthropic.api_key` if set
    /// 2. `ANTHROPIC_API_KEY` environment variable
    /// 3. Returns error if neither is available
    ///
    /// # System Prompt Resolution
    /// 1. `config.agent.system_prompt_file` if set and file exists (read from disk)
    /// 2. `config.agent.system_prompt` if set
    /// 3. Default: "You are {name}, a concise personal assistant."
    pub async fn new(config: &BlufioConfig) -> Result<Self, BlufioError> {
        let api_key = resolve_api_key(&config.anthropic.api_key)?;
        let system_prompt = load_system_prompt(
            &config.agent.name,
            &config.agent.system_prompt,
            &config.agent.system_prompt_file,
        )
        .await;

        let client = AnthropicClient::new(
            api_key,
            config.anthropic.api_version.clone(),
            config.anthropic.default_model.clone(),
        )?;

        info!(
            model = config.anthropic.default_model,
            "Anthropic provider initialized"
        );

        Ok(Self {
            client,
            system_prompt,
        })
    }

    /// Creates a provider with an existing client (for testing).
    #[cfg(test)]
    fn with_client(client: AnthropicClient, system_prompt: String) -> Self {
        Self {
            client,
            system_prompt,
        }
    }

    /// Converts a [`ProviderRequest`] to an Anthropic [`MessageRequest`].
    fn to_message_request(&self, request: &ProviderRequest) -> MessageRequest {
        let messages: Vec<ApiMessage> = request
            .messages
            .iter()
            .map(|m| ApiMessage {
                role: m.role.clone(),
                content: convert_content_blocks(&m.content),
            })
            .collect();

        let system = request
            .system_prompt
            .clone()
            .or_else(|| Some(self.system_prompt.clone()));

        MessageRequest {
            model: request.model.clone(),
            messages,
            system,
            max_tokens: request.max_tokens,
            stream: request.stream,
        }
    }
}

#[async_trait]
impl PluginAdapter for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn version(&self) -> semver::Version {
        semver::Version::new(0, 1, 0)
    }

    fn adapter_type(&self) -> AdapterType {
        AdapterType::Provider
    }

    async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
        // A simple health check: verify the client is constructable.
        // A full check would make a lightweight API call, but we avoid
        // consuming tokens on health checks.
        Ok(HealthStatus::Healthy)
    }

    async fn shutdown(&self) -> Result<(), BlufioError> {
        debug!("Anthropic provider shutting down");
        Ok(())
    }
}

#[async_trait]
impl ProviderAdapter for AnthropicProvider {
    async fn complete(
        &self,
        request: ProviderRequest,
    ) -> Result<ProviderResponse, BlufioError> {
        let api_request = self.to_message_request(&request);
        let response = self.client.complete_message(&api_request).await?;

        // Extract text content from response blocks.
        let content = response
            .content
            .iter()
            .filter_map(|block| match block {
                crate::types::ResponseContentBlock::Text { text } => Some(text.as_str()),
            })
            .collect::<Vec<_>>()
            .join("");

        Ok(ProviderResponse {
            id: response.id,
            content,
            model: response.model,
            stop_reason: response.stop_reason,
            usage: TokenUsage {
                input_tokens: response.usage.input_tokens,
                output_tokens: response.usage.output_tokens,
            },
        })
    }

    async fn stream(
        &self,
        request: ProviderRequest,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<ProviderStreamChunk, BlufioError>> + Send>>,
        BlufioError,
    > {
        let api_request = self.to_message_request(&request);
        let event_stream = self.client.stream_message(&api_request).await?;

        // Map StreamEvent -> ProviderStreamChunk, filtering out non-content events.
        let chunk_stream = event_stream.filter_map(|result| async move {
            match result {
                Ok(event) => map_stream_event_to_chunk(event),
                Err(e) => Some(Err(e)),
            }
        });

        Ok(Box::pin(chunk_stream))
    }
}

/// Maps an SSE [`StreamEvent`] to a [`ProviderStreamChunk`].
///
/// Returns `None` for events that don't produce user-facing output
/// (Ping, ContentBlockStart, ContentBlockStop).
fn map_stream_event_to_chunk(
    event: StreamEvent,
) -> Option<Result<ProviderStreamChunk, BlufioError>> {
    match event {
        StreamEvent::ContentBlockDelta(delta) => {
            match delta.delta {
                crate::types::SseDelta::TextDelta { text } => Some(Ok(ProviderStreamChunk {
                    event_type: StreamEventType::ContentBlockDelta,
                    text: Some(text),
                    usage: None,
                    error: None,
                })),
                crate::types::SseDelta::InputJsonDelta { .. } => {
                    // Tool use deltas -- skip for now (Phase 3 doesn't use tool calling).
                    None
                }
            }
        }
        StreamEvent::MessageStart(ms) => Some(Ok(ProviderStreamChunk {
            event_type: StreamEventType::MessageStart,
            text: None,
            usage: Some(TokenUsage {
                input_tokens: ms.message.usage.input_tokens,
                output_tokens: ms.message.usage.output_tokens,
            }),
            error: None,
        })),
        StreamEvent::MessageDelta(md) => Some(Ok(ProviderStreamChunk {
            event_type: StreamEventType::MessageDelta,
            text: None,
            usage: md.usage.map(|u| TokenUsage {
                input_tokens: u.input_tokens,
                output_tokens: u.output_tokens,
            }),
            error: None,
        })),
        StreamEvent::MessageStop => Some(Ok(ProviderStreamChunk {
            event_type: StreamEventType::MessageStop,
            text: None,
            usage: None,
            error: None,
        })),
        StreamEvent::Error(err) => Some(Ok(ProviderStreamChunk {
            event_type: StreamEventType::Error,
            text: None,
            usage: None,
            error: Some(format!("{}: {}", err.error.type_, err.error.message)),
        })),
        // Ping, ContentBlockStart, ContentBlockStop -- no user-facing output.
        _ => None,
    }
}

/// Resolves the API key from config or environment.
fn resolve_api_key(config_key: &Option<String>) -> Result<String, BlufioError> {
    if let Some(key) = config_key {
        if !key.is_empty() {
            return Ok(key.clone());
        }
    }

    std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
        BlufioError::Config(
            "Anthropic API key not found. Set anthropic.api_key in config or ANTHROPIC_API_KEY environment variable.".into(),
        )
    })
}

/// Loads the system prompt following priority: file > inline > default.
async fn load_system_prompt(
    agent_name: &str,
    inline_prompt: &Option<String>,
    prompt_file: &Option<String>,
) -> String {
    // Priority 1: file path
    if let Some(file_path) = prompt_file {
        match tokio::fs::read_to_string(file_path).await {
            Ok(content) => {
                let trimmed = content.trim().to_string();
                if !trimmed.is_empty() {
                    info!(path = file_path, "loaded system prompt from file");
                    return trimmed;
                }
            }
            Err(e) => {
                tracing::warn!(
                    path = file_path,
                    error = %e,
                    "failed to read system prompt file, falling back"
                );
            }
        }
    }

    // Priority 2: inline string
    if let Some(prompt) = inline_prompt {
        if !prompt.is_empty() {
            return prompt.clone();
        }
    }

    // Priority 3: default
    format!("You are {agent_name}, a concise personal assistant.")
}

/// Converts core [`ContentBlock`]s to Anthropic API [`ApiContent`].
fn convert_content_blocks(blocks: &[ContentBlock]) -> ApiContent {
    if blocks.len() == 1 {
        if let ContentBlock::Text { text } = &blocks[0] {
            return ApiContent::Text(text.clone());
        }
    }

    let api_blocks: Vec<ApiContentBlock> = blocks
        .iter()
        .map(|block| match block {
            ContentBlock::Text { text } => ApiContentBlock::Text { text: text.clone() },
            ContentBlock::Image {
                source_type,
                media_type,
                data,
            } => ApiContentBlock::Image {
                source: ImageSource {
                    source_type: source_type.clone(),
                    media_type: media_type.clone(),
                    data: data.clone(),
                },
            },
        })
        .collect();

    ApiContent::Blocks(api_blocks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use blufio_core::ProviderMessage;

    #[test]
    fn resolve_api_key_from_config() {
        let result = resolve_api_key(&Some("sk-test-123".into()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "sk-test-123");
    }

    #[test]
    fn resolve_api_key_empty_config_falls_back_to_env() {
        let result = resolve_api_key(&Some("".into()));
        // Will fail unless ANTHROPIC_API_KEY is set, which is fine for tests.
        // We just verify it doesn't return the empty string.
        if result.is_ok() {
            assert!(!result.unwrap().is_empty());
        }
    }

    #[test]
    fn resolve_api_key_none_falls_back_to_env() {
        let result = resolve_api_key(&None);
        // Will succeed if env is set, fail otherwise.
        if result.is_err() {
            let err = result.unwrap_err().to_string();
            assert!(err.contains("API key not found"), "got: {err}");
        }
    }

    #[tokio::test]
    async fn system_prompt_default() {
        let prompt = load_system_prompt("blufio", &None, &None).await;
        assert_eq!(prompt, "You are blufio, a concise personal assistant.");
    }

    #[tokio::test]
    async fn system_prompt_inline_overrides_default() {
        let prompt =
            load_system_prompt("blufio", &Some("Custom prompt.".into()), &None).await;
        assert_eq!(prompt, "Custom prompt.");
    }

    #[tokio::test]
    async fn system_prompt_file_overrides_inline() {
        // Write a temp file with a prompt.
        let dir = std::env::temp_dir().join("blufio-test-prompt");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("test-prompt.md");
        std::fs::write(&file_path, "File-based prompt.").unwrap();

        let prompt = load_system_prompt(
            "blufio",
            &Some("Inline prompt.".into()),
            &Some(file_path.to_string_lossy().into_owned()),
        )
        .await;
        assert_eq!(prompt, "File-based prompt.");

        // Cleanup.
        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[tokio::test]
    async fn system_prompt_missing_file_falls_back_to_inline() {
        let prompt = load_system_prompt(
            "blufio",
            &Some("Fallback prompt.".into()),
            &Some("/nonexistent/path/prompt.md".into()),
        )
        .await;
        assert_eq!(prompt, "Fallback prompt.");
    }

    #[test]
    fn convert_single_text_block_to_string() {
        let blocks = vec![ContentBlock::Text {
            text: "Hello".into(),
        }];
        let result = convert_content_blocks(&blocks);
        match result {
            ApiContent::Text(t) => assert_eq!(t, "Hello"),
            _ => panic!("expected Text, got Blocks"),
        }
    }

    #[test]
    fn convert_mixed_blocks_to_array() {
        let blocks = vec![
            ContentBlock::Text {
                text: "What is this?".into(),
            },
            ContentBlock::Image {
                source_type: "base64".into(),
                media_type: "image/jpeg".into(),
                data: "abc123".into(),
            },
        ];
        let result = convert_content_blocks(&blocks);
        match result {
            ApiContent::Blocks(b) => {
                assert_eq!(b.len(), 2);
                assert!(matches!(&b[0], ApiContentBlock::Text { .. }));
                assert!(matches!(&b[1], ApiContentBlock::Image { .. }));
            }
            _ => panic!("expected Blocks"),
        }
    }

    #[test]
    fn to_message_request_conversion() {
        let client = AnthropicClient::new(
            "test-key".into(),
            "2023-06-01".into(),
            "claude-sonnet-4-20250514".into(),
        )
        .unwrap();

        let provider = AnthropicProvider::with_client(client, "Test prompt.".into());

        let request = ProviderRequest {
            model: "claude-sonnet-4-20250514".into(),
            system_prompt: None,
            messages: vec![ProviderMessage {
                role: "user".into(),
                content: vec![ContentBlock::Text {
                    text: "Hi".into(),
                }],
            }],
            max_tokens: 2048,
            stream: true,
        };

        let api_req = provider.to_message_request(&request);
        assert_eq!(api_req.model, "claude-sonnet-4-20250514");
        assert_eq!(api_req.max_tokens, 2048);
        assert_eq!(api_req.system.as_deref(), Some("Test prompt."));
        assert_eq!(api_req.messages.len(), 1);
        assert_eq!(api_req.messages[0].role, "user");
    }

    #[test]
    fn to_message_request_uses_explicit_system_prompt() {
        let client = AnthropicClient::new(
            "test-key".into(),
            "2023-06-01".into(),
            "claude-sonnet-4-20250514".into(),
        )
        .unwrap();

        let provider = AnthropicProvider::with_client(client, "Default prompt.".into());

        let request = ProviderRequest {
            model: "claude-sonnet-4-20250514".into(),
            system_prompt: Some("Override prompt.".into()),
            messages: vec![],
            max_tokens: 1024,
            stream: false,
        };

        let api_req = provider.to_message_request(&request);
        assert_eq!(api_req.system.as_deref(), Some("Override prompt."));
    }

    #[test]
    fn map_content_block_delta_text() {
        let event = StreamEvent::ContentBlockDelta(crate::types::SseContentBlockDelta {
            index: 0,
            delta: crate::types::SseDelta::TextDelta {
                text: "Hello".into(),
            },
        });
        let chunk = map_stream_event_to_chunk(event).unwrap().unwrap();
        assert_eq!(chunk.event_type, StreamEventType::ContentBlockDelta);
        assert_eq!(chunk.text.as_deref(), Some("Hello"));
    }

    #[test]
    fn map_message_stop_event() {
        let event = StreamEvent::MessageStop;
        let chunk = map_stream_event_to_chunk(event).unwrap().unwrap();
        assert_eq!(chunk.event_type, StreamEventType::MessageStop);
        assert!(chunk.text.is_none());
    }

    #[test]
    fn map_ping_returns_none() {
        let event = StreamEvent::Ping;
        assert!(map_stream_event_to_chunk(event).is_none());
    }

    #[test]
    fn map_error_event() {
        let event = StreamEvent::Error(crate::types::SseError {
            error: crate::types::SseErrorDetail {
                type_: "overloaded_error".into(),
                message: "Overloaded".into(),
            },
        });
        let chunk = map_stream_event_to_chunk(event).unwrap().unwrap();
        assert_eq!(chunk.event_type, StreamEventType::Error);
        assert!(chunk.error.as_ref().unwrap().contains("overloaded_error"));
    }

    #[test]
    fn plugin_adapter_metadata() {
        let client = AnthropicClient::new(
            "test-key".into(),
            "2023-06-01".into(),
            "claude-sonnet-4-20250514".into(),
        )
        .unwrap();
        let provider = AnthropicProvider::with_client(client, "test".into());

        assert_eq!(provider.name(), "anthropic");
        assert_eq!(provider.version(), semver::Version::new(0, 1, 0));
        assert_eq!(provider.adapter_type(), AdapterType::Provider);
    }
}
