// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! OpenRouter API request/response types and SSE event types.
//!
//! OpenRouter uses an OpenAI-compatible format with additional fields
//! for provider preference routing. Types are owned independently
//! (not shared with blufio-openai) for decoupling.

use serde::{Deserialize, Serialize};

// --- Provider preference types ---

/// Provider routing preferences for OpenRouter.
///
/// Controls which backend providers are preferred and whether
/// fallbacks are allowed when the primary provider is unavailable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPreferences {
    /// Preferred provider ordering (e.g., ["Anthropic", "Google"]).
    pub order: Vec<String>,

    /// Whether to allow fallback to other providers.
    pub allow_fallbacks: bool,
}

// --- Request types ---

/// A request to the OpenRouter Chat Completions API.
///
/// Extends the OpenAI chat completions format with an optional `provider`
/// field for routing preferences.
#[derive(Debug, Clone, Serialize)]
pub struct RouterRequest {
    /// Model identifier (full OpenRouter format, e.g., "anthropic/claude-sonnet-4").
    pub model: String,

    /// Conversation messages.
    pub messages: Vec<ChatMessage>,

    /// Maximum tokens to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,

    /// Whether to stream the response.
    pub stream: bool,

    /// Tool definitions available for the model to use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAITool>>,

    /// Enable streaming usage reporting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,

    /// OpenRouter-specific provider routing preferences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderPreferences>,
}

/// Options for streaming responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamOptions {
    /// Include usage stats in the stream.
    pub include_usage: bool,
}

/// A single message in the OpenAI conversation format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Role: "system", "user", "assistant", or "tool".
    pub role: String,

    /// Content -- either a plain string or an array of content parts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<ChatContent>,

    /// Tool calls made by the assistant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,

    /// Tool call ID (for role="tool" messages).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Content within a message -- either a string or array of content parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatContent {
    /// Simple text content.
    Text(String),
    /// Array of typed content parts (text, image_url, etc.).
    Parts(Vec<ContentPart>),
}

/// A typed content part within a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
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
        image_url: ImageUrlData,
    },
}

/// Image URL data for vision content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrlData {
    /// The URL of the image (can be a data: URI with base64).
    pub url: String,
}

/// An OpenAI-compatible tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAITool {
    /// Tool type (always "function").
    #[serde(rename = "type")]
    pub tool_type: String,
    /// Function definition.
    pub function: FunctionDef,
}

/// Function definition within an OpenAI tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    /// Function name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// JSON Schema for parameters.
    pub parameters: serde_json::Value,
}

/// A tool call made by the assistant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique tool call identifier.
    pub id: String,
    /// Tool type (always "function").
    #[serde(rename = "type")]
    pub call_type: String,
    /// Function call details.
    pub function: FunctionCall,
}

/// Function call details within a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Function name.
    pub name: String,
    /// JSON-serialized arguments.
    pub arguments: String,
}

// --- Response types ---

/// A full response from the OpenRouter Chat Completions API.
#[derive(Debug, Clone, Deserialize)]
pub struct RouterResponse {
    /// Response ID.
    pub id: String,
    /// Response choices.
    pub choices: Vec<Choice>,
    /// Model that generated the response.
    pub model: String,
    /// Token usage statistics.
    #[serde(default)]
    pub usage: Option<OpenAIUsage>,
}

/// A single choice in a response.
#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    /// Generated message.
    pub message: ChatMessage,
    /// Reason the generation stopped.
    pub finish_reason: Option<String>,
    /// Choice index.
    pub index: u32,
}

/// Token usage statistics from the API (OpenAI-compatible).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct OpenAIUsage {
    /// Number of prompt tokens consumed.
    #[serde(default)]
    pub prompt_tokens: u32,
    /// Number of completion tokens generated.
    #[serde(default)]
    pub completion_tokens: u32,
    /// Total tokens used.
    #[serde(default)]
    pub total_tokens: u32,
}

// --- SSE streaming types ---

/// A single SSE chunk from the streaming API.
#[derive(Debug, Clone, Deserialize)]
pub struct SseChunk {
    /// Chunk ID.
    #[serde(default)]
    pub id: Option<String>,
    /// Stream choices.
    #[serde(default)]
    pub choices: Vec<SseDelta>,
    /// Model identifier.
    #[serde(default)]
    pub model: Option<String>,
    /// Token usage (appears in final chunk when stream_options.include_usage is true).
    #[serde(default)]
    pub usage: Option<OpenAIUsage>,
}

/// A single delta choice within an SSE chunk.
#[derive(Debug, Clone, Deserialize)]
pub struct SseDelta {
    /// Delta update to the message.
    pub delta: DeltaMessage,
    /// Finish reason (present in the final chunk).
    pub finish_reason: Option<String>,
    /// Choice index.
    #[serde(default)]
    pub index: u32,
}

/// Delta message content within an SSE chunk.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DeltaMessage {
    /// Role (present in first delta only).
    #[serde(default)]
    pub role: Option<String>,
    /// Text content delta.
    #[serde(default)]
    pub content: Option<String>,
    /// Tool call deltas.
    #[serde(default)]
    pub tool_calls: Option<Vec<DeltaToolCall>>,
}

/// A tool call delta within a streaming response.
#[derive(Debug, Clone, Deserialize)]
pub struct DeltaToolCall {
    /// Tool call index (for accumulation across deltas).
    pub index: usize,
    /// Tool call ID (present in the first delta for this tool call).
    #[serde(default)]
    pub id: Option<String>,
    /// Tool type.
    #[serde(default, rename = "type")]
    pub call_type: Option<String>,
    /// Function delta.
    #[serde(default)]
    pub function: Option<DeltaFunction>,
}

/// Function delta within a tool call streaming response.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DeltaFunction {
    /// Function name (present in first delta).
    #[serde(default)]
    pub name: Option<String>,
    /// Partial JSON arguments (accumulated across deltas).
    #[serde(default)]
    pub arguments: Option<String>,
}

// --- Error types ---

/// API error response (non-streaming).
#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorResponse {
    /// Error details.
    pub error: ApiErrorDetail,
}

/// Error detail within an API error response.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorDetail {
    /// Human-readable error message.
    pub message: String,
    /// Error type identifier.
    #[serde(rename = "type")]
    pub type_: Option<String>,
    /// Error code.
    #[serde(default)]
    pub code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_router_request_with_provider_preferences() {
        let req = RouterRequest {
            model: "anthropic/claude-sonnet-4".into(),
            messages: vec![
                ChatMessage {
                    role: "system".into(),
                    content: Some(ChatContent::Text("You are helpful.".into())),
                    tool_calls: None,
                    tool_call_id: None,
                },
                ChatMessage {
                    role: "user".into(),
                    content: Some(ChatContent::Text("Hello".into())),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            max_completion_tokens: Some(4096),
            stream: true,
            tools: None,
            stream_options: None,
            provider: Some(ProviderPreferences {
                order: vec!["Anthropic".into(), "Google".into()],
                allow_fallbacks: true,
            }),
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "anthropic/claude-sonnet-4");
        assert_eq!(json["stream"], true);
        assert_eq!(json["max_completion_tokens"], 4096);

        // Verify provider preferences.
        let provider = &json["provider"];
        assert_eq!(provider["order"][0], "Anthropic");
        assert_eq!(provider["order"][1], "Google");
        assert_eq!(provider["allow_fallbacks"], true);
    }

    #[test]
    fn serialize_router_request_without_provider_preferences() {
        let req = RouterRequest {
            model: "anthropic/claude-sonnet-4".into(),
            messages: vec![],
            max_completion_tokens: None,
            stream: false,
            tools: None,
            stream_options: None,
            provider: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert!(
            json.get("provider").is_none(),
            "provider should be omitted when None"
        );
        assert!(json.get("max_completion_tokens").is_none());
    }

    #[test]
    fn deserialize_router_response() {
        let json = r#"{
            "id": "chatcmpl-test",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Hello!"
                },
                "finish_reason": "stop",
                "index": 0
            }],
            "model": "anthropic/claude-sonnet-4",
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        }"#;
        let resp: RouterResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, "chatcmpl-test");
        assert_eq!(resp.model, "anthropic/claude-sonnet-4");
        assert_eq!(resp.choices.len(), 1);
        assert_eq!(resp.choices[0].finish_reason.as_deref(), Some("stop"));
        let usage = resp.usage.as_ref().unwrap();
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 5);
    }

    #[test]
    fn deserialize_sse_chunk_text_delta() {
        let json = r#"{
            "id": "chatcmpl-xxx",
            "choices": [{
                "delta": {"content": "Hello"},
                "finish_reason": null,
                "index": 0
            }],
            "model": "anthropic/claude-sonnet-4"
        }"#;
        let chunk: SseChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices.len(), 1);
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("Hello"));
        assert!(chunk.choices[0].finish_reason.is_none());
    }

    #[test]
    fn deserialize_sse_chunk_tool_call_delta() {
        let json = r#"{
            "id": "chatcmpl-xxx",
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_abc",
                        "type": "function",
                        "function": {
                            "name": "bash",
                            "arguments": "{\"command\":"
                        }
                    }]
                },
                "finish_reason": null,
                "index": 0
            }],
            "model": "anthropic/claude-sonnet-4"
        }"#;
        let chunk: SseChunk = serde_json::from_str(json).unwrap();
        let tool_calls = chunk.choices[0].delta.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id.as_deref(), Some("call_abc"));
        let func = tool_calls[0].function.as_ref().unwrap();
        assert_eq!(func.name.as_deref(), Some("bash"));
        assert_eq!(func.arguments.as_deref(), Some("{\"command\":"));
    }

    #[test]
    fn deserialize_sse_chunk_finish_reason_with_usage() {
        let json = r#"{
            "id": "chatcmpl-xxx",
            "choices": [{
                "delta": {},
                "finish_reason": "stop",
                "index": 0
            }],
            "model": "anthropic/claude-sonnet-4",
            "usage": {"prompt_tokens": 10, "completion_tokens": 20, "total_tokens": 30}
        }"#;
        let chunk: SseChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices[0].finish_reason.as_deref(), Some("stop"));
        let usage = chunk.usage.as_ref().unwrap();
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 20);
    }

    #[test]
    fn deserialize_done_signal_is_not_valid_json() {
        // Verify [DONE] is not parseable as SseChunk -- the SSE parser handles it.
        let result = serde_json::from_str::<SseChunk>("[DONE]");
        assert!(result.is_err());
    }

    #[test]
    fn serialize_tool_definitions() {
        let tool = OpenAITool {
            tool_type: "function".into(),
            function: FunctionDef {
                name: "bash".into(),
                description: "Execute a bash command".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {"type": "string"}
                    },
                    "required": ["command"]
                }),
            },
        };
        let json = serde_json::to_value(&tool).unwrap();
        assert_eq!(json["type"], "function");
        assert_eq!(json["function"]["name"], "bash");
        assert_eq!(json["function"]["description"], "Execute a bash command");
    }

    #[test]
    fn deserialize_api_error_response() {
        let json = r#"{
            "error": {
                "message": "Invalid API key",
                "type": "invalid_request_error",
                "code": "invalid_api_key"
            }
        }"#;
        let err: ApiErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(err.error.message, "Invalid API key");
        assert_eq!(err.error.type_.as_deref(), Some("invalid_request_error"));
        assert_eq!(err.error.code.as_deref(), Some("invalid_api_key"));
    }
}
