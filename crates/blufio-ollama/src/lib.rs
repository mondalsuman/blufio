// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ollama native provider adapter for the Blufio agent framework.
//!
//! This crate implements [`ProviderAdapter`] for Ollama's native `/api/chat`
//! endpoint with NDJSON streaming, tool calling, and local model discovery.
//!
//! Key differences from cloud providers:
//! - No API key required (Ollama runs locally)
//! - NDJSON streaming (not SSE)
//! - Native `/api/chat` endpoint (not OpenAI compatibility shim)
//! - `/api/tags` for local model discovery

pub mod client;
pub mod stream;
pub mod types;

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
use tracing::{debug, info, warn};

use crate::client::OllamaClient;
use crate::types::{
    OllamaFunction, OllamaFunctionCall, OllamaMessage, OllamaRequest, OllamaResponse, OllamaTool,
    OllamaToolCall,
};

/// Ollama provider implementing [`ProviderAdapter`].
///
/// Communicates with a local Ollama instance via the native `/api/chat` endpoint.
/// No API key required. Uses NDJSON streaming (not SSE).
pub struct OllamaProvider {
    client: OllamaClient,
    #[allow(dead_code)]
    system_prompt: String,
}

impl std::fmt::Debug for OllamaProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OllamaProvider")
            .field("system_prompt", &"<redacted>")
            .finish()
    }
}

impl OllamaProvider {
    /// Creates a new Ollama provider from the given configuration.
    ///
    /// # Validation
    /// 1. `config.providers.ollama.default_model` must be set (no auto-pick)
    /// 2. Ollama must be reachable at the configured base_url
    ///
    /// # System Prompt Resolution
    /// 1. `config.agent.system_prompt_file` if set and file exists
    /// 2. `config.agent.system_prompt` if set
    /// 3. Default: "You are {name}, a concise personal assistant."
    pub async fn new(config: &BlufioConfig) -> Result<Self, BlufioError> {
        // Validate default_model is set.
        let default_model = config
            .providers
            .ollama
            .default_model
            .as_ref()
            .filter(|m| !m.is_empty())
            .ok_or_else(|| {
                BlufioError::Config(
                    "Ollama requires an explicit default_model in config. \
                     Set providers.ollama.default_model."
                        .into(),
                )
            })?
            .clone();

        let client = OllamaClient::new(config.providers.ollama.base_url.clone(), default_model)?;

        // Health check: verify Ollama is reachable.
        client.health_check().await.map_err(|_| {
            BlufioError::Config(format!(
                "Ollama not reachable at {}. Is it running?",
                config.providers.ollama.base_url
            ))
        })?;

        let system_prompt = load_system_prompt(
            &config.agent.name,
            &config.agent.system_prompt,
            &config.agent.system_prompt_file,
        )
        .await;

        info!(
            model = config
                .providers
                .ollama
                .default_model
                .as_deref()
                .unwrap_or("unset"),
            base_url = config.providers.ollama.base_url,
            "Ollama provider initialized"
        );

        Ok(Self {
            client,
            system_prompt,
        })
    }

    /// Creates a provider with an existing client (for testing).
    #[cfg(test)]
    fn with_client(client: OllamaClient, system_prompt: String) -> Self {
        Self {
            client,
            system_prompt,
        }
    }

    /// Lists locally available models via `/api/tags`.
    ///
    /// Returns a list of model names (e.g., `["llama3.2:latest", "mistral:7b"]`).
    pub async fn list_local_models(&self) -> Result<Vec<String>, BlufioError> {
        let tags = self.client.list_tags().await?;
        Ok(tags.models.into_iter().map(|m| m.name).collect())
    }

    /// Converts a [`ProviderRequest`] to an Ollama [`OllamaRequest`].
    ///
    /// Key mappings:
    /// - `system_prompt` -> system role message prepended to messages
    /// - `ContentBlock::Text` -> plain text content
    /// - `ContentBlock::ToolUse` -> assistant message with tool_calls array
    /// - `ContentBlock::ToolResult` -> tool role message
    /// - `ContentBlock::Image` -> skipped with warning (model-dependent)
    /// - `tools` -> `{"type":"function","function":{...}}` format (same as OpenAI)
    fn to_ollama_request(&self, request: &ProviderRequest) -> OllamaRequest {
        let mut messages: Vec<OllamaMessage> = Vec::new();

        // System prompt -> system role message.
        let system_text = request
            .system_prompt
            .clone()
            .unwrap_or_else(|| self.system_prompt.clone());
        messages.push(OllamaMessage {
            role: "system".into(),
            content: system_text,
            tool_calls: None,
        });

        // Convert provider messages.
        for msg in &request.messages {
            messages.extend(convert_provider_message(msg));
        }

        // Convert tools.
        let tools = request.tools.as_ref().map(|defs| {
            defs.iter()
                .map(|td| OllamaTool {
                    type_: "function".into(),
                    function: OllamaFunction {
                        name: td.name.clone(),
                        description: td.description.clone(),
                        parameters: td.input_schema.clone(),
                    },
                })
                .collect::<Vec<_>>()
        });

        OllamaRequest {
            model: request.model.clone(),
            messages,
            stream: request.stream,
            tools,
            format: None,
        }
    }
}

#[async_trait]
impl PluginAdapter for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    fn version(&self) -> semver::Version {
        semver::Version::new(0, 1, 0)
    }

    fn adapter_type(&self) -> AdapterType {
        AdapterType::Provider
    }

    async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
        match self.client.health_check().await {
            Ok(()) => Ok(HealthStatus::Healthy),
            Err(e) => Ok(HealthStatus::Unhealthy(format!("Ollama unreachable: {e}"))),
        }
    }

    async fn shutdown(&self) -> Result<(), BlufioError> {
        debug!("Ollama provider shutting down");
        Ok(())
    }
}

#[async_trait]
impl ProviderAdapter for OllamaProvider {
    async fn complete(&self, request: ProviderRequest) -> Result<ProviderResponse, BlufioError> {
        let api_request = self.to_ollama_request(&request);
        let response = self.client.chat(&api_request).await?;

        // Generate a UUID for the response ID (Ollama doesn't provide one).
        let response_id = format!("ollama-{}", uuid::Uuid::new_v4());

        // Map done_reason to provider stop_reason.
        let stop_reason = response
            .done_reason
            .as_deref()
            .map(|r| map_done_reason(r).to_string());

        // Map Ollama token counts to provider usage.
        let usage = TokenUsage {
            input_tokens: response.prompt_eval_count.unwrap_or(0),
            output_tokens: response.eval_count.unwrap_or(0),
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        };

        Ok(ProviderResponse {
            id: response_id,
            content: response.message.content,
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
        let api_request = self.to_ollama_request(&request);
        let ndjson_stream = self.client.chat_stream(&api_request).await?;

        let mut is_first = true;

        let mapped = ndjson_stream.filter_map(move |result| {
            let chunk = match result {
                Ok(ollama_resp) => map_ollama_response_to_chunks(&ollama_resp, &mut is_first),
                Err(e) => vec![Err(e)],
            };
            async move {
                if chunk.is_empty() {
                    None
                } else {
                    Some(futures::stream::iter(chunk))
                }
            }
        });

        let flattened = mapped.flatten();
        Ok(Box::pin(flattened))
    }
}

/// Maps an Ollama NDJSON response chunk to provider stream chunks.
///
/// Ollama streaming differs from SSE-based providers:
/// - Each NDJSON line has `message.content` (text delta) or `message.tool_calls` (complete)
/// - `done: true` signals the end with timing metadata
/// - Tool calls arrive complete (not as partial deltas like OpenAI)
fn map_ollama_response_to_chunks(
    response: &OllamaResponse,
    is_first: &mut bool,
) -> Vec<Result<ProviderStreamChunk, BlufioError>> {
    let mut chunks = Vec::new();

    // Emit MessageStart on first chunk.
    if *is_first {
        *is_first = false;
        chunks.push(Ok(ProviderStreamChunk {
            event_type: StreamEventType::MessageStart,
            text: None,
            usage: None,
            error: None,
            tool_use: None,
            stop_reason: None,
        }));
    }

    // Handle tool_calls (Ollama sends complete tool calls, not partial deltas).
    if let Some(ref tool_calls) = response.message.tool_calls {
        for tc in tool_calls {
            let tool_use_id = format!("ollama-tc-{}", uuid::Uuid::new_v4());
            chunks.push(Ok(ProviderStreamChunk {
                event_type: StreamEventType::ContentBlockStop,
                text: None,
                usage: None,
                error: None,
                tool_use: Some(ToolUseData {
                    id: tool_use_id,
                    name: tc.function.name.clone(),
                    input: tc.function.arguments.clone(),
                }),
                stop_reason: None,
            }));
        }
    }

    // Handle text content delta.
    if !response.message.content.is_empty() && !response.done {
        chunks.push(Ok(ProviderStreamChunk {
            event_type: StreamEventType::ContentBlockDelta,
            text: Some(response.message.content.clone()),
            usage: None,
            error: None,
            tool_use: None,
            stop_reason: None,
        }));
    }

    // Handle done signal.
    if response.done {
        let stop_reason = response
            .done_reason
            .as_deref()
            .map(|r| map_done_reason(r).to_string());

        // Emit MessageDelta with usage.
        let usage = TokenUsage {
            input_tokens: response.prompt_eval_count.unwrap_or(0),
            output_tokens: response.eval_count.unwrap_or(0),
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        };

        chunks.push(Ok(ProviderStreamChunk {
            event_type: StreamEventType::MessageDelta,
            text: None,
            usage: Some(usage),
            error: None,
            tool_use: None,
            stop_reason: stop_reason.clone(),
        }));

        // Emit MessageStop.
        chunks.push(Ok(ProviderStreamChunk {
            event_type: StreamEventType::MessageStop,
            text: None,
            usage: None,
            error: None,
            tool_use: None,
            stop_reason,
        }));
    }

    chunks
}

/// Maps Ollama `done_reason` to provider-agnostic `stop_reason`.
fn map_done_reason(reason: &str) -> &str {
    match reason {
        "stop" => "end_turn",
        "length" => "max_tokens",
        other => other,
    }
}

/// Converts a [`ProviderMessage`] into one or more Ollama [`OllamaMessage`]s.
///
/// A single ProviderMessage with mixed content blocks may produce multiple
/// OllamaMessages (e.g., ToolUse blocks become a separate assistant message).
fn convert_provider_message(msg: &blufio_core::ProviderMessage) -> Vec<OllamaMessage> {
    let mut messages = Vec::new();

    let mut text_parts: Vec<String> = Vec::new();
    let mut tool_uses: Vec<(String, String, serde_json::Value)> = Vec::new();
    let mut tool_results: Vec<(String, String, Option<bool>)> = Vec::new();

    for block in &msg.content {
        match block {
            ContentBlock::Text { text } => {
                text_parts.push(text.clone());
            }
            ContentBlock::Image { .. } => {
                warn!(
                    "Ollama image content blocks are model-dependent and not universally supported; skipping"
                );
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

    // Emit text content as a regular message if present.
    if !text_parts.is_empty() {
        messages.push(OllamaMessage {
            role: msg.role.clone(),
            content: text_parts.join(""),
            tool_calls: None,
        });
    }

    // Emit tool_use blocks as an assistant message with tool_calls.
    if !tool_uses.is_empty() {
        let calls: Vec<OllamaToolCall> = tool_uses
            .into_iter()
            .map(|(_id, name, input)| OllamaToolCall {
                function: OllamaFunctionCall {
                    name,
                    arguments: input,
                },
            })
            .collect();
        messages.push(OllamaMessage {
            role: "assistant".into(),
            content: String::new(),
            tool_calls: Some(calls),
        });
    }

    // Emit tool results as individual tool messages.
    for (_tool_use_id, content, _is_error) in tool_results {
        messages.push(OllamaMessage {
            role: "tool".into(),
            content,
            tool_calls: None,
        });
    }

    // If nothing was emitted, still emit the message.
    if messages.is_empty() {
        messages.push(OllamaMessage {
            role: msg.role.clone(),
            content: String::new(),
            tool_calls: None,
        });
    }

    messages
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
                warn!(
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

    fn test_provider() -> OllamaProvider {
        let client = OllamaClient::new("http://localhost:11434".into(), "llama3.2".into()).unwrap();
        OllamaProvider::with_client(client, "Test system prompt.".into())
    }

    #[test]
    fn plugin_adapter_name() {
        let provider = test_provider();
        assert_eq!(provider.name(), "ollama");
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
    fn new_fails_if_default_model_is_none() {
        let config: BlufioConfig = toml::from_str("").unwrap();
        // default_model is None by default.
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(OllamaProvider::new(&config));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("default_model"),
            "expected default_model error, got: {err}"
        );
    }

    #[test]
    fn new_fails_if_default_model_is_empty() {
        let config: BlufioConfig = toml::from_str(
            r#"
[providers.ollama]
default_model = ""
"#,
        )
        .unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(OllamaProvider::new(&config));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("default_model"),
            "expected default_model error, got: {err}"
        );
    }

    #[test]
    fn new_fails_if_ollama_unreachable() {
        let config: BlufioConfig = toml::from_str(
            r#"
[providers.ollama]
base_url = "http://127.0.0.1:19999"
default_model = "llama3.2"
"#,
        )
        .unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(OllamaProvider::new(&config));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not reachable") || err.contains("Ollama"),
            "expected reachability error, got: {err}"
        );
    }

    #[test]
    fn to_ollama_request_basic() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "llama3.2".into(),
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

        let ollama_req = provider.to_ollama_request(&request);
        assert_eq!(ollama_req.model, "llama3.2");
        assert!(ollama_req.stream);
        // First message is system, second is user.
        assert_eq!(ollama_req.messages.len(), 2);
        assert_eq!(ollama_req.messages[0].role, "system");
        assert_eq!(ollama_req.messages[0].content, "Test system prompt.");
        assert_eq!(ollama_req.messages[1].role, "user");
        assert_eq!(ollama_req.messages[1].content, "Hello");
    }

    #[test]
    fn to_ollama_request_uses_explicit_system_prompt() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "llama3.2".into(),
            system_prompt: Some("Override prompt.".into()),
            system_blocks: None,
            messages: vec![],
            max_tokens: 1024,
            stream: false,
            tools: None,
        };

        let ollama_req = provider.to_ollama_request(&request);
        assert_eq!(ollama_req.messages[0].role, "system");
        assert_eq!(ollama_req.messages[0].content, "Override prompt.");
    }

    #[test]
    fn to_ollama_request_maps_tool_use_to_assistant_tool_calls() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "llama3.2".into(),
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

        let ollama_req = provider.to_ollama_request(&request);
        // Message 0: system, Message 1: assistant with tool_calls
        assert_eq!(ollama_req.messages.len(), 2);
        assert_eq!(ollama_req.messages[1].role, "assistant");
        let tool_calls = ollama_req.messages[1].tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].function.name, "bash");
        assert_eq!(tool_calls[0].function.arguments["command"], "echo hello");
    }

    #[test]
    fn to_ollama_request_maps_tool_result_to_tool_message() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "llama3.2".into(),
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

        let ollama_req = provider.to_ollama_request(&request);
        // Message 0: system, Message 1: tool result
        assert_eq!(ollama_req.messages.len(), 2);
        assert_eq!(ollama_req.messages[1].role, "tool");
        assert_eq!(ollama_req.messages[1].content, "hello\n");
    }

    #[test]
    fn to_ollama_request_maps_tool_definitions() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "llama3.2".into(),
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

        let ollama_req = provider.to_ollama_request(&request);
        let tools = ollama_req.tools.as_ref().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].type_, "function");
        assert_eq!(tools[0].function.name, "bash");
        assert_eq!(tools[0].function.description, "Execute a bash command");
        assert_eq!(tools[0].function.parameters["type"], "object");
    }

    #[test]
    fn map_done_reason_stop_to_end_turn() {
        assert_eq!(map_done_reason("stop"), "end_turn");
    }

    #[test]
    fn map_done_reason_length_to_max_tokens() {
        assert_eq!(map_done_reason("length"), "max_tokens");
    }

    #[test]
    fn map_done_reason_unknown_passes_through() {
        assert_eq!(map_done_reason("custom"), "custom");
    }

    #[test]
    fn map_streaming_text_content() {
        let response = OllamaResponse {
            model: "llama3.2".into(),
            message: OllamaMessage {
                role: "assistant".into(),
                content: "Hello".into(),
                tool_calls: None,
            },
            done: false,
            done_reason: None,
            created_at: None,
            prompt_eval_count: None,
            eval_count: None,
            total_duration: None,
            load_duration: None,
            prompt_eval_duration: None,
            eval_duration: None,
        };

        let mut is_first = true;
        let chunks = map_ollama_response_to_chunks(&response, &mut is_first);

        // Should emit MessageStart + ContentBlockDelta = 2
        assert_eq!(chunks.len(), 2);
        let start = chunks[0].as_ref().unwrap();
        assert_eq!(start.event_type, StreamEventType::MessageStart);

        let delta = chunks[1].as_ref().unwrap();
        assert_eq!(delta.event_type, StreamEventType::ContentBlockDelta);
        assert_eq!(delta.text.as_deref(), Some("Hello"));
    }

    #[test]
    fn map_streaming_done_with_stop() {
        let response = OllamaResponse {
            model: "llama3.2".into(),
            message: OllamaMessage {
                role: "assistant".into(),
                content: String::new(),
                tool_calls: None,
            },
            done: true,
            done_reason: Some("stop".into()),
            created_at: None,
            prompt_eval_count: Some(26),
            eval_count: Some(15),
            total_duration: None,
            load_duration: None,
            prompt_eval_duration: None,
            eval_duration: None,
        };

        let mut is_first = false;
        let chunks = map_ollama_response_to_chunks(&response, &mut is_first);

        // Should emit MessageDelta + MessageStop = 2
        assert_eq!(chunks.len(), 2);

        let delta = chunks[0].as_ref().unwrap();
        assert_eq!(delta.event_type, StreamEventType::MessageDelta);
        assert_eq!(delta.stop_reason.as_deref(), Some("end_turn"));
        let usage = delta.usage.as_ref().unwrap();
        assert_eq!(usage.input_tokens, 26);
        assert_eq!(usage.output_tokens, 15);

        let stop = chunks[1].as_ref().unwrap();
        assert_eq!(stop.event_type, StreamEventType::MessageStop);
        assert_eq!(stop.stop_reason.as_deref(), Some("end_turn"));
    }

    #[test]
    fn map_streaming_tool_calls() {
        let response = OllamaResponse {
            model: "llama3.2".into(),
            message: OllamaMessage {
                role: "assistant".into(),
                content: String::new(),
                tool_calls: Some(vec![OllamaToolCall {
                    function: OllamaFunctionCall {
                        name: "bash".into(),
                        arguments: serde_json::json!({"command": "echo hello"}),
                    },
                }]),
            },
            done: false,
            done_reason: None,
            created_at: None,
            prompt_eval_count: None,
            eval_count: None,
            total_duration: None,
            load_duration: None,
            prompt_eval_duration: None,
            eval_duration: None,
        };

        let mut is_first = false;
        let chunks = map_ollama_response_to_chunks(&response, &mut is_first);

        // Should emit ContentBlockStop with tool_use data
        assert_eq!(chunks.len(), 1);
        let tc_chunk = chunks[0].as_ref().unwrap();
        assert_eq!(tc_chunk.event_type, StreamEventType::ContentBlockStop);
        let tool_use = tc_chunk.tool_use.as_ref().unwrap();
        assert_eq!(tool_use.name, "bash");
        assert_eq!(tool_use.input["command"], "echo hello");
        assert!(tool_use.id.starts_with("ollama-tc-"));
    }

    #[test]
    fn token_usage_maps_correctly() {
        let response = OllamaResponse {
            model: "llama3.2".into(),
            message: OllamaMessage {
                role: "assistant".into(),
                content: String::new(),
                tool_calls: None,
            },
            done: true,
            done_reason: Some("stop".into()),
            created_at: None,
            prompt_eval_count: Some(100),
            eval_count: Some(50),
            total_duration: None,
            load_duration: None,
            prompt_eval_duration: None,
            eval_duration: None,
        };

        let mut is_first = false;
        let chunks = map_ollama_response_to_chunks(&response, &mut is_first);

        let delta = chunks[0].as_ref().unwrap();
        let usage = delta.usage.as_ref().unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.cache_read_tokens, 0);
        assert_eq!(usage.cache_creation_tokens, 0);
    }

    #[tokio::test]
    async fn list_local_models_returns_names() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "models": [
                    {"name": "llama3.2:latest"},
                    {"name": "mistral:7b"},
                    {"name": "codellama:13b"}
                ]
            })))
            .mount(&server)
            .await;

        let client = OllamaClient::new(server.uri(), "llama3.2".into()).unwrap();
        let provider = OllamaProvider::with_client(client, "test".into());

        let models = provider.list_local_models().await.unwrap();
        assert_eq!(
            models,
            vec!["llama3.2:latest", "mistral:7b", "codellama:13b"]
        );
    }

    #[tokio::test]
    async fn complete_maps_response_correctly() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "model": "llama3.2",
                "message": {"role": "assistant", "content": "Hello there!"},
                "done": true,
                "done_reason": "stop",
                "prompt_eval_count": 26,
                "eval_count": 15
            })))
            .mount(&server)
            .await;

        let client = OllamaClient::new(server.uri(), "llama3.2".into()).unwrap();
        let provider = OllamaProvider::with_client(client, "Test prompt.".into());

        let request = ProviderRequest {
            model: "llama3.2".into(),
            system_prompt: None,
            system_blocks: None,
            messages: vec![ProviderMessage {
                role: "user".into(),
                content: vec![ContentBlock::Text {
                    text: "Hello".into(),
                }],
            }],
            max_tokens: 2048,
            stream: false,
            tools: None,
        };

        let response = provider.complete(request).await.unwrap();
        assert!(response.id.starts_with("ollama-"));
        assert_eq!(response.content, "Hello there!");
        assert_eq!(response.model, "llama3.2");
        assert_eq!(response.stop_reason.as_deref(), Some("end_turn"));
        assert_eq!(response.usage.input_tokens, 26);
        assert_eq!(response.usage.output_tokens, 15);
    }

    #[tokio::test]
    async fn stream_maps_ndjson_to_chunks() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        let ndjson_body = concat!(
            r#"{"model":"llama3.2","message":{"role":"assistant","content":"Hello"},"done":false}"#,
            "\n",
            r#"{"model":"llama3.2","message":{"role":"assistant","content":" world"},"done":false}"#,
            "\n",
            r#"{"model":"llama3.2","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","prompt_eval_count":10,"eval_count":5}"#,
            "\n",
        );

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string(ndjson_body))
            .mount(&server)
            .await;

        let client = OllamaClient::new(server.uri(), "llama3.2".into()).unwrap();
        let provider = OllamaProvider::with_client(client, "Test prompt.".into());

        let request = ProviderRequest {
            model: "llama3.2".into(),
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

        let stream = provider.stream(request).await.unwrap();
        let chunks: Vec<_> = stream.collect().await;

        // Expect: MessageStart, ContentBlockDelta("Hello"), ContentBlockDelta(" world"),
        //         MessageDelta, MessageStop = 5 chunks
        assert_eq!(chunks.len(), 5, "got {} chunks: {:?}", chunks.len(), chunks);

        let c0 = chunks[0].as_ref().unwrap();
        assert_eq!(c0.event_type, StreamEventType::MessageStart);

        let c1 = chunks[1].as_ref().unwrap();
        assert_eq!(c1.event_type, StreamEventType::ContentBlockDelta);
        assert_eq!(c1.text.as_deref(), Some("Hello"));

        let c2 = chunks[2].as_ref().unwrap();
        assert_eq!(c2.event_type, StreamEventType::ContentBlockDelta);
        assert_eq!(c2.text.as_deref(), Some(" world"));

        let c3 = chunks[3].as_ref().unwrap();
        assert_eq!(c3.event_type, StreamEventType::MessageDelta);
        assert_eq!(c3.stop_reason.as_deref(), Some("end_turn"));
        let usage = c3.usage.as_ref().unwrap();
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 5);

        let c4 = chunks[4].as_ref().unwrap();
        assert_eq!(c4.event_type, StreamEventType::MessageStop);
    }

    #[tokio::test]
    async fn system_prompt_default() {
        let prompt = load_system_prompt("blufio", &None, &None).await;
        assert_eq!(prompt, "You are blufio, a concise personal assistant.");
    }

    #[tokio::test]
    async fn system_prompt_inline_overrides_default() {
        let prompt = load_system_prompt("blufio", &Some("Custom prompt.".into()), &None).await;
        assert_eq!(prompt, "Custom prompt.");
    }
}
