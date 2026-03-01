// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Three-zone context engine for Blufio LLM prompt assembly.
//!
//! Assembles prompts from three zones:
//! - **Static zone**: System prompt with cache-aligned blocks
//! - **Conditional zone**: Session-specific context (stubbed for Phase 5/7)
//! - **Dynamic zone**: Conversation history with compaction
//!
//! The context engine orchestrates these zones to produce a [`ProviderRequest`]
//! ready to send to the LLM, while keeping token overhead within budget.

pub mod compaction;
pub mod conditional;
pub mod dynamic;
pub mod static_zone;

use blufio_config::model::{AgentConfig, ContextConfig};
use blufio_core::error::BlufioError;
use blufio_core::traits::{ProviderAdapter, StorageAdapter};
use blufio_core::types::{InboundMessage, ProviderRequest, TokenUsage};

pub use compaction::{generate_compaction_summary, persist_compaction_summary};
pub use conditional::ConditionalProvider;
pub use dynamic::{DynamicResult, DynamicZone};
pub use static_zone::StaticZone;

/// Result of context assembly, containing the provider request and any
/// side-effect costs (e.g., compaction) that must be recorded by the caller.
#[derive(Debug)]
pub struct AssembledContext {
    /// The provider request ready to send to the LLM.
    pub request: ProviderRequest,
    /// Token usage from compaction, if it was triggered during assembly.
    /// The caller (SessionActor) MUST record this in the cost ledger
    /// with FeatureType::Compaction. This is NOT included in the main
    /// response usage -- it is a separate LLM call to Haiku.
    pub compaction_usage: Option<TokenUsage>,
    /// Model used for compaction (needed by caller for cost calculation).
    pub compaction_model: Option<String>,
}

/// The context engine orchestrates three-zone prompt assembly.
///
/// Assembles a [`ProviderRequest`] from:
/// 1. Static zone (system prompt as cache-aligned blocks)
/// 2. Conditional zone (session-specific context from registered providers)
/// 3. Dynamic zone (conversation history with compaction)
pub struct ContextEngine {
    /// The static zone holding the system prompt.
    static_zone: StaticZone,
    /// Registered conditional context providers.
    conditional_providers: Vec<Box<dyn ConditionalProvider>>,
    /// The dynamic zone for history assembly and compaction.
    dynamic_zone: DynamicZone,
    /// Model used for compaction (from config).
    compaction_model: String,
}

impl ContextEngine {
    /// Creates a new context engine.
    ///
    /// Loads the static zone from agent config and configures the dynamic
    /// zone from context config.
    pub async fn new(
        agent_config: &AgentConfig,
        context_config: &ContextConfig,
    ) -> Result<Self, BlufioError> {
        let static_zone = StaticZone::new(agent_config).await?;
        let dynamic_zone = DynamicZone::new(context_config);

        Ok(Self {
            static_zone,
            conditional_providers: Vec::new(),
            dynamic_zone,
            compaction_model: context_config.compaction_model.clone(),
        })
    }

    /// Assembles a complete provider request from all three zones.
    ///
    /// Returns an [`AssembledContext`] containing the request and any
    /// compaction costs that must be recorded separately.
    pub async fn assemble(
        &self,
        provider: &dyn ProviderAdapter,
        storage: &dyn StorageAdapter,
        session_id: &str,
        inbound: &InboundMessage,
        model: &str,
        max_tokens: u32,
    ) -> Result<AssembledContext, BlufioError> {
        // 1. Get system blocks from static zone.
        let system_blocks = self.static_zone.system_blocks();

        // 2. Get conditional context (iterate providers, extend messages).
        let mut conditional_messages = Vec::new();
        for cp in &self.conditional_providers {
            let ctx = cp.provide_context(session_id).await?;
            conditional_messages.extend(ctx);
        }

        // 3. Get dynamic messages with compaction.
        let dynamic_result = self
            .dynamic_zone
            .assemble_messages(provider, storage, session_id, inbound)
            .await?;

        // 4. Combine conditional + dynamic messages.
        let mut all_messages = conditional_messages;
        all_messages.extend(dynamic_result.messages);

        // 5. Build ProviderRequest with system_blocks.
        let request = ProviderRequest {
            model: model.to_string(),
            system_prompt: None,
            system_blocks: Some(system_blocks),
            messages: all_messages,
            max_tokens,
            stream: true,
            tools: None,
        };

        // 6. Return AssembledContext with compaction info.
        let compaction_model = if dynamic_result.compaction_usage.is_some() {
            Some(self.compaction_model.clone())
        } else {
            None
        };

        Ok(AssembledContext {
            request,
            compaction_usage: dynamic_result.compaction_usage,
            compaction_model,
        })
    }

    /// Registers a conditional context provider.
    ///
    /// Providers are called in registration order during assembly.
    /// Phase 5 (Memory) and Phase 7 (Skills) will use this to inject
    /// session-specific context.
    pub fn add_conditional_provider(&mut self, provider: Box<dyn ConditionalProvider>) {
        self.conditional_providers.push(provider);
    }

    /// Returns a reference to the static zone.
    pub fn static_zone(&self) -> &StaticZone {
        &self.static_zone
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn context_engine_new() {
        let agent_config = AgentConfig {
            system_prompt: Some("Test engine.".into()),
            ..Default::default()
        };
        let context_config = ContextConfig::default();

        let engine = ContextEngine::new(&agent_config, &context_config)
            .await
            .unwrap();
        assert_eq!(engine.static_zone().system_prompt(), "Test engine.");
        assert!(engine.conditional_providers.is_empty());
        assert_eq!(engine.compaction_model, "claude-haiku-4-5-20250901");
    }

    #[tokio::test]
    async fn assembled_context_structure() {
        let ctx = AssembledContext {
            request: ProviderRequest {
                model: "test-model".into(),
                system_prompt: None,
                system_blocks: Some(serde_json::json!([{"type": "text", "text": "sys"}])),
                messages: vec![],
                max_tokens: 1024,
                stream: true,
                tools: None,
            },
            compaction_usage: Some(TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            }),
            compaction_model: Some("claude-haiku-4-5-20250901".into()),
        };

        assert!(ctx.compaction_usage.is_some());
        assert_eq!(ctx.compaction_usage.unwrap().input_tokens, 100);
        assert_eq!(ctx.compaction_model.unwrap(), "claude-haiku-4-5-20250901");
        assert!(ctx.request.system_blocks.is_some());
    }

    #[tokio::test]
    async fn assembled_context_without_compaction() {
        let ctx = AssembledContext {
            request: ProviderRequest {
                model: "test-model".into(),
                system_prompt: None,
                system_blocks: None,
                messages: vec![],
                max_tokens: 1024,
                stream: true,
                tools: None,
            },
            compaction_usage: None,
            compaction_model: None,
        };

        assert!(ctx.compaction_usage.is_none());
        assert!(ctx.compaction_model.is_none());
    }
}
