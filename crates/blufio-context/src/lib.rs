// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

//! Three-zone context engine for Blufio LLM prompt assembly.
//!
//! Assembles prompts from three zones:
//! - **Static zone**: System prompt with cache-aligned blocks
//! - **Conditional zone**: Session-specific context (stubbed for Phase 5/7)
//! - **Dynamic zone**: Conversation history with compaction
//!
//! The context engine orchestrates these zones to produce a [`ProviderRequest`]
//! ready to send to the LLM, while keeping token overhead within budget.

pub mod budget;
pub mod compaction;
pub mod conditional;
pub mod dynamic;
pub mod static_zone;

use std::sync::Arc;

use blufio_config::model::{AgentConfig, ContextConfig};
use blufio_core::error::BlufioError;
use blufio_core::token_counter::TokenizerCache;
use blufio_core::traits::{ProviderAdapter, StorageAdapter};
use blufio_core::types::{InboundMessage, ProviderRequest, TokenUsage};

pub use budget::ZoneBudget;
pub use compaction::{generate_compaction_summary, persist_compaction_summary};
pub use conditional::ConditionalProvider;
pub use dynamic::{DynamicResult, DynamicZone};
pub use static_zone::StaticZone;

/// Parameters for [`ContextEngine::assemble_with_boundaries`].
pub struct AssemblyParams<'a> {
    /// The provider adapter for LLM calls.
    pub provider: &'a dyn ProviderAdapter,
    /// The storage adapter for persistence.
    pub storage: &'a dyn StorageAdapter,
    /// The session ID.
    pub session_id: &'a str,
    /// The inbound user message.
    pub inbound: &'a InboundMessage,
    /// The model name.
    pub model: &'a str,
    /// Max tokens for the LLM request.
    pub max_tokens: u32,
    /// Optional boundary manager for L3 HMAC protection.
    pub boundary_manager: Option<&'a blufio_injection::boundary::BoundaryManager>,
}

/// Result of context assembly, containing the provider request and any
/// side-effect costs (e.g., compaction) that must be recorded by the caller.
#[derive(Debug)]
pub struct AssembledContext {
    /// The provider request ready to send to the LLM.
    pub request: ProviderRequest,
    /// Token usages from compaction LLM calls during assembly.
    /// May contain multiple entries if cascade compaction triggered (L1 then L2).
    /// Each entry is a separate LLM call to the compaction model.
    /// The caller (SessionActor) MUST record each in the cost ledger
    /// with FeatureType::Compaction. These are NOT included in the main
    /// response usage.
    pub compaction_usages: Vec<TokenUsage>,
    /// Model used for compaction (needed by caller for cost calculation).
    pub compaction_model: Option<String>,
    /// Names of conditional providers that were dropped during budget enforcement.
    /// Empty when all providers fit within the conditional zone budget.
    /// Useful for debugging context assembly decisions.
    pub dropped_providers: Vec<String>,
    /// Entity strings extracted during compaction that the caller should persist
    /// as Memory entries with `MemorySource::Extracted`. Forwarded from
    /// [`DynamicResult::extracted_entities`].
    pub extracted_entities: Vec<String>,
    /// L3 boundary validation failure events (if boundary protection was active).
    /// The caller should emit these via the EventBus for audit logging.
    pub boundary_events: Vec<blufio_bus::events::SecurityEvent>,
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
    /// Cached tokenizer instances for accurate token counting.
    token_cache: Arc<TokenizerCache>,
    /// Per-zone token budget configuration.
    zone_budget: ZoneBudget,
}

impl ContextEngine {
    /// Creates a new context engine.
    ///
    /// Loads the static zone from agent config and configures the dynamic
    /// zone from context config.
    pub async fn new(
        agent_config: &AgentConfig,
        context_config: &ContextConfig,
        token_cache: Arc<TokenizerCache>,
    ) -> Result<Self, BlufioError> {
        let static_zone = StaticZone::new(agent_config).await?;
        let dynamic_zone = DynamicZone::new(context_config, token_cache.clone());
        let zone_budget = ZoneBudget::from_config(context_config);

        Ok(Self {
            static_zone,
            conditional_providers: Vec::new(),
            dynamic_zone,
            compaction_model: context_config.compaction_model.clone(),
            token_cache,
            zone_budget,
        })
    }

    /// Assembles a complete provider request from all three zones with
    /// per-zone budget enforcement.
    pub async fn assemble(
        &self,
        provider: &dyn ProviderAdapter,
        storage: &dyn StorageAdapter,
        session_id: &str,
        inbound: &InboundMessage,
        model: &str,
        max_tokens: u32,
    ) -> Result<AssembledContext, BlufioError> {
        self.assemble_with_boundaries(AssemblyParams {
            provider,
            storage,
            session_id,
            inbound,
            model,
            max_tokens,
            boundary_manager: None,
        })
        .await
    }

    /// Assembles a complete provider request from all three zones with
    /// per-zone budget enforcement and optional L3 HMAC boundary protection.
    ///
    /// When `boundary_manager` is `Some`, each zone's text content is wrapped
    /// with HMAC boundary tokens during assembly, then validated and stripped
    /// before the LLM sees the content. Any tampered or spoofed zones are
    /// detected and removed, with [`SecurityEvent::BoundaryFailure`] events
    /// returned in the result for the caller to emit.
    pub async fn assemble_with_boundaries(
        &self,
        params: AssemblyParams<'_>,
    ) -> Result<AssembledContext, BlufioError> {
        // OTel: Context assembly span with per-zone token count attributes.
        // Created as a handle (not entered) because entered spans are !Send.
        // Recording fields on the handle still exports them via OTel.
        let _ctx_span = tracing::info_span!(
            "blufio.context.assemble",
            "blufio.context.static_tokens" = tracing::field::Empty,
            "blufio.context.conditional_tokens" = tracing::field::Empty,
            "blufio.context.dynamic_tokens" = tracing::field::Empty,
            "blufio.context.total_tokens" = tracing::field::Empty,
        );

        let AssemblyParams {
            provider,
            storage,
            session_id,
            inbound,
            model,
            max_tokens,
            boundary_manager,
        } = params;

        // --- Step 1: Static zone ---
        let system_blocks = self.static_zone.system_blocks();
        let actual_static = self.static_zone.token_count(&self.token_cache, model).await;
        self.static_zone
            .check_budget(actual_static, self.zone_budget.static_budget);
        metrics::gauge!("blufio_context_zone_tokens", "zone" => "static").set(actual_static as f64);

        // --- Step 2: Conditional zone ---
        let mut provider_results: Vec<(String, Vec<blufio_core::types::ProviderMessage>)> =
            Vec::new();
        for (i, cp) in self.conditional_providers.iter().enumerate() {
            let ctx = cp.provide_context(session_id).await?;
            let name = format!("conditional_provider_{}", i);
            provider_results.push((name, ctx));
        }

        let effective_budget = self.zone_budget.conditional_effective();
        let (conditional_messages, dropped) = budget::enforce_conditional_budget(
            provider_results,
            effective_budget,
            &self.token_cache,
            model,
        )
        .await;

        let counter = self.token_cache.get_counter(model);
        let actual_conditional =
            budget::count_messages_tokens(&conditional_messages, counter.as_ref()).await;
        metrics::gauge!("blufio_context_zone_tokens", "zone" => "conditional")
            .set(actual_conditional as f64);

        // --- Step 3: Dynamic zone ---
        let dynamic_budget = self
            .zone_budget
            .dynamic_budget(actual_static as u32, actual_conditional as u32);

        let dynamic_result = self
            .dynamic_zone
            .assemble_messages(
                provider,
                storage,
                session_id,
                inbound,
                model,
                dynamic_budget,
            )
            .await?;

        let actual_dynamic =
            budget::count_messages_tokens(&dynamic_result.messages, counter.as_ref()).await;
        metrics::gauge!("blufio_context_zone_tokens", "zone" => "dynamic")
            .set(actual_dynamic as f64);

        // OTel: Record per-zone and total token counts on context assembly span.
        _ctx_span.record("blufio.context.static_tokens", actual_static as u64);
        _ctx_span.record(
            "blufio.context.conditional_tokens",
            actual_conditional as u64,
        );
        _ctx_span.record("blufio.context.dynamic_tokens", actual_dynamic as u64);
        _ctx_span.record(
            "blufio.context.total_tokens",
            (actual_static + actual_conditional + actual_dynamic) as u64,
        );

        // --- Step 4: Combine conditional + dynamic messages ---
        let mut all_messages = conditional_messages;
        all_messages.extend(dynamic_result.messages);

        // --- Step 4b: L3 HMAC boundary protection ---
        // Wrap system blocks and messages with HMAC boundaries, then validate
        // and strip before the LLM sees the content.
        let (system_blocks, all_messages, boundary_events) = if let Some(bm) = boundary_manager {
            use blufio_injection::boundary::ZoneType;

            // Wrap system_blocks text content with static zone boundaries.
            let wrapped_system =
                wrap_system_blocks(system_blocks.clone(), bm, ZoneType::Static, "system");

            // Wrap message content blocks with zone boundaries.
            // Conditional provider messages = first N messages (before dynamic).
            // Dynamic messages = remaining messages.
            let wrapped_messages = wrap_messages_with_boundaries(&all_messages, bm);

            // Concatenate all wrapped text for boundary validation.
            let mut all_wrapped_text = String::new();
            if let serde_json::Value::Array(ref blocks) = wrapped_system {
                for block in blocks {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        all_wrapped_text.push_str(text);
                        all_wrapped_text.push('\n');
                    }
                }
            }
            for msg in &wrapped_messages {
                for block in &msg.content {
                    if let blufio_core::types::ContentBlock::Text { text } = block {
                        all_wrapped_text.push_str(text);
                        all_wrapped_text.push('\n');
                    }
                }
            }

            // Validate boundaries and collect failure events.
            let boundary_corr_id = uuid::Uuid::new_v4().to_string();
            let (_stripped, events) = bm.validate_and_strip(&all_wrapped_text, &boundary_corr_id);

            // Log any boundary failures.
            if !events.is_empty() {
                tracing::warn!(
                    session_id = session_id,
                    failure_count = events.len(),
                    "L3: boundary validation detected {} failure(s)",
                    events.len()
                );
                blufio_injection::metrics::record_boundary_failures(events.len() as u64);
            } else {
                blufio_injection::metrics::record_boundary_validations(1);
            }

            // Strip boundary tokens from the actual content before sending to LLM.
            let clean_system = strip_boundary_tokens_from_system_blocks(wrapped_system);
            let clean_messages = strip_boundary_tokens_from_messages(wrapped_messages);

            (clean_system, clean_messages, events)
        } else {
            (system_blocks, all_messages, vec![])
        };

        // --- Step 5: Build ProviderRequest ---
        let request = ProviderRequest {
            model: model.to_string(),
            system_prompt: None,
            system_blocks: Some(system_blocks),
            messages: all_messages,
            max_tokens,
            stream: true,
            tools: None,
        };

        // --- Step 6: Return AssembledContext ---
        let compaction_model = if !dynamic_result.compaction_usages.is_empty() {
            Some(self.compaction_model.clone())
        } else {
            None
        };

        Ok(AssembledContext {
            request,
            compaction_usages: dynamic_result.compaction_usages,
            compaction_model,
            dropped_providers: dropped,
            extracted_entities: dynamic_result.extracted_entities,
            boundary_events,
        })
    }

    /// Registers a conditional context provider.
    pub fn add_conditional_provider(&mut self, provider: Box<dyn ConditionalProvider>) {
        self.conditional_providers.push(provider);
    }

    /// Returns a reference to the static zone.
    pub fn static_zone(&self) -> &StaticZone {
        &self.static_zone
    }
}

// ---------------------------------------------------------------------------
// L3 boundary helper functions
// ---------------------------------------------------------------------------

/// Regex for stripping boundary tokens from text content.
static BOUNDARY_STRIP_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"<<BLUF-ZONE-v1:\w+:.+?:[0-9a-f]{64}>>")
        .expect("boundary strip regex must compile")
});

/// Wrap text blocks within system_blocks JSON with HMAC boundary tokens.
fn wrap_system_blocks(
    system_blocks: serde_json::Value,
    bm: &blufio_injection::boundary::BoundaryManager,
    zone: blufio_injection::boundary::ZoneType,
    source: &str,
) -> serde_json::Value {
    if let serde_json::Value::Array(blocks) = system_blocks {
        let wrapped: Vec<serde_json::Value> = blocks
            .into_iter()
            .map(|mut block| {
                if let Some(text) = block
                    .get("text")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string())
                {
                    let wrapped_text = bm.wrap_content(zone, source, &text);
                    block["text"] = serde_json::Value::String(wrapped_text);
                }
                block
            })
            .collect();
        serde_json::Value::Array(wrapped)
    } else {
        system_blocks
    }
}

/// Wrap text content blocks within messages with HMAC boundary tokens.
fn wrap_messages_with_boundaries(
    messages: &[blufio_core::types::ProviderMessage],
    bm: &blufio_injection::boundary::BoundaryManager,
) -> Vec<blufio_core::types::ProviderMessage> {
    messages
        .iter()
        .map(|msg| {
            // Determine zone type based on role.
            let zone = if msg.role == "system" {
                blufio_injection::boundary::ZoneType::Static
            } else {
                blufio_injection::boundary::ZoneType::Dynamic
            };
            let source = &msg.role;

            let content = msg
                .content
                .iter()
                .map(|block| match block {
                    blufio_core::types::ContentBlock::Text { text } => {
                        blufio_core::types::ContentBlock::Text {
                            text: bm.wrap_content(zone, source, text),
                        }
                    }
                    other => other.clone(),
                })
                .collect();

            blufio_core::types::ProviderMessage {
                role: msg.role.clone(),
                content,
            }
        })
        .collect()
}

/// Strip boundary tokens from system_blocks JSON text content.
fn strip_boundary_tokens_from_system_blocks(system_blocks: serde_json::Value) -> serde_json::Value {
    if let serde_json::Value::Array(blocks) = system_blocks {
        let stripped: Vec<serde_json::Value> = blocks
            .into_iter()
            .map(|mut block| {
                if let Some(text) = block
                    .get("text")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string())
                {
                    let clean = BOUNDARY_STRIP_RE.replace_all(&text, "").to_string();
                    block["text"] = serde_json::Value::String(clean);
                }
                block
            })
            .collect();
        serde_json::Value::Array(stripped)
    } else {
        system_blocks
    }
}

/// Strip boundary tokens from message text content blocks.
fn strip_boundary_tokens_from_messages(
    messages: Vec<blufio_core::types::ProviderMessage>,
) -> Vec<blufio_core::types::ProviderMessage> {
    messages
        .into_iter()
        .map(|msg| {
            let content = msg
                .content
                .into_iter()
                .map(|block| match block {
                    blufio_core::types::ContentBlock::Text { text } => {
                        blufio_core::types::ContentBlock::Text {
                            text: BOUNDARY_STRIP_RE.replace_all(&text, "").to_string(),
                        }
                    }
                    other => other,
                })
                .collect();

            blufio_core::types::ProviderMessage {
                role: msg.role,
                content,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use blufio_core::token_counter::{TokenizerCache, TokenizerMode};

    #[tokio::test]
    async fn context_engine_new() {
        let agent_config = AgentConfig {
            system_prompt: Some("Test engine.".into()),
            ..Default::default()
        };
        let context_config = ContextConfig::default();
        let token_cache = Arc::new(TokenizerCache::new(TokenizerMode::Fast));

        let engine = ContextEngine::new(&agent_config, &context_config, token_cache)
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
            compaction_usages: vec![TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            }],
            compaction_model: Some("claude-haiku-4-5-20250901".into()),
            dropped_providers: vec![],
            extracted_entities: vec![],
            boundary_events: vec![],
        };

        assert_eq!(ctx.compaction_usages.len(), 1);
        assert_eq!(ctx.compaction_usages[0].input_tokens, 100);
        assert_eq!(ctx.compaction_model.unwrap(), "claude-haiku-4-5-20250901");
        assert!(ctx.request.system_blocks.is_some());
        assert!(ctx.dropped_providers.is_empty());
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
            compaction_usages: vec![],
            compaction_model: None,
            dropped_providers: vec![],
            extracted_entities: vec![],
            boundary_events: vec![],
        };

        assert!(ctx.compaction_usages.is_empty());
        assert!(ctx.compaction_model.is_none());
        assert!(ctx.dropped_providers.is_empty());
    }

    #[tokio::test]
    async fn assembled_context_with_dropped_providers() {
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
            compaction_usages: vec![],
            compaction_model: None,
            dropped_providers: vec!["archive".to_string()],
            extracted_entities: vec![],
            boundary_events: vec![],
        };

        assert_eq!(ctx.dropped_providers.len(), 1);
        assert_eq!(ctx.dropped_providers[0], "archive");
    }
}
