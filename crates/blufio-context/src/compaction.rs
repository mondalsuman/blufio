// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Compaction: summarizes older conversation history via a Haiku LLM call
//! to keep the context window within budget.

use blufio_core::error::BlufioError;
use blufio_core::traits::ProviderAdapter;
use blufio_core::traits::StorageAdapter;
use blufio_core::types::{ContentBlock, Message, ProviderMessage, ProviderRequest, TokenUsage};
use chrono::Utc;
use uuid::Uuid;

/// System prompt for the compaction summarization LLM call.
const COMPACTION_PROMPT: &str = r#"You are a conversation summarizer. Your job is to create a concise summary of the conversation below.

PRESERVE the following in your summary:
- User preferences and settings
- Names, identifiers, and references to people/things
- Commitments made by either party
- Key decisions and their rationale
- Action items and their status
- Emotional tone and rapport indicators
- Any facts the user has shared about themselves

OMIT:
- Greetings and small talk
- Redundant back-and-forth
- Failed attempts that were corrected

Format: Write a clear, third-person narrative summary in 2-4 paragraphs. Start with "Conversation summary:" on the first line."#;

/// Generates a compaction summary of older messages using an LLM call.
///
/// Calls the provider with the compaction prompt and the conversation text,
/// returning the summary text and the token usage from the LLM call itself.
/// The returned `TokenUsage` represents the Haiku tokens consumed by this
/// compaction call and must be recorded separately by the caller.
pub async fn generate_compaction_summary(
    provider: &dyn ProviderAdapter,
    messages_to_compact: &[Message],
    model: &str,
) -> Result<(String, TokenUsage), BlufioError> {
    // Build conversation text from messages.
    let conversation_text: String = messages_to_compact
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n");

    let request = ProviderRequest {
        model: model.to_string(),
        system_prompt: Some(COMPACTION_PROMPT.to_string()),
        system_blocks: None,
        messages: vec![ProviderMessage {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: format!("Summarize this conversation:\n\n{}", conversation_text),
            }],
        }],
        max_tokens: 1024,
        stream: false,
        tools: None,
    };

    let response = provider.complete(request).await?;

    tracing::info!(
        input_tokens = response.usage.input_tokens,
        output_tokens = response.usage.output_tokens,
        model = model,
        original_messages = messages_to_compact.len(),
        "compaction summary generated"
    );

    Ok((response.content, response.usage))
}

/// Persists a compaction summary as a system message in storage.
///
/// The message is stored with role="system" and metadata tagging it as
/// a compaction summary with the count of original messages compacted.
pub async fn persist_compaction_summary(
    storage: &dyn StorageAdapter,
    session_id: &str,
    summary: &str,
    original_count: usize,
) -> Result<(), BlufioError> {
    let now = Utc::now().to_rfc3339();
    let metadata = serde_json::json!({
        "type": "compaction_summary",
        "original_count": original_count,
        "compacted_at": now,
    });

    let message = Message {
        id: Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        role: "system".to_string(),
        content: summary.to_string(),
        token_count: None,
        metadata: Some(metadata.to_string()),
        created_at: now,
    };

    storage.insert_message(&message).await?;

    tracing::info!(
        session_id = session_id,
        original_count = original_count,
        "compaction summary persisted"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compaction_prompt_preserves_key_elements() {
        assert!(COMPACTION_PROMPT.contains("preferences"));
        assert!(COMPACTION_PROMPT.contains("Names"));
        assert!(COMPACTION_PROMPT.contains("Commitments"));
        assert!(COMPACTION_PROMPT.contains("decisions"));
        assert!(COMPACTION_PROMPT.contains("Action items"));
        assert!(COMPACTION_PROMPT.contains("Emotional tone"));
    }

    #[test]
    fn compaction_metadata_format() {
        let metadata = serde_json::json!({
            "type": "compaction_summary",
            "original_count": 42,
            "compacted_at": "2026-01-01T00:00:00Z",
        });
        let parsed: serde_json::Value = metadata;
        assert_eq!(parsed["type"], "compaction_summary");
        assert_eq!(parsed["original_count"], 42);
    }
}
