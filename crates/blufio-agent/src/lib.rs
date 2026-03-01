// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Agent loop and session management for the Blufio agent framework.
//!
//! The [`AgentLoop`] is the central coordinator that:
//! - Receives messages from a channel adapter
//! - Routes them to per-session actors
//! - Streams LLM responses back through the channel
//! - Enforces budget caps and records costs
//! - Handles graceful shutdown

pub mod context;
pub mod session;
pub mod shutdown;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use blufio_config::model::BlufioConfig;
use blufio_context::ContextEngine;
use blufio_cost::{BudgetTracker, CostLedger};
use blufio_core::error::BlufioError;
use blufio_core::types::{
    InboundMessage, OutboundMessage, Session, StreamEventType, TokenUsage,
};
use blufio_core::{ChannelAdapter, ProviderAdapter, StorageAdapter};
use blufio_memory::{MemoryExtractor, MemoryProvider};
use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::session::SessionActor;

/// The main agent loop that coordinates message flow between channel, provider, and storage.
///
/// Receives inbound messages from a channel adapter, routes them to per-session
/// actors, streams LLM responses back, and manages session lifecycle.
pub struct AgentLoop {
    channel: Box<dyn ChannelAdapter + Send + Sync>,
    provider: Arc<dyn ProviderAdapter + Send + Sync>,
    storage: Arc<dyn StorageAdapter + Send + Sync>,
    context_engine: Arc<ContextEngine>,
    cost_ledger: Arc<CostLedger>,
    budget_tracker: Arc<tokio::sync::Mutex<BudgetTracker>>,
    /// Memory provider for setting current query before context assembly.
    memory_provider: Option<MemoryProvider>,
    /// Memory extractor for end-of-conversation fact extraction.
    memory_extractor: Option<Arc<MemoryExtractor>>,
    config: BlufioConfig,
    sessions: HashMap<String, SessionActor>,
}

impl AgentLoop {
    /// Creates a new agent loop with the given adapters, context engine, and cost components.
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        channel: Box<dyn ChannelAdapter + Send + Sync>,
        provider: Arc<dyn ProviderAdapter + Send + Sync>,
        storage: Arc<dyn StorageAdapter + Send + Sync>,
        context_engine: Arc<ContextEngine>,
        cost_ledger: Arc<CostLedger>,
        budget_tracker: Arc<tokio::sync::Mutex<BudgetTracker>>,
        memory_provider: Option<MemoryProvider>,
        memory_extractor: Option<Arc<MemoryExtractor>>,
        config: BlufioConfig,
    ) -> Result<Self, BlufioError> {
        info!(
            agent_name = config.agent.name.as_str(),
            "agent loop initialized"
        );

        Ok(Self {
            channel,
            provider,
            storage,
            context_engine,
            cost_ledger,
            budget_tracker,
            memory_provider,
            memory_extractor,
            config,
            sessions: HashMap::new(),
        })
    }

    /// Runs the main agent loop until the cancellation token is triggered.
    ///
    /// The loop:
    /// 1. Waits for inbound messages from the channel
    /// 2. Routes each message to a session actor
    /// 3. Streams the LLM response back to the channel
    /// 4. On cancellation, drains active sessions before exiting
    pub async fn run(&mut self, cancel: CancellationToken) -> Result<(), BlufioError> {
        info!("agent loop running");

        loop {
            tokio::select! {
                msg = self.channel.receive() => {
                    match msg {
                        Ok(inbound) => {
                            if let Err(e) = self.handle_inbound(inbound).await {
                                error!(error = %e, "failed to handle inbound message");
                            }
                        }
                        Err(e) => {
                            error!(error = %e, "channel receive error");
                            // If the channel is closed, break out of the loop.
                            if e.to_string().contains("closed") {
                                break;
                            }
                        }
                    }
                }
                _ = cancel.cancelled() => {
                    info!("shutdown signal received, stopping agent loop");
                    break;
                }
            }
        }

        // Drain active sessions.
        shutdown::drain_sessions(&self.sessions, Duration::from_secs(30)).await;

        // Close storage.
        self.storage.close().await?;

        info!("agent loop stopped");
        Ok(())
    }

    /// Handles a single inbound message: resolves session, calls LLM, sends response.
    ///
    /// If a `BudgetExhausted` error is returned from the session actor, sends
    /// the budget message to the user instead of logging it as an error.
    async fn handle_inbound(&mut self, inbound: InboundMessage) -> Result<(), BlufioError> {
        let sender_id = inbound.sender_id.clone();
        let channel_name = inbound.channel.clone();
        let metadata = inbound.metadata.clone();

        debug!(
            sender_id = sender_id.as_str(),
            channel = channel_name.as_str(),
            "handling inbound message"
        );

        // Resolve or create session.
        let session_id = self
            .resolve_or_create_session(&sender_id, &channel_name)
            .await?;

        // Extract chat_id from metadata for Telegram responses.
        let chat_id = extract_chat_id_from_metadata(&metadata).unwrap_or_default();

        // Send typing indicator.
        if !chat_id.is_empty()
            && let Err(e) = self.channel.send_typing(&chat_id).await
        {
            debug!(error = %e, "failed to send typing indicator");
        }

        // Get the session actor.
        let actor = self.sessions.get_mut(&session_id).ok_or_else(|| {
            BlufioError::Internal(format!("session actor not found for {session_id}"))
        })?;

        // Handle message: persist user message, check budget, assemble context, get stream.
        let stream_result = actor.handle_message(inbound).await;

        // Check for BudgetExhausted -- send user-facing message instead of error.
        let mut stream = match stream_result {
            Ok(s) => s,
            Err(BlufioError::BudgetExhausted { ref message }) => {
                warn!(
                    session_id = session_id.as_str(),
                    "budget exhausted, sending user notification"
                );
                let out = OutboundMessage {
                    session_id: Some(session_id.clone()),
                    channel: channel_name.clone(),
                    content: message.clone(),
                    reply_to: None,
                    parse_mode: None,
                    metadata: metadata.clone(),
                };
                if let Err(e) = self.channel.send(out).await {
                    error!(error = %e, "failed to send budget exhausted message");
                }
                return Ok(());
            }
            Err(e) => return Err(e),
        };

        // Consume the stream: accumulate text for persistence and send to channel.
        let mut full_response = String::new();
        let mut usage: Option<TokenUsage> = None;
        let mut sent_message_id: Option<String> = None;
        let supports_edit = self.channel.capabilities().supports_edit;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    match chunk.event_type {
                        StreamEventType::ContentBlockDelta => {
                            if let Some(text) = &chunk.text {
                                full_response.push_str(text);

                                if supports_edit {
                                    // Edit-in-place streaming.
                                    match &sent_message_id {
                                        None => {
                                            // Send initial message.
                                            let out = OutboundMessage {
                                                session_id: Some(session_id.clone()),
                                                channel: channel_name.clone(),
                                                content: full_response.clone(),
                                                reply_to: None,
                                                parse_mode: None,
                                                metadata: metadata.clone(),
                                            };
                                            match self.channel.send(out).await {
                                                Ok(mid) => {
                                                    sent_message_id = Some(mid.0);
                                                }
                                                Err(e) => {
                                                    warn!(error = %e, "failed to send initial message");
                                                }
                                            }
                                        }
                                        Some(mid) => {
                                            // Edit existing message.
                                            if let Err(e) = self
                                                .channel
                                                .edit_message(
                                                    &chat_id,
                                                    mid,
                                                    &full_response,
                                                    None,
                                                )
                                                .await
                                            {
                                                debug!(error = %e, "failed to edit message");
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        StreamEventType::MessageStart | StreamEventType::MessageDelta => {
                            if let Some(u) = chunk.usage {
                                usage = Some(u);
                            }
                        }
                        StreamEventType::MessageStop => {
                            break;
                        }
                        StreamEventType::Error => {
                            if let Some(err) = &chunk.error {
                                error!(error = err.as_str(), "LLM stream error");
                            }
                            break;
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    error!(error = %e, "stream chunk error");
                    break;
                }
            }
        }

        // If we haven't sent anything yet (non-edit channel or no delta arrived), send now.
        if sent_message_id.is_none() && !full_response.is_empty() {
            let out = OutboundMessage {
                session_id: Some(session_id.clone()),
                channel: channel_name.clone(),
                content: full_response.clone(),
                reply_to: None,
                parse_mode: None,
                metadata: metadata.clone(),
            };
            if let Err(e) = self.channel.send(out).await {
                error!(error = %e, "failed to send response message");
            }
        } else if sent_message_id.is_some() && !full_response.is_empty() {
            // Final edit to ensure the complete response is shown.
            if let Some(mid) = &sent_message_id
                && let Err(e) = self
                    .channel
                    .edit_message(&chat_id, mid, &full_response, None)
                    .await
            {
                debug!(error = %e, "failed to send final edit");
            }
        }

        // Persist assistant response (also records cost).
        let actor = self.sessions.get_mut(&session_id).ok_or_else(|| {
            BlufioError::Internal(format!("session actor not found for {session_id}"))
        })?;
        actor.persist_response(&full_response, usage.clone()).await?;

        if let Some(u) = &usage {
            info!(
                session_id = session_id.as_str(),
                input_tokens = u.input_tokens,
                output_tokens = u.output_tokens,
                "response complete"
            );
        } else {
            info!(
                session_id = session_id.as_str(),
                "response complete (no usage data)"
            );
        }

        Ok(())
    }

    /// Resolves an existing session or creates a new one for the sender.
    ///
    /// Looks up by sender_id + channel in the in-memory map first, then
    /// falls back to storage, and finally creates a new session if needed.
    async fn resolve_or_create_session(
        &mut self,
        sender_id: &str,
        channel: &str,
    ) -> Result<String, BlufioError> {
        // Check in-memory sessions first.
        let session_key = format!("{channel}:{sender_id}");
        if let Some(actor) = self.sessions.get(&session_key) {
            return Ok(actor.session_id().to_string());
        }

        // Check storage for existing active session.
        let active_sessions = self.storage.list_sessions(Some("active")).await?;
        for session in &active_sessions {
            if session.channel == channel
                && session.user_id.as_deref() == Some(sender_id)
            {
                debug!(
                    session_id = session.id.as_str(),
                    "resuming existing session"
                );
                // Create actor for the existing session.
                let actor = SessionActor::new(
                    session.id.clone(),
                    self.storage.clone(),
                    self.provider.clone(),
                    self.context_engine.clone(),
                    self.budget_tracker.clone(),
                    self.cost_ledger.clone(),
                    self.memory_provider.as_ref().cloned(),
                    self.memory_extractor.clone(),
                    channel.to_string(),
                    self.config.anthropic.default_model.clone(),
                    self.config.anthropic.max_tokens,
                    self.config.memory.idle_timeout_secs,
                );
                let session_id = session.id.clone();
                self.sessions.insert(session_key, actor);
                return Ok(session_id);
            }
        }

        // Create a new session.
        let session_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let new_session = Session {
            id: session_id.clone(),
            channel: channel.to_string(),
            user_id: Some(sender_id.to_string()),
            state: "active".to_string(),
            metadata: None,
            created_at: now.clone(),
            updated_at: now,
        };

        self.storage.create_session(&new_session).await?;

        info!(
            session_id = session_id.as_str(),
            sender_id = sender_id,
            channel = channel,
            "created new session"
        );

        let actor = SessionActor::new(
            session_id.clone(),
            self.storage.clone(),
            self.provider.clone(),
            self.context_engine.clone(),
            self.budget_tracker.clone(),
            self.cost_ledger.clone(),
            self.memory_provider.as_ref().cloned(),
            self.memory_extractor.clone(),
            channel.to_string(),
            self.config.anthropic.default_model.clone(),
            self.config.anthropic.max_tokens,
            self.config.memory.idle_timeout_secs,
        );
        self.sessions.insert(session_key, actor);

        Ok(session_id)
    }
}

/// Extracts chat_id from an optional JSON metadata string.
fn extract_chat_id_from_metadata(metadata: &Option<String>) -> Option<String> {
    metadata.as_ref().and_then(|m| {
        serde_json::from_str::<serde_json::Value>(m)
            .ok()
            .and_then(|v| v.get("chat_id").and_then(|c| c.as_str()).map(String::from))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_chat_id_from_valid_metadata() {
        let meta = Some(r#"{"chat_id":"12345"}"#.to_string());
        assert_eq!(
            extract_chat_id_from_metadata(&meta),
            Some("12345".to_string())
        );
    }

    #[test]
    fn extract_chat_id_from_none() {
        assert_eq!(extract_chat_id_from_metadata(&None), None);
    }

    #[test]
    fn extract_chat_id_from_invalid_json() {
        let meta = Some("not json".to_string());
        assert_eq!(extract_chat_id_from_metadata(&meta), None);
    }

    #[test]
    fn extract_chat_id_missing_field() {
        let meta = Some(r#"{"other":"value"}"#.to_string());
        assert_eq!(extract_chat_id_from_metadata(&meta), None);
    }
}
