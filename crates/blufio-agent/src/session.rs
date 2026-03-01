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

use blufio_context::ContextEngine;
use blufio_cost::ledger::{CostRecord, FeatureType};
use blufio_cost::pricing;
use blufio_cost::BudgetTracker;
use blufio_cost::CostLedger;
use blufio_core::error::BlufioError;
use blufio_core::types::{
    InboundMessage, Message, ProviderStreamChunk, TokenUsage,
};
use blufio_core::{ProviderAdapter, StorageAdapter};
use futures::Stream;
use tracing::{debug, info};

use crate::context;

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
/// - Calling the LLM provider
/// - Recording costs (both message and compaction) after responses
/// - Persisting assistant responses
pub struct SessionActor {
    session_id: String,
    state: SessionState,
    storage: Arc<dyn StorageAdapter + Send + Sync>,
    provider: Arc<dyn ProviderAdapter + Send + Sync>,
    context_engine: Arc<ContextEngine>,
    budget_tracker: Arc<tokio::sync::Mutex<BudgetTracker>>,
    cost_ledger: Arc<CostLedger>,
    channel: String,
    model: String,
    max_tokens: u32,
}

impl SessionActor {
    /// Creates a new session actor with context engine and cost tracking.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: String,
        storage: Arc<dyn StorageAdapter + Send + Sync>,
        provider: Arc<dyn ProviderAdapter + Send + Sync>,
        context_engine: Arc<ContextEngine>,
        budget_tracker: Arc<tokio::sync::Mutex<BudgetTracker>>,
        cost_ledger: Arc<CostLedger>,
        channel: String,
        model: String,
        max_tokens: u32,
    ) -> Self {
        Self {
            session_id,
            state: SessionState::Idle,
            storage,
            provider,
            context_engine,
            budget_tracker,
            cost_ledger,
            channel,
            model,
            max_tokens,
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

    /// Handles an inbound message: persists it, checks budget, assembles context,
    /// records compaction costs, and starts streaming.
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

        // Persist the inbound user message.
        let text_content = context::message_content_to_text(&inbound.content);
        let now = chrono::Utc::now().to_rfc3339();
        let msg = Message {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: self.session_id.clone(),
            role: "user".to_string(),
            content: text_content,
            token_count: None,
            metadata: inbound.metadata.clone(),
            created_at: now,
        };
        self.storage.insert_message(&msg).await?;

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

        // Assemble context using the three-zone context engine.
        let assembled = self.context_engine.assemble(
            self.provider.as_ref(),
            self.storage.as_ref(),
            &self.session_id,
            &inbound,
            &self.model,
            self.max_tokens,
        )
        .await?;

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
        if let Some(ref usage) = usage {
            let model_pricing = pricing::get_pricing(&self.model);
            let cost_usd = pricing::calculate_cost(usage, &model_pricing);

            let record = CostRecord::new(
                self.session_id.clone(),
                self.model.clone(),
                FeatureType::Message,
                usage,
                cost_usd,
            );

            self.cost_ledger.record(&record).await?;

            {
                let mut tracker = self.budget_tracker.lock().await;
                tracker.record_cost(cost_usd);
            }

            info!(
                session_id = %self.session_id,
                model = %self.model,
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
        assert_eq!(SessionState::Draining.to_string(), "draining");
    }

    #[test]
    fn session_state_equality() {
        assert_eq!(SessionState::Idle, SessionState::Idle);
        assert_ne!(SessionState::Idle, SessionState::Responding);
    }
}
