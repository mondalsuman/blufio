// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Context assembly for LLM requests.
//!
//! Loads the system prompt from config and assembles conversation context
//! from session history plus the current inbound message.

use blufio_config::model::AgentConfig;
use blufio_core::error::BlufioError;
use blufio_core::types::{
    ContentBlock, InboundMessage, MessageContent, ProviderMessage, ProviderRequest,
};
use blufio_core::StorageAdapter;
use tracing::info;

/// Default number of recent messages to include in context.
const DEFAULT_HISTORY_LIMIT: i64 = 20;

/// Loads the system prompt following config priority: file > inline > default.
///
/// # Priority
/// 1. `config.system_prompt_file` -- reads from disk
/// 2. `config.system_prompt` -- inline string
/// 3. Default: "You are {name}, a concise personal assistant."
pub async fn load_system_prompt(config: &AgentConfig) -> Result<String, BlufioError> {
    // Priority 1: file path
    if let Some(ref file_path) = config.system_prompt_file {
        match tokio::fs::read_to_string(file_path).await {
            Ok(content) => {
                let trimmed: String = content.trim().to_string();
                if !trimmed.is_empty() {
                    info!(path = file_path.as_str(), "loaded system prompt from file");
                    return Ok(trimmed);
                }
            }
            Err(e) => {
                tracing::warn!(
                    path = file_path.as_str(),
                    error = %e,
                    "failed to read system prompt file, falling back"
                );
            }
        }
    }

    // Priority 2: inline string
    if let Some(ref prompt) = config.system_prompt {
        if !prompt.is_empty() {
            return Ok(prompt.clone());
        }
    }

    // Priority 3: default
    Ok(format!(
        "You are {}, a concise personal assistant.",
        config.name
    ))
}

/// Assembles a [`ProviderRequest`] from session history and the current inbound message.
///
/// Loads the last [`DEFAULT_HISTORY_LIMIT`] messages from storage, converts them
/// to provider messages, appends the current inbound message, and builds the request.
pub async fn assemble_context(
    storage: &dyn StorageAdapter,
    session_id: &str,
    system_prompt: &str,
    inbound: &InboundMessage,
    model: &str,
    max_tokens: u32,
) -> Result<ProviderRequest, BlufioError> {
    // Load recent messages from storage.
    let history = storage
        .get_messages(session_id, Some(DEFAULT_HISTORY_LIMIT))
        .await?;

    // Convert stored messages to ProviderMessage format.
    let mut messages: Vec<ProviderMessage> = history
        .iter()
        .map(|msg| ProviderMessage {
            role: msg.role.clone(),
            content: vec![ContentBlock::Text {
                text: msg.content.clone(),
            }],
        })
        .collect();

    // Append the current inbound message.
    let inbound_content = message_content_to_blocks(&inbound.content);
    messages.push(ProviderMessage {
        role: "user".to_string(),
        content: inbound_content,
    });

    Ok(ProviderRequest {
        model: model.to_string(),
        system_prompt: Some(system_prompt.to_string()),
        messages,
        max_tokens,
        stream: true,
    })
}

/// Converts a [`MessageContent`] into provider [`ContentBlock`]s.
fn message_content_to_blocks(content: &MessageContent) -> Vec<ContentBlock> {
    match content {
        MessageContent::Text(text) => vec![ContentBlock::Text {
            text: text.clone(),
        }],
        MessageContent::Image {
            data,
            mime_type,
            caption,
        } => {
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode(data);
            let mut blocks = vec![ContentBlock::Image {
                source_type: "base64".to_string(),
                media_type: mime_type.clone(),
                data: encoded,
            }];
            if let Some(cap) = caption {
                blocks.push(ContentBlock::Text {
                    text: cap.clone(),
                });
            }
            blocks
        }
        MessageContent::Document {
            data: _,
            filename,
            mime_type,
        } => {
            // For documents, create a text representation.
            // Binary document content cannot be directly sent to the LLM.
            let desc = if mime_type.starts_with("text/") {
                format!("[Document: {filename}]")
            } else {
                format!("[Document: {filename} ({mime_type}) - binary content attached]")
            };
            vec![ContentBlock::Text { text: desc }]
        }
        MessageContent::Voice { duration_secs, .. } => {
            let duration_str = duration_secs
                .map(|d| format!("{d:.0}s"))
                .unwrap_or_else(|| "unknown duration".to_string());
            vec![ContentBlock::Text {
                text: format!("[Voice message, {duration_str} - transcription pending]"),
            }]
        }
    }
}

/// Converts a [`MessageContent`] to its text representation for storage persistence.
pub fn message_content_to_text(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(text) => text.clone(),
        MessageContent::Image { caption, .. } => caption
            .clone()
            .unwrap_or_else(|| "[Image]".to_string()),
        MessageContent::Document { filename, .. } => format!("[Document: {filename}]"),
        MessageContent::Voice { duration_secs, .. } => {
            let d = duration_secs
                .map(|d| format!("{d:.0}s"))
                .unwrap_or_else(|| "?s".to_string());
            format!("[Voice message, {d}]")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blufio_config::model::AgentConfig;

    #[tokio::test]
    async fn load_system_prompt_default() {
        let config = AgentConfig {
            name: "testbot".to_string(),
            system_prompt: None,
            system_prompt_file: None,
            ..Default::default()
        };
        let prompt = load_system_prompt(&config).await.unwrap();
        assert_eq!(prompt, "You are testbot, a concise personal assistant.");
    }

    #[tokio::test]
    async fn load_system_prompt_inline() {
        let config = AgentConfig {
            name: "testbot".to_string(),
            system_prompt: Some("Custom inline prompt.".to_string()),
            system_prompt_file: None,
            ..Default::default()
        };
        let prompt = load_system_prompt(&config).await.unwrap();
        assert_eq!(prompt, "Custom inline prompt.");
    }

    #[tokio::test]
    async fn load_system_prompt_file_overrides_inline() {
        let dir = std::env::temp_dir().join("blufio-agent-test-prompt");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("test-prompt.md");
        std::fs::write(&file_path, "File-based prompt.").unwrap();

        let config = AgentConfig {
            name: "testbot".to_string(),
            system_prompt: Some("Inline prompt.".to_string()),
            system_prompt_file: Some(file_path.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let prompt = load_system_prompt(&config).await.unwrap();
        assert_eq!(prompt, "File-based prompt.");

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[tokio::test]
    async fn load_system_prompt_missing_file_falls_back() {
        let config = AgentConfig {
            name: "testbot".to_string(),
            system_prompt: Some("Fallback prompt.".to_string()),
            system_prompt_file: Some("/nonexistent/path/prompt.md".to_string()),
            ..Default::default()
        };
        let prompt = load_system_prompt(&config).await.unwrap();
        assert_eq!(prompt, "Fallback prompt.");
    }

    #[test]
    fn text_content_to_blocks() {
        let content = MessageContent::Text("hello".to_string());
        let blocks = message_content_to_blocks(&content);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Text { text } => assert_eq!(text, "hello"),
            _ => panic!("expected Text block"),
        }
    }

    #[test]
    fn voice_content_to_blocks() {
        let content = MessageContent::Voice {
            data: vec![],
            duration_secs: Some(5.0),
        };
        let blocks = message_content_to_blocks(&content);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Text { text } => {
                assert!(text.contains("Voice message"));
                assert!(text.contains("5s"));
            }
            _ => panic!("expected Text block"),
        }
    }

    #[test]
    fn document_content_to_blocks() {
        let content = MessageContent::Document {
            data: vec![1, 2, 3],
            filename: "test.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
        };
        let blocks = message_content_to_blocks(&content);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Text { text } => {
                assert!(text.contains("test.pdf"));
                assert!(text.contains("binary content"));
            }
            _ => panic!("expected Text block"),
        }
    }

    #[test]
    fn image_content_to_blocks_with_caption() {
        let content = MessageContent::Image {
            data: vec![0xFF, 0xD8],
            mime_type: "image/jpeg".to_string(),
            caption: Some("A photo".to_string()),
        };
        let blocks = message_content_to_blocks(&content);
        assert_eq!(blocks.len(), 2);
        assert!(matches!(&blocks[0], ContentBlock::Image { .. }));
        match &blocks[1] {
            ContentBlock::Text { text } => assert_eq!(text, "A photo"),
            _ => panic!("expected Text block for caption"),
        }
    }

    #[test]
    fn message_content_to_text_variants() {
        assert_eq!(
            message_content_to_text(&MessageContent::Text("hi".into())),
            "hi"
        );
        assert_eq!(
            message_content_to_text(&MessageContent::Image {
                data: vec![],
                mime_type: "image/png".into(),
                caption: Some("sunset".into()),
            }),
            "sunset"
        );
        assert_eq!(
            message_content_to_text(&MessageContent::Document {
                data: vec![],
                filename: "doc.txt".into(),
                mime_type: "text/plain".into(),
            }),
            "[Document: doc.txt]"
        );
        assert_eq!(
            message_content_to_text(&MessageContent::Voice {
                data: vec![],
                duration_secs: Some(3.0),
            }),
            "[Voice message, 3s]"
        );
    }
}
