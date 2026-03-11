// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Entity extraction before compaction.
//!
//! Extracts named entities, key facts, decisions, and numerical data from
//! conversation messages. Returns extracted strings; the caller is responsible
//! for persisting them as Memory entries (avoids circular dependency with
//! blufio-memory which depends on blufio-context).
//!
//! Runs only before L1 compaction.

use blufio_core::error::BlufioError;
use blufio_core::traits::ProviderAdapter;
use blufio_core::types::{ContentBlock, Message, ProviderMessage, ProviderRequest, TokenUsage};

/// System prompt for entity extraction.
const EXTRACTION_PROMPT: &str = r#"You are a fact extractor. Extract all named entities, key facts, decisions, and numerical data from the conversation below.

Return a JSON array of strings, where each string is one standalone fact.

Example output:
["User's name is Alice", "Project deadline is March 15, 2026", "Decided to use PostgreSQL instead of MySQL", "Budget limit is $5000/month"]

Rules:
- Each fact should be a complete, standalone statement
- Include names, dates, amounts, decisions, preferences, and commitments
- Do NOT include opinions, greetings, or conversation filler
- Return ONLY the JSON array, no other text"#;

/// Result of entity extraction.
#[derive(Debug)]
pub struct ExtractionOutput {
    /// Extracted entity/fact strings.
    pub entities: Vec<String>,
    /// Token usage from the extraction LLM call.
    pub usage: TokenUsage,
}

/// Extracts entities and facts from messages before compaction.
///
/// Sends the conversation to the LLM with an extraction prompt and parses
/// the JSON array response. On parse failure, logs a warning and returns
/// an empty vec (non-blocking).
///
/// The caller is responsible for persisting extracted entities as Memory entries
/// with `MemorySource::Extracted` -- this avoids a circular dependency between
/// blufio-context and blufio-memory.
pub async fn extract_entities(
    provider: &dyn ProviderAdapter,
    messages: &[Message],
    model: &str,
) -> Result<ExtractionOutput, BlufioError> {
    if messages.is_empty() {
        return Ok(ExtractionOutput {
            entities: Vec::new(),
            usage: TokenUsage {
                input_tokens: 0,
                output_tokens: 0,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            },
        });
    }

    // Build conversation text.
    let conversation_text: String = messages
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n");

    let request = ProviderRequest {
        model: model.to_string(),
        system_prompt: Some(EXTRACTION_PROMPT.to_string()),
        system_blocks: None,
        messages: vec![ProviderMessage {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: format!("Extract facts from this conversation:\n\n{}", conversation_text),
            }],
        }],
        max_tokens: 1024,
        stream: false,
        tools: None,
    };

    let response = provider.complete(request).await?;
    let usage = response.usage;

    // Parse JSON array from response. On failure, return empty vec.
    let entities: Vec<String> = parse_json_array(&response.content);

    tracing::info!(
        entity_count = entities.len(),
        input_tokens = usage.input_tokens,
        output_tokens = usage.output_tokens,
        "entity extraction complete"
    );

    Ok(ExtractionOutput { entities, usage })
}

/// Attempts to parse a JSON array of strings from LLM response text.
///
/// Handles common cases where the model wraps the JSON in markdown or
/// explanatory text.
fn parse_json_array(text: &str) -> Vec<String> {
    // Try direct parse first.
    if let Ok(parsed) = serde_json::from_str::<Vec<String>>(text) {
        return parsed;
    }

    // Try to extract JSON array from surrounding text.
    if let Some(start) = text.find('[') {
        if let Some(end) = text.rfind(']') {
            if let Ok(parsed) = serde_json::from_str::<Vec<String>>(&text[start..=end]) {
                return parsed;
            }
        }
    }

    tracing::warn!("entity extraction JSON parse failed, returning empty");
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_prompt_contains_key_instructions() {
        assert!(EXTRACTION_PROMPT.contains("entities"));
        assert!(EXTRACTION_PROMPT.contains("decisions"));
        assert!(EXTRACTION_PROMPT.contains("numerical"));
        assert!(EXTRACTION_PROMPT.contains("JSON array"));
    }

    #[test]
    fn parse_valid_json_array() {
        let json = r#"["Alice is the project lead", "Budget is $5000"]"#;
        let parsed = parse_json_array(json);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0], "Alice is the project lead");
    }

    #[test]
    fn parse_json_with_surrounding_text() {
        let response = r#"Here are the extracted facts:
["Alice is the project lead", "Budget is $5000"]
That's all."#;
        let parsed = parse_json_array(response);
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn invalid_json_returns_empty() {
        let bad_json = "not a json array at all";
        let parsed = parse_json_array(bad_json);
        assert!(parsed.is_empty());
    }

    #[test]
    fn empty_array_returns_empty() {
        let parsed = parse_json_array("[]");
        assert!(parsed.is_empty());
    }
}
