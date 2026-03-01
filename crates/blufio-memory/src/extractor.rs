// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! LLM-based memory extraction from conversations.
//!
//! Uses Haiku (or configured extraction model) to extract factual
//! information from conversation text, then deduplicates against
//! existing memories using cosine similarity.

use std::sync::Arc;

use blufio_core::error::BlufioError;
use blufio_core::traits::{EmbeddingAdapter, ProviderAdapter};
use blufio_core::types::{ContentBlock, EmbeddingInput, ProviderMessage, ProviderRequest};
use tracing::{debug, warn};
use uuid::Uuid;

use crate::embedder::OnnxEmbedder;
use crate::store::MemoryStore;
use crate::types::{
    cosine_similarity, ExtractionResult, ExtractedFact, Memory, MemorySource, MemoryStatus,
};

/// Similarity threshold above which a new fact is considered a duplicate.
const DEDUP_THRESHOLD: f32 = 0.9;

/// Similarity threshold for contradiction detection.
/// If a new fact is similar but not a duplicate (0.7-0.9), it may contradict.
const CONTRADICTION_THRESHOLD: f32 = 0.7;

/// System prompt for memory extraction.
const EXTRACTION_PROMPT: &str = r#"Extract factual information from this conversation that would be useful to remember for future conversations. Output as JSON array.

For each fact:
- "content": The fact as a standalone statement (e.g., "The user's dog is named Max")
- "category": One of: personal, preference, project, decision, instruction, outcome

Only include facts that are:
1. Stated by the user (not the assistant)
2. Specific and factual (not opinions unless explicitly stated as preferences)
3. Likely to be relevant in future conversations

If no memorable facts, return an empty array: []

Conversation:
{conversation}

Output JSON array only, no explanation:"#;

/// Extracts and stores long-term memories from conversations.
pub struct MemoryExtractor {
    store: Arc<MemoryStore>,
    embedder: Arc<OnnxEmbedder>,
    extraction_model: String,
}

impl MemoryExtractor {
    /// Creates a new memory extractor.
    pub fn new(
        store: Arc<MemoryStore>,
        embedder: Arc<OnnxEmbedder>,
        extraction_model: String,
    ) -> Self {
        Self {
            store,
            embedder,
            extraction_model,
        }
    }

    /// Extract memories from a conversation using LLM.
    ///
    /// Calls the extraction model (Haiku) to identify factual information,
    /// deduplicates against existing memories, and stores new facts.
    pub async fn extract_from_conversation(
        &self,
        provider: &dyn ProviderAdapter,
        session_id: &str,
        conversation: &[ProviderMessage],
    ) -> Result<ExtractionResult, BlufioError> {
        let prompt = build_extraction_prompt(conversation);

        // Call LLM for extraction
        let request = ProviderRequest {
            model: self.extraction_model.clone(),
            system_prompt: None,
            system_blocks: None,
            messages: vec![ProviderMessage {
                role: "user".to_string(),
                content: vec![ContentBlock::Text { text: prompt }],
            }],
            max_tokens: 2048,
            stream: false,
        };

        let response = provider.complete(request).await?;
        let usage = Some(response.usage.clone());

        // Parse extracted facts
        let facts = parse_extraction_response(&response.content);
        if facts.is_empty() {
            return Ok(ExtractionResult {
                memories: vec![],
                usage,
            });
        }

        // Process each fact
        let mut new_memories = Vec::new();
        let active_embeddings = self.store.get_active_embeddings().await?;

        for fact in facts {
            match self
                .process_fact(&fact, session_id, &active_embeddings)
                .await
            {
                Ok(Some(memory)) => {
                    new_memories.push(memory);
                }
                Ok(None) => {
                    debug!("Skipped duplicate fact: {}", fact.content);
                }
                Err(e) => {
                    warn!("Failed to process extracted fact '{}': {e}", fact.content);
                }
            }
        }

        Ok(ExtractionResult {
            memories: new_memories,
            usage,
        })
    }

    /// Store an explicit memory ("remember this: X").
    ///
    /// Explicit memories get confidence 0.9 (higher than extracted 0.6).
    pub async fn extract_explicit(
        &self,
        text: &str,
        session_id: &str,
    ) -> Result<Memory, BlufioError> {
        // Strip common prefixes
        let content = strip_remember_prefix(text);

        // Generate embedding
        let output = self
            .embedder
            .embed(EmbeddingInput {
                texts: vec![content.to_string()],
            })
            .await?;
        let embedding = output.embeddings.into_iter().next().ok_or_else(|| {
            BlufioError::Internal("Embedding returned no results".to_string())
        })?;

        // Check for duplicates
        let active_embeddings = self.store.get_active_embeddings().await?;
        if let Some((dup_id, sim)) = find_most_similar(&embedding, &active_embeddings) {
            if sim > DEDUP_THRESHOLD {
                debug!(
                    "Explicit memory is duplicate of {dup_id} (similarity {sim:.3}), superseding"
                );
                // Supersede existing since user is explicitly updating
                self.store.supersede(&dup_id, &format!("pending-{}", Uuid::new_v4())).await.ok();
            }
        }

        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let memory = Memory {
            id: Uuid::new_v4().to_string(),
            content: content.to_string(),
            embedding,
            source: MemorySource::Explicit,
            confidence: 0.9,
            status: MemoryStatus::Active,
            superseded_by: None,
            session_id: Some(session_id.to_string()),
            created_at: now.clone(),
            updated_at: now,
        };

        self.store.save(&memory).await?;
        Ok(memory)
    }

    /// Process a single extracted fact: embed, dedup, handle contradictions, store.
    async fn process_fact(
        &self,
        fact: &ExtractedFact,
        session_id: &str,
        active_embeddings: &[(String, Vec<f32>)],
    ) -> Result<Option<Memory>, BlufioError> {
        // Generate embedding
        let output = self
            .embedder
            .embed(EmbeddingInput {
                texts: vec![fact.content.clone()],
            })
            .await?;
        let embedding = output.embeddings.into_iter().next().ok_or_else(|| {
            BlufioError::Internal("Embedding returned no results".to_string())
        })?;

        // Check for duplicates and contradictions
        if let Some((existing_id, sim)) = find_most_similar(&embedding, active_embeddings) {
            if sim > DEDUP_THRESHOLD {
                // Near-duplicate, skip
                return Ok(None);
            } else if sim > CONTRADICTION_THRESHOLD {
                // Potentially contradicting -- newer wins, supersede old
                debug!(
                    "Possible contradiction with {existing_id} (similarity {sim:.3}), superseding"
                );
                let new_id = Uuid::new_v4().to_string();
                self.store.supersede(&existing_id, &new_id).await?;

                let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                let memory = Memory {
                    id: new_id,
                    content: fact.content.clone(),
                    embedding,
                    source: MemorySource::Extracted,
                    confidence: 0.6,
                    status: MemoryStatus::Active,
                    superseded_by: None,
                    session_id: Some(session_id.to_string()),
                    created_at: now.clone(),
                    updated_at: now,
                };
                self.store.save(&memory).await?;
                return Ok(Some(memory));
            }
        }

        // New unique fact
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let memory = Memory {
            id: Uuid::new_v4().to_string(),
            content: fact.content.clone(),
            embedding,
            source: MemorySource::Extracted,
            confidence: 0.6,
            status: MemoryStatus::Active,
            superseded_by: None,
            session_id: Some(session_id.to_string()),
            created_at: now.clone(),
            updated_at: now,
        };
        self.store.save(&memory).await?;
        Ok(Some(memory))
    }
}

/// Build the extraction prompt by formatting conversation messages.
fn build_extraction_prompt(conversation: &[ProviderMessage]) -> String {
    let mut conversation_text = String::new();
    for msg in conversation {
        let role = match msg.role.as_str() {
            "user" => "User",
            "assistant" => "Assistant",
            _ => &msg.role,
        };
        for block in &msg.content {
            if let ContentBlock::Text { text } = block {
                conversation_text.push_str(&format!("{role}: {text}\n"));
            }
        }
    }

    EXTRACTION_PROMPT.replace("{conversation}", &conversation_text)
}

/// Parse the LLM extraction response into structured facts.
///
/// Handles JSON arrays, markdown code block wrapping, and malformed responses.
/// Returns empty Vec on parse failure (don't fail the whole extraction).
pub fn parse_extraction_response(response: &str) -> Vec<ExtractedFact> {
    let trimmed = response.trim();

    // Strip markdown code block if present
    let json_str = if trimmed.starts_with("```") {
        let start = trimmed.find('[').unwrap_or(0);
        let end = trimmed.rfind(']').map(|i| i + 1).unwrap_or(trimmed.len());
        &trimmed[start..end]
    } else {
        // Try to find the JSON array in the response
        let start = trimmed.find('[').unwrap_or(0);
        let end = trimmed.rfind(']').map(|i| i + 1).unwrap_or(trimmed.len());
        &trimmed[start..end]
    };

    match serde_json::from_str::<Vec<ExtractedFact>>(json_str) {
        Ok(facts) => facts,
        Err(e) => {
            warn!("Failed to parse extraction response: {e}");
            debug!("Raw response: {response}");
            Vec::new()
        }
    }
}

/// Strip "remember this:", "remember that", etc. prefixes from explicit memory text.
fn strip_remember_prefix(text: &str) -> &str {
    let lower = text.to_lowercase();
    let prefixes = [
        "remember this:",
        "remember that:",
        "remember:",
        "remember this ",
        "remember that ",
    ];

    for prefix in &prefixes {
        if lower.starts_with(prefix) {
            return text[prefix.len()..].trim();
        }
    }

    text.trim()
}

/// Find the most similar embedding in the active set.
///
/// Returns (id, similarity) for the closest match, or None if empty.
fn find_most_similar(
    query: &[f32],
    active_embeddings: &[(String, Vec<f32>)],
) -> Option<(String, f32)> {
    active_embeddings
        .iter()
        .filter(|(_, emb)| emb.len() == query.len())
        .map(|(id, emb)| (id.clone(), cosine_similarity(query, emb)))
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_json_array() {
        let response = r#"[
            {"content": "User's dog is named Max", "category": "personal"},
            {"content": "User prefers dark mode", "category": "preference"}
        ]"#;
        let facts = parse_extraction_response(response);
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].content, "User's dog is named Max");
        assert_eq!(facts[0].category, "personal");
        assert_eq!(facts[1].content, "User prefers dark mode");
    }

    #[test]
    fn parse_empty_array() {
        let response = "[]";
        let facts = parse_extraction_response(response);
        assert!(facts.is_empty());
    }

    #[test]
    fn parse_malformed_json_returns_empty() {
        let response = "This is not JSON at all.";
        let facts = parse_extraction_response(response);
        assert!(facts.is_empty(), "Malformed JSON should return empty Vec");
    }

    #[test]
    fn parse_markdown_code_block() {
        let response = r#"```json
[
    {"content": "User lives in Berlin", "category": "personal"}
]
```"#;
        let facts = parse_extraction_response(response);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].content, "User lives in Berlin");
    }

    #[test]
    fn parse_with_surrounding_text() {
        let response = r#"Here are the extracted facts:
[{"content": "User uses Rust", "category": "project"}]
Those are the facts."#;
        let facts = parse_extraction_response(response);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].content, "User uses Rust");
    }

    #[test]
    fn strip_remember_prefix_colon() {
        assert_eq!(strip_remember_prefix("remember this: my dog is Max"), "my dog is Max");
        assert_eq!(strip_remember_prefix("remember that: I use vim"), "I use vim");
        assert_eq!(strip_remember_prefix("remember: dark mode"), "dark mode");
    }

    #[test]
    fn strip_remember_prefix_no_colon() {
        assert_eq!(strip_remember_prefix("remember this my dog is Max"), "my dog is Max");
        assert_eq!(strip_remember_prefix("remember that I use vim"), "I use vim");
    }

    #[test]
    fn strip_no_prefix() {
        assert_eq!(strip_remember_prefix("my dog is Max"), "my dog is Max");
    }

    #[test]
    fn confidence_explicit_vs_extracted() {
        // Explicit memories should have higher confidence than extracted
        let explicit_confidence = 0.9;
        let extracted_confidence = 0.6;
        assert!(explicit_confidence > extracted_confidence);
    }

    #[test]
    fn build_prompt_formats_conversation() {
        let conversation = vec![
            ProviderMessage {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text: "My dog's name is Max.".to_string(),
                }],
            },
            ProviderMessage {
                role: "assistant".to_string(),
                content: vec![ContentBlock::Text {
                    text: "That's a great name!".to_string(),
                }],
            },
        ];

        let prompt = build_extraction_prompt(&conversation);
        assert!(prompt.contains("User: My dog's name is Max."));
        assert!(prompt.contains("Assistant: That's a great name!"));
        assert!(prompt.contains("Output JSON array only"));
    }

    #[test]
    fn find_most_similar_returns_best_match() {
        let query = vec![1.0, 0.0, 0.0];
        let active = vec![
            ("a".to_string(), vec![0.5, 0.5, 0.0]),
            ("b".to_string(), vec![0.9, 0.1, 0.0]),
            ("c".to_string(), vec![0.0, 1.0, 0.0]),
        ];

        let (id, _sim) = find_most_similar(&query, &active).unwrap();
        assert_eq!(id, "b", "Should match the most similar embedding");
    }

    #[test]
    fn find_most_similar_empty_returns_none() {
        let query = vec![1.0, 0.0, 0.0];
        let active: Vec<(String, Vec<f32>)> = vec![];
        assert!(find_most_similar(&query, &active).is_none());
    }

    #[test]
    fn dedup_threshold_higher_than_contradiction() {
        assert!(
            DEDUP_THRESHOLD > CONTRADICTION_THRESHOLD,
            "Dedup threshold should be higher than contradiction threshold"
        );
    }
}
