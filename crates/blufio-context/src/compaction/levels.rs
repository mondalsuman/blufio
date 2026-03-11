// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Multi-level compaction engine: L0 raw -> L1 turn-pair summaries -> L2 session summary.
//!
//! L3 (cross-session archive) is handled in Plan 03 at session close.

use blufio_core::error::BlufioError;
use blufio_core::traits::ProviderAdapter;
use blufio_core::types::{ContentBlock, Message, ProviderMessage, ProviderRequest, TokenUsage};
use serde::{Deserialize, Serialize};

use super::COMPACTION_PROMPT;

/// Compaction level in the progressive summarization hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactionLevel {
    /// Raw messages (no compaction applied).
    L0,
    /// Turn-pair bullet summaries (soft trigger).
    L1,
    /// Session narrative summary (hard trigger / cascade).
    L2,
    /// Cross-session archive (session close).
    L3,
}

impl CompactionLevel {
    /// Returns the string representation of the compaction level.
    pub fn as_str(&self) -> &'static str {
        match self {
            CompactionLevel::L0 => "L0",
            CompactionLevel::L1 => "L1",
            CompactionLevel::L2 => "L2",
            CompactionLevel::L3 => "L3",
        }
    }
}

/// Result of a compaction pass at any level.
#[derive(Debug, Clone)]
pub struct CompactionResult {
    /// The generated summary text.
    pub summary: String,
    /// Which compaction level produced this result.
    pub level: CompactionLevel,
    /// Quality score from the scoring system (None if scoring disabled).
    pub quality_score: Option<f64>,
    /// Number of tokens saved by this compaction pass.
    pub tokens_saved: usize,
    /// Number of original messages (or L1 summaries) that were compacted.
    pub original_count: usize,
    /// Structured metadata for the compaction summary message.
    pub metadata: serde_json::Value,
    /// Token usage from the compaction LLM call.
    pub usage: TokenUsage,
}

/// System prompt for L1 compaction: turn-pair bullet summaries.
const L1_COMPACTION_PROMPT: &str = r#"You are a conversation compressor. Your job is to summarize conversation turn pairs into concise bullet points.

For each user-assistant turn pair, produce ONE bullet point (1-2 sentences) that captures:
- Named entities (people, places, tools, projects)
- Key decisions and their rationale
- Action items and commitments
- Numerical data (dates, amounts, counts, measurements)
- User preferences or instructions

Format: Return a bulleted list where each line starts with "- ".
Do NOT include greetings, small talk, or filler.
Do NOT number the bullets.
Be precise and factual."#;

/// Compacts raw messages (L0) into turn-pair bullet summaries (L1).
///
/// Groups messages into user-assistant turn pairs and sends ALL pairs to
/// the LLM in a single call. Returns bullet-point summaries preserving
/// entities, decisions, actions, and numerical data.
pub async fn compact_to_l1(
    provider: &dyn ProviderAdapter,
    messages: &[Message],
    model: &str,
    max_tokens: u32,
) -> Result<CompactionResult, BlufioError> {
    // Build turn-pair text from messages.
    let mut turn_pairs = Vec::new();
    let mut i = 0;
    while i < messages.len() {
        if messages[i].role == "user" && i + 1 < messages.len() && messages[i + 1].role == "assistant" {
            turn_pairs.push(format!(
                "Turn {}:\nUser: {}\nAssistant: {}",
                turn_pairs.len() + 1,
                messages[i].content,
                messages[i + 1].content,
            ));
            i += 2;
        } else {
            // Non-pair message (system, consecutive same-role): include as standalone.
            turn_pairs.push(format!(
                "Turn {}:\n{}: {}",
                turn_pairs.len() + 1,
                messages[i].role,
                messages[i].content,
            ));
            i += 1;
        }
    }

    let conversation_text = turn_pairs.join("\n\n");

    // Cap max_tokens: multiply per-pair budget by pair count, cap at 2048.
    let effective_max_tokens = (max_tokens as usize * turn_pairs.len().max(1)).min(2048) as u32;

    let request = ProviderRequest {
        model: model.to_string(),
        system_prompt: Some(L1_COMPACTION_PROMPT.to_string()),
        system_blocks: None,
        messages: vec![ProviderMessage {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: format!(
                    "Summarize each turn pair as a bullet point:\n\n{}",
                    conversation_text
                ),
            }],
        }],
        max_tokens: effective_max_tokens,
        stream: false,
        tools: None,
    };

    let response = provider.complete(request).await?;

    let now = chrono::Utc::now().to_rfc3339();
    let metadata = build_compaction_metadata(
        &CompactionLevel::L1,
        messages.len(),
        None, // quality_score filled later by scoring system
        &now,
    );

    tracing::info!(
        input_tokens = response.usage.input_tokens,
        output_tokens = response.usage.output_tokens,
        model = model,
        original_messages = messages.len(),
        turn_pairs = turn_pairs.len(),
        "L1 compaction complete"
    );

    Ok(CompactionResult {
        summary: response.content,
        level: CompactionLevel::L1,
        quality_score: None,
        tokens_saved: 0, // Caller computes actual savings from token counts.
        original_count: messages.len(),
        metadata,
        usage: response.usage,
    })
}

/// Compacts L1 bullet summaries into an L2 session narrative summary.
///
/// Takes the L1 bullet-point text and produces a 2-4 paragraph narrative
/// using the existing COMPACTION_PROMPT (third-person narrative format).
pub async fn compact_to_l2(
    provider: &dyn ProviderAdapter,
    l1_summaries: &str,
    model: &str,
    max_tokens: u32,
) -> Result<CompactionResult, BlufioError> {
    let request = ProviderRequest {
        model: model.to_string(),
        system_prompt: Some(COMPACTION_PROMPT.to_string()),
        system_blocks: None,
        messages: vec![ProviderMessage {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: format!(
                    "Summarize this conversation (provided as bullet-point summaries):\n\n{}",
                    l1_summaries
                ),
            }],
        }],
        max_tokens,
        stream: false,
        tools: None,
    };

    let response = provider.complete(request).await?;

    let now = chrono::Utc::now().to_rfc3339();
    let metadata = build_compaction_metadata(&CompactionLevel::L2, 0, None, &now);

    tracing::info!(
        input_tokens = response.usage.input_tokens,
        output_tokens = response.usage.output_tokens,
        model = model,
        "L2 compaction complete"
    );

    Ok(CompactionResult {
        summary: response.content,
        level: CompactionLevel::L2,
        quality_score: None,
        tokens_saved: 0,
        original_count: 0, // L2 compacts L1 summaries, not raw messages.
        metadata,
        usage: response.usage,
    })
}

/// Builds structured compaction metadata per CONTEXT.md spec.
pub fn build_compaction_metadata(
    level: &CompactionLevel,
    original_count: usize,
    quality_score: Option<f64>,
    compacted_at: &str,
) -> serde_json::Value {
    serde_json::json!({
        "type": "compaction_summary",
        "compaction_level": level.as_str(),
        "original_count": original_count,
        "quality_score": quality_score,
        "compacted_at": compacted_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compaction_level_as_str() {
        assert_eq!(CompactionLevel::L0.as_str(), "L0");
        assert_eq!(CompactionLevel::L1.as_str(), "L1");
        assert_eq!(CompactionLevel::L2.as_str(), "L2");
        assert_eq!(CompactionLevel::L3.as_str(), "L3");
    }

    #[test]
    fn compaction_level_serialize_roundtrip() {
        let level = CompactionLevel::L2;
        let json = serde_json::to_string(&level).unwrap();
        let deserialized: CompactionLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, level);
    }

    #[test]
    fn build_metadata_format() {
        let meta = build_compaction_metadata(
            &CompactionLevel::L1,
            42,
            Some(0.82),
            "2026-01-01T00:00:00Z",
        );
        assert_eq!(meta["type"], "compaction_summary");
        assert_eq!(meta["compaction_level"], "L1");
        assert_eq!(meta["original_count"], 42);
        assert_eq!(meta["quality_score"], 0.82);
        assert_eq!(meta["compacted_at"], "2026-01-01T00:00:00Z");
    }

    #[test]
    fn build_metadata_without_quality_score() {
        let meta = build_compaction_metadata(
            &CompactionLevel::L2,
            10,
            None,
            "2026-03-11T00:00:00Z",
        );
        assert_eq!(meta["type"], "compaction_summary");
        assert_eq!(meta["compaction_level"], "L2");
        assert!(meta["quality_score"].is_null());
    }

    #[test]
    fn l1_prompt_contains_key_preservation_instructions() {
        assert!(L1_COMPACTION_PROMPT.contains("entities"));
        assert!(L1_COMPACTION_PROMPT.contains("decisions"));
        assert!(L1_COMPACTION_PROMPT.contains("Action items"));
        assert!(L1_COMPACTION_PROMPT.contains("Numerical data"));
        assert!(L1_COMPACTION_PROMPT.contains("bullet"));
    }

    #[test]
    fn l1_prompt_preserves_bullet_format() {
        // L1 output should be bullets for L2 re-compaction.
        assert!(L1_COMPACTION_PROMPT.contains("- "));
    }
}
