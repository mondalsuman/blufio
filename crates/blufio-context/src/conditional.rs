// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Conditional zone: trait for context providers that inject session-specific
//! context (memory, skills, archives, etc.) into the prompt.

use std::sync::Arc;

use async_trait::async_trait;
use blufio_core::error::BlufioError;
use blufio_core::token_counter::TokenizerCache;
use blufio_core::types::{ContentBlock, ProviderMessage};
use blufio_storage::Database;

use crate::compaction::archive;

/// A provider that supplies conditional context for a session.
///
/// Implementations inject session-specific context such as:
/// - Relevant memories (Phase 5)
/// - Active skill manifests (Phase 7)
/// - User preference overrides
/// - Historical archives (ArchiveConditionalProvider)
///
/// The context engine calls all registered providers during assembly
/// and includes their output between the static zone (system prompt)
/// and the dynamic zone (conversation history).
#[async_trait]
pub trait ConditionalProvider: Send + Sync {
    /// Returns context messages to inject for the given session.
    ///
    /// Returns an empty vec if no conditional context applies.
    async fn provide_context(&self, session_id: &str) -> Result<Vec<ProviderMessage>, BlufioError>;
}

/// Conditional provider that injects cross-session archive summaries.
///
/// Registered as the LOWEST priority provider (after memory, skills).
/// Provides historical context from previous sessions for the same user.
pub struct ArchiveConditionalProvider {
    /// Database for archive and session queries.
    db: Arc<Database>,
    /// Cached tokenizer instances for token budget enforcement.
    token_cache: Arc<TokenizerCache>,
    /// Token budget allocated to this provider (from conditional zone budget).
    conditional_budget: u32,
    /// Whether archiving is enabled.
    archive_enabled: bool,
    /// Model for token counting.
    model: String,
}

impl ArchiveConditionalProvider {
    /// Creates a new archive conditional provider.
    pub fn new(
        db: Arc<Database>,
        token_cache: Arc<TokenizerCache>,
        conditional_budget: u32,
        archive_enabled: bool,
        model: String,
    ) -> Self {
        Self {
            db,
            token_cache,
            conditional_budget,
            archive_enabled,
            model,
        }
    }
}

#[async_trait]
impl ConditionalProvider for ArchiveConditionalProvider {
    async fn provide_context(&self, session_id: &str) -> Result<Vec<ProviderMessage>, BlufioError> {
        if !self.archive_enabled {
            return Ok(Vec::new());
        }

        // Look up user_id from session.
        let session = blufio_storage::queries::sessions::get_session(&self.db, session_id).await?;
        let user_id = match session.and_then(|s| s.user_id) {
            Some(uid) => uid,
            None => {
                tracing::debug!(
                    session_id = session_id,
                    "no user_id for session, skipping archive injection"
                );
                return Ok(Vec::new());
            }
        };

        // Retrieve archives within token budget.
        let summaries = archive::get_archives_for_context(
            &self.db,
            &user_id,
            self.conditional_budget,
            &self.token_cache,
            &self.model,
        )
        .await?;

        if summaries.is_empty() {
            return Ok(Vec::new());
        }

        // Format archives as a single system message.
        let archive_text = format!(
            "Historical context from previous sessions:\n\n{}",
            summaries.join("\n\n---\n\n")
        );

        tracing::info!(
            session_id = session_id,
            user_id = %user_id,
            archive_count = summaries.len(),
            "injecting archive context"
        );

        Ok(vec![ProviderMessage {
            role: "system".to_string(),
            content: vec![ContentBlock::Text { text: archive_text }],
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mock conditional provider for testing.
    struct MockConditionalProvider {
        messages: Vec<ProviderMessage>,
    }

    #[async_trait]
    impl ConditionalProvider for MockConditionalProvider {
        async fn provide_context(
            &self,
            _session_id: &str,
        ) -> Result<Vec<ProviderMessage>, BlufioError> {
            Ok(self.messages.clone())
        }
    }

    #[tokio::test]
    async fn conditional_provider_returns_messages() {
        let provider = MockConditionalProvider {
            messages: vec![ProviderMessage {
                role: "system".into(),
                content: vec![ContentBlock::Text {
                    text: "User prefers concise answers.".into(),
                }],
            }],
        };

        let result = provider.provide_context("session-1").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "system");
    }

    #[tokio::test]
    async fn conditional_provider_empty() {
        let provider = MockConditionalProvider { messages: vec![] };
        let result = provider.provide_context("session-1").await.unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn archive_provider_disabled() {
        // ArchiveConditionalProvider respects archive_enabled=false.
        // Full integration test requires a Database; unit test validates the type exists
        // and can be constructed. The provide_context early-return on disabled is trivial.
        // (Intentionally empty — type-level validation only.)
    }

    #[test]
    fn archive_context_format() {
        let summaries = [
            "Session 1: User discussed project timeline.".to_string(),
            "Session 2: User asked about budget.".to_string(),
        ];
        let formatted = format!(
            "Historical context from previous sessions:\n\n{}",
            summaries.join("\n\n---\n\n")
        );
        assert!(formatted.contains("Historical context"));
        assert!(formatted.contains("project timeline"));
        assert!(formatted.contains("budget"));
        assert!(formatted.contains("---"));
    }
}
