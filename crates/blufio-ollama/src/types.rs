// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ollama native API request/response types.
//!
//! These map directly to Ollama's `/api/chat` and `/api/tags` wire format,
//! using NDJSON (newline-delimited JSON) for streaming instead of SSE.

use serde::{Deserialize, Serialize};

// --- Request types ---

/// A request to the Ollama `/api/chat` endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct OllamaRequest {
    /// Model name (e.g., "llama3.2").
    pub model: String,

    /// Conversation messages.
    pub messages: Vec<OllamaMessage>,

    /// Whether to stream the response as NDJSON.
    pub stream: bool,

    /// Tool definitions available for the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OllamaTool>>,

    /// Optional output format (e.g., "json").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

/// A single message in the Ollama conversation format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaMessage {
    /// Role: "system", "user", "assistant", or "tool".
    pub role: String,

    /// Text content of the message.
    pub content: String,

    /// Tool calls made by the assistant (present when role is "assistant").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OllamaToolCall>>,
}

/// An Ollama tool definition (same format as OpenAI: type/function wrapper).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaTool {
    /// Tool type (always "function").
    #[serde(rename = "type")]
    pub type_: String,

    /// Function definition.
    pub function: OllamaFunction,
}

/// Function definition within an Ollama tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaFunction {
    /// Function name.
    pub name: String,

    /// Human-readable description.
    pub description: String,

    /// JSON Schema for parameters.
    pub parameters: serde_json::Value,
}

/// A tool call made by the assistant in an Ollama response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaToolCall {
    /// Function call details.
    pub function: OllamaFunctionCall,
}

/// Function call details within a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaFunctionCall {
    /// Function name.
    pub name: String,

    /// Parsed arguments as a JSON value.
    pub arguments: serde_json::Value,
}

// --- Response types ---

/// A response (or streaming chunk) from the Ollama `/api/chat` endpoint.
///
/// In streaming mode, each line of NDJSON is one `OllamaResponse`.
/// The final chunk has `done: true` with timing metadata.
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaResponse {
    /// Model name.
    pub model: String,

    /// The assistant's message (partial in streaming, full in non-streaming).
    pub message: OllamaMessage,

    /// Whether this is the final chunk.
    pub done: bool,

    /// Reason the generation stopped (present when done=true).
    #[serde(default)]
    pub done_reason: Option<String>,

    /// ISO 8601 creation timestamp.
    #[serde(default)]
    pub created_at: Option<String>,

    /// Number of tokens in the prompt evaluation.
    #[serde(default)]
    pub prompt_eval_count: Option<u32>,

    /// Number of tokens generated.
    #[serde(default)]
    pub eval_count: Option<u32>,

    /// Total duration in nanoseconds.
    #[serde(default)]
    pub total_duration: Option<u64>,

    /// Model load duration in nanoseconds.
    #[serde(default)]
    pub load_duration: Option<u64>,

    /// Prompt evaluation duration in nanoseconds.
    #[serde(default)]
    pub prompt_eval_duration: Option<u64>,

    /// Response evaluation duration in nanoseconds.
    #[serde(default)]
    pub eval_duration: Option<u64>,
}

// --- Tags response types ---

/// Response from the Ollama `/api/tags` endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct TagsResponse {
    /// Available models.
    pub models: Vec<ModelInfo>,
}

/// Information about a locally available Ollama model.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelInfo {
    /// Model name (e.g., "llama3.2:latest").
    pub name: String,

    /// Last modified timestamp.
    #[serde(default)]
    pub modified_at: Option<String>,

    /// Model size in bytes.
    #[serde(default)]
    pub size: Option<u64>,

    /// Model digest.
    #[serde(default)]
    pub digest: Option<String>,

    /// Additional model details.
    #[serde(default)]
    pub details: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_ollama_request_basic() {
        let req = OllamaRequest {
            model: "llama3.2".into(),
            messages: vec![OllamaMessage {
                role: "user".into(),
                content: "Hello".into(),
                tool_calls: None,
            }],
            stream: true,
            tools: None,
            format: None,
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "llama3.2");
        assert!(json["stream"].as_bool().unwrap());
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["messages"][0]["content"], "Hello");
        // tools and format should be absent when None
        assert!(json.get("tools").is_none());
        assert!(json.get("format").is_none());
    }

    #[test]
    fn serialize_ollama_request_with_tools() {
        let req = OllamaRequest {
            model: "llama3.2".into(),
            messages: vec![OllamaMessage {
                role: "user".into(),
                content: "List files".into(),
                tool_calls: None,
            }],
            stream: true,
            tools: Some(vec![OllamaTool {
                type_: "function".into(),
                function: OllamaFunction {
                    name: "bash".into(),
                    description: "Execute a bash command".into(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "command": {"type": "string"}
                        }
                    }),
                },
            }]),
            format: None,
        };

        let json = serde_json::to_value(&req).unwrap();
        let tools = json["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["function"]["name"], "bash");
        assert_eq!(
            tools[0]["function"]["description"],
            "Execute a bash command"
        );
    }

    #[test]
    fn deserialize_ollama_response_streaming_content() {
        let json = r#"{
            "model": "llama3.2",
            "message": {"role": "assistant", "content": "Hi"},
            "done": false
        }"#;
        let resp: OllamaResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.model, "llama3.2");
        assert_eq!(resp.message.role, "assistant");
        assert_eq!(resp.message.content, "Hi");
        assert!(!resp.done);
        assert!(resp.done_reason.is_none());
    }

    #[test]
    fn deserialize_ollama_response_done() {
        let json = r#"{
            "model": "llama3.2",
            "message": {"role": "assistant", "content": ""},
            "done": true,
            "done_reason": "stop",
            "prompt_eval_count": 26,
            "eval_count": 15,
            "total_duration": 1234567890,
            "load_duration": 100000,
            "prompt_eval_duration": 500000,
            "eval_duration": 600000
        }"#;
        let resp: OllamaResponse = serde_json::from_str(json).unwrap();
        assert!(resp.done);
        assert_eq!(resp.done_reason.as_deref(), Some("stop"));
        assert_eq!(resp.prompt_eval_count, Some(26));
        assert_eq!(resp.eval_count, Some(15));
        assert_eq!(resp.total_duration, Some(1_234_567_890));
        assert_eq!(resp.load_duration, Some(100_000));
        assert_eq!(resp.prompt_eval_duration, Some(500_000));
        assert_eq!(resp.eval_duration, Some(600_000));
    }

    #[test]
    fn deserialize_ollama_response_with_tool_calls() {
        let json = r#"{
            "model": "llama3.2",
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "function": {
                        "name": "bash",
                        "arguments": {"command": "echo hello"}
                    }
                }]
            },
            "done": false
        }"#;
        let resp: OllamaResponse = serde_json::from_str(json).unwrap();
        let tool_calls = resp.message.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].function.name, "bash");
        assert_eq!(tool_calls[0].function.arguments["command"], "echo hello");
    }

    #[test]
    fn deserialize_tags_response() {
        let json = r#"{
            "models": [
                {
                    "name": "llama3.2:latest",
                    "modified_at": "2025-01-15T10:00:00Z",
                    "size": 1234567890,
                    "digest": "abc123",
                    "details": {"family": "llama"}
                },
                {
                    "name": "mistral:7b",
                    "modified_at": "2025-01-14T10:00:00Z",
                    "size": 987654321
                }
            ]
        }"#;
        let resp: TagsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.models.len(), 2);
        assert_eq!(resp.models[0].name, "llama3.2:latest");
        assert_eq!(resp.models[0].size, Some(1_234_567_890));
        assert_eq!(resp.models[0].digest.as_deref(), Some("abc123"));
        assert!(resp.models[0].details.is_some());
        assert_eq!(resp.models[1].name, "mistral:7b");
        assert!(resp.models[1].digest.is_none());
    }
}
