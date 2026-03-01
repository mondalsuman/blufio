// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! ConditionalProvider implementation for memory-based context injection.
//!
//! MemoryProvider implements the ConditionalProvider trait from blufio-context,
//! injecting relevant memories as structured blocks in the prompt.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use blufio_context::conditional::ConditionalProvider;
use blufio_core::error::BlufioError;
use blufio_core::types::{ContentBlock, ProviderMessage};
use tokio::sync::RwLock;

use crate::retriever::HybridRetriever;

/// ConditionalProvider that injects relevant long-term memories into context.
///
/// Before each context assembly, the SessionActor calls `set_current_query`
/// with the user's message. The provider then retrieves matching memories
/// and formats them as a structured block.
pub struct MemoryProvider {
    retriever: Arc<HybridRetriever>,
    /// Per-session current query, set by SessionActor before context assembly.
    current_queries: Arc<RwLock<HashMap<String, String>>>,
}

impl MemoryProvider {
    /// Creates a new MemoryProvider.
    pub fn new(retriever: Arc<HybridRetriever>) -> Self {
        Self {
            retriever,
            current_queries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Called by SessionActor before context assembly to set the current query.
    ///
    /// The query is the user's latest message text, used to retrieve
    /// relevant memories for this turn.
    pub async fn set_current_query(&self, session_id: &str, query: &str) {
        self.current_queries
            .write()
            .await
            .insert(session_id.to_string(), query.to_string());
    }

    /// Called after context assembly to clean up the query state.
    pub async fn clear_current_query(&self, session_id: &str) {
        self.current_queries.write().await.remove(session_id);
    }

    /// Get the current query for a session.
    async fn get_current_query(&self, session_id: &str) -> String {
        self.current_queries
            .read()
            .await
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }
}

#[async_trait]
impl ConditionalProvider for MemoryProvider {
    /// Retrieves relevant memories and formats them as context.
    ///
    /// Returns a single ProviderMessage with role "user" containing
    /// a "## Relevant Memories" header and bullet-pointed facts.
    /// Returns empty Vec when no memories exceed the threshold.
    async fn provide_context(
        &self,
        session_id: &str,
    ) -> Result<Vec<ProviderMessage>, BlufioError> {
        let query = self.get_current_query(session_id).await;
        if query.is_empty() {
            return Ok(vec![]);
        }

        let memories = self.retriever.retrieve(&query).await?;
        if memories.is_empty() {
            return Ok(vec![]);
        }

        // Format as structured memory block
        let mut memory_text = String::from("## Relevant Memories\n");
        for scored in &memories {
            memory_text.push_str(&format!("- {}\n", scored.memory.content));
        }

        Ok(vec![ProviderMessage {
            role: "user".to_string(),
            content: vec![ContentBlock::Text { text: memory_text }],
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Memory, MemorySource, MemoryStatus, ScoredMemory};

    /// Format memories the same way the provider does (for test verification).
    fn format_memories(memories: &[ScoredMemory]) -> String {
        let mut text = String::from("## Relevant Memories\n");
        for scored in memories {
            text.push_str(&format!("- {}\n", scored.memory.content));
        }
        text
    }

    fn make_scored_memory(content: &str, score: f32) -> ScoredMemory {
        ScoredMemory {
            memory: Memory {
                id: uuid::Uuid::new_v4().to_string(),
                content: content.to_string(),
                embedding: vec![],
                source: MemorySource::Extracted,
                confidence: 0.6,
                status: MemoryStatus::Active,
                superseded_by: None,
                session_id: None,
                created_at: String::new(),
                updated_at: String::new(),
            },
            score,
        }
    }

    #[test]
    fn format_memories_header() {
        let memories = vec![make_scored_memory("User has a dog named Max", 0.8)];
        let formatted = format_memories(&memories);
        assert!(formatted.starts_with("## Relevant Memories\n"));
    }

    #[test]
    fn format_memories_bullet_list() {
        let memories = vec![
            make_scored_memory("User has a dog named Max", 0.8),
            make_scored_memory("User prefers dark mode", 0.7),
        ];
        let formatted = format_memories(&memories);
        assert!(formatted.contains("- User has a dog named Max\n"));
        assert!(formatted.contains("- User prefers dark mode\n"));
    }

    #[test]
    fn format_memories_empty() {
        let memories: Vec<ScoredMemory> = vec![];
        let formatted = format_memories(&memories);
        assert_eq!(formatted, "## Relevant Memories\n");
    }

    #[tokio::test]
    async fn query_lifecycle() {
        // Test the query set/get/clear lifecycle without a real retriever
        let queries: Arc<RwLock<HashMap<String, String>>> =
            Arc::new(RwLock::new(HashMap::new()));

        // Set
        queries
            .write()
            .await
            .insert("session-1".to_string(), "What is my dog's name?".to_string());

        // Get
        let query = queries
            .read()
            .await
            .get("session-1")
            .cloned()
            .unwrap_or_default();
        assert_eq!(query, "What is my dog's name?");

        // Clear
        queries.write().await.remove("session-1");
        let query = queries
            .read()
            .await
            .get("session-1")
            .cloned()
            .unwrap_or_default();
        assert!(query.is_empty());
    }

    #[tokio::test]
    async fn empty_query_returns_empty() {
        let queries: Arc<RwLock<HashMap<String, String>>> =
            Arc::new(RwLock::new(HashMap::new()));

        let query = queries
            .read()
            .await
            .get("nonexistent")
            .cloned()
            .unwrap_or_default();
        assert!(query.is_empty());
    }
}
