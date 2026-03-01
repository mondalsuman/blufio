// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Anthropic Messages API request/response types and SSE event types.

use serde::{Deserialize, Serialize};

// --- Cache control types ---

/// Marker for Anthropic prompt caching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControlMarker {
    /// Cache control type (e.g., "ephemeral").
    #[serde(rename = "type")]
    pub control_type: String,
}

impl CacheControlMarker {
    /// Creates an ephemeral cache control marker.
    pub fn ephemeral() -> Self {
        Self {
            control_type: "ephemeral".to_string(),
        }
    }
}

/// System prompt content -- either a plain string or structured blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SystemContent {
    /// Simple text system prompt.
    Text(String),
    /// Array of structured system blocks with optional cache control.
    Blocks(Vec<SystemBlock>),
}

/// A structured block within a system prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemBlock {
    /// Block type (e.g., "text").
    #[serde(rename = "type")]
    pub block_type: String,
    /// Text content of the block.
    pub text: String,
    /// Optional cache control marker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlMarker>,
}

// --- Tool types ---

/// A tool definition for the Anthropic Messages API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name (unique identifier).
    pub name: String,
    /// Human-readable description of what the tool does.
    pub description: String,
    /// JSON Schema describing the tool's input parameters.
    pub input_schema: serde_json::Value,
}

// --- Request types ---

/// A request to the Anthropic Messages API.
#[derive(Debug, Clone, Serialize)]
pub struct MessageRequest {
    /// Model identifier (e.g., "claude-sonnet-4-20250514").
    pub model: String,

    /// Conversation messages.
    pub messages: Vec<ApiMessage>,

    /// System prompt (optional) -- can be plain text or structured blocks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemContent>,

    /// Maximum tokens to generate.
    pub max_tokens: u32,

    /// Whether to stream the response.
    pub stream: bool,

    /// Top-level cache control for the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlMarker>,

    /// Tool definitions available for the model to use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
}

/// A single message in the Anthropic conversation format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMessage {
    /// Role: "user" or "assistant".
    pub role: String,

    /// Content -- either a plain string or an array of content blocks.
    pub content: ApiContent,
}

/// Content within an API message -- can be a simple string or structured blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ApiContent {
    /// Simple text content.
    Text(String),
    /// Array of typed content blocks (text, image, etc.).
    Blocks(Vec<ApiContentBlock>),
}

/// A typed content block within a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ApiContentBlock {
    /// Text content block.
    #[serde(rename = "text")]
    Text { text: String },
    /// Image content block (base64 encoded).
    #[serde(rename = "image")]
    Image { source: ImageSource },
    /// Tool use content block (sent by assistant).
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Tool result content block (sent by user in response to tool_use).
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

/// Source data for an image content block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    /// Source type (always "base64" for inline images).
    #[serde(rename = "type")]
    pub source_type: String,
    /// MIME type (e.g., "image/jpeg", "image/png").
    pub media_type: String,
    /// Base64-encoded image data.
    pub data: String,
}

// --- Response types ---

/// A full response from the Anthropic Messages API.
#[derive(Debug, Clone, Deserialize)]
pub struct MessageResponse {
    /// Response ID.
    pub id: String,
    /// Response type (always "message").
    #[serde(rename = "type")]
    pub type_: String,
    /// Role (always "assistant").
    pub role: String,
    /// Content blocks in the response.
    pub content: Vec<ResponseContentBlock>,
    /// Model that generated the response.
    pub model: String,
    /// Reason the generation stopped.
    pub stop_reason: Option<String>,
    /// Token usage statistics.
    pub usage: ApiUsage,
}

/// A content block in a response.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum ResponseContentBlock {
    /// Text content block.
    #[serde(rename = "text")]
    Text { text: String },
    /// Tool use content block -- the model is requesting a tool invocation.
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

/// Token usage statistics from the API.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ApiUsage {
    /// Number of input tokens consumed.
    pub input_tokens: u32,
    /// Number of output tokens generated.
    pub output_tokens: u32,
    /// Number of tokens read from prompt cache.
    #[serde(default)]
    pub cache_read_input_tokens: u32,
    /// Number of tokens written to prompt cache.
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
}

// --- SSE event types ---

/// SSE event: message_start
#[derive(Debug, Clone, Deserialize)]
pub struct SseMessageStart {
    /// The initial message object.
    pub message: MessageResponse,
}

/// SSE event: content_block_start
#[derive(Debug, Clone, Deserialize)]
pub struct SseContentBlockStart {
    /// Index of the content block.
    pub index: usize,
    /// The content block being started.
    pub content_block: ResponseContentBlock,
}

/// SSE event: content_block_delta
#[derive(Debug, Clone, Deserialize)]
pub struct SseContentBlockDelta {
    /// Index of the content block being updated.
    pub index: usize,
    /// The delta update.
    pub delta: SseDelta,
}

/// A delta update within a content block.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum SseDelta {
    /// Text delta -- appends text to the current block.
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    /// JSON delta for tool use -- appends partial JSON.
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
}

/// SSE event: content_block_stop
#[derive(Debug, Clone, Deserialize)]
pub struct SseContentBlockStop {
    /// Index of the content block that stopped.
    pub index: usize,
}

/// SSE event: message_delta
#[derive(Debug, Clone, Deserialize)]
pub struct SseMessageDelta {
    /// Delta information (stop reason, etc.).
    pub delta: SseMessageDeltaInfo,
    /// Updated usage statistics.
    pub usage: Option<ApiUsage>,
}

/// Delta information for a message_delta event.
#[derive(Debug, Clone, Deserialize)]
pub struct SseMessageDeltaInfo {
    /// Reason the generation stopped.
    pub stop_reason: Option<String>,
}

/// SSE event: error
#[derive(Debug, Clone, Deserialize)]
pub struct SseError {
    /// Error details.
    pub error: SseErrorDetail,
}

/// Error detail within an SSE error event.
#[derive(Debug, Clone, Deserialize)]
pub struct SseErrorDetail {
    /// Error type identifier.
    #[serde(rename = "type")]
    pub type_: String,
    /// Human-readable error message.
    pub message: String,
}

/// API error response (non-streaming).
#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorResponse {
    /// Error details.
    pub error: ApiErrorDetail,
}

/// Error detail within an API error response.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorDetail {
    /// Error type identifier.
    #[serde(rename = "type")]
    pub type_: String,
    /// Human-readable error message.
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_message_request_with_stream() {
        let req = MessageRequest {
            model: "claude-sonnet-4-20250514".into(),
            messages: vec![ApiMessage {
                role: "user".into(),
                content: ApiContent::Text("Hello".into()),
            }],
            system: Some(SystemContent::Text("You are helpful.".into())),
            max_tokens: 4096,
            stream: true,
            cache_control: None,
            tools: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "claude-sonnet-4-20250514");
        assert_eq!(json["stream"], true);
        assert_eq!(json["max_tokens"], 4096);
        assert_eq!(json["system"], "You are helpful.");
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["messages"][0]["content"], "Hello");
        assert!(json.get("cache_control").is_none());
    }

    #[test]
    fn serialize_message_request_without_system() {
        let req = MessageRequest {
            model: "claude-sonnet-4-20250514".into(),
            messages: vec![],
            system: None,
            max_tokens: 1024,
            stream: false,
            cache_control: None,
            tools: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("system").is_none());
    }

    #[test]
    fn serialize_system_content_text() {
        let sc = SystemContent::Text("hello".into());
        let json = serde_json::to_value(&sc).unwrap();
        assert_eq!(json, "hello");
    }

    #[test]
    fn serialize_system_content_blocks() {
        let sc = SystemContent::Blocks(vec![SystemBlock {
            block_type: "text".into(),
            text: "System prompt here.".into(),
            cache_control: Some(CacheControlMarker::ephemeral()),
        }]);
        let json = serde_json::to_value(&sc).unwrap();
        assert!(json.is_array());
        assert_eq!(json[0]["type"], "text");
        assert_eq!(json[0]["text"], "System prompt here.");
        assert_eq!(json[0]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn serialize_cache_control_marker() {
        let m = CacheControlMarker::ephemeral();
        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(json["type"], "ephemeral");
    }

    #[test]
    fn deserialize_api_usage_with_cache_fields() {
        let json = r#"{
            "input_tokens": 100,
            "output_tokens": 50,
            "cache_read_input_tokens": 80,
            "cache_creation_input_tokens": 20
        }"#;
        let usage: ApiUsage = serde_json::from_str(json).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.cache_read_input_tokens, 80);
        assert_eq!(usage.cache_creation_input_tokens, 20);
    }

    #[test]
    fn deserialize_api_usage_without_cache_fields_defaults_zero() {
        let json = r#"{"input_tokens": 10, "output_tokens": 5}"#;
        let usage: ApiUsage = serde_json::from_str(json).unwrap();
        assert_eq!(usage.cache_read_input_tokens, 0);
        assert_eq!(usage.cache_creation_input_tokens, 0);
    }

    #[test]
    fn serialize_message_request_with_cache_control() {
        let req = MessageRequest {
            model: "claude-sonnet-4-20250514".into(),
            messages: vec![],
            system: Some(SystemContent::Blocks(vec![SystemBlock {
                block_type: "text".into(),
                text: "cached prompt".into(),
                cache_control: Some(CacheControlMarker::ephemeral()),
            }])),
            max_tokens: 1024,
            stream: false,
            cache_control: Some(CacheControlMarker::ephemeral()),
            tools: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["cache_control"]["type"], "ephemeral");
        assert!(json["system"].is_array());
    }

    #[test]
    fn serialize_image_content_block() {
        let msg = ApiMessage {
            role: "user".into(),
            content: ApiContent::Blocks(vec![
                ApiContentBlock::Text {
                    text: "What is this?".into(),
                },
                ApiContentBlock::Image {
                    source: ImageSource {
                        source_type: "base64".into(),
                        media_type: "image/jpeg".into(),
                        data: "abc123==".into(),
                    },
                },
            ]),
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["content"][0]["type"], "text");
        assert_eq!(json["content"][1]["type"], "image");
        assert_eq!(json["content"][1]["source"]["type"], "base64");
    }

    #[test]
    fn deserialize_message_response() {
        let json = r#"{
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "Hello!"}],
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        }"#;
        let resp: MessageResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, "msg_123");
        assert_eq!(resp.model, "claude-sonnet-4-20250514");
        assert_eq!(resp.stop_reason, Some("end_turn".into()));
        assert_eq!(resp.usage.input_tokens, 10);
        assert_eq!(resp.usage.output_tokens, 5);
        assert_eq!(resp.content.len(), 1);
    }

    #[test]
    fn deserialize_sse_content_block_delta_text() {
        let json = r#"{"index": 0, "delta": {"type": "text_delta", "text": "Hello"}}"#;
        let delta: SseContentBlockDelta = serde_json::from_str(json).unwrap();
        assert_eq!(delta.index, 0);
        match delta.delta {
            SseDelta::TextDelta { ref text } => assert_eq!(text, "Hello"),
            _ => panic!("expected TextDelta"),
        }
    }

    #[test]
    fn deserialize_sse_content_block_delta_json() {
        let json =
            r#"{"index": 0, "delta": {"type": "input_json_delta", "partial_json": "{\"key\":"}}"#;
        let delta: SseContentBlockDelta = serde_json::from_str(json).unwrap();
        match delta.delta {
            SseDelta::InputJsonDelta { ref partial_json } => {
                assert_eq!(partial_json, "{\"key\":")
            }
            _ => panic!("expected InputJsonDelta"),
        }
    }

    #[test]
    fn deserialize_sse_message_delta() {
        let json = r#"{"delta": {"stop_reason": "end_turn"}, "usage": {"input_tokens": 100, "output_tokens": 50}}"#;
        let md: SseMessageDelta = serde_json::from_str(json).unwrap();
        assert_eq!(md.delta.stop_reason, Some("end_turn".into()));
        assert_eq!(md.usage.as_ref().unwrap().output_tokens, 50);
    }

    #[test]
    fn deserialize_sse_error() {
        let json = r#"{"error": {"type": "overloaded_error", "message": "Overloaded"}}"#;
        let err: SseError = serde_json::from_str(json).unwrap();
        assert_eq!(err.error.type_, "overloaded_error");
        assert_eq!(err.error.message, "Overloaded");
    }

    #[test]
    fn deserialize_api_content_text() {
        let json = r#"{"role": "user", "content": "Hello"}"#;
        let msg: ApiMessage = serde_json::from_str(json).unwrap();
        match msg.content {
            ApiContent::Text(ref t) => assert_eq!(t, "Hello"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn deserialize_api_content_blocks() {
        let json = r#"{"role": "assistant", "content": [{"type": "text", "text": "Hi"}]}"#;
        let msg: ApiMessage = serde_json::from_str(json).unwrap();
        match msg.content {
            ApiContent::Blocks(ref blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ApiContentBlock::Text { text } => assert_eq!(text, "Hi"),
                    _ => panic!("expected Text block"),
                }
            }
            _ => panic!("expected Blocks"),
        }
    }

    #[test]
    fn serialize_message_request_with_tools() {
        let req = MessageRequest {
            model: "claude-sonnet-4-20250514".into(),
            messages: vec![],
            system: None,
            max_tokens: 1024,
            stream: false,
            cache_control: None,
            tools: Some(vec![ToolDefinition {
                name: "bash".into(),
                description: "Execute a bash command".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {"type": "string"}
                    },
                    "required": ["command"]
                }),
            }]),
        };
        let json = serde_json::to_value(&req).unwrap();
        let tools = json["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "bash");
        assert_eq!(tools[0]["description"], "Execute a bash command");
        assert!(tools[0]["input_schema"]["properties"]["command"].is_object());
    }

    #[test]
    fn serialize_message_request_without_tools_omits_field() {
        let req = MessageRequest {
            model: "claude-sonnet-4-20250514".into(),
            messages: vec![],
            system: None,
            max_tokens: 1024,
            stream: false,
            cache_control: None,
            tools: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("tools").is_none());
    }

    #[test]
    fn deserialize_tool_use_response_content_block() {
        let json = r#"{
            "type": "tool_use",
            "id": "toolu_abc123",
            "name": "bash",
            "input": {"command": "echo hello"}
        }"#;
        let block: ResponseContentBlock = serde_json::from_str(json).unwrap();
        match block {
            ResponseContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "toolu_abc123");
                assert_eq!(name, "bash");
                assert_eq!(input["command"], "echo hello");
            }
            _ => panic!("expected ToolUse"),
        }
    }

    #[test]
    fn serialize_tool_result_content_block() {
        let block = ApiContentBlock::ToolResult {
            tool_use_id: "toolu_abc123".into(),
            content: "hello\n".into(),
            is_error: None,
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "tool_result");
        assert_eq!(json["tool_use_id"], "toolu_abc123");
        assert_eq!(json["content"], "hello\n");
        assert!(json.get("is_error").is_none());
    }

    #[test]
    fn serialize_tool_result_with_error() {
        let block = ApiContentBlock::ToolResult {
            tool_use_id: "toolu_xyz".into(),
            content: "command failed".into(),
            is_error: Some(true),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "tool_result");
        assert_eq!(json["is_error"], true);
    }

    #[test]
    fn serialize_tool_use_content_block() {
        let block = ApiContentBlock::ToolUse {
            id: "toolu_abc".into(),
            name: "bash".into(),
            input: serde_json::json!({"command": "ls"}),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "tool_use");
        assert_eq!(json["id"], "toolu_abc");
        assert_eq!(json["name"], "bash");
        assert_eq!(json["input"]["command"], "ls");
    }

    #[test]
    fn deserialize_tool_definition() {
        let json = r#"{
            "name": "http",
            "description": "Make HTTP requests",
            "input_schema": {"type": "object", "properties": {"url": {"type": "string"}}}
        }"#;
        let def: ToolDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(def.name, "http");
        assert_eq!(def.description, "Make HTTP requests");
    }

    #[test]
    fn deserialize_message_response_with_tool_use() {
        let json = r#"{
            "id": "msg_tool",
            "type": "message",
            "role": "assistant",
            "content": [
                {"type": "text", "text": "Let me run that command."},
                {"type": "tool_use", "id": "toolu_123", "name": "bash", "input": {"command": "echo hi"}}
            ],
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 20, "output_tokens": 15}
        }"#;
        let resp: MessageResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.content.len(), 2);
        assert!(matches!(&resp.content[0], ResponseContentBlock::Text { .. }));
        assert!(matches!(&resp.content[1], ResponseContentBlock::ToolUse { .. }));
        assert_eq!(resp.stop_reason, Some("tool_use".into()));
    }
}
