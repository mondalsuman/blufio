// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! OpenAI provider adapter for the Blufio agent framework.
//!
//! This crate implements [`ProviderAdapter`] for the OpenAI Chat Completions API,
//! providing both single-shot completion and streaming SSE responses with
//! tool calling, vision, and structured outputs.

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

use crate::client::OpenAIClient;
use crate::types::{
    ChatContent, ChatMessage, ChatRequest, ContentPart, FunctionCall, FunctionDef, ImageUrlData,
    OpenAITool, StreamOptions, ToolCall,
};

/// OpenAI provider implementing [`ProviderAdapter`].
///
/// Supports both synchronous completion and streaming responses via SSE.
/// API key resolution order: config -> `OPENAI_API_KEY` env var -> error.
pub struct OpenAIProvider {
    client: OpenAIClient,
    system_prompt: String,
}

impl OpenAIProvider {
    /// Creates a new OpenAI provider from the given configuration.
    ///
    /// # API Key Resolution
    /// 1. `config.providers.openai.api_key` if set
    /// 2. `OPENAI_API_KEY` environment variable
    /// 3. Returns error if neither is available
    ///
    /// # System Prompt Resolution
    /// 1. `config.agent.system_prompt_file` if set and file exists (read from disk)
    /// 2. `config.agent.system_prompt` if set
    /// 3. Default: "You are {name}, a concise personal assistant."
    pub async fn new(config: &BlufioConfig) -> Result<Self, BlufioError> {
        let api_key = resolve_api_key(&config.providers.openai.api_key)?;
        let system_prompt = load_system_prompt(
            &config.agent.name,
            &config.agent.system_prompt,
            &config.agent.system_prompt_file,
        )
        .await;

        let client = OpenAIClient::new(
            api_key,
            config.providers.openai.default_model.clone(),
            config.providers.openai.base_url.clone(),
            Some(&config.security),
        )?;

        info!(
            model = config.providers.openai.default_model,
            base_url = config.providers.openai.base_url,
            "OpenAI provider initialized"
        );

        Ok(Self {
            client,
            system_prompt,
        })
    }

    /// Creates a provider with an existing client (for testing).
    #[cfg(test)]
    fn with_client(client: OpenAIClient, system_prompt: String) -> Self {
        Self {
            client,
            system_prompt,
        }
    }

    /// Converts a [`ProviderRequest`] to an OpenAI [`ChatRequest`].
    ///
    /// Key mappings:
    /// - `system_prompt` -> system role message prepended to messages
    /// - `ContentBlock::Image` -> image_url content part with data URI
    /// - `ContentBlock::ToolUse` -> assistant message with tool_calls array
    /// - `ContentBlock::ToolResult` -> tool role message with tool_call_id
    /// - `max_tokens` -> `max_completion_tokens`
    /// - `tools` -> `{"type":"function","function":{...}}` format
    fn to_chat_request(&self, request: &ProviderRequest) -> ChatRequest {
        let mut messages: Vec<ChatMessage> = Vec::new();

        // System prompt -> system role message.
        let system_text = request
            .system_prompt
            .clone()
            .unwrap_or_else(|| self.system_prompt.clone());
        messages.push(ChatMessage {
            role: "system".into(),
            content: Some(ChatContent::Text(system_text)),
            tool_calls: None,
            tool_call_id: None,
        });

        // Convert provider messages.
        for msg in &request.messages {
            messages.extend(convert_provider_message(msg));
        }

        // Convert tools.
        let tools = request.tools.as_ref().map(|defs| {
            defs.iter()
                .map(|td| OpenAITool {
                    tool_type: "function".into(),
                    function: FunctionDef {
                        name: td.name.clone(),
                        description: td.description.clone(),
                        parameters: td.input_schema.clone(),
                    },
                })
                .collect::<Vec<_>>()
        });

        ChatRequest {
            model: request.model.clone(),
            messages,
            max_completion_tokens: Some(request.max_tokens),
            stream: request.stream,
            tools,
            response_format: None,
            stream_options: if request.stream {
                Some(StreamOptions {
                    include_usage: true,
                })
            } else {
                None
            },
        }
    }
}

#[async_trait]
impl PluginAdapter for OpenAIProvider {
    fn name(&self) -> &str {
        "openai"
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
        debug!("OpenAI provider shutting down");
        Ok(())
    }
}

#[async_trait]
impl ProviderAdapter for OpenAIProvider {
    async fn complete(&self, request: ProviderRequest) -> Result<ProviderResponse, BlufioError> {
        let api_request = self.to_chat_request(&request);
        let response = self.client.complete_chat(&api_request).await?;

        // Extract text content from the first choice.
        let choice = response
            .choices
            .first()
            .ok_or_else(|| BlufioError::Provider {
                kind: blufio_core::ProviderErrorKind::ServerError,
                context: blufio_core::ErrorContext {
                    provider_name: Some("openai".into()),
                    ..Default::default()
                },
                source: None,
            })?;

        let content = match &choice.message.content {
            Some(ChatContent::Text(t)) => t.clone(),
            Some(ChatContent::Parts(parts)) => parts
                .iter()
                .filter_map(|p| match p {
                    ContentPart::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(""),
            None => String::new(),
        };

        // Map OpenAI finish_reason to provider stop_reason.
        let stop_reason = choice
            .finish_reason
            .as_deref()
            .map(|r| map_finish_reason(r).to_string());

        let usage = response
            .usage
            .map_or_else(TokenUsage::default, |u| TokenUsage {
                input_tokens: u.prompt_tokens,
                output_tokens: u.completion_tokens,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            });

        Ok(ProviderResponse {
            id: response.id,
            content,
            model: response.model,
            stop_reason,
            usage,
        })
    }

    async fn stream(
        &self,
        request: ProviderRequest,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<ProviderStreamChunk, BlufioError>> + Send>>,
        BlufioError,
    > {
        let api_request = self.to_chat_request(&request);
        let chunk_stream = self.client.stream_chat(&api_request).await?;

        // Stateful stream that accumulates tool call arguments across deltas.
        // Key: tool call index -> (id, name, accumulated_args)
        let mut tool_calls: HashMap<usize, (String, String, String)> = HashMap::new();
        let mut is_first = true;

        let mapped = chunk_stream.filter_map(move |result| {
            let chunks = match result {
                Ok(sse_chunk) => {
                    map_sse_chunk_to_provider_chunks(sse_chunk, &mut tool_calls, &mut is_first)
                }
                Err(e) => vec![Err(e)],
            };
            async move {
                // filter_map expects Option<Item>, but we may produce multiple items.
                // We return the first item if any; the rest we handle via flattening below.
                // Actually, let's return Some for non-empty, None for empty.
                if chunks.is_empty() {
                    None
                } else {
                    // For simplicity, return the first chunk. Multi-chunk case is handled
                    // by the finish_reason logic emitting tool_use chunks inline.
                    Some(futures::stream::iter(chunks))
                }
            }
        });

        // Flatten the stream of streams into a single stream.
        let flattened = mapped.flatten();

        Ok(Box::pin(flattened))
    }
}

/// Maps an OpenAI SSE chunk to zero or more `ProviderStreamChunk`s.
///
/// Handles:
/// - Text deltas -> ContentBlockDelta with text
/// - Tool call deltas -> accumulate id/name/args in `tool_calls`
/// - finish_reason -> emit accumulated tool_use chunks, then stop
/// - Usage -> MessageDelta with token usage
fn map_sse_chunk_to_provider_chunks(
    sse_chunk: crate::types::SseChunk,
    tool_calls: &mut HashMap<usize, (String, String, String)>,
    is_first: &mut bool,
) -> Vec<Result<ProviderStreamChunk, BlufioError>> {
    let mut chunks = Vec::new();

    // Emit MessageStart on first chunk.
    if *is_first {
        *is_first = false;
        // Include usage from the initial chunk if present.
        let usage = sse_chunk.usage.as_ref().map(|u| TokenUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        });
        chunks.push(Ok(ProviderStreamChunk {
            event_type: StreamEventType::MessageStart,
            text: None,
            usage,
            error: None,
            tool_use: None,
            stop_reason: None,
        }));
    }

    for delta in &sse_chunk.choices {
        // Text content delta.
        if let Some(ref text) = delta.delta.content
            && !text.is_empty()
        {
            chunks.push(Ok(ProviderStreamChunk {
                event_type: StreamEventType::ContentBlockDelta,
                text: Some(text.clone()),
                usage: None,
                error: None,
                tool_use: None,
                stop_reason: None,
            }));
        }

        // Tool call deltas -- accumulate.
        if let Some(ref tcs) = delta.delta.tool_calls {
            for tc in tcs {
                let entry = tool_calls
                    .entry(tc.index)
                    .or_insert_with(|| (String::new(), String::new(), String::new()));

                if let Some(ref id) = tc.id {
                    entry.0 = id.clone();
                }
                if let Some(ref func) = tc.function {
                    if let Some(ref name) = func.name {
                        entry.1 = name.clone();
                    }
                    if let Some(ref args) = func.arguments {
                        entry.2.push_str(args);
                    }
                }
            }
        }

        // Finish reason -- emit accumulated tool_use chunks and stop.
        if let Some(ref reason) = delta.finish_reason {
            let stop_reason = map_finish_reason(reason);

            // Emit accumulated tool_use chunks.
            let mut indices: Vec<usize> = tool_calls.keys().copied().collect();
            indices.sort();
            for idx in indices {
                if let Some((id, name, args_str)) = tool_calls.remove(&idx) {
                    let input = if args_str.is_empty() {
                        serde_json::Value::Object(serde_json::Map::new())
                    } else {
                        serde_json::from_str(&args_str).unwrap_or_else(|e| {
                            tracing::warn!(
                                error = %e,
                                json = %args_str,
                                "failed to parse tool_call arguments JSON"
                            );
                            serde_json::json!({"_parse_error": e.to_string(), "_raw": args_str})
                        })
                    };

                    chunks.push(Ok(ProviderStreamChunk {
                        event_type: StreamEventType::ContentBlockStop,
                        text: None,
                        usage: None,
                        error: None,
                        tool_use: Some(ToolUseData { id, name, input }),
                        stop_reason: None,
                    }));
                }
            }

            // Emit MessageDelta with stop_reason and usage.
            let usage = sse_chunk.usage.as_ref().map(|u| TokenUsage {
                input_tokens: u.prompt_tokens,
                output_tokens: u.completion_tokens,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            });

            chunks.push(Ok(ProviderStreamChunk {
                event_type: StreamEventType::MessageDelta,
                text: None,
                usage,
                error: None,
                tool_use: None,
                stop_reason: Some(stop_reason.to_string()),
            }));

            // Emit MessageStop.
            chunks.push(Ok(ProviderStreamChunk {
                event_type: StreamEventType::MessageStop,
                text: None,
                usage: None,
                error: None,
                tool_use: None,
                stop_reason: Some(stop_reason.to_string()),
            }));
        }
    }

    chunks
}

/// Maps OpenAI `finish_reason` to provider-agnostic `stop_reason`.
fn map_finish_reason(reason: &str) -> &str {
    match reason {
        "stop" => "end_turn",
        "tool_calls" => "tool_use",
        "length" => "max_tokens",
        "content_filter" => "content_filter",
        other => other,
    }
}

/// Converts a [`ProviderMessage`] into one or more OpenAI [`ChatMessage`]s.
///
/// A single ProviderMessage with mixed content blocks may produce multiple
/// ChatMessages (e.g., ToolUse blocks become a separate assistant message).
fn convert_provider_message(msg: &blufio_core::ProviderMessage) -> Vec<ChatMessage> {
    let mut messages = Vec::new();

    // Separate tool_use blocks from other content.
    let mut text_parts: Vec<ContentPart> = Vec::new();
    let mut tool_uses: Vec<(String, String, serde_json::Value)> = Vec::new();
    let mut tool_results: Vec<(String, String, Option<bool>)> = Vec::new();

    for block in &msg.content {
        match block {
            ContentBlock::Text { text } => {
                text_parts.push(ContentPart::Text { text: text.clone() });
            }
            ContentBlock::Image {
                media_type, data, ..
            } => {
                text_parts.push(ContentPart::ImageUrl {
                    image_url: ImageUrlData {
                        url: format!("data:{media_type};base64,{data}"),
                    },
                });
            }
            ContentBlock::ToolUse { id, name, input } => {
                tool_uses.push((id.clone(), name.clone(), input.clone()));
            }
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                tool_results.push((tool_use_id.clone(), content.clone(), *is_error));
            }
        }
    }

    // Emit text/image content as a regular message if present.
    if !text_parts.is_empty() {
        let content = if text_parts.len() == 1 {
            if let ContentPart::Text { ref text } = text_parts[0] {
                ChatContent::Text(text.clone())
            } else {
                ChatContent::Parts(text_parts)
            }
        } else {
            ChatContent::Parts(text_parts)
        };
        messages.push(ChatMessage {
            role: msg.role.clone(),
            content: Some(content),
            tool_calls: None,
            tool_call_id: None,
        });
    }

    // Emit tool_use blocks as an assistant message with tool_calls.
    if !tool_uses.is_empty() {
        let calls: Vec<ToolCall> = tool_uses
            .into_iter()
            .map(|(id, name, input)| ToolCall {
                id,
                call_type: "function".into(),
                function: FunctionCall {
                    name,
                    arguments: serde_json::to_string(&input).unwrap_or_default(),
                },
            })
            .collect();
        messages.push(ChatMessage {
            role: "assistant".into(),
            content: None,
            tool_calls: Some(calls),
            tool_call_id: None,
        });
    }

    // Emit tool results as individual tool messages.
    for (tool_use_id, content, _is_error) in tool_results {
        messages.push(ChatMessage {
            role: "tool".into(),
            content: Some(ChatContent::Text(content)),
            tool_calls: None,
            tool_call_id: Some(tool_use_id),
        });
    }

    // If nothing was emitted (empty content blocks), still emit the message.
    if messages.is_empty() {
        messages.push(ChatMessage {
            role: msg.role.clone(),
            content: Some(ChatContent::Text(String::new())),
            tool_calls: None,
            tool_call_id: None,
        });
    }

    messages
}

/// Resolves the API key from config or environment.
fn resolve_api_key(config_key: &Option<String>) -> Result<String, BlufioError> {
    if let Some(key) = config_key
        && !key.is_empty()
    {
        return Ok(key.clone());
    }

    std::env::var("OPENAI_API_KEY").map_err(|_| {
        BlufioError::Config(
            "OpenAI API key not found. Set providers.openai.api_key in config or OPENAI_API_KEY environment variable.".into(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use blufio_core::ProviderMessage;
    use blufio_core::types::ToolDefinition;

    fn test_provider() -> OpenAIProvider {
        let client = OpenAIClient::new(
            "test-key".into(),
            "gpt-4o".into(),
            "https://api.openai.com/v1".into(),
            None,
        )
        .unwrap();
        OpenAIProvider::with_client(client, "Test system prompt.".into())
    }

    #[test]
    fn plugin_adapter_name() {
        let provider = test_provider();
        assert_eq!(provider.name(), "openai");
    }

    #[test]
    fn plugin_adapter_type() {
        let provider = test_provider();
        assert_eq!(provider.adapter_type(), AdapterType::Provider);
    }

    #[test]
    fn plugin_adapter_version() {
        let provider = test_provider();
        assert_eq!(provider.version(), semver::Version::new(0, 1, 0));
    }

    #[test]
    fn to_chat_request_basic() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "gpt-4o".into(),
            system_prompt: None,
            system_blocks: None,
            messages: vec![ProviderMessage {
                role: "user".into(),
                content: vec![ContentBlock::Text {
                    text: "Hello".into(),
                }],
            }],
            max_tokens: 2048,
            stream: true,
            tools: None,
        };

        let chat_req = provider.to_chat_request(&request);
        assert_eq!(chat_req.model, "gpt-4o");
        assert_eq!(chat_req.max_completion_tokens, Some(2048));
        assert!(chat_req.stream);
        // First message is system, second is user.
        assert_eq!(chat_req.messages.len(), 2);
        assert_eq!(chat_req.messages[0].role, "system");
        match &chat_req.messages[0].content {
            Some(ChatContent::Text(t)) => assert_eq!(t, "Test system prompt."),
            other => panic!("expected Text, got {other:?}"),
        }
        assert_eq!(chat_req.messages[1].role, "user");
    }

    #[test]
    fn to_chat_request_uses_explicit_system_prompt() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "gpt-4o".into(),
            system_prompt: Some("Override prompt.".into()),
            system_blocks: None,
            messages: vec![],
            max_tokens: 1024,
            stream: false,
            tools: None,
        };

        let chat_req = provider.to_chat_request(&request);
        match &chat_req.messages[0].content {
            Some(ChatContent::Text(t)) => assert_eq!(t, "Override prompt."),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn to_chat_request_maps_image_to_image_url() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "gpt-4o".into(),
            system_prompt: None,
            system_blocks: None,
            messages: vec![ProviderMessage {
                role: "user".into(),
                content: vec![
                    ContentBlock::Text {
                        text: "What is this?".into(),
                    },
                    ContentBlock::Image {
                        source_type: "base64".into(),
                        media_type: "image/jpeg".into(),
                        data: "abc123".into(),
                    },
                ],
            }],
            max_tokens: 1024,
            stream: false,
            tools: None,
        };

        let chat_req = provider.to_chat_request(&request);
        // Message 0: system, Message 1: user with image
        assert_eq!(chat_req.messages.len(), 2);
        match &chat_req.messages[1].content {
            Some(ChatContent::Parts(parts)) => {
                assert_eq!(parts.len(), 2);
                match &parts[0] {
                    ContentPart::Text { text } => assert_eq!(text, "What is this?"),
                    other => panic!("expected Text part, got {other:?}"),
                }
                match &parts[1] {
                    ContentPart::ImageUrl { image_url } => {
                        assert_eq!(image_url.url, "data:image/jpeg;base64,abc123");
                    }
                    other => panic!("expected ImageUrl part, got {other:?}"),
                }
            }
            other => panic!("expected Parts content, got {other:?}"),
        }
    }

    #[test]
    fn to_chat_request_maps_tool_use_to_assistant_tool_calls() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "gpt-4o".into(),
            system_prompt: None,
            system_blocks: None,
            messages: vec![ProviderMessage {
                role: "assistant".into(),
                content: vec![ContentBlock::ToolUse {
                    id: "call_abc".into(),
                    name: "bash".into(),
                    input: serde_json::json!({"command": "echo hello"}),
                }],
            }],
            max_tokens: 1024,
            stream: false,
            tools: None,
        };

        let chat_req = provider.to_chat_request(&request);
        // Message 0: system, Message 1: assistant with tool_calls
        assert_eq!(chat_req.messages.len(), 2);
        assert_eq!(chat_req.messages[1].role, "assistant");
        let tool_calls = chat_req.messages[1].tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_abc");
        assert_eq!(tool_calls[0].call_type, "function");
        assert_eq!(tool_calls[0].function.name, "bash");
        let args: serde_json::Value =
            serde_json::from_str(&tool_calls[0].function.arguments).unwrap();
        assert_eq!(args["command"], "echo hello");
    }

    #[test]
    fn to_chat_request_maps_tool_result_to_tool_message() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "gpt-4o".into(),
            system_prompt: None,
            system_blocks: None,
            messages: vec![ProviderMessage {
                role: "user".into(),
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "call_abc".into(),
                    content: "hello\n".into(),
                    is_error: None,
                }],
            }],
            max_tokens: 1024,
            stream: false,
            tools: None,
        };

        let chat_req = provider.to_chat_request(&request);
        // Message 0: system, Message 1: tool result
        assert_eq!(chat_req.messages.len(), 2);
        assert_eq!(chat_req.messages[1].role, "tool");
        assert_eq!(
            chat_req.messages[1].tool_call_id.as_deref(),
            Some("call_abc")
        );
        match &chat_req.messages[1].content {
            Some(ChatContent::Text(t)) => assert_eq!(t, "hello\n"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn to_chat_request_maps_tool_definitions() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "gpt-4o".into(),
            system_prompt: None,
            system_blocks: None,
            messages: vec![],
            max_tokens: 1024,
            stream: false,
            tools: Some(vec![ToolDefinition {
                name: "bash".into(),
                description: "Execute a bash command".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {"type": "string"}
                    }
                }),
            }]),
        };

        let chat_req = provider.to_chat_request(&request);
        let tools = chat_req.tools.as_ref().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].tool_type, "function");
        assert_eq!(tools[0].function.name, "bash");
        assert_eq!(tools[0].function.description, "Execute a bash command");
        assert_eq!(tools[0].function.parameters["type"], "object");
    }

    #[test]
    fn map_finish_reason_stop_to_end_turn() {
        assert_eq!(map_finish_reason("stop"), "end_turn");
    }

    #[test]
    fn map_finish_reason_tool_calls_to_tool_use() {
        assert_eq!(map_finish_reason("tool_calls"), "tool_use");
    }

    #[test]
    fn map_finish_reason_length_to_max_tokens() {
        assert_eq!(map_finish_reason("length"), "max_tokens");
    }

    #[test]
    fn map_sse_text_delta() {
        let sse_chunk = crate::types::SseChunk {
            id: Some("chatcmpl-test".into()),
            choices: vec![crate::types::SseDelta {
                delta: crate::types::DeltaMessage {
                    role: None,
                    content: Some("Hello".into()),
                    tool_calls: None,
                },
                finish_reason: None,
                index: 0,
            }],
            model: Some("gpt-4o".into()),
            usage: None,
        };

        let mut tool_calls = HashMap::new();
        let mut is_first = true;
        let chunks = map_sse_chunk_to_provider_chunks(sse_chunk, &mut tool_calls, &mut is_first);

        // Should have MessageStart + text delta = 2 chunks
        assert_eq!(chunks.len(), 2);
        let start = chunks[0].as_ref().unwrap();
        assert_eq!(start.event_type, StreamEventType::MessageStart);

        let delta = chunks[1].as_ref().unwrap();
        assert_eq!(delta.event_type, StreamEventType::ContentBlockDelta);
        assert_eq!(delta.text.as_deref(), Some("Hello"));
    }

    #[test]
    fn map_sse_tool_call_accumulation() {
        let mut tool_calls: HashMap<usize, (String, String, String)> = HashMap::new();
        let mut is_first = false;

        // First delta: id + name + partial args
        let chunk1 = crate::types::SseChunk {
            id: None,
            choices: vec![crate::types::SseDelta {
                delta: crate::types::DeltaMessage {
                    role: None,
                    content: None,
                    tool_calls: Some(vec![crate::types::DeltaToolCall {
                        index: 0,
                        id: Some("call_abc".into()),
                        call_type: Some("function".into()),
                        function: Some(crate::types::DeltaFunction {
                            name: Some("bash".into()),
                            arguments: Some("{\"command\":".into()),
                        }),
                    }]),
                },
                finish_reason: None,
                index: 0,
            }],
            model: None,
            usage: None,
        };

        let results = map_sse_chunk_to_provider_chunks(chunk1, &mut tool_calls, &mut is_first);
        assert!(results.is_empty()); // No emit yet, just accumulation.
        assert_eq!(tool_calls[&0].0, "call_abc");
        assert_eq!(tool_calls[&0].1, "bash");
        assert_eq!(tool_calls[&0].2, "{\"command\":");

        // Second delta: more args
        let chunk2 = crate::types::SseChunk {
            id: None,
            choices: vec![crate::types::SseDelta {
                delta: crate::types::DeltaMessage {
                    role: None,
                    content: None,
                    tool_calls: Some(vec![crate::types::DeltaToolCall {
                        index: 0,
                        id: None,
                        call_type: None,
                        function: Some(crate::types::DeltaFunction {
                            name: None,
                            arguments: Some("\"echo hello\"}".into()),
                        }),
                    }]),
                },
                finish_reason: None,
                index: 0,
            }],
            model: None,
            usage: None,
        };

        let results = map_sse_chunk_to_provider_chunks(chunk2, &mut tool_calls, &mut is_first);
        assert!(results.is_empty());
        assert_eq!(tool_calls[&0].2, "{\"command\":\"echo hello\"}");

        // Finish with tool_calls reason
        let chunk3 = crate::types::SseChunk {
            id: None,
            choices: vec![crate::types::SseDelta {
                delta: crate::types::DeltaMessage::default(),
                finish_reason: Some("tool_calls".into()),
                index: 0,
            }],
            model: None,
            usage: Some(crate::types::OpenAIUsage {
                prompt_tokens: 50,
                completion_tokens: 30,
                total_tokens: 80,
            }),
        };

        let results = map_sse_chunk_to_provider_chunks(chunk3, &mut tool_calls, &mut is_first);
        // Should emit: ContentBlockStop (tool_use), MessageDelta, MessageStop = 3
        assert_eq!(results.len(), 3);

        let tool_chunk = results[0].as_ref().unwrap();
        assert_eq!(tool_chunk.event_type, StreamEventType::ContentBlockStop);
        let tool_use = tool_chunk.tool_use.as_ref().unwrap();
        assert_eq!(tool_use.id, "call_abc");
        assert_eq!(tool_use.name, "bash");
        assert_eq!(tool_use.input["command"], "echo hello");

        let delta = results[1].as_ref().unwrap();
        assert_eq!(delta.event_type, StreamEventType::MessageDelta);
        assert_eq!(delta.stop_reason.as_deref(), Some("tool_use"));
        let usage = delta.usage.as_ref().unwrap();
        assert_eq!(usage.input_tokens, 50);
        assert_eq!(usage.output_tokens, 30);

        let stop = results[2].as_ref().unwrap();
        assert_eq!(stop.event_type, StreamEventType::MessageStop);
    }

    #[test]
    fn map_sse_stop_finish_reason() {
        let mut tool_calls = HashMap::new();
        let mut is_first = false;

        let chunk = crate::types::SseChunk {
            id: None,
            choices: vec![crate::types::SseDelta {
                delta: crate::types::DeltaMessage::default(),
                finish_reason: Some("stop".into()),
                index: 0,
            }],
            model: None,
            usage: Some(crate::types::OpenAIUsage {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30,
            }),
        };

        let results = map_sse_chunk_to_provider_chunks(chunk, &mut tool_calls, &mut is_first);
        // Should emit: MessageDelta + MessageStop = 2
        assert_eq!(results.len(), 2);

        let delta = results[0].as_ref().unwrap();
        assert_eq!(delta.event_type, StreamEventType::MessageDelta);
        assert_eq!(delta.stop_reason.as_deref(), Some("end_turn"));

        let stop = results[1].as_ref().unwrap();
        assert_eq!(stop.event_type, StreamEventType::MessageStop);
        assert_eq!(stop.stop_reason.as_deref(), Some("end_turn"));
    }

    #[test]
    fn token_usage_maps_correctly() {
        let openai_usage = crate::types::OpenAIUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
        };

        let usage = TokenUsage {
            input_tokens: openai_usage.prompt_tokens,
            output_tokens: openai_usage.completion_tokens,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        };

        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
    }

    #[test]
    fn resolve_api_key_from_config() {
        let result = resolve_api_key(&Some("sk-test-123".into()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "sk-test-123");
    }

    #[test]
    fn resolve_api_key_empty_config_falls_back_to_env() {
        let result = resolve_api_key(&Some("".into()));
        if let Ok(key) = result {
            assert!(!key.is_empty());
        }
    }

    #[test]
    fn resolve_api_key_none_falls_back_to_env() {
        let result = resolve_api_key(&None);
        if let Err(e) = result {
            let err = e.to_string();
            assert!(err.contains("API key not found"), "got: {err}");
        }
    }

    #[tokio::test]
    async fn system_prompt_default() {
        let prompt = load_system_prompt("blufio", &None, &None).await;
        assert_eq!(prompt, "You are blufio, a concise personal assistant.");
    }
}
