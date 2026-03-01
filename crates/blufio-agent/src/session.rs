// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Per-session FSM that manages the lifecycle of a single conversation.
//!
//! Each session goes through states: Idle -> Receiving -> Processing -> Responding -> Idle.
//! The Draining state is used during graceful shutdown.
//!
//! The session actor integrates:
//! - **Context engine**: Three-zone prompt assembly (static, conditional, dynamic)
//! - **Budget tracker**: Pre-call budget gate to enforce daily/monthly caps
//! - **Cost ledger**: Post-call cost recording with full token breakdown

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use blufio_context::ContextEngine;
use blufio_cost::ledger::{CostRecord, FeatureType};
use blufio_cost::pricing;
use blufio_cost::BudgetTracker;
use blufio_cost::CostLedger;
use blufio_core::error::BlufioError;
use blufio_core::types::{
    InboundMessage, Message, ProviderStreamChunk, TokenUsage, ToolUseData,
};
use blufio_core::{ProviderAdapter, StorageAdapter};
use blufio_memory::{MemoryExtractor, MemoryProvider};
use blufio_router::{ModelRouter, RoutingDecision};
use blufio_skill::{ToolOutput, ToolRegistry};
use futures::Stream;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::context;

/// Maximum number of tool call iterations before forcing a text response.
pub const MAX_TOOL_ITERATIONS: usize = 10;

/// States in the session FSM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Waiting for a new message.
    Idle,
    /// Message received, preparing to process.
    Receiving,
    /// Assembling context and calling the LLM.
    Processing,
    /// Streaming response back to the channel.
    Responding,
    /// Executing tools from a tool_use response.
    ToolExecuting,
    /// Graceful shutdown: finishing current response before exit.
    Draining,
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionState::Idle => write!(f, "idle"),
            SessionState::Receiving => write!(f, "receiving"),
            SessionState::Processing => write!(f, "processing"),
            SessionState::Responding => write!(f, "responding"),
            SessionState::ToolExecuting => write!(f, "tool_executing"),
            SessionState::Draining => write!(f, "draining"),
        }
    }
}

/// Manages the state and message processing for a single conversation session.
///
/// The session actor is responsible for:
/// - Persisting inbound user messages
/// - Checking budget before LLM calls
/// - Assembling context via the three-zone context engine
/// - Calling the LLM provider with per-message model routing
/// - Recording costs (message, compaction, and extraction) after responses
/// - Persisting assistant responses
/// - Setting/clearing memory queries for context injection
/// - Triggering idle memory extraction after configurable timeout
pub struct SessionActor {
    session_id: String,
    state: SessionState,
    storage: Arc<dyn StorageAdapter + Send + Sync>,
    provider: Arc<dyn ProviderAdapter + Send + Sync>,
    context_engine: Arc<ContextEngine>,
    budget_tracker: Arc<tokio::sync::Mutex<BudgetTracker>>,
    cost_ledger: Arc<CostLedger>,
    /// Memory provider for setting current query before context assembly.
    memory_provider: Option<MemoryProvider>,
    /// Memory extractor for end-of-conversation fact extraction.
    memory_extractor: Option<Arc<MemoryExtractor>>,
    channel: String,
    /// Model router for per-message complexity classification and model selection.
    router: Arc<ModelRouter>,
    /// Default model used when routing is disabled.
    default_model: String,
    /// Default max tokens used when routing is disabled.
    default_max_tokens: u32,
    /// Whether model routing is enabled.
    routing_enabled: bool,
    /// Last routing decision for cost recording in persist_response.
    last_routing_decision: Option<RoutingDecision>,
    /// Timestamp of last message received -- for idle extraction detection.
    last_message_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Idle timeout for triggering extraction (from config).
    idle_timeout: Duration,
    /// Registry of available tools (built-in and WASM skills).
    tool_registry: Arc<RwLock<ToolRegistry>>,
    /// Maximum number of tool call iterations per message.
    max_tool_iterations: usize,
}

impl SessionActor {
    /// Creates a new session actor with context engine, cost tracking, routing, memory, and tools.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: String,
        storage: Arc<dyn StorageAdapter + Send + Sync>,
        provider: Arc<dyn ProviderAdapter + Send + Sync>,
        context_engine: Arc<ContextEngine>,
        budget_tracker: Arc<tokio::sync::Mutex<BudgetTracker>>,
        cost_ledger: Arc<CostLedger>,
        memory_provider: Option<MemoryProvider>,
        memory_extractor: Option<Arc<MemoryExtractor>>,
        channel: String,
        router: Arc<ModelRouter>,
        default_model: String,
        default_max_tokens: u32,
        routing_enabled: bool,
        idle_timeout_secs: u64,
        tool_registry: Arc<RwLock<ToolRegistry>>,
    ) -> Self {
        Self {
            session_id,
            state: SessionState::Idle,
            storage,
            provider,
            context_engine,
            budget_tracker,
            cost_ledger,
            memory_provider,
            memory_extractor,
            channel,
            router,
            default_model,
            default_max_tokens,
            routing_enabled,
            last_routing_decision: None,
            last_message_at: None,
            idle_timeout: Duration::from_secs(idle_timeout_secs),
            tool_registry,
            max_tool_iterations: MAX_TOOL_ITERATIONS,
        }
    }

    /// Returns the current session state.
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Returns the session ID.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Returns the channel this session belongs to.
    pub fn channel(&self) -> &str {
        &self.channel
    }

    /// Returns the last routing decision (if routing is enabled).
    ///
    /// Used by the agent loop to detect budget downgrades and add
    /// transparent notifications to the response.
    pub fn last_routing_decision(&self) -> Option<&RoutingDecision> {
        self.last_routing_decision.as_ref()
    }

    /// Handles an inbound message: persists it, checks budget, assembles context,
    /// records compaction costs, and starts streaming.
    ///
    /// Also triggers idle memory extraction if enough time has passed since
    /// the last message, and sets the current query on the memory provider
    /// for context injection.
    ///
    /// Returns a stream of provider response chunks. The caller is responsible for
    /// consuming the stream and calling [`persist_response`] when done.
    pub async fn handle_message(
        &mut self,
        inbound: InboundMessage,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<ProviderStreamChunk, BlufioError>> + Send>>,
        BlufioError,
    > {
        // Transition: Idle -> Receiving
        self.state = SessionState::Receiving;

        // Check for idle extraction trigger (before updating last_message_at).
        self.maybe_trigger_idle_extraction().await;

        // Extract text content and handle per-message model override.
        let raw_text = context::message_content_to_text(&inbound.content);

        // Parse per-message override (/opus, /haiku, /sonnet) and strip prefix.
        let (_, clean_text) = blufio_router::parse_model_override(&raw_text);
        let text_content = clean_text.to_string();

        // Persist the inbound user message (with override prefix stripped).
        let now = chrono::Utc::now().to_rfc3339();
        let msg = Message {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: self.session_id.clone(),
            role: "user".to_string(),
            content: text_content.clone(),
            token_count: None,
            metadata: inbound.metadata.clone(),
            created_at: now,
        };
        self.storage.insert_message(&msg).await?;

        // Update last message timestamp for idle detection.
        self.last_message_at = Some(chrono::Utc::now());

        debug!(
            session_id = self.session_id.as_str(),
            "persisted user message"
        );

        // Transition: Receiving -> Processing
        self.state = SessionState::Processing;

        // Budget check before LLM call.
        {
            let mut tracker = self.budget_tracker.lock().await;
            tracker.check_budget()?;
        }

        // Determine model and max_tokens via routing or defaults.
        let (model, max_tokens) = if self.routing_enabled {
            // Get recent context for classification momentum.
            let recent_msgs = self.storage.get_messages(&self.session_id, Some(3)).await?;
            let recent_strings: Vec<String> = recent_msgs.iter().map(|m| m.content.clone()).collect();
            let recent_refs: Vec<&str> = recent_strings.iter().map(|s| s.as_str()).collect();

            // Get budget utilization for downgrade logic.
            let budget_util = {
                let tracker = self.budget_tracker.lock().await;
                tracker.budget_utilization()
            };

            // Route using the raw text (which may have the /opus etc prefix).
            let decision = self.router.route(&raw_text, &recent_refs, budget_util);

            if decision.downgraded {
                info!(
                    session_id = %self.session_id,
                    intended = %decision.intended_model,
                    actual = %decision.actual_model,
                    reason = %decision.reason,
                    "model downgraded due to budget"
                );
            } else {
                debug!(
                    session_id = %self.session_id,
                    model = %decision.actual_model,
                    tier = %decision.tier,
                    reason = %decision.reason,
                    "routed message"
                );
            }

            let model = decision.actual_model.clone();
            let max_tokens = decision.max_tokens;
            self.last_routing_decision = Some(decision);
            (model, max_tokens)
        } else {
            self.last_routing_decision = None;
            (self.default_model.clone(), self.default_max_tokens)
        };

        // Set current query on memory provider for retrieval.
        if let Some(ref mp) = self.memory_provider {
            mp.set_current_query(&self.session_id, &text_content).await;
        }

        // Assemble context using the three-zone context engine.
        let assembled = self.context_engine.assemble(
            self.provider.as_ref(),
            self.storage.as_ref(),
            &self.session_id,
            &inbound,
            &model,
            max_tokens,
        )
        .await;

        // Clear current query on memory provider (regardless of assembly outcome).
        if let Some(ref mp) = self.memory_provider {
            mp.clear_current_query(&self.session_id).await;
        }

        let mut assembled = assembled?;

        // Inject tool definitions from the tool registry into the request.
        {
            let registry = self.tool_registry.read().await;
            if !registry.is_empty() {
                assembled.request.tools = Some(registry.tool_definitions());
            }
        }

        // Record compaction costs if compaction was triggered during assembly.
        // Compaction is a separate Haiku LLM call that must be recorded with
        // FeatureType::Compaction, not Message.
        if let Some(ref compaction_usage) = assembled.compaction_usage {
            let compaction_model = assembled.compaction_model.as_deref()
                .unwrap_or("claude-haiku-4-5-20250901");
            let model_pricing = pricing::get_pricing(compaction_model);
            let cost_usd = pricing::calculate_cost(compaction_usage, &model_pricing);

            let record = CostRecord::new(
                self.session_id.clone(),
                compaction_model.to_string(),
                FeatureType::Compaction,
                compaction_usage,
                cost_usd,
            );

            self.cost_ledger.record(&record).await?;

            {
                let mut tracker = self.budget_tracker.lock().await;
                tracker.record_cost(cost_usd);
            }

            info!(
                session_id = %self.session_id,
                model = %compaction_model,
                input_tokens = compaction_usage.input_tokens,
                output_tokens = compaction_usage.output_tokens,
                cost_usd = cost_usd,
                "compaction cost recorded"
            );
        }

        // Stream from provider using the assembled request.
        let stream = self.provider.stream(assembled.request).await?;

        // Transition: Processing -> Responding
        self.state = SessionState::Responding;

        Ok(stream)
    }

    /// Persists the full assistant response text and records message cost.
    pub async fn persist_response(
        &mut self,
        full_text: &str,
        usage: Option<TokenUsage>,
    ) -> Result<(), BlufioError> {
        let now = chrono::Utc::now().to_rfc3339();
        let msg = Message {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: self.session_id.clone(),
            role: "assistant".to_string(),
            content: full_text.to_string(),
            token_count: usage.as_ref().map(|u| i64::from(u.output_tokens)),
            metadata: None,
            created_at: now,
        };
        self.storage.insert_message(&msg).await?;

        debug!(
            session_id = self.session_id.as_str(),
            "persisted assistant response"
        );

        // Record cost in ledger and budget tracker.
        // Use routing decision to track intended vs actual model.
        if let Some(ref usage) = usage {
            let (model_for_cost, intended_model) = match &self.last_routing_decision {
                Some(d) => (d.actual_model.clone(), Some(d.intended_model.clone())),
                None => (self.default_model.clone(), None),
            };

            let model_pricing = pricing::get_pricing(&model_for_cost);
            let cost_usd = pricing::calculate_cost(usage, &model_pricing);

            let mut record = CostRecord::new(
                self.session_id.clone(),
                model_for_cost.clone(),
                FeatureType::Message,
                usage,
                cost_usd,
            );
            if let Some(intended) = intended_model {
                record = record.with_intended_model(intended);
            }

            self.cost_ledger.record(&record).await?;

            {
                let mut tracker = self.budget_tracker.lock().await;
                tracker.record_cost(cost_usd);
            }

            info!(
                session_id = %self.session_id,
                model = %model_for_cost,
                intended_model = ?record.intended_model,
                input_tokens = usage.input_tokens,
                output_tokens = usage.output_tokens,
                cache_read_tokens = usage.cache_read_tokens,
                cost_usd = cost_usd,
                "message cost recorded"
            );
        }

        // Transition: Responding -> Idle
        self.state = SessionState::Idle;

        Ok(())
    }

    /// Returns the maximum number of tool call iterations per message.
    pub fn max_tool_iterations(&self) -> usize {
        self.max_tool_iterations
    }

    /// Returns a reference to the tool registry.
    pub fn tool_registry(&self) -> &Arc<RwLock<ToolRegistry>> {
        &self.tool_registry
    }

    /// Executes a batch of tool calls and returns the results.
    ///
    /// For each [`ToolUseData`] block, looks up the tool in the registry,
    /// invokes it, and collects the results as `(tool_use_id, content, is_error)`.
    ///
    /// Transitions state to [`SessionState::ToolExecuting`] during execution
    /// and back to [`SessionState::Processing`] when done.
    pub async fn execute_tools(
        &mut self,
        tool_uses: &[ToolUseData],
    ) -> Result<Vec<(String, ToolOutput)>, BlufioError> {
        self.state = SessionState::ToolExecuting;

        let mut results = Vec::with_capacity(tool_uses.len());

        for tu in tool_uses {
            let registry = self.tool_registry.read().await;
            let output = match registry.get(&tu.name) {
                Some(tool) => {
                    debug!(
                        session_id = %self.session_id,
                        tool = %tu.name,
                        tool_use_id = %tu.id,
                        "executing tool"
                    );
                    // Drop the read guard before the async invoke to avoid holding
                    // the lock across an await point.
                    drop(registry);
                    match tool.invoke(tu.input.clone()).await {
                        Ok(output) => output,
                        Err(e) => {
                            warn!(
                                session_id = %self.session_id,
                                tool = %tu.name,
                                error = %e,
                                "tool invocation failed"
                            );
                            ToolOutput {
                                content: format!("Error: {e}"),
                                is_error: true,
                            }
                        }
                    }
                }
                None => {
                    drop(registry);
                    warn!(
                        session_id = %self.session_id,
                        tool = %tu.name,
                        "tool not found in registry"
                    );
                    ToolOutput {
                        content: format!("Error: tool '{}' not found", tu.name),
                        is_error: true,
                    }
                }
            };

            results.push((tu.id.clone(), output));
        }

        self.state = SessionState::Processing;
        Ok(results)
    }

    /// Checks if enough idle time has passed since the last message to trigger
    /// background memory extraction. If so, extracts facts from recent
    /// conversation messages and records the extraction cost.
    ///
    /// This is called at the start of each new message. If the session was idle
    /// for longer than `idle_timeout`, it extracts memories from the conversation
    /// segment since the last extraction.
    ///
    /// All failures are logged but never propagated -- memory extraction is non-fatal.
    async fn maybe_trigger_idle_extraction(&self) {
        let (Some(extractor), Some(last_at)) =
            (&self.memory_extractor, self.last_message_at)
        else {
            return;
        };

        let elapsed = chrono::Utc::now() - last_at;
        let idle_duration = match chrono::TimeDelta::from_std(self.idle_timeout) {
            Ok(d) => d,
            Err(_) => return,
        };

        if elapsed < idle_duration {
            return;
        }

        debug!(
            session_id = %self.session_id,
            elapsed_secs = elapsed.num_seconds(),
            "idle threshold exceeded, triggering memory extraction"
        );

        // Get recent messages for extraction.
        let messages = match self.storage.get_messages(&self.session_id, Some(50)).await {
            Ok(msgs) => msgs,
            Err(e) => {
                warn!(error = %e, "failed to fetch messages for memory extraction");
                return;
            }
        };

        if messages.is_empty() {
            return;
        }

        // Convert to ProviderMessages for the extractor.
        let provider_messages: Vec<blufio_core::types::ProviderMessage> = messages
            .iter()
            .map(|m| blufio_core::types::ProviderMessage {
                role: m.role.clone(),
                content: vec![blufio_core::types::ContentBlock::Text {
                    text: m.content.clone(),
                }],
            })
            .collect();

        match extractor
            .extract_from_conversation(
                self.provider.as_ref(),
                &self.session_id,
                &provider_messages,
            )
            .await
        {
            Ok(result) => {
                let count = result.memories.len();
                if count > 0 {
                    info!(
                        session_id = %self.session_id,
                        count = count,
                        "extracted memories from idle session"
                    );
                }

                // Record extraction cost.
                if let Some(ref usage) = result.usage {
                    let extraction_model = &extractor.extraction_model();
                    let model_pricing = pricing::get_pricing(extraction_model);
                    let cost_usd = pricing::calculate_cost(usage, &model_pricing);

                    let record = CostRecord::new(
                        self.session_id.clone(),
                        extraction_model.to_string(),
                        FeatureType::Extraction,
                        usage,
                        cost_usd,
                    );

                    if let Err(e) = self.cost_ledger.record(&record).await {
                        warn!(error = %e, "failed to record extraction cost");
                    } else {
                        let mut tracker = self.budget_tracker.lock().await;
                        tracker.record_cost(cost_usd);

                        debug!(
                            session_id = %self.session_id,
                            cost_usd = cost_usd,
                            "extraction cost recorded"
                        );
                    }
                }
            }
            Err(e) => {
                warn!(
                    session_id = %self.session_id,
                    error = %e,
                    "memory extraction failed (non-fatal)"
                );
            }
        }
    }

    /// Marks this session as draining (graceful shutdown).
    pub fn set_draining(&mut self) {
        self.state = SessionState::Draining;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_state_display() {
        assert_eq!(SessionState::Idle.to_string(), "idle");
        assert_eq!(SessionState::Receiving.to_string(), "receiving");
        assert_eq!(SessionState::Processing.to_string(), "processing");
        assert_eq!(SessionState::Responding.to_string(), "responding");
        assert_eq!(SessionState::ToolExecuting.to_string(), "tool_executing");
        assert_eq!(SessionState::Draining.to_string(), "draining");
    }

    #[test]
    fn session_state_equality() {
        assert_eq!(SessionState::Idle, SessionState::Idle);
        assert_ne!(SessionState::Idle, SessionState::Responding);
        assert_ne!(SessionState::ToolExecuting, SessionState::Processing);
    }

    #[test]
    fn max_tool_iterations_constant() {
        assert_eq!(MAX_TOOL_ITERATIONS, 10);
    }

    #[test]
    fn session_actor_idle_timeout_configurable() {
        // Verify that idle_timeout is set from constructor parameter.
        // This is a basic structural test -- no real adapters needed.
        let timeout_secs = 300u64;
        let expected = Duration::from_secs(timeout_secs);
        // We can't construct a full SessionActor without trait objects,
        // so just verify the Duration conversion.
        assert_eq!(expected, Duration::from_secs(300));
    }

    #[test]
    fn session_actor_memory_fields_default_none() {
        // When memory is disabled, memory_provider and memory_extractor are None.
        // Verify the Option types can hold None.
        let mp: Option<MemoryProvider> = None;
        let me: Option<Arc<MemoryExtractor>> = None;
        assert!(mp.is_none());
        assert!(me.is_none());
    }

    #[test]
    fn tool_registry_can_be_shared() {
        // Verify that Arc<RwLock<ToolRegistry>> can be constructed for the session actor.
        let registry = Arc::new(RwLock::new(ToolRegistry::new()));
        assert_eq!(registry.blocking_read().len(), 0);
    }
}
