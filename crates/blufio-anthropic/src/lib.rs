// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Anthropic Claude provider adapter for the Blufio agent framework.
//!
//! This crate implements [`ProviderAdapter`] for the Anthropic Messages API,
//! providing both single-shot completion and streaming SSE responses.

pub mod client;
pub mod sse;
pub mod types;

use std::collections::HashMap;
use std::pin::Pin;

use async_trait::async_trait;
use blufio_config::BlufioConfig;
use blufio_core::error::BlufioError;
use blufio_core::traits::{PluginAdapter, ProviderAdapter};
use blufio_core::types::{
    AdapterType, ContentBlock, HealthStatus, ProviderRequest, ProviderResponse,
    ProviderStreamChunk, StreamEventType, TokenUsage, ToolUseData,
};
use futures::stream::{Stream, StreamExt};
use tracing::{debug, info};

use crate::client::AnthropicClient;
use crate::sse::StreamEvent;
use crate::types::{
    ApiContent, ApiContentBlock, ApiMessage, CacheControlMarker, ImageSource, MessageRequest,
    ResponseContentBlock, SystemBlock, SystemContent,
};

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
    ///
    /// When `system_blocks` is present, deserializes it as `Vec<SystemBlock>` and
    /// uses `SystemContent::Blocks`. Otherwise falls back to `SystemContent::Text`
    /// from `system_prompt` or the provider's default prompt.
    fn to_message_request(&self, request: &ProviderRequest) -> MessageRequest {
        let messages: Vec<ApiMessage> = request
            .messages
            .iter()
            .map(|m| ApiMessage {
                role: m.role.clone(),
                content: convert_content_blocks(&m.content),
            })
            .collect();

        let system = if let Some(ref blocks_value) = request.system_blocks {
            // Structured system blocks -- deserialize as Vec<SystemBlock>.
            match serde_json::from_value::<Vec<SystemBlock>>(blocks_value.clone()) {
                Ok(blocks) => Some(SystemContent::Blocks(blocks)),
                Err(e) => {
                    tracing::warn!(error = %e, "failed to parse system_blocks, falling back to text");
                    let text = request
                        .system_prompt
                        .clone()
                        .unwrap_or_else(|| self.system_prompt.clone());
                    Some(SystemContent::Text(text))
                }
            }
        } else {
            let text = request
                .system_prompt
                .clone()
                .or_else(|| Some(self.system_prompt.clone()));
            text.map(SystemContent::Text)
        };

        // Convert tool definitions from serde_json::Value to ToolDefinition structs.
        let tools = request.tools.as_ref().map(|tool_values| {
            tool_values
                .iter()
                .filter_map(|v| serde_json::from_value::<crate::types::ToolDefinition>(v.clone()).ok())
                .collect::<Vec<_>>()
        }).and_then(|v| if v.is_empty() { None } else { Some(v) });

        MessageRequest {
            model: request.model.clone(),
            messages,
            system,
            max_tokens: request.max_tokens,
            stream: request.stream,
            cache_control: Some(CacheControlMarker::ephemeral()),
            tools,
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
                ResponseContentBlock::Text { text } => Some(text.as_str()),
                ResponseContentBlock::ToolUse { .. } => None,
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
                cache_read_tokens: response.usage.cache_read_input_tokens,
                cache_creation_tokens: response.usage.cache_creation_input_tokens,
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

        // Stateful stream that accumulates tool_use JSON across deltas.
        // Key: content block index -> (tool_use_id, tool_name, accumulated_json)
        let mut tool_use_blocks: HashMap<usize, (String, String, String)> = HashMap::new();
        let mut stop_reason: Option<String> = None;

        let chunk_stream = event_stream.filter_map(move |result| {
            let chunk = match result {
                Ok(event) => map_stream_event_to_chunk_stateful(
                    event,
                    &mut tool_use_blocks,
                    &mut stop_reason,
                ),
                Err(e) => Some(Err(e)),
            };
            async move { chunk }
        });

        Ok(Box::pin(chunk_stream))
    }
}

/// Maps an SSE [`StreamEvent`] to a [`ProviderStreamChunk`] with stateful
/// accumulation of tool_use JSON deltas.
///
/// `tool_use_blocks` tracks active tool_use content blocks by their index.
/// When a tool_use block starts, its id and name are stored. Input JSON
/// deltas are accumulated. On block stop, the complete JSON is parsed and
/// a chunk with `tool_use` data is emitted.
fn map_stream_event_to_chunk_stateful(
    event: StreamEvent,
    tool_use_blocks: &mut HashMap<usize, (String, String, String)>,
    stop_reason: &mut Option<String>,
) -> Option<Result<ProviderStreamChunk, BlufioError>> {
    match event {
        StreamEvent::ContentBlockStart(cbs) => {
            // Check if this is a tool_use block.
            match &cbs.content_block {
                ResponseContentBlock::ToolUse { id, name, .. } => {
                    tool_use_blocks.insert(cbs.index, (id.clone(), name.clone(), String::new()));
                    None
                }
                ResponseContentBlock::Text { .. } => None,
            }
        }
        StreamEvent::ContentBlockDelta(delta) => {
            match delta.delta {
                crate::types::SseDelta::TextDelta { text } => Some(Ok(ProviderStreamChunk {
                    event_type: StreamEventType::ContentBlockDelta,
                    text: Some(text),
                    usage: None,
                    error: None,
                    tool_use: None,
                    stop_reason: None,
                })),
                crate::types::SseDelta::InputJsonDelta { partial_json } => {
                    // Accumulate partial JSON for tool_use blocks.
                    if let Some((_id, _name, json)) = tool_use_blocks.get_mut(&delta.index)
                    {
                        json.push_str(&partial_json);
                    }
                    None
                }
            }
        }
        StreamEvent::ContentBlockStop(cbs) => {
            // If this was a tool_use block, parse the accumulated JSON and emit.
            if let Some((id, name, json_str)) = tool_use_blocks.remove(&cbs.index) {
                let input = if json_str.is_empty() {
                    serde_json::Value::Object(serde_json::Map::new())
                } else {
                    serde_json::from_str(&json_str).unwrap_or_else(|e| {
                        tracing::warn!(error = %e, json = %json_str, "failed to parse tool_use input JSON");
                        serde_json::json!({"_parse_error": e.to_string(), "_raw": json_str})
                    })
                };

                Some(Ok(ProviderStreamChunk {
                    event_type: StreamEventType::ContentBlockStop,
                    text: None,
                    usage: None,
                    error: None,
                    tool_use: Some(ToolUseData { id, name, input }),
                    stop_reason: None,
                }))
            } else {
                None
            }
        }
        StreamEvent::MessageStart(ms) => Some(Ok(ProviderStreamChunk {
            event_type: StreamEventType::MessageStart,
            text: None,
            usage: Some(TokenUsage {
                input_tokens: ms.message.usage.input_tokens,
                output_tokens: ms.message.usage.output_tokens,
                cache_read_tokens: ms.message.usage.cache_read_input_tokens,
                cache_creation_tokens: ms.message.usage.cache_creation_input_tokens,
            }),
            error: None,
            tool_use: None,
            stop_reason: None,
        })),
        StreamEvent::MessageDelta(md) => {
            // Capture the stop_reason for use in subsequent events.
            if let Some(ref reason) = md.delta.stop_reason {
                *stop_reason = Some(reason.clone());
            }
            Some(Ok(ProviderStreamChunk {
                event_type: StreamEventType::MessageDelta,
                text: None,
                usage: md.usage.map(|u| TokenUsage {
                    input_tokens: u.input_tokens,
                    output_tokens: u.output_tokens,
                    cache_read_tokens: u.cache_read_input_tokens,
                    cache_creation_tokens: u.cache_creation_input_tokens,
                }),
                error: None,
                tool_use: None,
                stop_reason: md.delta.stop_reason,
            }))
        }
        StreamEvent::MessageStop => Some(Ok(ProviderStreamChunk {
            event_type: StreamEventType::MessageStop,
            text: None,
            usage: None,
            error: None,
            tool_use: None,
            stop_reason: stop_reason.clone(),
        })),
        StreamEvent::Error(err) => Some(Ok(ProviderStreamChunk {
            event_type: StreamEventType::Error,
            text: None,
            usage: None,
            error: Some(format!("{}: {}", err.error.type_, err.error.message)),
            tool_use: None,
            stop_reason: None,
        })),
        // Ping -- no user-facing output.
        StreamEvent::Ping => None,
    }
}

/// Resolves the API key from config or environment.
fn resolve_api_key(config_key: &Option<String>) -> Result<String, BlufioError> {
    if let Some(key) = config_key
        && !key.is_empty()
    {
        return Ok(key.clone());
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
    if let Some(prompt) = inline_prompt
        && !prompt.is_empty()
    {
        return prompt.clone();
    }

    // Priority 3: default
    format!("You are {agent_name}, a concise personal assistant.")
}

/// Converts core [`ContentBlock`]s to Anthropic API [`ApiContent`].
fn convert_content_blocks(blocks: &[ContentBlock]) -> ApiContent {
    if blocks.len() == 1
        && let ContentBlock::Text { text } = &blocks[0]
    {
        return ApiContent::Text(text.clone());
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
            ContentBlock::ToolUse { id, name, input } => ApiContentBlock::ToolUse {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
            },
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => ApiContentBlock::ToolResult {
                tool_use_id: tool_use_id.clone(),
                content: content.clone(),
                is_error: *is_error,
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
            system_blocks: None,
            messages: vec![ProviderMessage {
                role: "user".into(),
                content: vec![ContentBlock::Text {
                    text: "Hi".into(),
                }],
            }],
            max_tokens: 2048,
            stream: true,
            tools: None,
        };

        let api_req = provider.to_message_request(&request);
        assert_eq!(api_req.model, "claude-sonnet-4-20250514");
        assert_eq!(api_req.max_tokens, 2048);
        // System falls back to provider default when no system_prompt
        match &api_req.system {
            Some(SystemContent::Text(t)) => assert_eq!(t, "Test prompt."),
            other => panic!("expected SystemContent::Text, got {:?}", other),
        }
        assert_eq!(api_req.messages.len(), 1);
        assert_eq!(api_req.messages[0].role, "user");
        // Cache control should be set automatically
        assert!(api_req.cache_control.is_some());
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
            system_blocks: None,
            messages: vec![],
            max_tokens: 1024,
            stream: false,
            tools: None,
        };

        let api_req = provider.to_message_request(&request);
        match &api_req.system {
            Some(SystemContent::Text(t)) => assert_eq!(t, "Override prompt."),
            other => panic!("expected SystemContent::Text, got {:?}", other),
        }
    }

    #[test]
    fn to_message_request_uses_system_blocks_when_present() {
        let client = AnthropicClient::new(
            "test-key".into(),
            "2023-06-01".into(),
            "claude-sonnet-4-20250514".into(),
        )
        .unwrap();

        let provider = AnthropicProvider::with_client(client, "Default prompt.".into());

        let blocks = serde_json::json!([{
            "type": "text",
            "text": "Structured system prompt.",
            "cache_control": {"type": "ephemeral"}
        }]);

        let request = ProviderRequest {
            model: "claude-sonnet-4-20250514".into(),
            system_prompt: Some("Ignored prompt.".into()),
            system_blocks: Some(blocks),
            messages: vec![],
            max_tokens: 1024,
            stream: false,
            tools: None,
        };

        let api_req = provider.to_message_request(&request);
        match &api_req.system {
            Some(SystemContent::Blocks(blocks)) => {
                assert_eq!(blocks.len(), 1);
                assert_eq!(blocks[0].text, "Structured system prompt.");
                assert!(blocks[0].cache_control.is_some());
            }
            other => panic!("expected SystemContent::Blocks, got {:?}", other),
        }
    }

    #[test]
    fn map_content_block_delta_text() {
        let mut tool_blocks = HashMap::new();
        let mut stop_reason = None;
        let event = StreamEvent::ContentBlockDelta(crate::types::SseContentBlockDelta {
            index: 0,
            delta: crate::types::SseDelta::TextDelta {
                text: "Hello".into(),
            },
        });
        let chunk = map_stream_event_to_chunk_stateful(event, &mut tool_blocks, &mut stop_reason)
            .unwrap()
            .unwrap();
        assert_eq!(chunk.event_type, StreamEventType::ContentBlockDelta);
        assert_eq!(chunk.text.as_deref(), Some("Hello"));
    }

    #[test]
    fn map_message_stop_event() {
        let mut tool_blocks = HashMap::new();
        let mut stop_reason = None;
        let event = StreamEvent::MessageStop;
        let chunk = map_stream_event_to_chunk_stateful(event, &mut tool_blocks, &mut stop_reason)
            .unwrap()
            .unwrap();
        assert_eq!(chunk.event_type, StreamEventType::MessageStop);
        assert!(chunk.text.is_none());
    }

    #[test]
    fn map_ping_returns_none() {
        let mut tool_blocks = HashMap::new();
        let mut stop_reason = None;
        let event = StreamEvent::Ping;
        assert!(
            map_stream_event_to_chunk_stateful(event, &mut tool_blocks, &mut stop_reason).is_none()
        );
    }

    #[test]
    fn map_error_event() {
        let mut tool_blocks = HashMap::new();
        let mut stop_reason = None;
        let event = StreamEvent::Error(crate::types::SseError {
            error: crate::types::SseErrorDetail {
                type_: "overloaded_error".into(),
                message: "Overloaded".into(),
            },
        });
        let chunk = map_stream_event_to_chunk_stateful(event, &mut tool_blocks, &mut stop_reason)
            .unwrap()
            .unwrap();
        assert_eq!(chunk.event_type, StreamEventType::Error);
        assert!(chunk.error.as_ref().unwrap().contains("overloaded_error"));
    }

    #[test]
    fn map_tool_use_block_accumulates_json() {
        let mut tool_blocks = HashMap::new();
        let mut stop_reason = None;

        // 1. content_block_start with tool_use
        let start_event = StreamEvent::ContentBlockStart(crate::types::SseContentBlockStart {
            index: 1,
            content_block: ResponseContentBlock::ToolUse {
                id: "toolu_abc".into(),
                name: "bash".into(),
                input: serde_json::json!({}),
            },
        });
        assert!(
            map_stream_event_to_chunk_stateful(start_event, &mut tool_blocks, &mut stop_reason)
                .is_none()
        );

        // 2. Two input_json_delta events
        let delta1 = StreamEvent::ContentBlockDelta(crate::types::SseContentBlockDelta {
            index: 1,
            delta: crate::types::SseDelta::InputJsonDelta {
                partial_json: "{\"command\":".into(),
            },
        });
        assert!(
            map_stream_event_to_chunk_stateful(delta1, &mut tool_blocks, &mut stop_reason)
                .is_none()
        );

        let delta2 = StreamEvent::ContentBlockDelta(crate::types::SseContentBlockDelta {
            index: 1,
            delta: crate::types::SseDelta::InputJsonDelta {
                partial_json: "\"echo hello\"}".into(),
            },
        });
        assert!(
            map_stream_event_to_chunk_stateful(delta2, &mut tool_blocks, &mut stop_reason)
                .is_none()
        );

        // 3. content_block_stop emits the tool_use chunk
        let stop_event = StreamEvent::ContentBlockStop(crate::types::SseContentBlockStop {
            index: 1,
        });
        let chunk =
            map_stream_event_to_chunk_stateful(stop_event, &mut tool_blocks, &mut stop_reason)
                .unwrap()
                .unwrap();

        assert_eq!(chunk.event_type, StreamEventType::ContentBlockStop);
        let tool_use = chunk.tool_use.unwrap();
        assert_eq!(tool_use.id, "toolu_abc");
        assert_eq!(tool_use.name, "bash");
        assert_eq!(tool_use.input["command"], "echo hello");
    }

    #[test]
    fn map_text_block_stop_returns_none() {
        let mut tool_blocks = HashMap::new();
        let mut stop_reason = None;
        // Stop for a text block (not in tool_use_blocks) should return None
        let event = StreamEvent::ContentBlockStop(crate::types::SseContentBlockStop { index: 0 });
        assert!(
            map_stream_event_to_chunk_stateful(event, &mut tool_blocks, &mut stop_reason).is_none()
        );
    }

    #[test]
    fn map_message_delta_captures_stop_reason() {
        let mut tool_blocks = HashMap::new();
        let mut stop_reason = None;
        let event = StreamEvent::MessageDelta(crate::types::SseMessageDelta {
            delta: crate::types::SseMessageDeltaInfo {
                stop_reason: Some("tool_use".into()),
            },
            usage: Some(crate::types::ApiUsage {
                input_tokens: 50,
                output_tokens: 30,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
            }),
        });
        let chunk = map_stream_event_to_chunk_stateful(event, &mut tool_blocks, &mut stop_reason)
            .unwrap()
            .unwrap();
        assert_eq!(chunk.stop_reason.as_deref(), Some("tool_use"));
        assert_eq!(stop_reason.as_deref(), Some("tool_use"));
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
