// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! OpenAI-compatible wire types for the gateway API.
//!
//! These types are completely separate from `blufio-openai::types` (which
//! handles outbound requests TO the OpenAI API). These types handle
//! inbound requests FROM external callers and outbound responses TO them.
//!
//! Key difference: These use `finish_reason` (OpenAI convention), not
//! `stop_reason` (internal convention).

use blufio_core::traits::provider_registry::ModelInfo;
use blufio_core::types::{ContentBlock, ProviderMessage, ProviderRequest, ToolDefinition};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Request types (Deserialize — incoming from OpenAI SDK clients)
// ---------------------------------------------------------------------------

/// POST /v1/chat/completions request body.
#[derive(Debug, Clone, Deserialize)]
pub struct GatewayCompletionRequest {
    /// Model identifier. Supports `provider/model` format (e.g., "openai/gpt-4o")
    /// or bare model names (routed to the default provider).
    pub model: String,

    /// Conversation messages.
    pub messages: Vec<GatewayMessage>,

    /// Whether to stream the response via SSE.
    #[serde(default)]
    pub stream: bool,

    /// Sampling temperature (0.0 - 2.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    /// Maximum tokens to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Tool definitions available for the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<GatewayTool>>,

    /// Tool choice constraint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,

    /// Response format (e.g., `{"type": "json_object"}`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<serde_json::Value>,

    /// Stop sequences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<serde_json::Value>,

    /// Number of completions (only 1 supported).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,

    /// Stream options (e.g., include_usage).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<GatewayStreamOptions>,
}

/// A message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayMessage {
    /// Role: "system", "user", "assistant", or "tool".
    pub role: String,

    /// Content — either a string or array of content parts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<GatewayContent>,

    /// Tool calls made by the assistant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<GatewayToolCall>>,

    /// Tool call ID (for role="tool" messages).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,

    /// Optional name field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Content within a message — either a string or array of typed parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GatewayContent {
    /// Simple text content.
    Text(String),
    /// Array of typed content parts (text, image_url, etc.).
    Parts(Vec<GatewayContentPart>),
}

/// A typed content part within a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GatewayContentPart {
    /// Text content part.
    #[serde(rename = "text")]
    Text {
        /// The text content.
        text: String,
    },
    /// Image URL content part (for vision).
    #[serde(rename = "image_url")]
    ImageUrl {
        /// The image URL data.
        image_url: GatewayImageUrl,
    },
}

/// Image URL data for vision content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayImageUrl {
    /// The URL (can be data: URI with base64).
    pub url: String,
}

/// An OpenAI tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayTool {
    /// Tool type (always "function").
    #[serde(rename = "type")]
    pub tool_type: String,
    /// Function definition.
    pub function: GatewayFunctionDef,
}

/// Function definition within a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayFunctionDef {
    /// Function name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// JSON Schema for parameters.
    pub parameters: serde_json::Value,
}

/// A tool call made by the assistant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayToolCall {
    /// Unique tool call identifier.
    pub id: String,
    /// Tool type (always "function").
    #[serde(rename = "type")]
    pub call_type: String,
    /// Function call details.
    pub function: GatewayFunctionCall,
}

/// Function call details within a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayFunctionCall {
    /// Function name.
    pub name: String,
    /// JSON-serialized arguments.
    pub arguments: String,
}

/// Stream options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayStreamOptions {
    /// Whether to include usage stats in the stream.
    #[serde(default)]
    pub include_usage: bool,
}

// ---------------------------------------------------------------------------
// Response types (Serialize — outgoing to OpenAI SDK clients)
// ---------------------------------------------------------------------------

/// POST /v1/chat/completions response body (non-streaming).
#[derive(Debug, Clone, Serialize)]
pub struct GatewayCompletionResponse {
    /// Response ID.
    pub id: String,
    /// Object type (always "chat.completion").
    pub object: String,
    /// Unix timestamp of creation.
    pub created: i64,
    /// Model that generated the response.
    pub model: String,
    /// Response choices.
    pub choices: Vec<GatewayChoice>,
    /// Token usage statistics.
    pub usage: GatewayUsage,
    /// Extended: provider name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x_provider: Option<String>,
    /// Extended: latency in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x_latency_ms: Option<u64>,
}

/// A single choice in a response.
#[derive(Debug, Clone, Serialize)]
pub struct GatewayChoice {
    /// Choice index.
    pub index: u32,
    /// Generated message.
    pub message: GatewayResponseMessage,
    /// Reason the generation stopped (OpenAI convention).
    pub finish_reason: Option<String>,
}

/// Response message (different from request message — has no tool_call_id).
#[derive(Debug, Clone, Serialize)]
pub struct GatewayResponseMessage {
    /// Role (always "assistant").
    pub role: String,
    /// Text content.
    pub content: Option<String>,
    /// Tool calls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<GatewayToolCall>>,
}

/// Token usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayUsage {
    /// Number of prompt tokens consumed.
    pub prompt_tokens: u32,
    /// Number of completion tokens generated.
    pub completion_tokens: u32,
    /// Total tokens used.
    pub total_tokens: u32,
}

// ---------------------------------------------------------------------------
// SSE streaming types (Serialize)
// ---------------------------------------------------------------------------

/// A single SSE chunk in the streaming response.
#[derive(Debug, Clone, Serialize)]
pub struct GatewaySseChunk {
    /// Chunk ID.
    pub id: String,
    /// Object type (always "chat.completion.chunk").
    pub object: String,
    /// Unix timestamp.
    pub created: i64,
    /// Model identifier.
    pub model: String,
    /// Delta choices.
    pub choices: Vec<GatewaySseDelta>,
    /// Usage (only in final chunk when stream_options.include_usage is true).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<GatewayUsage>,
}

/// A single delta choice within an SSE chunk.
#[derive(Debug, Clone, Serialize)]
pub struct GatewaySseDelta {
    /// Choice index.
    pub index: u32,
    /// Delta content.
    pub delta: GatewayDeltaMessage,
    /// Finish reason (present in the final chunk).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// Delta message content within an SSE chunk.
#[derive(Debug, Clone, Default, Serialize)]
pub struct GatewayDeltaMessage {
    /// Role (present in first delta only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Text content delta.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Tool call deltas.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<GatewayDeltaToolCall>>,
}

/// A tool call delta within a streaming response.
#[derive(Debug, Clone, Serialize)]
pub struct GatewayDeltaToolCall {
    /// Tool call index.
    pub index: u32,
    /// Tool call ID (present in first delta for this tool call).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Tool type.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub call_type: Option<String>,
    /// Function delta.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<GatewayDeltaFunction>,
}

/// Function delta within a streaming tool call.
#[derive(Debug, Clone, Serialize)]
pub struct GatewayDeltaFunction {
    /// Function name (present in first delta).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Partial JSON arguments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// OpenAI-compatible error response.
#[derive(Debug, Clone, Serialize)]
pub struct GatewayErrorResponse {
    /// Error details.
    pub error: GatewayErrorDetail,
}

/// Error detail within an error response.
#[derive(Debug, Clone, Serialize)]
pub struct GatewayErrorDetail {
    /// Human-readable error message.
    pub message: String,
    /// Error type (e.g., "invalid_request_error", "server_error").
    #[serde(rename = "type")]
    pub error_type: String,
    /// Parameter that caused the error (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
    /// Error code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Extended: provider that returned the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Extended: retry-after seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<u64>,
}

// ---------------------------------------------------------------------------
// Models list types
// ---------------------------------------------------------------------------

/// Response for GET /v1/models.
#[derive(Debug, Serialize)]
pub struct ModelsListResponse {
    /// Object type (always "list").
    pub object: String,
    /// Model data.
    pub data: Vec<ModelInfo>,
}

/// Query parameters for GET /v1/models.
#[derive(Debug, Deserialize)]
pub struct ModelsQueryParams {
    /// Filter by provider name.
    #[serde(default)]
    pub provider: Option<String>,
}

// ---------------------------------------------------------------------------
// Conversion functions
// ---------------------------------------------------------------------------

/// Maps internal `stop_reason` to OpenAI `finish_reason`.
pub fn stop_reason_to_finish_reason(stop: &str) -> &str {
    match stop {
        "end_turn" => "stop",
        "tool_use" => "tool_calls",
        "max_tokens" => "length",
        "content_filter" => "content_filter",
        other => other,
    }
}

/// Maps OpenAI `finish_reason` to internal `stop_reason`.
pub fn finish_reason_to_stop_reason(finish: &str) -> &str {
    match finish {
        "stop" => "end_turn",
        "tool_calls" => "tool_use",
        "length" => "max_tokens",
        "content_filter" => "content_filter",
        other => other,
    }
}

/// Parse a model string into (provider_name, model_name).
///
/// Format: `provider/model` (e.g., "openai/gpt-4o") or bare model name.
/// Bare names use the default provider.
pub fn parse_model_string(model: &str, default_provider: &str) -> (String, String) {
    if let Some(idx) = model.find('/') {
        let provider = &model[..idx];
        let model_name = &model[idx + 1..];
        (provider.to_string(), model_name.to_string())
    } else {
        (default_provider.to_string(), model.to_string())
    }
}

/// Convert a `GatewayCompletionRequest` to a `ProviderRequest`.
pub fn gateway_request_to_provider_request(
    req: &GatewayCompletionRequest,
) -> Result<ProviderRequest, String> {
    if req.messages.is_empty() {
        return Err("messages must not be empty".into());
    }

    // Check n parameter (only 1 supported).
    if let Some(n) = req.n
        && n != 1
    {
        return Err("n parameter must be 1 (multiple completions not supported)".into());
    }

    // Separate system messages from conversation messages.
    let mut system_prompt: Option<String> = None;
    let mut messages: Vec<ProviderMessage> = Vec::new();

    for msg in &req.messages {
        if msg.role == "system" {
            // Extract system prompt from system role message.
            let text = match &msg.content {
                Some(GatewayContent::Text(t)) => t.clone(),
                Some(GatewayContent::Parts(parts)) => parts
                    .iter()
                    .filter_map(|p| match p {
                        GatewayContentPart::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(""),
                None => String::new(),
            };
            system_prompt = Some(text);
            continue;
        }

        // Convert gateway message to provider message.
        let mut content_blocks: Vec<ContentBlock> = Vec::new();

        // Extract text/image content.
        match &msg.content {
            Some(GatewayContent::Text(text)) => {
                content_blocks.push(ContentBlock::Text { text: text.clone() });
            }
            Some(GatewayContent::Parts(parts)) => {
                for part in parts {
                    match part {
                        GatewayContentPart::Text { text } => {
                            content_blocks.push(ContentBlock::Text { text: text.clone() });
                        }
                        GatewayContentPart::ImageUrl { image_url } => {
                            // Parse data: URIs into source_type, media_type, data.
                            if let Some(rest) = image_url.url.strip_prefix("data:")
                                && let Some(semi) = rest.find(';')
                            {
                                let media_type = &rest[..semi];
                                let after_semi = &rest[semi + 1..];
                                if let Some(comma) = after_semi.find(',') {
                                    let data = &after_semi[comma + 1..];
                                    content_blocks.push(ContentBlock::Image {
                                        source_type: "base64".into(),
                                        media_type: media_type.to_string(),
                                        data: data.to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            None => {}
        }

        // Extract tool calls (assistant message).
        if let Some(tool_calls) = &msg.tool_calls {
            for tc in tool_calls {
                content_blocks.push(ContentBlock::ToolUse {
                    id: tc.id.clone(),
                    name: tc.function.name.clone(),
                    input: serde_json::from_str(&tc.function.arguments)
                        .unwrap_or(serde_json::json!({})),
                });
            }
        }

        // Extract tool result (tool message).
        if msg.role == "tool"
            && let Some(tool_call_id) = &msg.tool_call_id
        {
            let text = match &msg.content {
                Some(GatewayContent::Text(t)) => t.clone(),
                _ => String::new(),
            };
            content_blocks = vec![ContentBlock::ToolResult {
                tool_use_id: tool_call_id.clone(),
                content: text,
                is_error: None,
            }];
        }

        if content_blocks.is_empty() {
            content_blocks.push(ContentBlock::Text {
                text: String::new(),
            });
        }

        messages.push(ProviderMessage {
            role: msg.role.clone(),
            content: content_blocks,
        });
    }

    // Convert tools.
    let tools = req.tools.as_ref().map(|ts| {
        ts.iter()
            .map(|t| ToolDefinition {
                name: t.function.name.clone(),
                description: t.function.description.clone(),
                input_schema: t.function.parameters.clone(),
            })
            .collect()
    });

    Ok(ProviderRequest {
        model: req.model.clone(),
        system_prompt,
        system_blocks: None,
        messages,
        max_tokens: req.max_tokens.unwrap_or(4096),
        stream: req.stream,
        tools,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_completion_request_deserializes_basic() {
        let json = r#"{
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "Hello"}]
        }"#;
        let req: GatewayCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "gpt-4o");
        assert!(!req.stream);
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].role, "user");
    }

    #[test]
    fn gateway_completion_request_with_tools() {
        let json = r#"{
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "Hello"}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "bash",
                    "description": "Execute a bash command",
                    "parameters": {"type": "object", "properties": {"command": {"type": "string"}}}
                }
            }],
            "stream": true,
            "stream_options": {"include_usage": true}
        }"#;
        let req: GatewayCompletionRequest = serde_json::from_str(json).unwrap();
        assert!(req.stream);
        let tools = req.tools.as_ref().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].function.name, "bash");
        let so = req.stream_options.as_ref().unwrap();
        assert!(so.include_usage);
    }

    #[test]
    fn gateway_completion_response_serializes() {
        let resp = GatewayCompletionResponse {
            id: "chatcmpl-test".into(),
            object: "chat.completion".into(),
            created: 1700000000,
            model: "gpt-4o".into(),
            choices: vec![GatewayChoice {
                index: 0,
                message: GatewayResponseMessage {
                    role: "assistant".into(),
                    content: Some("Hello!".into()),
                    tool_calls: None,
                },
                finish_reason: Some("stop".into()),
            }],
            usage: GatewayUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
            x_provider: Some("openai".into()),
            x_latency_ms: Some(150),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["id"], "chatcmpl-test");
        assert_eq!(json["object"], "chat.completion");
        assert_eq!(json["choices"][0]["finish_reason"], "stop");
        assert_eq!(json["usage"]["prompt_tokens"], 10);
        assert_eq!(json["x_provider"], "openai");
        assert_eq!(json["x_latency_ms"], 150);
    }

    #[test]
    fn gateway_sse_chunk_serializes() {
        let chunk = GatewaySseChunk {
            id: "chatcmpl-test".into(),
            object: "chat.completion.chunk".into(),
            created: 1700000000,
            model: "gpt-4o".into(),
            choices: vec![GatewaySseDelta {
                index: 0,
                delta: GatewayDeltaMessage {
                    role: None,
                    content: Some("Hello".into()),
                    tool_calls: None,
                },
                finish_reason: None,
            }],
            usage: None,
        };
        let json = serde_json::to_value(&chunk).unwrap();
        assert_eq!(json["object"], "chat.completion.chunk");
        assert_eq!(json["choices"][0]["delta"]["content"], "Hello");
        assert!(json.get("usage").is_none());
    }

    #[test]
    fn gateway_error_response_serializes() {
        let err = GatewayErrorResponse {
            error: GatewayErrorDetail {
                message: "Invalid model".into(),
                error_type: "invalid_request_error".into(),
                param: Some("model".into()),
                code: Some("model_not_found".into()),
                provider: None,
                retry_after: None,
            },
        };
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["error"]["message"], "Invalid model");
        assert_eq!(json["error"]["type"], "invalid_request_error");
        assert_eq!(json["error"]["param"], "model");
        assert_eq!(json["error"]["code"], "model_not_found");
    }

    #[test]
    fn stop_reason_to_finish_reason_mappings() {
        assert_eq!(stop_reason_to_finish_reason("end_turn"), "stop");
        assert_eq!(stop_reason_to_finish_reason("tool_use"), "tool_calls");
        assert_eq!(stop_reason_to_finish_reason("max_tokens"), "length");
        assert_eq!(
            stop_reason_to_finish_reason("content_filter"),
            "content_filter"
        );
        assert_eq!(stop_reason_to_finish_reason("unknown"), "unknown");
    }

    #[test]
    fn finish_reason_to_stop_reason_mappings() {
        assert_eq!(finish_reason_to_stop_reason("stop"), "end_turn");
        assert_eq!(finish_reason_to_stop_reason("tool_calls"), "tool_use");
        assert_eq!(finish_reason_to_stop_reason("length"), "max_tokens");
    }

    #[test]
    fn parse_model_string_with_provider() {
        let (prov, model) = parse_model_string("openai/gpt-4o", "anthropic");
        assert_eq!(prov, "openai");
        assert_eq!(model, "gpt-4o");
    }

    #[test]
    fn parse_model_string_bare_name() {
        let (prov, model) = parse_model_string("gpt-4o", "anthropic");
        assert_eq!(prov, "anthropic");
        assert_eq!(model, "gpt-4o");
    }

    #[test]
    fn gateway_request_to_provider_request_basic() {
        let req = GatewayCompletionRequest {
            model: "gpt-4o".into(),
            messages: vec![
                GatewayMessage {
                    role: "system".into(),
                    content: Some(GatewayContent::Text("You are helpful.".into())),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                },
                GatewayMessage {
                    role: "user".into(),
                    content: Some(GatewayContent::Text("Hello".into())),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                },
            ],
            stream: false,
            temperature: None,
            max_tokens: Some(1024),
            tools: None,
            tool_choice: None,
            response_format: None,
            stop: None,
            n: None,
            stream_options: None,
        };

        let provider_req = gateway_request_to_provider_request(&req).unwrap();
        assert_eq!(
            provider_req.system_prompt.as_deref(),
            Some("You are helpful.")
        );
        assert_eq!(provider_req.messages.len(), 1); // system extracted, user remains
        assert_eq!(provider_req.messages[0].role, "user");
        assert_eq!(provider_req.max_tokens, 1024);
    }

    #[test]
    fn gateway_request_empty_messages_returns_error() {
        let req = GatewayCompletionRequest {
            model: "gpt-4o".into(),
            messages: vec![],
            stream: false,
            temperature: None,
            max_tokens: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            stop: None,
            n: None,
            stream_options: None,
        };

        let result = gateway_request_to_provider_request(&req);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("messages must not be empty"));
    }

    #[test]
    fn gateway_request_with_tool_calls() {
        let req = GatewayCompletionRequest {
            model: "gpt-4o".into(),
            messages: vec![GatewayMessage {
                role: "assistant".into(),
                content: None,
                tool_calls: Some(vec![GatewayToolCall {
                    id: "call_abc".into(),
                    call_type: "function".into(),
                    function: GatewayFunctionCall {
                        name: "bash".into(),
                        arguments: r#"{"command":"echo hello"}"#.into(),
                    },
                }]),
                tool_call_id: None,
                name: None,
            }],
            stream: false,
            temperature: None,
            max_tokens: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            stop: None,
            n: None,
            stream_options: None,
        };

        let provider_req = gateway_request_to_provider_request(&req).unwrap();
        assert_eq!(provider_req.messages.len(), 1);
        assert!(matches!(
            &provider_req.messages[0].content[0],
            ContentBlock::ToolUse { id, name, .. } if id == "call_abc" && name == "bash"
        ));
    }

    #[test]
    fn gateway_request_with_tool_result() {
        let req = GatewayCompletionRequest {
            model: "gpt-4o".into(),
            messages: vec![GatewayMessage {
                role: "tool".into(),
                content: Some(GatewayContent::Text("hello\n".into())),
                tool_calls: None,
                tool_call_id: Some("call_abc".into()),
                name: None,
            }],
            stream: false,
            temperature: None,
            max_tokens: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            stop: None,
            n: None,
            stream_options: None,
        };

        let provider_req = gateway_request_to_provider_request(&req).unwrap();
        assert!(matches!(
            &provider_req.messages[0].content[0],
            ContentBlock::ToolResult { tool_use_id, content, .. } if tool_use_id == "call_abc" && content == "hello\n"
        ));
    }

    #[test]
    fn models_list_response_serializes() {
        let resp = ModelsListResponse {
            object: "list".into(),
            data: vec![ModelInfo {
                id: "openai/gpt-4o".into(),
                object: "model".into(),
                created: 0,
                owned_by: "openai".into(),
            }],
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["object"], "list");
        assert_eq!(json["data"][0]["id"], "openai/gpt-4o");
    }

    #[test]
    fn stream_options_roundtrips() {
        let json = r#"{"include_usage": true}"#;
        let so: GatewayStreamOptions = serde_json::from_str(json).unwrap();
        assert!(so.include_usage);
        let out = serde_json::to_value(&so).unwrap();
        assert_eq!(out["include_usage"], true);
    }

    #[test]
    fn gateway_message_with_array_content() {
        let json = r#"{
            "role": "user",
            "content": [
                {"type": "text", "text": "What is this?"},
                {"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,abc123"}}
            ]
        }"#;
        let msg: GatewayMessage = serde_json::from_str(json).unwrap();
        match &msg.content {
            Some(GatewayContent::Parts(parts)) => {
                assert_eq!(parts.len(), 2);
            }
            other => panic!("expected Parts, got {other:?}"),
        }
    }
}
