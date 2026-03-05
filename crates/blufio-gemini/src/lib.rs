// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Google Gemini provider adapter for the Blufio agent framework.
//!
//! This crate implements [`ProviderAdapter`] for Google's native Gemini API,
//! providing both single-shot completion and streaming responses via
//! streamGenerateContent with function calling support.
//!
//! Key differences from OpenAI/Anthropic:
//! - System prompt uses `systemInstruction` field (not in contents)
//! - Role mapping: "assistant" -> "model"
//! - Function calling uses `functionDeclarations` / `functionCall` / `functionResponse`
//! - Auth via query parameter `?key=` (not header)
//! - Streaming returns chunked JSON (not SSE)

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
use tracing::{debug, info};
use uuid::Uuid;

use crate::client::GeminiClient;
use crate::types::{
    FunctionCall, FunctionCallPart, FunctionDeclaration, FunctionResponse, FunctionResponsePart,
    GenerateContentRequest, GenerateContentResponse, GenerationConfig, GeminiContent, GeminiPart,
    GeminiSystemInstruction, GeminiTool, InlineData, InlineDataPart, TextPart,
};

/// Google Gemini provider implementing [`ProviderAdapter`].
///
/// Uses Google's native Gemini API format (NOT the OpenAI-compatible shim)
/// for best feature support including function calling and vision.
///
/// API key resolution order: config -> `GEMINI_API_KEY` env var -> error.
pub struct GeminiProvider {
    client: GeminiClient,
    system_prompt: String,
}

impl GeminiProvider {
    /// Creates a new Gemini provider from the given configuration.
    ///
    /// # API Key Resolution
    /// 1. `config.providers.gemini.api_key` if set
    /// 2. `GEMINI_API_KEY` environment variable
    /// 3. Returns error if neither is available
    ///
    /// # System Prompt Resolution
    /// 1. `config.agent.system_prompt_file` if set and file exists (read from disk)
    /// 2. `config.agent.system_prompt` if set
    /// 3. Default: "You are {name}, a concise personal assistant."
    pub async fn new(config: &BlufioConfig) -> Result<Self, BlufioError> {
        let api_key = resolve_api_key(&config.providers.gemini.api_key)?;
        let system_prompt = load_system_prompt(
            &config.agent.name,
            &config.agent.system_prompt,
            &config.agent.system_prompt_file,
        )
        .await;

        let client = GeminiClient::new(
            api_key,
            config.providers.gemini.default_model.clone(),
            Some(&config.security),
        )?;

        info!(
            model = config.providers.gemini.default_model,
            "Gemini provider initialized"
        );

        Ok(Self {
            client,
            system_prompt,
        })
    }

    /// Creates a provider with an existing client (for testing).
    #[cfg(test)]
    fn with_client(client: GeminiClient, system_prompt: String) -> Self {
        Self {
            client,
            system_prompt,
        }
    }

    /// Converts a [`ProviderRequest`] to a Gemini [`GenerateContentRequest`].
    ///
    /// Key differences from OpenAI:
    /// - System prompt -> `systemInstruction` field (not in contents)
    /// - "assistant" role -> "model" role
    /// - `ContentBlock::ToolUse` -> `FunctionCallPart` in model content
    /// - `ContentBlock::ToolResult` -> `FunctionResponsePart` in user content
    /// - `ContentBlock::Image` -> `InlineDataPart`
    /// - `tools` -> `GeminiTool { functionDeclarations }`
    fn to_gemini_request(&self, request: &ProviderRequest) -> GenerateContentRequest {
        // System prompt -> systemInstruction (separate from contents).
        let system_text = request
            .system_prompt
            .clone()
            .unwrap_or_else(|| self.system_prompt.clone());

        let system_instruction = Some(GeminiSystemInstruction {
            parts: vec![GeminiPart::Text(TextPart {
                text: system_text,
            })],
        });

        // Convert messages.
        let contents = convert_messages(&request.messages);

        // Convert tools.
        let tools = request.tools.as_ref().map(|defs| {
            vec![GeminiTool {
                function_declarations: defs
                    .iter()
                    .map(|td| FunctionDeclaration {
                        name: td.name.clone(),
                        description: td.description.clone(),
                        parameters: td.input_schema.clone(),
                    })
                    .collect(),
            }]
        });

        // Generation config.
        let generation_config = Some(GenerationConfig {
            max_output_tokens: Some(request.max_tokens),
            temperature: None,
        });

        GenerateContentRequest {
            contents,
            system_instruction,
            tools,
            generation_config,
        }
    }
}

#[async_trait]
impl PluginAdapter for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
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
        debug!("Gemini provider shutting down");
        Ok(())
    }
}

#[async_trait]
impl ProviderAdapter for GeminiProvider {
    async fn complete(&self, request: ProviderRequest) -> Result<ProviderResponse, BlufioError> {
        let model = request.model.clone();
        let api_request = self.to_gemini_request(&request);
        let response = self
            .client
            .generate_content(&api_request, Some(&model))
            .await?;

        map_response_to_provider(response, &model)
    }

    async fn stream(
        &self,
        request: ProviderRequest,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<ProviderStreamChunk, BlufioError>> + Send>>,
        BlufioError,
    > {
        let model = request.model.clone();
        let api_request = self.to_gemini_request(&request);
        let chunk_stream = self
            .client
            .stream_generate_content(&api_request, Some(&model))
            .await?;

        let mut is_first = true;

        let mapped = chunk_stream.flat_map(move |result| {
            let chunks = match result {
                Ok(response) => {
                    map_stream_response_to_chunks(&response, &mut is_first)
                }
                Err(e) => vec![Err(e)],
            };
            futures::stream::iter(chunks)
        });

        Ok(Box::pin(mapped))
    }
}

/// Maps a Gemini `GenerateContentResponse` to a provider-agnostic `ProviderResponse`.
fn map_response_to_provider(
    response: GenerateContentResponse,
    model: &str,
) -> Result<ProviderResponse, BlufioError> {
    let candidate = response.candidates.first().ok_or_else(|| BlufioError::Provider {
        message: "Gemini response contained no candidates".into(),
        source: None,
    })?;

    // Extract text and function calls from parts.
    let mut text_parts = Vec::new();
    let mut has_function_call = false;

    for part in &candidate.content.parts {
        match part {
            GeminiPart::Text(tp) => text_parts.push(tp.text.as_str()),
            GeminiPart::FunctionCall(_) => has_function_call = true,
            _ => {}
        }
    }

    let content = text_parts.join("");

    // Map finish reason.
    let stop_reason = if has_function_call {
        Some("tool_use".to_string())
    } else {
        candidate
            .finish_reason
            .as_deref()
            .map(|r| map_finish_reason(r).to_string())
    };

    // Map usage.
    let usage = response
        .usage_metadata
        .map_or_else(TokenUsage::default, |u| TokenUsage {
            input_tokens: u.prompt_token_count,
            output_tokens: u.candidates_token_count,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        });

    // Gemini doesn't provide a response ID in the same format, generate a UUID.
    let id = Uuid::new_v4().to_string();

    Ok(ProviderResponse {
        id,
        content,
        model: model.to_string(),
        stop_reason,
        usage,
    })
}

/// Maps a single streaming `GenerateContentResponse` chunk to provider stream chunks.
///
/// Gemini streams complete `GenerateContentResponse` objects (unlike OpenAI's delta model).
/// Each chunk may contain text parts, function calls, or finish reasons.
fn map_stream_response_to_chunks(
    response: &GenerateContentResponse,
    is_first: &mut bool,
) -> Vec<Result<ProviderStreamChunk, BlufioError>> {
    let mut chunks = Vec::new();

    // Emit MessageStart on first chunk.
    if *is_first {
        *is_first = false;
        let usage = response.usage_metadata.as_ref().map(|u| TokenUsage {
            input_tokens: u.prompt_token_count,
            output_tokens: u.candidates_token_count,
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

    // Process candidate content.
    if let Some(candidate) = response.candidates.first() {
        for part in &candidate.content.parts {
            match part {
                GeminiPart::Text(tp) => {
                    if !tp.text.is_empty() {
                        chunks.push(Ok(ProviderStreamChunk {
                            event_type: StreamEventType::ContentBlockDelta,
                            text: Some(tp.text.clone()),
                            usage: None,
                            error: None,
                            tool_use: None,
                            stop_reason: None,
                        }));
                    }
                }
                GeminiPart::FunctionCall(fc) => {
                    // Gemini sends complete function calls (not partial deltas).
                    let tool_use = ToolUseData {
                        id: Uuid::new_v4().to_string(),
                        name: fc.function_call.name.clone(),
                        input: fc.function_call.args.clone(),
                    };
                    chunks.push(Ok(ProviderStreamChunk {
                        event_type: StreamEventType::ContentBlockStop,
                        text: None,
                        usage: None,
                        error: None,
                        tool_use: Some(tool_use),
                        stop_reason: None,
                    }));
                }
                _ => {}
            }
        }

        // Handle finish reason.
        if let Some(ref reason) = candidate.finish_reason {
            let has_function_call = candidate.content.parts.iter().any(|p| {
                matches!(p, GeminiPart::FunctionCall(_))
            });

            let stop_reason = if has_function_call {
                "tool_use".to_string()
            } else {
                map_finish_reason(reason).to_string()
            };

            // Emit MessageDelta with stop_reason and usage.
            let usage = response.usage_metadata.as_ref().map(|u| TokenUsage {
                input_tokens: u.prompt_token_count,
                output_tokens: u.candidates_token_count,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            });

            chunks.push(Ok(ProviderStreamChunk {
                event_type: StreamEventType::MessageDelta,
                text: None,
                usage,
                error: None,
                tool_use: None,
                stop_reason: Some(stop_reason.clone()),
            }));

            // Emit MessageStop.
            chunks.push(Ok(ProviderStreamChunk {
                event_type: StreamEventType::MessageStop,
                text: None,
                usage: None,
                error: None,
                tool_use: None,
                stop_reason: Some(stop_reason),
            }));
        }
    }

    chunks
}

/// Maps Gemini `finishReason` to provider-agnostic `stop_reason`.
fn map_finish_reason(reason: &str) -> &str {
    match reason {
        "STOP" => "end_turn",
        "MAX_TOKENS" => "max_tokens",
        "SAFETY" => "content_filter",
        "RECITATION" => "content_filter",
        other => other,
    }
}

/// Converts provider-agnostic messages to Gemini format.
///
/// Key transformations:
/// - "assistant" role -> "model"
/// - `ContentBlock::Text` -> `TextPart`
/// - `ContentBlock::Image` -> `InlineDataPart`
/// - `ContentBlock::ToolUse` -> `FunctionCallPart` in "model" role
/// - `ContentBlock::ToolResult` -> `FunctionResponsePart` in "user" role
fn convert_messages(messages: &[blufio_core::ProviderMessage]) -> Vec<GeminiContent> {
    let mut contents: Vec<GeminiContent> = Vec::new();

    for msg in messages {
        // Separate tool_use and tool_result from regular content.
        let mut regular_parts: Vec<GeminiPart> = Vec::new();
        let mut function_call_parts: Vec<GeminiPart> = Vec::new();
        let mut function_response_parts: Vec<GeminiPart> = Vec::new();

        for block in &msg.content {
            match block {
                ContentBlock::Text { text } => {
                    regular_parts.push(GeminiPart::Text(TextPart {
                        text: text.clone(),
                    }));
                }
                ContentBlock::Image {
                    media_type, data, ..
                } => {
                    regular_parts.push(GeminiPart::InlineData(InlineDataPart {
                        inline_data: InlineData {
                            mime_type: media_type.clone(),
                            data: data.clone(),
                        },
                    }));
                }
                ContentBlock::ToolUse { name, input, .. } => {
                    function_call_parts.push(GeminiPart::FunctionCall(FunctionCallPart {
                        function_call: FunctionCall {
                            name: name.clone(),
                            args: input.clone(),
                        },
                    }));
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    ..
                } => {
                    function_response_parts.push(GeminiPart::FunctionResponse(
                        FunctionResponsePart {
                            function_response: FunctionResponse {
                                name: tool_use_id.clone(),
                                response: serde_json::json!({"result": content}),
                            },
                        },
                    ));
                }
            }
        }

        // Map role: "assistant" -> "model", others stay the same.
        let role = match msg.role.as_str() {
            "assistant" => "model".to_string(),
            other => other.to_string(),
        };

        let has_regular = !regular_parts.is_empty();
        let has_fc = !function_call_parts.is_empty();
        let has_fr = !function_response_parts.is_empty();

        // Emit regular content as a user/model turn.
        if has_regular {
            contents.push(GeminiContent {
                role: role.clone(),
                parts: regular_parts,
            });
        }

        // Emit function calls as a model turn (even if original role was assistant).
        if has_fc {
            contents.push(GeminiContent {
                role: "model".to_string(),
                parts: function_call_parts,
            });
        }

        // Emit function responses as a user turn.
        if has_fr {
            contents.push(GeminiContent {
                role: "user".to_string(),
                parts: function_response_parts,
            });
        }

        // If message had no content at all, emit empty text.
        if !has_regular && !has_fc && !has_fr {
            contents.push(GeminiContent {
                role,
                parts: vec![GeminiPart::Text(TextPart {
                    text: String::new(),
                })],
            });
        }
    }

    contents
}

/// Resolves the API key from config or environment.
fn resolve_api_key(config_key: &Option<String>) -> Result<String, BlufioError> {
    if let Some(key) = config_key
        && !key.is_empty()
    {
        return Ok(key.clone());
    }

    std::env::var("GEMINI_API_KEY").map_err(|_| {
        BlufioError::Config(
            "Gemini API key not found. Set providers.gemini.api_key in config or GEMINI_API_KEY environment variable.".into(),
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
    use blufio_core::types::ToolDefinition;
    use blufio_core::ProviderMessage;

    fn test_provider() -> GeminiProvider {
        let client =
            GeminiClient::new("test-key".into(), "gemini-2.0-flash".into(), None).unwrap();
        GeminiProvider::with_client(client, "Test system prompt.".into())
    }

    // --- PluginAdapter tests ---

    #[test]
    fn plugin_adapter_name() {
        let provider = test_provider();
        assert_eq!(provider.name(), "gemini");
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

    // --- to_gemini_request tests ---

    #[test]
    fn system_prompt_maps_to_system_instruction() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "gemini-2.0-flash".into(),
            system_prompt: None,
            system_blocks: None,
            messages: vec![],
            max_tokens: 2048,
            stream: false,
            tools: None,
        };

        let gemini_req = provider.to_gemini_request(&request);
        let si = gemini_req.system_instruction.unwrap();
        match &si.parts[0] {
            GeminiPart::Text(tp) => assert_eq!(tp.text, "Test system prompt."),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn explicit_system_prompt_overrides_default() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "gemini-2.0-flash".into(),
            system_prompt: Some("Override prompt.".into()),
            system_blocks: None,
            messages: vec![],
            max_tokens: 1024,
            stream: false,
            tools: None,
        };

        let gemini_req = provider.to_gemini_request(&request);
        let si = gemini_req.system_instruction.unwrap();
        match &si.parts[0] {
            GeminiPart::Text(tp) => assert_eq!(tp.text, "Override prompt."),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn user_role_stays_user() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "gemini-2.0-flash".into(),
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

        let gemini_req = provider.to_gemini_request(&request);
        assert_eq!(gemini_req.contents[0].role, "user");
    }

    #[test]
    fn assistant_role_maps_to_model() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "gemini-2.0-flash".into(),
            system_prompt: None,
            system_blocks: None,
            messages: vec![ProviderMessage {
                role: "assistant".into(),
                content: vec![ContentBlock::Text {
                    text: "Hi there!".into(),
                }],
            }],
            max_tokens: 2048,
            stream: false,
            tools: None,
        };

        let gemini_req = provider.to_gemini_request(&request);
        assert_eq!(gemini_req.contents[0].role, "model");
    }

    #[test]
    fn text_block_maps_to_text_part() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "gemini-2.0-flash".into(),
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

        let gemini_req = provider.to_gemini_request(&request);
        match &gemini_req.contents[0].parts[0] {
            GeminiPart::Text(tp) => assert_eq!(tp.text, "Hello"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn image_block_maps_to_inline_data_part() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "gemini-2.0-flash".into(),
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

        let gemini_req = provider.to_gemini_request(&request);
        assert_eq!(gemini_req.contents[0].parts.len(), 2);
        match &gemini_req.contents[0].parts[1] {
            GeminiPart::InlineData(idp) => {
                assert_eq!(idp.inline_data.mime_type, "image/jpeg");
                assert_eq!(idp.inline_data.data, "abc123");
            }
            other => panic!("expected InlineData, got {other:?}"),
        }
    }

    #[test]
    fn tool_use_maps_to_function_call_part() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "gemini-2.0-flash".into(),
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

        let gemini_req = provider.to_gemini_request(&request);
        // Function call parts go in a model turn.
        let fc_turn = gemini_req
            .contents
            .iter()
            .find(|c| c.role == "model")
            .expect("expected model turn with function call");
        match &fc_turn.parts[0] {
            GeminiPart::FunctionCall(fcp) => {
                assert_eq!(fcp.function_call.name, "bash");
                assert_eq!(fcp.function_call.args["command"], "echo hello");
            }
            other => panic!("expected FunctionCall, got {other:?}"),
        }
    }

    #[test]
    fn tool_result_maps_to_function_response_in_user_content() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "gemini-2.0-flash".into(),
            system_prompt: None,
            system_blocks: None,
            messages: vec![ProviderMessage {
                role: "user".into(),
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "bash".into(),
                    content: "hello\n".into(),
                    is_error: None,
                }],
            }],
            max_tokens: 1024,
            stream: false,
            tools: None,
        };

        let gemini_req = provider.to_gemini_request(&request);
        // Function response parts go in a user turn.
        let fr_turn = gemini_req
            .contents
            .iter()
            .find(|c| c.role == "user")
            .expect("expected user turn with function response");
        match &fr_turn.parts[0] {
            GeminiPart::FunctionResponse(frp) => {
                assert_eq!(frp.function_response.name, "bash");
                assert_eq!(frp.function_response.response["result"], "hello\n");
            }
            other => panic!("expected FunctionResponse, got {other:?}"),
        }
    }

    #[test]
    fn tool_definitions_map_to_function_declarations() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "gemini-2.0-flash".into(),
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

        let gemini_req = provider.to_gemini_request(&request);
        let tools = gemini_req.tools.as_ref().unwrap();
        assert_eq!(tools.len(), 1);
        let decls = &tools[0].function_declarations;
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].name, "bash");
        assert_eq!(decls[0].description, "Execute a bash command");
        assert_eq!(decls[0].parameters["type"], "object");
    }

    #[test]
    fn max_tokens_maps_to_generation_config() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "gemini-2.0-flash".into(),
            system_prompt: None,
            system_blocks: None,
            messages: vec![],
            max_tokens: 4096,
            stream: false,
            tools: None,
        };

        let gemini_req = provider.to_gemini_request(&request);
        let gc = gemini_req.generation_config.as_ref().unwrap();
        assert_eq!(gc.max_output_tokens, Some(4096));
    }

    // --- Response mapping tests ---

    #[test]
    fn map_text_response() {
        let response = GenerateContentResponse {
            candidates: vec![crate::types::Candidate {
                content: GeminiContent {
                    role: "model".into(),
                    parts: vec![GeminiPart::Text(TextPart {
                        text: "Hello there!".into(),
                    })],
                },
                finish_reason: Some("STOP".into()),
            }],
            usage_metadata: Some(crate::types::UsageMetadata {
                prompt_token_count: 10,
                candidates_token_count: 5,
                total_token_count: 15,
            }),
        };

        let result = map_response_to_provider(response, "gemini-2.0-flash").unwrap();
        assert_eq!(result.content, "Hello there!");
        assert_eq!(result.stop_reason.as_deref(), Some("end_turn"));
        assert_eq!(result.model, "gemini-2.0-flash");
        assert_eq!(result.usage.input_tokens, 10);
        assert_eq!(result.usage.output_tokens, 5);
    }

    #[test]
    fn map_function_call_response() {
        let response = GenerateContentResponse {
            candidates: vec![crate::types::Candidate {
                content: GeminiContent {
                    role: "model".into(),
                    parts: vec![GeminiPart::FunctionCall(FunctionCallPart {
                        function_call: FunctionCall {
                            name: "bash".into(),
                            args: serde_json::json!({"command": "echo hello"}),
                        },
                    })],
                },
                finish_reason: Some("STOP".into()),
            }],
            usage_metadata: Some(crate::types::UsageMetadata {
                prompt_token_count: 20,
                candidates_token_count: 10,
                total_token_count: 30,
            }),
        };

        let result = map_response_to_provider(response, "gemini-2.0-flash").unwrap();
        // When function call present, stop_reason should be "tool_use".
        assert_eq!(result.stop_reason.as_deref(), Some("tool_use"));
        assert!(result.content.is_empty()); // No text content.
    }

    #[test]
    fn map_finish_reason_stop() {
        assert_eq!(map_finish_reason("STOP"), "end_turn");
    }

    #[test]
    fn map_finish_reason_max_tokens() {
        assert_eq!(map_finish_reason("MAX_TOKENS"), "max_tokens");
    }

    #[test]
    fn map_finish_reason_safety() {
        assert_eq!(map_finish_reason("SAFETY"), "content_filter");
    }

    #[test]
    fn token_usage_maps_correctly() {
        let response = GenerateContentResponse {
            candidates: vec![crate::types::Candidate {
                content: GeminiContent {
                    role: "model".into(),
                    parts: vec![GeminiPart::Text(TextPart {
                        text: "hi".into(),
                    })],
                },
                finish_reason: Some("STOP".into()),
            }],
            usage_metadata: Some(crate::types::UsageMetadata {
                prompt_token_count: 100,
                candidates_token_count: 50,
                total_token_count: 150,
            }),
        };

        let result = map_response_to_provider(response, "gemini-2.0-flash").unwrap();
        assert_eq!(result.usage.input_tokens, 100);
        assert_eq!(result.usage.output_tokens, 50);
        assert_eq!(result.usage.cache_read_tokens, 0);
        assert_eq!(result.usage.cache_creation_tokens, 0);
    }

    // --- Stream mapping tests ---

    #[test]
    fn stream_text_chunk() {
        let response = GenerateContentResponse {
            candidates: vec![crate::types::Candidate {
                content: GeminiContent {
                    role: "model".into(),
                    parts: vec![GeminiPart::Text(TextPart {
                        text: "Hello".into(),
                    })],
                },
                finish_reason: None,
            }],
            usage_metadata: None,
        };

        let mut is_first = true;
        let chunks = map_stream_response_to_chunks(&response, &mut is_first);

        // Should have MessageStart + ContentBlockDelta = 2 chunks.
        assert_eq!(chunks.len(), 2);
        let start = chunks[0].as_ref().unwrap();
        assert_eq!(start.event_type, StreamEventType::MessageStart);

        let delta = chunks[1].as_ref().unwrap();
        assert_eq!(delta.event_type, StreamEventType::ContentBlockDelta);
        assert_eq!(delta.text.as_deref(), Some("Hello"));
    }

    #[test]
    fn stream_function_call_chunk() {
        let response = GenerateContentResponse {
            candidates: vec![crate::types::Candidate {
                content: GeminiContent {
                    role: "model".into(),
                    parts: vec![GeminiPart::FunctionCall(FunctionCallPart {
                        function_call: FunctionCall {
                            name: "bash".into(),
                            args: serde_json::json!({"command": "echo hello"}),
                        },
                    })],
                },
                finish_reason: Some("STOP".into()),
            }],
            usage_metadata: Some(crate::types::UsageMetadata {
                prompt_token_count: 20,
                candidates_token_count: 10,
                total_token_count: 30,
            }),
        };

        let mut is_first = false;
        let chunks = map_stream_response_to_chunks(&response, &mut is_first);

        // Should have: ContentBlockStop (tool_use) + MessageDelta + MessageStop = 3 chunks.
        assert_eq!(chunks.len(), 3);

        let tool_chunk = chunks[0].as_ref().unwrap();
        assert_eq!(tool_chunk.event_type, StreamEventType::ContentBlockStop);
        let tool_use = tool_chunk.tool_use.as_ref().unwrap();
        assert_eq!(tool_use.name, "bash");
        assert_eq!(tool_use.input["command"], "echo hello");

        let delta = chunks[1].as_ref().unwrap();
        assert_eq!(delta.event_type, StreamEventType::MessageDelta);
        assert_eq!(delta.stop_reason.as_deref(), Some("tool_use"));
        let usage = delta.usage.as_ref().unwrap();
        assert_eq!(usage.input_tokens, 20);
        assert_eq!(usage.output_tokens, 10);

        let stop = chunks[2].as_ref().unwrap();
        assert_eq!(stop.event_type, StreamEventType::MessageStop);
    }

    #[test]
    fn stream_finish_stop() {
        let response = GenerateContentResponse {
            candidates: vec![crate::types::Candidate {
                content: GeminiContent {
                    role: "model".into(),
                    parts: vec![GeminiPart::Text(TextPart {
                        text: "Done.".into(),
                    })],
                },
                finish_reason: Some("STOP".into()),
            }],
            usage_metadata: Some(crate::types::UsageMetadata {
                prompt_token_count: 10,
                candidates_token_count: 20,
                total_token_count: 30,
            }),
        };

        let mut is_first = false;
        let chunks = map_stream_response_to_chunks(&response, &mut is_first);

        // ContentBlockDelta + MessageDelta + MessageStop = 3
        assert_eq!(chunks.len(), 3);

        let delta = chunks[1].as_ref().unwrap();
        assert_eq!(delta.stop_reason.as_deref(), Some("end_turn"));

        let stop = chunks[2].as_ref().unwrap();
        assert_eq!(stop.stop_reason.as_deref(), Some("end_turn"));
    }

    // --- API key resolution tests ---

    #[test]
    fn resolve_api_key_from_config() {
        let result = resolve_api_key(&Some("test-key-123".into()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test-key-123");
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

    // --- System prompt tests ---

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

    // --- Full serialization round-trip test ---

    #[test]
    fn gemini_request_serializes_with_correct_casing() {
        let provider = test_provider();
        let request = ProviderRequest {
            model: "gemini-2.0-flash".into(),
            system_prompt: Some("Be helpful.".into()),
            system_blocks: None,
            messages: vec![ProviderMessage {
                role: "user".into(),
                content: vec![ContentBlock::Text {
                    text: "Hello".into(),
                }],
            }],
            max_tokens: 4096,
            stream: false,
            tools: Some(vec![ToolDefinition {
                name: "bash".into(),
                description: "Run command".into(),
                input_schema: serde_json::json!({"type": "object"}),
            }]),
        };

        let gemini_req = provider.to_gemini_request(&request);
        let json = serde_json::to_value(&gemini_req).unwrap();

        // Verify camelCase field names.
        assert!(json.get("systemInstruction").is_some());
        assert!(json.get("generationConfig").is_some());
        assert_eq!(json["generationConfig"]["maxOutputTokens"], 4096);
        assert!(json["tools"][0].get("functionDeclarations").is_some());

        // Verify system instruction is separate from contents.
        assert_eq!(
            json["systemInstruction"]["parts"][0]["text"],
            "Be helpful."
        );
        // Contents should only have the user message, not the system prompt.
        assert_eq!(json["contents"].as_array().unwrap().len(), 1);
        assert_eq!(json["contents"][0]["role"], "user");
    }
}
