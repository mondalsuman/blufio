// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Wire types for the OpenResponses /v1/responses API.
//!
//! Separate from chat completions types — uses semantic event streaming
//! compatible with the OpenAI Agents SDK.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

/// POST /v1/responses request body.
#[derive(Debug, Deserialize)]
pub struct ResponsesRequest {
    /// Model identifier (supports provider/model format).
    pub model: String,

    /// Input — either a plain text string or an array of messages.
    pub input: ResponsesInput,

    /// System instructions for the model.
    #[serde(default)]
    pub instructions: Option<String>,

    /// Tool definitions.
    #[serde(default)]
    pub tools: Option<Vec<ResponsesTool>>,

    /// Previous response ID for multi-turn conversations.
    #[serde(default)]
    pub previous_response_id: Option<String>,

    /// Whether to stream the response (default: true).
    #[serde(default = "default_true")]
    pub stream: bool,

    /// Sampling temperature.
    #[serde(default)]
    pub temperature: Option<f32>,

    /// Maximum output tokens.
    #[serde(default)]
    pub max_output_tokens: Option<u32>,
}

fn default_true() -> bool {
    true
}

/// Input for a responses request.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ResponsesInput {
    /// Simple text input.
    Text(String),
    /// Array of messages.
    Messages(Vec<ResponsesInputMessage>),
}

/// A single input message.
#[derive(Debug, Deserialize)]
pub struct ResponsesInputMessage {
    /// Role: "user", "assistant", etc.
    pub role: String,
    /// Message content.
    pub content: String,
}

/// A tool definition in the responses API.
#[derive(Debug, Deserialize)]
pub struct ResponsesTool {
    /// Tool type ("function" or built-in like "web_search").
    #[serde(rename = "type")]
    pub tool_type: String,

    /// Function definition (for type="function").
    #[serde(default)]
    pub function: Option<ResponsesFunction>,

    /// Built-in tool name (for non-function tools).
    #[serde(default)]
    pub name: Option<String>,
}

/// Function definition for a responses tool.
#[derive(Debug, Deserialize)]
pub struct ResponsesFunction {
    /// Function name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// JSON Schema for parameters.
    pub parameters: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Event types (SSE — each event has event: and data: lines)
// ---------------------------------------------------------------------------

/// All possible response events, emitted as SSE.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ResponseEvent {
    /// Response object created.
    #[serde(rename = "response.created")]
    ResponseCreated { response: ResponseObject },

    /// A new output item started.
    #[serde(rename = "response.output_item.added")]
    OutputItemAdded { output_index: u32, item: OutputItem },

    /// A content part started.
    #[serde(rename = "response.content_part.added")]
    ContentPartAdded {
        output_index: u32,
        content_index: u32,
        part: ContentPart,
    },

    /// Text content delta.
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta {
        output_index: u32,
        content_index: u32,
        delta: String,
    },

    /// Text content complete.
    #[serde(rename = "response.output_text.done")]
    OutputTextDone {
        output_index: u32,
        content_index: u32,
        text: String,
    },

    /// Function call arguments delta.
    #[serde(rename = "response.function_call_arguments.delta")]
    FunctionCallArgumentsDelta {
        output_index: u32,
        call_id: String,
        delta: String,
    },

    /// Function call arguments complete.
    #[serde(rename = "response.function_call_arguments.done")]
    FunctionCallArgumentsDone {
        output_index: u32,
        call_id: String,
        name: String,
        arguments: String,
    },

    /// Content part complete.
    #[serde(rename = "response.content_part.done")]
    ContentPartDone {
        output_index: u32,
        content_index: u32,
        part: ContentPart,
    },

    /// Output item complete.
    #[serde(rename = "response.output_item.done")]
    OutputItemDone { output_index: u32, item: OutputItem },

    /// Response complete.
    #[serde(rename = "response.completed")]
    ResponseCompleted { response: ResponseObject },

    /// Response failed.
    #[serde(rename = "response.failed")]
    ResponseFailed {
        response: ResponseObject,
        error: ResponseError,
    },
}

// ---------------------------------------------------------------------------
// Supporting types
// ---------------------------------------------------------------------------

/// Response object metadata.
#[derive(Debug, Clone, Serialize)]
pub struct ResponseObject {
    /// Response identifier.
    pub id: String,
    /// Object type (always "response").
    pub object: String,
    /// Status: "in_progress", "completed", "failed".
    pub status: String,
    /// Model used.
    pub model: String,
    /// Unix timestamp.
    pub created_at: i64,
    /// Output items (populated in completed event).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Vec<OutputItem>>,
    /// Usage statistics (populated in completed event).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ResponsesUsage>,
}

/// An output item (message or function call).
#[derive(Debug, Clone, Serialize)]
pub struct OutputItem {
    /// Item type: "message" or "function_call".
    #[serde(rename = "type")]
    pub item_type: String,
    /// Role (for message items).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Content parts (for message items).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<ContentPart>>,
    /// Tool call ID (for function_call items).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    /// Function name (for function_call items).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Function arguments JSON string (for function_call items).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

/// A content part within an output item.
#[derive(Debug, Clone, Serialize)]
pub struct ContentPart {
    /// Part type (e.g., "output_text").
    #[serde(rename = "type")]
    pub part_type: String,
    /// Text content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// Usage statistics for a response.
#[derive(Debug, Clone, Serialize)]
pub struct ResponsesUsage {
    /// Input tokens consumed.
    pub input_tokens: u32,
    /// Output tokens generated.
    pub output_tokens: u32,
    /// Total tokens.
    pub total_tokens: u32,
}

/// Error information for failed responses.
#[derive(Debug, Clone, Serialize)]
pub struct ResponseError {
    /// Error message.
    pub message: String,
    /// Error type.
    #[serde(rename = "type")]
    pub error_type: String,
    /// Error code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_request_deserializes_with_text_input() {
        let json = r#"{
            "model": "gpt-4o",
            "input": "Hello, world!"
        }"#;
        let req: ResponsesRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "gpt-4o");
        assert!(req.stream); // default true
        match req.input {
            ResponsesInput::Text(t) => assert_eq!(t, "Hello, world!"),
            _ => panic!("expected Text input"),
        }
    }

    #[test]
    fn responses_request_deserializes_with_messages_input() {
        let json = r#"{
            "model": "gpt-4o",
            "input": [
                {"role": "user", "content": "Hello"},
                {"role": "assistant", "content": "Hi there!"}
            ],
            "instructions": "Be helpful"
        }"#;
        let req: ResponsesRequest = serde_json::from_str(json).unwrap();
        match req.input {
            ResponsesInput::Messages(msgs) => {
                assert_eq!(msgs.len(), 2);
                assert_eq!(msgs[0].role, "user");
            }
            _ => panic!("expected Messages input"),
        }
        assert_eq!(req.instructions.as_deref(), Some("Be helpful"));
    }

    #[test]
    fn response_created_event_serializes() {
        let event = ResponseEvent::ResponseCreated {
            response: ResponseObject {
                id: "resp_test".into(),
                object: "response".into(),
                status: "in_progress".into(),
                model: "gpt-4o".into(),
                created_at: 1700000000,
                output: None,
                usage: None,
            },
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "response.created");
        assert_eq!(json["response"]["id"], "resp_test");
        assert_eq!(json["response"]["status"], "in_progress");
    }

    #[test]
    fn output_text_delta_event_serializes() {
        let event = ResponseEvent::OutputTextDelta {
            output_index: 0,
            content_index: 0,
            delta: "Hello".into(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "response.output_text.delta");
        assert_eq!(json["delta"], "Hello");
    }

    #[test]
    fn function_call_done_event_serializes() {
        let event = ResponseEvent::FunctionCallArgumentsDone {
            output_index: 1,
            call_id: "call_abc".into(),
            name: "bash".into(),
            arguments: r#"{"command":"echo hello"}"#.into(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "response.function_call_arguments.done");
        assert_eq!(json["call_id"], "call_abc");
        assert_eq!(json["name"], "bash");
    }

    #[test]
    fn response_completed_event_serializes() {
        let event = ResponseEvent::ResponseCompleted {
            response: ResponseObject {
                id: "resp_test".into(),
                object: "response".into(),
                status: "completed".into(),
                model: "gpt-4o".into(),
                created_at: 1700000000,
                output: Some(vec![OutputItem {
                    item_type: "message".into(),
                    role: Some("assistant".into()),
                    content: Some(vec![ContentPart {
                        part_type: "output_text".into(),
                        text: Some("Hello!".into()),
                    }]),
                    call_id: None,
                    name: None,
                    arguments: None,
                }]),
                usage: Some(ResponsesUsage {
                    input_tokens: 10,
                    output_tokens: 5,
                    total_tokens: 15,
                }),
            },
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "response.completed");
        assert_eq!(json["response"]["status"], "completed");
        assert_eq!(json["response"]["usage"]["total_tokens"], 15);
    }

    #[test]
    fn response_failed_event_serializes() {
        let event = ResponseEvent::ResponseFailed {
            response: ResponseObject {
                id: "resp_test".into(),
                object: "response".into(),
                status: "failed".into(),
                model: "gpt-4o".into(),
                created_at: 1700000000,
                output: None,
                usage: None,
            },
            error: ResponseError {
                message: "Provider error".into(),
                error_type: "server_error".into(),
                code: Some("provider_error".into()),
            },
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "response.failed");
        assert_eq!(json["error"]["message"], "Provider error");
    }
}
