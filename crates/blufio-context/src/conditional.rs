// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Conditional zone: trait for context providers that inject session-specific
//! context (memory, skills, etc.) into the prompt.
//!
//! This is stubbed for Phase 5 (Memory) and Phase 7 (Skills) to implement.

use async_trait::async_trait;
use blufio_core::error::BlufioError;
use blufio_core::types::ProviderMessage;

/// A provider that supplies conditional context for a session.
///
/// Implementations inject session-specific context such as:
/// - Relevant memories (Phase 5)
/// - Active skill manifests (Phase 7)
/// - User preference overrides
///
/// The context engine calls all registered providers during assembly
/// and includes their output between the static zone (system prompt)
/// and the dynamic zone (conversation history).
#[async_trait]
pub trait ConditionalProvider: Send + Sync {
    /// Returns context messages to inject for the given session.
    ///
    /// Returns an empty vec if no conditional context applies.
    async fn provide_context(
        &self,
        session_id: &str,
    ) -> Result<Vec<ProviderMessage>, BlufioError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use blufio_core::types::ContentBlock;

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
}
