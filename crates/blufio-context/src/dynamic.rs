// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Dynamic zone: assembles conversation history with sliding window
//! and triggers compaction when the token estimate exceeds the threshold.

use blufio_config::model::ContextConfig;
use blufio_core::error::BlufioError;
use blufio_core::traits::{ProviderAdapter, StorageAdapter};
use blufio_core::types::{
    ContentBlock, InboundMessage, MessageContent, ProviderMessage, TokenUsage,
};
use tracing::{debug, info};

use crate::compaction::{generate_compaction_summary, persist_compaction_summary};

/// Result of dynamic zone assembly, carrying messages and optional compaction cost.
#[derive(Debug)]
pub struct DynamicResult {
    /// Assembled messages for the provider request.
    pub messages: Vec<ProviderMessage>,
    /// Token usage from compaction LLM call, if compaction was triggered.
    /// Callers MUST propagate this for cost recording with FeatureType::Compaction.
    pub compaction_usage: Option<TokenUsage>,
}

/// The dynamic zone manages conversation history assembly with
/// automatic compaction when the context window fills up.
#[derive(Debug, Clone)]
pub struct DynamicZone {
    /// Compaction threshold as fraction of context budget (0.0-1.0).
    compaction_threshold: f64,
    /// Context window budget in tokens.
    context_budget: u32,
    /// Model to use for compaction summarization.
    compaction_model: String,
}

impl DynamicZone {
    /// Creates a new dynamic zone from context configuration.
    pub fn new(config: &ContextConfig) -> Self {
        Self {
            compaction_threshold: config.compaction_threshold,
            context_budget: config.context_budget,
            compaction_model: config.compaction_model.clone(),
        }
    }

    /// Assembles conversation messages from storage, triggering compaction if needed.
    ///
    /// Returns a `DynamicResult` containing the assembled messages and optional
    /// compaction token usage that the caller must propagate for cost tracking.
    pub async fn assemble_messages(
        &self,
        provider: &dyn ProviderAdapter,
        storage: &dyn StorageAdapter,
        session_id: &str,
        inbound: &InboundMessage,
    ) -> Result<DynamicResult, BlufioError> {
        // Load ALL messages from storage for this session.
        let history = storage.get_messages(session_id, None).await?;

        // Estimate total token count (rough heuristic: 4 chars per token).
        let estimated_tokens: usize = history.iter().map(|m| m.content.len() / 4).sum();
        let threshold = (self.context_budget as f64 * self.compaction_threshold) as usize;

        debug!(
            estimated_tokens = estimated_tokens,
            threshold = threshold,
            history_len = history.len(),
            "dynamic zone token estimate"
        );

        let (mut messages, compaction_usage) = if estimated_tokens > threshold && history.len() > 2
        {
            // Trigger compaction: split into older half and recent half.
            let split_point = history.len() / 2;
            let older = &history[..split_point];
            let recent = &history[split_point..];

            info!(
                older_count = older.len(),
                recent_count = recent.len(),
                estimated_tokens = estimated_tokens,
                threshold = threshold,
                "triggering compaction"
            );

            // Generate compaction summary via Haiku LLM call.
            let (summary, usage) =
                generate_compaction_summary(provider, older, &self.compaction_model).await?;

            // Persist the compaction summary in storage.
            persist_compaction_summary(storage, session_id, &summary, older.len()).await?;

            // Build messages: compaction summary + recent messages.
            let mut msgs = vec![ProviderMessage {
                role: "system".to_string(),
                content: vec![ContentBlock::Text { text: summary }],
            }];

            // Add recent messages.
            for msg in recent {
                msgs.push(ProviderMessage {
                    role: msg.role.clone(),
                    content: vec![ContentBlock::Text {
                        text: msg.content.clone(),
                    }],
                });
            }

            (msgs, Some(usage))
        } else {
            // Normal path: convert all history messages to ProviderMessage format.
            let msgs: Vec<ProviderMessage> = history
                .iter()
                .map(|msg| ProviderMessage {
                    role: msg.role.clone(),
                    content: vec![ContentBlock::Text {
                        text: msg.content.clone(),
                    }],
                })
                .collect();

            (msgs, None)
        };

        // Append the current inbound message.
        let inbound_content = message_content_to_blocks(&inbound.content);
        messages.push(ProviderMessage {
            role: "user".to_string(),
            content: inbound_content,
        });

        Ok(DynamicResult {
            messages,
            compaction_usage,
        })
    }
}

/// Converts a [`MessageContent`] into provider [`ContentBlock`]s.
///
/// Duplicated from blufio-agent/context.rs to avoid circular dependency
/// (blufio-context should NOT depend on blufio-agent).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_zone_new_from_config() {
        let config = ContextConfig::default();
        let zone = DynamicZone::new(&config);
        assert_eq!(zone.compaction_threshold, 0.70);
        assert_eq!(zone.context_budget, 180_000);
        assert_eq!(zone.compaction_model, "claude-haiku-4-5-20250901");
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
    fn dynamic_result_without_compaction() {
        let result = DynamicResult {
            messages: vec![ProviderMessage {
                role: "user".into(),
                content: vec![ContentBlock::Text {
                    text: "Hello".into(),
                }],
            }],
            compaction_usage: None,
        };
        assert!(result.compaction_usage.is_none());
        assert_eq!(result.messages.len(), 1);
    }

    #[test]
    fn dynamic_result_with_compaction() {
        let result = DynamicResult {
            messages: vec![],
            compaction_usage: Some(TokenUsage {
                input_tokens: 500,
                output_tokens: 100,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            }),
        };
        assert!(result.compaction_usage.is_some());
        assert_eq!(result.compaction_usage.unwrap().input_tokens, 500);
    }
}
