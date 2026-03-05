// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Gemini API wire format types for the generateContent endpoint.
//!
//! Uses `#[serde(rename_all = "camelCase")]` to match Gemini's camelCase JSON convention.

use serde::{Deserialize, Serialize};

// --- Request types ---

/// A request to the Gemini generateContent endpoint.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentRequest {
    /// Conversation contents (user/model turns).
    pub contents: Vec<GeminiContent>,

    /// System instruction (separate from contents).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<GeminiSystemInstruction>,

    /// Tool declarations available to the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<GeminiTool>>,

    /// Generation parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<GenerationConfig>,
}

/// A content turn in a Gemini conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiContent {
    /// Role: "user" or "model".
    pub role: String,

    /// Content parts within this turn.
    pub parts: Vec<GeminiPart>,
}

/// A single part within a Gemini content turn.
///
/// Gemini uses untagged serialization -- each part variant is distinguished
/// by which fields are present in the JSON object.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GeminiPart {
    /// Text content.
    Text(TextPart),

    /// Function call from the model.
    FunctionCall(FunctionCallPart),

    /// Function response from the user.
    FunctionResponse(FunctionResponsePart),

    /// Inline binary data (images, etc.).
    InlineData(InlineDataPart),
}

/// A text content part.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextPart {
    /// The text content.
    pub text: String,
}

/// A function call part (model requests tool execution).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCallPart {
    /// The function call details.
    pub function_call: FunctionCall,
}

/// Function call details.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCall {
    /// Function name.
    pub name: String,

    /// Function arguments as a JSON object.
    pub args: serde_json::Value,
}

/// A function response part (user provides tool result).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionResponsePart {
    /// The function response details.
    pub function_response: FunctionResponse,
}

/// Function response details.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionResponse {
    /// Function name (matches the original call).
    pub name: String,

    /// Response data.
    pub response: serde_json::Value,
}

/// Inline binary data part (images, audio, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineDataPart {
    /// The inline data.
    pub inline_data: InlineData,
}

/// Inline binary data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineData {
    /// MIME type (e.g., "image/jpeg").
    pub mime_type: String,

    /// Base64-encoded data.
    pub data: String,
}

/// System instruction for Gemini (separate from conversation contents).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiSystemInstruction {
    /// Parts of the system instruction.
    pub parts: Vec<GeminiPart>,
}

/// A tool declaration for Gemini.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiTool {
    /// Function declarations available to the model.
    pub function_declarations: Vec<FunctionDeclaration>,
}

/// A function declaration within a Gemini tool.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionDeclaration {
    /// Function name.
    pub name: String,

    /// Human-readable description.
    pub description: String,

    /// JSON Schema describing the function parameters.
    pub parameters: serde_json::Value,
}

/// Generation configuration parameters.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    /// Maximum number of output tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,

    /// Temperature for generation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

// --- Response types ---

/// A response from the Gemini generateContent endpoint.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentResponse {
    /// Response candidates.
    #[serde(default)]
    pub candidates: Vec<Candidate>,

    /// Token usage metadata.
    #[serde(default)]
    pub usage_metadata: Option<UsageMetadata>,
}

/// A single candidate in a Gemini response.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    /// The generated content.
    pub content: GeminiContent,

    /// Reason the generation stopped.
    #[serde(default)]
    pub finish_reason: Option<String>,
}

/// Token usage metadata.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageMetadata {
    /// Number of prompt tokens.
    #[serde(default)]
    pub prompt_token_count: u32,

    /// Number of generated tokens.
    #[serde(default)]
    pub candidates_token_count: u32,

    /// Total tokens.
    #[serde(default)]
    pub total_token_count: u32,
}

// --- Error types ---

/// API error response from Gemini.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorResponse {
    /// Error details.
    pub error: ApiErrorDetail,
}

/// Error detail within an API error response.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorDetail {
    /// Error code (numeric).
    #[serde(default)]
    pub code: Option<u32>,

    /// Human-readable error message.
    #[serde(default)]
    pub message: String,

    /// Error status string.
    #[serde(default)]
    pub status: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_generate_content_request_basic() {
        let req = GenerateContentRequest {
            contents: vec![GeminiContent {
                role: "user".into(),
                parts: vec![GeminiPart::Text(TextPart {
                    text: "Hello".into(),
                })],
            }],
            system_instruction: None,
            tools: None,
            generation_config: Some(GenerationConfig {
                max_output_tokens: Some(4096),
                temperature: Some(1.0),
            }),
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["contents"][0]["role"], "user");
        assert_eq!(json["contents"][0]["parts"][0]["text"], "Hello");
        assert_eq!(json["generationConfig"]["maxOutputTokens"], 4096);
        assert_eq!(json["generationConfig"]["temperature"], 1.0);
        // system_instruction should be absent when None
        assert!(json.get("systemInstruction").is_none());
    }

    #[test]
    fn serialize_system_instruction() {
        let req = GenerateContentRequest {
            contents: vec![],
            system_instruction: Some(GeminiSystemInstruction {
                parts: vec![GeminiPart::Text(TextPart {
                    text: "You are helpful.".into(),
                })],
            }),
            tools: None,
            generation_config: None,
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(
            json["systemInstruction"]["parts"][0]["text"],
            "You are helpful."
        );
    }

    #[test]
    fn serialize_function_declarations() {
        let req = GenerateContentRequest {
            contents: vec![],
            system_instruction: None,
            tools: Some(vec![GeminiTool {
                function_declarations: vec![FunctionDeclaration {
                    name: "bash".into(),
                    description: "Execute a bash command".into(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "command": {"type": "string"}
                        },
                        "required": ["command"]
                    }),
                }],
            }]),
            generation_config: None,
        };

        let json = serde_json::to_value(&req).unwrap();
        let decls = &json["tools"][0]["functionDeclarations"];
        assert_eq!(decls[0]["name"], "bash");
        assert_eq!(decls[0]["description"], "Execute a bash command");
        assert_eq!(decls[0]["parameters"]["type"], "object");
        assert!(decls[0]["parameters"]["properties"]["command"].is_object());
    }

    #[test]
    fn deserialize_response_with_text() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"text": "Hello there!"}]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5,
                "totalTokenCount": 15
            }
        }"#;

        let resp: GenerateContentResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.candidates.len(), 1);
        assert_eq!(resp.candidates[0].content.role, "model");
        assert_eq!(resp.candidates[0].finish_reason.as_deref(), Some("STOP"));

        match &resp.candidates[0].content.parts[0] {
            GeminiPart::Text(tp) => assert_eq!(tp.text, "Hello there!"),
            other => panic!("expected Text part, got {other:?}"),
        }

        let usage = resp.usage_metadata.as_ref().unwrap();
        assert_eq!(usage.prompt_token_count, 10);
        assert_eq!(usage.candidates_token_count, 5);
        assert_eq!(usage.total_token_count, 15);
    }

    #[test]
    fn deserialize_response_with_function_call() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "functionCall": {
                            "name": "bash",
                            "args": {"command": "echo hello"}
                        }
                    }]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 20,
                "candidatesTokenCount": 10,
                "totalTokenCount": 30
            }
        }"#;

        let resp: GenerateContentResponse = serde_json::from_str(json).unwrap();
        match &resp.candidates[0].content.parts[0] {
            GeminiPart::FunctionCall(fc) => {
                assert_eq!(fc.function_call.name, "bash");
                assert_eq!(fc.function_call.args["command"], "echo hello");
            }
            other => panic!("expected FunctionCall part, got {other:?}"),
        }
    }

    #[test]
    fn serialize_function_response_content() {
        let content = GeminiContent {
            role: "user".into(),
            parts: vec![GeminiPart::FunctionResponse(FunctionResponsePart {
                function_response: FunctionResponse {
                    name: "bash".into(),
                    response: serde_json::json!({"result": "hello\n"}),
                },
            })],
        };

        let json = serde_json::to_value(&content).unwrap();
        assert_eq!(json["role"], "user");
        assert_eq!(json["parts"][0]["functionResponse"]["name"], "bash");
        assert_eq!(
            json["parts"][0]["functionResponse"]["response"]["result"],
            "hello\n"
        );
    }

    #[test]
    fn serialize_inline_data_part() {
        let part = GeminiPart::InlineData(InlineDataPart {
            inline_data: InlineData {
                mime_type: "image/jpeg".into(),
                data: "abc123".into(),
            },
        });

        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(json["inlineData"]["mimeType"], "image/jpeg");
        assert_eq!(json["inlineData"]["data"], "abc123");
    }

    #[test]
    fn usage_metadata_defaults() {
        let json = r#"{
            "candidates": [{
                "content": {"role": "model", "parts": [{"text": "hi"}]}
            }]
        }"#;
        let resp: GenerateContentResponse = serde_json::from_str(json).unwrap();
        // usageMetadata missing should default to None.
        assert!(resp.usage_metadata.is_none());
    }

    #[test]
    fn deserialize_api_error() {
        let json = r#"{
            "error": {
                "code": 429,
                "message": "Resource has been exhausted",
                "status": "RESOURCE_EXHAUSTED"
            }
        }"#;
        let err: ApiErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(err.error.code, Some(429));
        assert_eq!(err.error.message, "Resource has been exhausted");
        assert_eq!(err.error.status.as_deref(), Some("RESOURCE_EXHAUSTED"));
    }
}
