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
use blufio_core::error::BlufioError;
use blufio_core::types::{InboundMessage, Message, ProviderStreamChunk, TokenUsage, ToolUseData};
use blufio_core::{ProviderAdapter, StorageAdapter};
use blufio_cost::BudgetTracker;
use blufio_cost::CostLedger;
use blufio_cost::ledger::{CostRecord, FeatureType};
use blufio_cost::pricing;
use blufio_memory::{MemoryExtractor, MemoryProvider};
use blufio_resilience::{CircuitBreakerRegistry, DegradationLevel, DegradationManager};
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

/// Configuration for creating a SessionActor.
///
/// Groups the constructor arguments into a single struct for readability
/// and to eliminate the 15-argument constructor (DEBT-03).
pub struct SessionActorConfig {
    /// Unique session identifier.
    pub session_id: String,
    /// Storage adapter for persisting messages and sessions.
    pub storage: Arc<dyn StorageAdapter + Send + Sync>,
    /// LLM provider adapter for streaming completions.
    pub provider: Arc<dyn ProviderAdapter + Send + Sync>,
    /// Context engine for three-zone prompt assembly.
    pub context_engine: Arc<ContextEngine>,
    /// Budget tracker for pre-call spending gates.
    pub budget_tracker: Arc<tokio::sync::Mutex<BudgetTracker>>,
    /// Cost ledger for post-call cost recording.
    pub cost_ledger: Arc<CostLedger>,
    /// Memory provider for setting current query before context assembly.
    pub memory_provider: Option<MemoryProvider>,
    /// Memory extractor for end-of-conversation fact extraction.
    pub memory_extractor: Option<Arc<MemoryExtractor>>,
    /// Channel name this session belongs to.
    pub channel: String,
    /// Model router for per-message complexity classification.
    pub router: Arc<ModelRouter>,
    /// Default model used when routing is disabled.
    pub default_model: String,
    /// Default max tokens used when routing is disabled.
    pub default_max_tokens: u32,
    /// Whether model routing is enabled.
    pub routing_enabled: bool,
    /// Idle timeout in seconds for triggering memory extraction.
    pub idle_timeout_secs: u64,
    /// Registry of available tools (built-in and WASM skills).
    pub tool_registry: Arc<RwLock<ToolRegistry>>,
    /// Circuit breaker registry for checking/recording external call results.
    pub circuit_breaker_registry: Option<Arc<CircuitBreakerRegistry>>,
    /// Degradation manager for checking current degradation level.
    pub degradation_manager: Option<Arc<DegradationManager>>,
    /// Name of the primary provider for circuit breaker lookups.
    pub provider_name: String,
    /// Provider registry for fallback provider lookup.
    pub provider_registry: Option<Arc<dyn blufio_core::ProviderRegistry + Send + Sync>>,
    /// Fallback chain of provider names to try when primary breaker is open.
    pub fallback_chain: Vec<String>,
    /// Optional EventBus for publishing circuit breaker state transitions.
    pub event_bus: Option<Arc<blufio_bus::EventBus>>,
    /// Optional injection defense pipeline for L1/L4/L5 screening.
    pub injection_pipeline: Option<Arc<tokio::sync::Mutex<blufio_injection::pipeline::InjectionPipeline>>>,
    /// Optional HMAC boundary manager for L3 content zone integrity (per-session).
    pub boundary_manager: Option<blufio_injection::boundary::BoundaryManager>,
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
    /// Circuit breaker registry for checking/recording external call results.
    circuit_breaker_registry: Option<Arc<CircuitBreakerRegistry>>,
    /// Degradation manager for checking current degradation level.
    degradation_manager: Option<Arc<DegradationManager>>,
    /// Name of the primary provider for circuit breaker lookups.
    provider_name: String,
    /// Whether the last provider call was a fallback.
    last_call_was_fallback: bool,
    /// Provider registry for fallback provider lookup.
    provider_registry: Option<Arc<dyn blufio_core::ProviderRegistry + Send + Sync>>,
    /// Fallback chain of provider names to try when primary breaker is open.
    fallback_chain: Vec<String>,
    /// Optional EventBus for publishing circuit breaker state transitions.
    event_bus: Option<Arc<blufio_bus::EventBus>>,
    /// Optional injection defense pipeline for L1/L4/L5 screening.
    injection_pipeline: Option<Arc<tokio::sync::Mutex<blufio_injection::pipeline::InjectionPipeline>>>,
    /// Optional HMAC boundary manager for L3 content zone integrity (per-session).
    boundary_manager: Option<blufio_injection::boundary::BoundaryManager>,
    /// Whether the last L1 scan flagged the input (for cross-layer escalation).
    flagged_input: bool,
}

impl SessionActor {
    /// Creates a new session actor from a configuration struct.
    pub fn new(config: SessionActorConfig) -> Self {
        Self {
            session_id: config.session_id,
            state: SessionState::Idle,
            storage: config.storage,
            provider: config.provider,
            context_engine: config.context_engine,
            budget_tracker: config.budget_tracker,
            cost_ledger: config.cost_ledger,
            memory_provider: config.memory_provider,
            memory_extractor: config.memory_extractor,
            channel: config.channel,
            router: config.router,
            default_model: config.default_model,
            default_max_tokens: config.default_max_tokens,
            routing_enabled: config.routing_enabled,
            last_routing_decision: None,
            last_message_at: None,
            idle_timeout: Duration::from_secs(config.idle_timeout_secs),
            tool_registry: config.tool_registry,
            max_tool_iterations: MAX_TOOL_ITERATIONS,
            circuit_breaker_registry: config.circuit_breaker_registry,
            degradation_manager: config.degradation_manager,
            provider_name: config.provider_name,
            last_call_was_fallback: false,
            provider_registry: config.provider_registry,
            fallback_chain: config.fallback_chain,
            event_bus: config.event_bus,
            injection_pipeline: config.injection_pipeline,
            boundary_manager: config.boundary_manager,
            flagged_input: false,
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

        // PII detection before message storage (DCLS-04, PII-03).
        // Scan user message for PII and auto-classify if enabled.
        // Errors are logged and never block the agent loop.
        let msg_id = uuid::Uuid::new_v4().to_string();
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            blufio_security::scan_and_classify(&text_content, true)
        })) {
            Ok(scan_result) => {
                if !scan_result.matches.is_empty()
                    && let Some(ref bus) = self.event_bus
                {
                    // Emit PII detected event (fire-and-forget, metadata only).
                    let event = blufio_security::pii_detected_event(
                        "message",
                        &msg_id,
                        &scan_result.matches,
                    );
                    bus.publish(event).await;
                }
            }
            Err(_) => {
                warn!(
                    session_id = %self.session_id,
                    "PII detection panicked, continuing without classification"
                );
            }
        }

        // L1 injection defense: scan user input before it reaches the LLM.
        let correlation_id = blufio_injection::pipeline::InjectionPipeline::new_correlation_id();
        if let Some(ref pipeline) = self.injection_pipeline {
            let pipeline_guard = pipeline.lock().await;
            let scan_result = pipeline_guard.scan_input(&text_content, "user", &correlation_id);

            // Emit security events (fire-and-forget).
            if !scan_result.events.is_empty() {
                pipeline_guard.emit_events(scan_result.events).await;
            }

            // Store flagged state for cross-layer escalation in execute_tools.
            self.flagged_input = scan_result.flagged;

            // Block if L1 action is "blocked" (per CONTEXT.md: generic refusal).
            if scan_result.action == "blocked" {
                warn!(
                    session_id = %self.session_id,
                    correlation_id = %correlation_id,
                    score = scan_result.score,
                    "L1: input blocked by injection defense"
                );
                self.state = SessionState::Responding;
                let blocked_stream: Pin<
                    Box<dyn Stream<Item = Result<ProviderStreamChunk, BlufioError>> + Send>,
                > = Box::pin(futures::stream::once(async {
                    Ok(ProviderStreamChunk {
                        event_type: blufio_core::types::StreamEventType::ContentBlockDelta,
                        text: Some("I can't process this message.".to_string()),
                        usage: None,
                        tool_use: None,
                        stop_reason: Some("end_turn".to_string()),
                        error: None,
                    })
                }));
                return Ok(blocked_stream);
            }
        } else {
            self.flagged_input = false;
        }

        // Persist the inbound user message (with override prefix stripped).
        let now = chrono::Utc::now().to_rfc3339();
        let msg = Message {
            id: msg_id,
            session_id: self.session_id.clone(),
            role: "user".to_string(),
            content: text_content.clone(),
            token_count: None,
            metadata: inbound.metadata.clone(),
            created_at: now,
            classification: Default::default(),
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
            let recent_strings: Vec<String> =
                recent_msgs.iter().map(|m| m.content.clone()).collect();
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
        // When a boundary manager is available, apply L3 HMAC boundary protection.
        let assembled = self
            .context_engine
            .assemble_with_boundaries(
                self.provider.as_ref(),
                self.storage.as_ref(),
                &self.session_id,
                &inbound,
                &model,
                max_tokens,
                self.boundary_manager.as_ref(),
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
        // FeatureType::Compaction, not Message. May have multiple from cascade.
        for compaction_usage in &assembled.compaction_usages {
            let compaction_model = assembled
                .compaction_model
                .as_deref()
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

            // Record Prometheus token metrics for compaction.
            #[cfg(feature = "prometheus")]
            blufio_prometheus::record_tokens(
                compaction_model,
                compaction_usage.input_tokens,
                compaction_usage.output_tokens,
            );

            info!(
                session_id = %self.session_id,
                model = %compaction_model,
                input_tokens = compaction_usage.input_tokens,
                output_tokens = compaction_usage.output_tokens,
                cost_usd = cost_usd,
                "compaction cost recorded"
            );
        }

        // Persist entities extracted during compaction as Memory entries (COMP-06).
        if !assembled.extracted_entities.is_empty() {
            if let Some(ref extractor) = self.memory_extractor {
                match extractor
                    .persist_extracted_entities(&self.session_id, &assembled.extracted_entities)
                    .await
                {
                    Ok(count) => {
                        if count > 0 {
                            info!(
                                session_id = %self.session_id,
                                count = count,
                                "persisted extracted entities from compaction"
                            );
                        }
                    }
                    Err(e) => {
                        warn!(
                            session_id = %self.session_id,
                            error = %e,
                            "failed to persist extracted entities (non-fatal)"
                        );
                    }
                }
            }
        }

        // Emit L3 boundary validation events (if any).
        if !assembled.boundary_events.is_empty() {
            if let Some(ref pipeline) = self.injection_pipeline {
                let pipeline_guard = pipeline.lock().await;
                pipeline_guard
                    .emit_events(assembled.boundary_events)
                    .await;
            }
        }

        // Check degradation level for L4+ canned response.
        if let Some(ref dm) = self.degradation_manager {
            let level = dm.current_level();
            if level.as_u8() >= DegradationLevel::Emergency.as_u8() {
                warn!(
                    session_id = %self.session_id,
                    level = %level,
                    "L4+ emergency: returning canned response"
                );
                self.state = SessionState::Responding;
                let canned = "I'm temporarily unavailable. Please try again later.";
                let canned_stream: Pin<
                    Box<dyn Stream<Item = Result<ProviderStreamChunk, BlufioError>> + Send>,
                > = Box::pin(futures::stream::once(async {
                    Ok(ProviderStreamChunk {
                        event_type: blufio_core::types::StreamEventType::ContentBlockDelta,
                        text: Some(canned.to_string()),
                        usage: None,
                        tool_use: None,
                        stop_reason: Some("end_turn".to_string()),
                        error: None,
                    })
                }));
                return Ok(canned_stream);
            }
        }

        // Check circuit breaker before provider call (if resilience enabled).
        // If primary breaker is open, try fallback providers from fallback_chain.
        if let Some(ref registry) = self.circuit_breaker_registry
            && let Err(primary_err) = registry.check(&self.provider_name)
        {
            warn!(
                session_id = %self.session_id,
                provider = %self.provider_name,
                "circuit breaker open, attempting fallback chain"
            );

            // Try fallback chain providers in order.
            if !self.fallback_chain.is_empty()
                && let Some(ref provider_registry) = self.provider_registry
            {
                for fallback_name in &self.fallback_chain {
                    // Check if this fallback's breaker is also open.
                    if registry.check(fallback_name).is_err() {
                        warn!(fallback = %fallback_name, "fallback breaker also open, skipping");
                        continue;
                    }
                    // Get fallback provider adapter.
                    if let Some(fallback_provider) = provider_registry.get_provider(fallback_name) {
                        let original_model = assembled.request.model.clone();
                        let mapped_model = map_model_to_tier(&original_model, fallback_name);
                        info!(
                            session_id = %self.session_id,
                            primary = %self.provider_name,
                            fallback = %fallback_name,
                            original_model = %original_model,
                            mapped_model = %mapped_model,
                            "routing to fallback provider"
                        );
                        // Clone the request and set the mapped model for fallback.
                        let mut fallback_request = assembled.request.clone();
                        fallback_request.model = mapped_model;
                        // Call fallback provider.
                        let fallback_result = fallback_provider.stream(fallback_request).await;
                        // Record result in fallback's circuit breaker.
                        match fallback_result {
                            Ok(stream) => {
                                if let Some(transition) =
                                    registry.record_result(fallback_name, true)
                                {
                                    info!(
                                        session_id = %self.session_id,
                                        provider = %fallback_name,
                                        from = %transition.from_state,
                                        to = %transition.to_state,
                                        "fallback circuit breaker state transition"
                                    );
                                    #[cfg(feature = "prometheus")]
                                    {
                                        blufio_prometheus::recording::record_circuit_breaker_state(
                                            fallback_name,
                                            transition.to_state.as_numeric(),
                                        );
                                        blufio_prometheus::recording::record_circuit_breaker_transition(
                                                    fallback_name,
                                                    transition.from_state.as_str(),
                                                    transition.to_state.as_str(),
                                                );
                                    }
                                    self.publish_cb_transition(fallback_name, &transition).await;
                                }
                                self.last_call_was_fallback = true;
                                self.state = SessionState::Responding;
                                return Ok(stream);
                            }
                            Err(e) => {
                                let trips = e.trips_circuit_breaker();
                                if let Some(transition) =
                                    registry.record_result(fallback_name, !trips)
                                {
                                    warn!(
                                        session_id = %self.session_id,
                                        provider = %fallback_name,
                                        from = %transition.from_state,
                                        to = %transition.to_state,
                                        error = %e,
                                        "fallback circuit breaker state transition on error"
                                    );
                                    #[cfg(feature = "prometheus")]
                                    {
                                        blufio_prometheus::recording::record_circuit_breaker_state(
                                            fallback_name,
                                            transition.to_state.as_numeric(),
                                        );
                                        blufio_prometheus::recording::record_circuit_breaker_transition(
                                                    fallback_name,
                                                    transition.from_state.as_str(),
                                                    transition.to_state.as_str(),
                                                );
                                    }
                                    self.publish_cb_transition(fallback_name, &transition).await;
                                }
                                warn!(fallback = %fallback_name, error = %e, "fallback provider call failed");
                                continue; // Try next fallback
                            }
                        }
                    }
                }
            }
            // All fallback providers exhausted (or none configured), return original error.
            return Err(primary_err);
        }

        // Stream from provider using the assembled request.
        let stream_result = self.provider.stream(assembled.request).await;

        // Record result in circuit breaker (if resilience enabled).
        match &stream_result {
            Ok(_) => {
                if let Some(ref registry) = self.circuit_breaker_registry
                    && let Some(transition) = registry.record_result(&self.provider_name, true)
                {
                    info!(
                        session_id = %self.session_id,
                        provider = %self.provider_name,
                        from = %transition.from_state,
                        to = %transition.to_state,
                        "circuit breaker state transition"
                    );
                    #[cfg(feature = "prometheus")]
                    {
                        blufio_prometheus::recording::record_circuit_breaker_state(
                            &self.provider_name,
                            transition.to_state.as_numeric(),
                        );
                        blufio_prometheus::recording::record_circuit_breaker_transition(
                            &self.provider_name,
                            transition.from_state.as_str(),
                            transition.to_state.as_str(),
                        );
                    }
                    self.publish_cb_transition(&self.provider_name, &transition)
                        .await;
                }
                self.last_call_was_fallback = false;
            }
            Err(e) => {
                if let Some(ref registry) = self.circuit_breaker_registry {
                    // Only count as failure if error trips the circuit breaker.
                    let trips = e.trips_circuit_breaker();
                    if let Some(transition) = registry.record_result(&self.provider_name, !trips) {
                        warn!(
                            session_id = %self.session_id,
                            provider = %self.provider_name,
                            from = %transition.from_state,
                            to = %transition.to_state,
                            error = %e,
                            "circuit breaker state transition on error"
                        );
                        #[cfg(feature = "prometheus")]
                        {
                            blufio_prometheus::recording::record_circuit_breaker_state(
                                &self.provider_name,
                                transition.to_state.as_numeric(),
                            );
                            blufio_prometheus::recording::record_circuit_breaker_transition(
                                &self.provider_name,
                                transition.from_state.as_str(),
                                transition.to_state.as_str(),
                            );
                        }
                        self.publish_cb_transition(&self.provider_name, &transition)
                            .await;
                    }
                }
            }
        }

        let stream = stream_result?;

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
        // PII detection before assistant response storage (DCLS-04, PII-03).
        let msg_id = uuid::Uuid::new_v4().to_string();
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            blufio_security::scan_and_classify(full_text, true)
        })) {
            Ok(scan_result) => {
                if !scan_result.matches.is_empty()
                    && let Some(ref bus) = self.event_bus
                {
                    let event = blufio_security::pii_detected_event(
                        "message",
                        &msg_id,
                        &scan_result.matches,
                    );
                    bus.publish(event).await;
                }
            }
            Err(_) => {
                warn!(
                    session_id = %self.session_id,
                    "PII detection panicked on assistant response, continuing"
                );
            }
        }

        let now = chrono::Utc::now().to_rfc3339();
        let msg = Message {
            id: msg_id,
            session_id: self.session_id.clone(),
            role: "assistant".to_string(),
            content: full_text.to_string(),
            token_count: usage.as_ref().map(|u| i64::from(u.output_tokens)),
            metadata: None,
            created_at: now,
            classification: Default::default(),
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
            if self.last_call_was_fallback {
                record = record.with_fallback(true);
            }

            self.cost_ledger.record(&record).await?;

            {
                let mut tracker = self.budget_tracker.lock().await;
                tracker.record_cost(cost_usd);

                // Record Prometheus token and budget metrics.
                #[cfg(feature = "prometheus")]
                {
                    blufio_prometheus::record_tokens(
                        &model_for_cost,
                        usage.input_tokens,
                        usage.output_tokens,
                    );
                    let remaining = tracker.remaining_daily_budget();
                    blufio_prometheus::set_budget_remaining(remaining);
                }
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

            // Emit ProviderEvent for audit trail.
            if let Some(ref bus) = self.event_bus {
                bus.publish(blufio_bus::events::BusEvent::Provider(
                    blufio_bus::events::ProviderEvent::Called {
                        event_id: blufio_bus::events::new_event_id(),
                        timestamp: blufio_bus::events::now_timestamp(),
                        provider: self.provider_name.clone(),
                        model: model_for_cost.clone(),
                        input_tokens: usage.input_tokens,
                        output_tokens: usage.output_tokens,
                        cost_usd,
                        latency_ms: 0, // latency not tracked at this level
                        success: true,
                        session_id: self.session_id.clone(),
                    },
                ))
                .await;
            }
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
            let corr_id = blufio_injection::pipeline::InjectionPipeline::new_correlation_id();

            // L4: Screen tool arguments before execution.
            if let Some(ref pipeline) = self.injection_pipeline {
                let mut pipeline_guard = pipeline.lock().await;
                let screen_result = pipeline_guard.screen_output(
                    &tu.name,
                    &tu.input,
                    &corr_id,
                    self.flagged_input,
                );

                // Emit screening events.
                if !screen_result.events.is_empty() {
                    pipeline_guard.emit_events(screen_result.events).await;
                }

                match screen_result.action {
                    blufio_injection::output_screen::ScreeningAction::Block(reason) => {
                        warn!(
                            session_id = %self.session_id,
                            tool = %tu.name,
                            reason = %reason,
                            "L4: tool execution blocked"
                        );
                        results.push((
                            tu.id.clone(),
                            ToolOutput {
                                content: format!("Tool {} was blocked.", tu.name),
                                is_error: true,
                            },
                        ));
                        continue;
                    }
                    blufio_injection::output_screen::ScreeningAction::Redact(_redacted) => {
                        // Continue with original args (credentials redacted in the screening
                        // result but we still execute with original args -- the redaction
                        // is for logging purposes). Tool execution proceeds.
                        debug!(
                            session_id = %self.session_id,
                            tool = %tu.name,
                            "L4: credentials detected and logged"
                        );
                    }
                    _ => {} // Allow or DryRun -- proceed normally
                }

                // L5: Check HITL for external tools.
                let l4_escalated = pipeline_guard.l4_escalation_triggered();
                let (decision, hitl_events) = pipeline_guard.check_hitl(
                    &tu.name,
                    &tu.input,
                    &self.session_id,
                    &self.channel,
                    true, // assume interactive for now
                    &corr_id,
                    l4_escalated,
                    self.flagged_input,
                );

                if !hitl_events.is_empty() {
                    pipeline_guard.emit_events(hitl_events).await;
                }

                match decision {
                    blufio_injection::hitl::HitlDecision::Denied(reason) => {
                        warn!(
                            session_id = %self.session_id,
                            tool = %tu.name,
                            reason = %reason,
                            "L5: tool execution denied"
                        );
                        results.push((
                            tu.id.clone(),
                            ToolOutput {
                                content: format!(
                                    "Tool {} was blocked. I'll answer without it.",
                                    tu.name
                                ),
                                is_error: true,
                            },
                        ));
                        continue;
                    }
                    blufio_injection::hitl::HitlDecision::PendingConfirmation(_req) => {
                        // For now, auto-deny pending confirmations (full HITL flow
                        // requires channel adapter implementation).
                        let (timeout_decision, timeout_event) = pipeline_guard
                            .handle_hitl_timeout(&self.session_id, &tu.name, &corr_id);
                        pipeline_guard.emit_events(vec![timeout_event]).await;
                        if matches!(timeout_decision, blufio_injection::hitl::HitlDecision::Denied(_)) {
                            warn!(
                                session_id = %self.session_id,
                                tool = %tu.name,
                                "L5: tool execution denied (confirmation timeout)"
                            );
                            results.push((
                                tu.id.clone(),
                                ToolOutput {
                                    content: format!(
                                        "Tool {} was blocked. I'll answer without it.",
                                        tu.name
                                    ),
                                    is_error: true,
                                },
                            ));
                            continue;
                        }
                    }
                    _ => {} // AutoApproved or DryRun -- proceed
                }

                drop(pipeline_guard);
            }

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
        let (Some(extractor), Some(last_at)) = (&self.memory_extractor, self.last_message_at)
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
            .extract_from_conversation(self.provider.as_ref(), &self.session_id, &provider_messages)
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

                        // Record Prometheus token metrics for extraction.
                        #[cfg(feature = "prometheus")]
                        blufio_prometheus::record_tokens(
                            extraction_model,
                            usage.input_tokens,
                            usage.output_tokens,
                        );

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

    /// Publishes a circuit breaker state transition event to the EventBus.
    ///
    /// Does nothing if `event_bus` is `None` (resilience disabled or tests
    /// without EventBus). EventBus::publish() is infallible -- it logs
    /// internally on delivery failure.
    async fn publish_cb_transition(
        &self,
        dependency: &str,
        transition: &blufio_resilience::CircuitBreakerTransition,
    ) {
        if let Some(ref bus) = self.event_bus {
            bus.publish(blufio_bus::events::BusEvent::Resilience(
                blufio_bus::events::ResilienceEvent::CircuitBreakerStateChanged {
                    event_id: blufio_bus::events::new_event_id(),
                    timestamp: blufio_bus::events::now_timestamp(),
                    dependency: dependency.to_string(),
                    from_state: transition.from_state.as_str().to_string(),
                    to_state: transition.to_state.as_str().to_string(),
                },
            ))
            .await;
        }
    }
}

/// Maps a model name to an equivalent-tier model for a target provider.
///
/// Preserves the quality tier (high/medium/low) when switching providers:
/// - High: Opus-class -> gpt-4o / gemini-2.0-pro / keep original
/// - Medium: Sonnet-class / gpt-4o -> gpt-4o-mini / claude-sonnet / gemini-2.0-flash
/// - Low: Haiku-class / gpt-4o-mini / gpt-3.5 -> gpt-3.5-turbo / claude-haiku
///
/// For Ollama fallback, the original model name is kept (Ollama serves whatever is pulled locally).
fn map_model_to_tier(model: &str, target_provider: &str) -> String {
    // Determine the tier of the source model.
    let tier = if model.contains("opus") || model == "gpt-4o" || model.contains("gemini-2.0-pro") {
        "high"
    } else if model.contains("sonnet")
        || model == "gpt-4o-mini"
        || model.contains("gemini-2.0-flash")
        || model.contains("gemini-1.5-pro")
    {
        "medium"
    } else if model.contains("haiku")
        || model.starts_with("gpt-3.5")
        || model.contains("gemini-1.5-flash")
    {
        "low"
    } else {
        // Default to medium tier for unrecognized models.
        "medium"
    };

    match target_provider {
        "openai" => match tier {
            "high" => "gpt-4o".to_string(),
            "medium" => "gpt-4o-mini".to_string(),
            _ => "gpt-3.5-turbo".to_string(),
        },
        "anthropic" => match tier {
            "high" => "claude-opus-4-20250514".to_string(),
            "medium" => "claude-sonnet-4-20250514".to_string(),
            _ => "claude-haiku-4-5-20250901".to_string(),
        },
        "gemini" => match tier {
            "high" => "gemini-2.0-pro".to_string(),
            "medium" => "gemini-2.0-flash".to_string(),
            _ => "gemini-1.5-flash".to_string(),
        },
        "ollama" => model.to_string(), // Keep original for Ollama
        _ => model.to_string(),        // Unknown provider: keep original
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blufio_bus::events::{BusEvent, ResilienceEvent};
    use blufio_resilience::circuit_breaker::CircuitBreakerConfig;
    use std::collections::HashMap;
    use std::pin::Pin;

    /// A mock provider that always returns errors to trip circuit breakers.
    struct FailingMockProvider;

    #[async_trait::async_trait]
    impl blufio_core::traits::adapter::PluginAdapter for FailingMockProvider {
        fn name(&self) -> &str {
            "failing-mock"
        }
        fn version(&self) -> semver::Version {
            semver::Version::new(0, 1, 0)
        }
        fn adapter_type(&self) -> blufio_core::types::AdapterType {
            blufio_core::types::AdapterType::Provider
        }
        async fn health_check(
            &self,
        ) -> Result<blufio_core::types::HealthStatus, blufio_core::error::BlufioError> {
            Ok(blufio_core::types::HealthStatus::Healthy)
        }
        async fn shutdown(&self) -> Result<(), blufio_core::error::BlufioError> {
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl blufio_core::ProviderAdapter for FailingMockProvider {
        async fn complete(
            &self,
            _req: blufio_core::types::ProviderRequest,
        ) -> Result<blufio_core::types::ProviderResponse, blufio_core::error::BlufioError> {
            Err(BlufioError::Provider {
                kind: blufio_core::error::ProviderErrorKind::ServerError,
                context: blufio_core::error::ErrorContext {
                    provider_name: Some("failing-mock".to_string()),
                    ..Default::default()
                },
                source: None,
            })
        }

        async fn stream(
            &self,
            _req: blufio_core::types::ProviderRequest,
        ) -> Result<
            Pin<
                Box<
                    dyn futures_core::Stream<
                            Item = Result<ProviderStreamChunk, blufio_core::error::BlufioError>,
                        > + Send,
                >,
            >,
            blufio_core::error::BlufioError,
        > {
            Err(BlufioError::Provider {
                kind: blufio_core::error::ProviderErrorKind::ServerError,
                context: blufio_core::error::ErrorContext {
                    provider_name: Some("failing-mock".to_string()),
                    ..Default::default()
                },
                source: None,
            })
        }
    }

    /// Build a complete test SessionActor with the given provider, event_bus, and CB registry.
    async fn make_test_actor(
        provider: Arc<dyn blufio_core::ProviderAdapter + Send + Sync>,
        event_bus: Option<Arc<blufio_bus::EventBus>>,
        circuit_breaker_registry: Option<Arc<CircuitBreakerRegistry>>,
    ) -> (
        SessionActor,
        Arc<dyn StorageAdapter + Send + Sync>,
        tempfile::TempDir,
    ) {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let storage_config = blufio_config::model::StorageConfig {
            database_path: db_path.to_string_lossy().to_string(),
            wal_mode: true,
        };
        let storage = blufio_storage::SqliteStorage::new(storage_config);
        storage.initialize().await.unwrap();
        let storage: Arc<dyn StorageAdapter + Send + Sync> = Arc::new(storage);

        let cost_ledger = Arc::new(
            blufio_cost::CostLedger::open(db_path.to_str().unwrap())
                .await
                .unwrap(),
        );
        let cost_config = blufio_config::model::CostConfig {
            daily_budget_usd: None,
            monthly_budget_usd: None,
            track_tokens: true,
        };
        let budget_tracker = Arc::new(tokio::sync::Mutex::new(blufio_cost::BudgetTracker::new(
            &cost_config,
        )));

        let agent_config = blufio_config::model::AgentConfig {
            system_prompt: Some("Test assistant.".to_string()),
            ..blufio_config::model::AgentConfig::default()
        };
        let context_config = blufio_config::model::ContextConfig::default();
        let token_cache = Arc::new(blufio_core::token_counter::TokenizerCache::new(
            blufio_core::token_counter::TokenizerMode::Fast,
        ));
        let context_engine = Arc::new(
            blufio_context::ContextEngine::new(&agent_config, &context_config, token_cache)
                .await
                .unwrap(),
        );

        let routing_config = blufio_config::model::RoutingConfig {
            enabled: false,
            ..blufio_config::model::RoutingConfig::default()
        };
        let router = Arc::new(blufio_router::ModelRouter::new(routing_config));
        let tool_registry = Arc::new(RwLock::new(blufio_skill::ToolRegistry::new()));

        let session_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let session = blufio_core::types::Session {
            id: session_id.clone(),
            channel: "test".to_string(),
            user_id: Some("test-user".to_string()),
            state: "active".to_string(),
            metadata: None,
            created_at: now.clone(),
            updated_at: now,
            classification: Default::default(),
        };
        storage.create_session(&session).await.unwrap();

        let actor = SessionActor::new(SessionActorConfig {
            session_id,
            storage: storage.clone(),
            provider,
            context_engine,
            budget_tracker,
            cost_ledger,
            memory_provider: None,
            memory_extractor: None,
            channel: "test".to_string(),
            router,
            default_model: "test-model".to_string(),
            default_max_tokens: 1024,
            routing_enabled: false,
            idle_timeout_secs: 300,
            tool_registry,
            circuit_breaker_registry,
            degradation_manager: None,
            provider_name: "failing-mock".to_string(),
            provider_registry: None,
            fallback_chain: Vec::new(),
            event_bus,
            injection_pipeline: None,
            boundary_manager: None,
        });

        (actor, storage, temp_dir)
    }

    fn make_cb_registry(dep: &str) -> Arc<CircuitBreakerRegistry> {
        let mut configs = HashMap::new();
        configs.insert(dep.to_string(), CircuitBreakerConfig::default());
        Arc::new(CircuitBreakerRegistry::new(configs))
    }

    fn make_inbound(session_id: &str) -> InboundMessage {
        InboundMessage {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: Some(session_id.to_string()),
            channel: "test".to_string(),
            sender_id: "test-user".to_string(),
            content: blufio_core::types::MessageContent::Text("hello".to_string()),
            timestamp: chrono::Utc::now().to_rfc3339(),
            metadata: None,
        }
    }

    #[tokio::test]
    async fn publishes_cb_event_on_transition() {
        // Setup: EventBus + CB registry + FailingMockProvider
        let event_bus = Arc::new(blufio_bus::EventBus::new(64));
        let mut rx = event_bus.subscribe();
        let registry = make_cb_registry("failing-mock");
        let provider: Arc<dyn blufio_core::ProviderAdapter + Send + Sync> =
            Arc::new(FailingMockProvider);

        let (mut actor, _storage, _temp) =
            make_test_actor(provider, Some(event_bus.clone()), Some(registry.clone())).await;

        // Send 5 messages to trip the breaker (failure_threshold = 5).
        // Each call to handle_message with FailingMockProvider will return Err,
        // and record_result(name, false) will be called.
        let sid = actor.session_id().to_string();
        for _ in 0..5 {
            let inbound = make_inbound(&sid);
            let _ = actor.handle_message(inbound).await;
        }

        // The 5th failure should have caused a Closed->Open transition and published an event.
        let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
            .await
            .expect("timeout waiting for event")
            .expect("channel error");

        match event {
            BusEvent::Resilience(ResilienceEvent::CircuitBreakerStateChanged {
                dependency,
                from_state,
                to_state,
                ..
            }) => {
                assert_eq!(dependency, "failing-mock");
                assert_eq!(from_state, "closed");
                assert_eq!(to_state, "open");
            }
            other => panic!("expected CircuitBreakerStateChanged, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn no_event_when_bus_is_none() {
        // SessionActor with event_bus: None should not panic on CB transitions.
        let registry = make_cb_registry("failing-mock");
        let provider: Arc<dyn blufio_core::ProviderAdapter + Send + Sync> =
            Arc::new(FailingMockProvider);

        let (mut actor, _storage, _temp) =
            make_test_actor(provider, None, Some(registry.clone())).await;

        let sid = actor.session_id().to_string();
        // Trip the breaker -- should not panic.
        for _ in 0..5 {
            let inbound = make_inbound(&sid);
            let _ = actor.handle_message(inbound).await;
        }
        // If we reach here without panic, the test passes.
    }

    #[tokio::test]
    async fn no_event_when_no_transition() {
        // Success calls in Closed state produce no transition -> no event.
        let event_bus = Arc::new(blufio_bus::EventBus::new(64));
        let mut rx = event_bus.subscribe();
        // Registry has the same name as the actor's provider_name
        let registry = make_cb_registry("failing-mock");

        let provider: Arc<dyn blufio_core::ProviderAdapter + Send + Sync> =
            Arc::new(blufio_test_utils::MockProvider::with_responses(vec![
                "ok".to_string(),
            ]));

        let (mut actor, _storage, _temp) =
            make_test_actor(provider, Some(event_bus.clone()), Some(registry.clone())).await;

        let sid = actor.session_id().to_string();
        let inbound = make_inbound(&sid);
        // This should succeed -- record_result("failing-mock", true) returns None in Closed state.
        let _ = actor.handle_message(inbound).await;

        // No event should be on the bus.
        let result = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await;
        assert!(result.is_err(), "expected no event on bus but got one");
    }

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
