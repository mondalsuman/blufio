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
pub mod heartbeat;
pub mod session;
pub mod shutdown;

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use blufio_config::model::BlufioConfig;
use blufio_context::ContextEngine;
use blufio_cost::{BudgetTracker, CostLedger};
use blufio_core::error::BlufioError;
use blufio_core::types::{
    ContentBlock, InboundMessage, OutboundMessage, ProviderMessage, ProviderRequest,
    ProviderStreamChunk, Session, StreamEventType, TokenUsage, ToolUseData,
};
use blufio_core::{ChannelAdapter, ProviderAdapter, StorageAdapter};
use blufio_memory::{MemoryExtractor, MemoryProvider};
use blufio_router::ModelRouter;
use blufio_skill::ToolRegistry;

pub use heartbeat::HeartbeatRunner;
use futures::{Stream, StreamExt};
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
    /// Model router for per-message complexity-based model selection.
    router: Arc<ModelRouter>,
    /// Heartbeat runner for proactive check-ins (None = disabled).
    heartbeat_runner: Option<Arc<HeartbeatRunner>>,
    /// Registry of available tools (built-in and WASM skills).
    tool_registry: Arc<tokio::sync::RwLock<ToolRegistry>>,
    config: BlufioConfig,
    sessions: HashMap<String, SessionActor>,
}

impl AgentLoop {
    /// Creates a new agent loop with the given adapters, context engine, cost components,
    /// model router, tool registry, and optional heartbeat runner.
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
        router: Arc<ModelRouter>,
        heartbeat_runner: Option<Arc<HeartbeatRunner>>,
        tool_registry: Arc<tokio::sync::RwLock<ToolRegistry>>,
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
            router,
            heartbeat_runner,
            tool_registry,
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
    ///
    /// Integrates heartbeat delivery: if a pending heartbeat exists from the
    /// `on_next_message` delivery mode, it is prepended to the response.
    ///
    /// After the LLM responds, if the response contains `tool_use` blocks,
    /// executes the tools, sends tool_result back, and re-calls the LLM
    /// in a loop (capped at [`MAX_TOOL_ITERATIONS`]).
    async fn handle_inbound(&mut self, inbound: InboundMessage) -> Result<(), BlufioError> {
        let sender_id = inbound.sender_id.clone();
        let channel_name = inbound.channel.clone();
        let metadata = inbound.metadata.clone();

        // Notify heartbeat runner of incoming message (for skip-when-unchanged detection).
        if let Some(ref runner) = self.heartbeat_runner {
            runner.notify_message_received().await;
        }

        // Check for pending heartbeat (on_next_message delivery).
        let pending_heartbeat = if let Some(ref runner) = self.heartbeat_runner {
            runner.take_pending_heartbeat().await
        } else {
            None
        };

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

        // Consume the initial stream and enter the tool loop.
        let max_iterations = {
            let actor = self.sessions.get(&session_id).ok_or_else(|| {
                BlufioError::Internal(format!("session actor not found for {session_id}"))
            })?;
            actor.max_tool_iterations()
        };

        let mut full_response = String::new();
        let mut usage: Option<TokenUsage> = None;
        let mut sent_message_id: Option<String> = None;
        let supports_edit = self.channel.capabilities().supports_edit;

        // Tool loop: consume stream, check for tool_use, execute, re-call LLM.
        for iteration in 0..=max_iterations {
            let (text, stream_usage, tool_uses, stop_reason) = consume_stream(&mut stream).await;

            full_response.push_str(&text);
            if let Some(u) = stream_usage {
                usage = Some(u);
            }

            // Stream text to channel (edit-in-place or send).
            if !text.is_empty() && supports_edit {
                match &sent_message_id {
                    None => {
                        let out = OutboundMessage {
                            session_id: Some(session_id.clone()),
                            channel: channel_name.clone(),
                            content: full_response.clone(),
                            reply_to: None,
                            parse_mode: None,
                            metadata: metadata.clone(),
                        };
                        match self.channel.send(out).await {
                            Ok(mid) => sent_message_id = Some(mid.0),
                            Err(e) => warn!(error = %e, "failed to send initial message"),
                        }
                    }
                    Some(mid) => {
                        if let Err(e) = self
                            .channel
                            .edit_message(&chat_id, mid, &full_response, None)
                            .await
                        {
                            debug!(error = %e, "failed to edit message");
                        }
                    }
                }
            }

            // Check if we have tool_use blocks to execute.
            let has_tool_use = !tool_uses.is_empty()
                || stop_reason.as_deref() == Some("tool_use");

            if !has_tool_use || tool_uses.is_empty() {
                // No tool calls -- we're done with this message.
                break;
            }

            if iteration >= max_iterations {
                warn!(
                    session_id = %session_id,
                    iterations = iteration,
                    "maximum tool iterations reached, forcing text response"
                );
                // Persist what we have and break -- the LLM's last response is the final answer.
                break;
            }

            // Execute tools via the session actor.
            info!(
                session_id = %session_id,
                tool_count = tool_uses.len(),
                iteration = iteration,
                "executing tool calls"
            );

            let actor = self.sessions.get_mut(&session_id).ok_or_else(|| {
                BlufioError::Internal(format!("session actor not found for {session_id}"))
            })?;

            // Persist the assistant message with tool_use content (text + tool calls).
            actor.persist_response(&text, usage.clone()).await?;

            let tool_results = actor.execute_tools(&tool_uses).await?;

            // Build tool_result messages and persist them as user messages.
            // Each tool_result is a separate content block in a single user message.
            for (tool_use_id, output) in &tool_results {
                let now = chrono::Utc::now().to_rfc3339();
                let result_content = serde_json::json!({
                    "type": "tool_result",
                    "tool_use_id": tool_use_id,
                    "content": output.content,
                    "is_error": output.is_error,
                });
                let msg = blufio_core::types::Message {
                    id: uuid::Uuid::new_v4().to_string(),
                    session_id: session_id.clone(),
                    role: "user".to_string(),
                    content: result_content.to_string(),
                    token_count: None,
                    metadata: Some(serde_json::json!({"tool_result": true}).to_string()),
                    created_at: now,
                };
                self.storage.insert_message(&msg).await?;
            }

            // Build the tool_result ProviderMessages for the follow-up LLM call.
            let tool_result_blocks: Vec<serde_json::Value> = tool_results
                .iter()
                .map(|(tool_use_id, output)| {
                    serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": tool_use_id,
                        "content": output.content,
                        "is_error": output.is_error,
                    })
                })
                .collect();

            // Build the assistant message with tool_use content blocks for the LLM.
            let mut assistant_content_blocks: Vec<serde_json::Value> = Vec::new();
            if !text.is_empty() {
                assistant_content_blocks.push(serde_json::json!({
                    "type": "text",
                    "text": text,
                }));
            }
            for tu in &tool_uses {
                assistant_content_blocks.push(serde_json::json!({
                    "type": "tool_use",
                    "id": tu.id,
                    "name": tu.name,
                    "input": tu.input,
                }));
            }

            // Re-assemble context for the follow-up call by getting history from storage.
            // The persisted messages now include the tool_use and tool_result messages.
            let history = self.storage.get_messages(&session_id, Some(50)).await?;
            let mut messages: Vec<ProviderMessage> = history
                .iter()
                .map(|m| ProviderMessage {
                    role: m.role.clone(),
                    content: vec![ContentBlock::Text {
                        text: m.content.clone(),
                    }],
                })
                .collect();

            // Replace the last assistant + user tool_result messages with properly
            // structured content blocks (the storage only has text representations).
            // Pop the tool_result user messages and the assistant tool_use message.
            let tool_result_count = tool_results.len();
            for _ in 0..(tool_result_count + 1) {
                messages.pop();
            }

            // Re-add the assistant message with structured tool_use blocks.
            messages.push(ProviderMessage {
                role: "assistant".to_string(),
                content: vec![ContentBlock::Text {
                    text: serde_json::to_string(&assistant_content_blocks)
                        .unwrap_or_default(),
                }],
            });

            // Re-add the user message with tool_result blocks.
            messages.push(ProviderMessage {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text: serde_json::to_string(&tool_result_blocks)
                        .unwrap_or_default(),
                }],
            });

            // Build follow-up ProviderRequest.
            let actor = self.sessions.get(&session_id).ok_or_else(|| {
                BlufioError::Internal(format!("session actor not found for {session_id}"))
            })?;

            let tool_defs = {
                let registry = actor.tool_registry().read().await;
                if !registry.is_empty() {
                    Some(registry.tool_definitions())
                } else {
                    None
                }
            };

            let follow_up_request = ProviderRequest {
                model: self.config.anthropic.default_model.clone(),
                system_prompt: None,
                system_blocks: None,
                messages,
                max_tokens: self.config.anthropic.max_tokens,
                stream: true,
                tools: tool_defs,
            };

            // Re-call the LLM with tool results.
            stream = self.provider.stream(follow_up_request).await?;

            // Reset for next iteration -- clear text accumulator but keep the
            // full_response for the final display.
            full_response.clear();
        }

        // Build the final display content, optionally prepending:
        // 1. Pending heartbeat content (on_next_message delivery)
        // 2. Budget downgrade notification
        let mut display_response = String::new();

        // Prepend pending heartbeat if present.
        if let Some(ref hb) = pending_heartbeat {
            display_response.push_str(hb);
            display_response.push_str("\n\n---\n\n");
        }

        // Check for budget downgrade notification from the session actor.
        {
            let actor = self.sessions.get(&session_id).ok_or_else(|| {
                BlufioError::Internal(format!("session actor not found for {session_id}"))
            })?;
            if let Some(decision) = actor.last_routing_decision() {
                if decision.downgraded {
                    let short_name = blufio_router::ModelRouter::short_model_name(&decision.actual_model);
                    let note = format!(
                        "_(Using {} -- budget at {:.0}%)_\n\n",
                        short_name,
                        // Approximate from reason string -- just show the note
                        // without re-querying budget since decision.reason has the info
                        decision.reason.split("budget at ").last()
                            .and_then(|s| s.strip_suffix(')'))
                            .unwrap_or("high")
                    );
                    display_response.push_str(&note);
                }
            }
        }

        display_response.push_str(&full_response);

        // If we haven't sent anything yet (non-edit channel or no delta arrived), send now.
        if sent_message_id.is_none() && !display_response.is_empty() {
            let out = OutboundMessage {
                session_id: Some(session_id.clone()),
                channel: channel_name.clone(),
                content: display_response.clone(),
                reply_to: None,
                parse_mode: None,
                metadata: metadata.clone(),
            };
            if let Err(e) = self.channel.send(out).await {
                error!(error = %e, "failed to send response message");
            }
        } else if sent_message_id.is_some() && !display_response.is_empty() {
            // Final edit to ensure the complete response is shown.
            if let Some(mid) = &sent_message_id
                && let Err(e) = self
                    .channel
                    .edit_message(&chat_id, mid, &display_response, None)
                    .await
            {
                debug!(error = %e, "failed to send final edit");
            }
        }

        // Persist final assistant response (also records cost).
        // Note: We persist the raw LLM response, not the display_response with prefixes.
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
                    self.router.clone(),
                    self.config.anthropic.default_model.clone(),
                    self.config.anthropic.max_tokens,
                    self.config.routing.enabled,
                    self.config.memory.idle_timeout_secs,
                    self.tool_registry.clone(),
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
            self.router.clone(),
            self.config.anthropic.default_model.clone(),
            self.config.anthropic.max_tokens,
            self.config.routing.enabled,
            self.config.memory.idle_timeout_secs,
            self.tool_registry.clone(),
        );
        self.sessions.insert(session_key, actor);

        Ok(session_id)
    }
}

/// Consumes a provider stream, collecting text, usage, tool_use blocks, and stop_reason.
///
/// Returns `(text, usage, tool_uses, stop_reason)`.
async fn consume_stream(
    stream: &mut Pin<Box<dyn Stream<Item = Result<ProviderStreamChunk, BlufioError>> + Send>>,
) -> (String, Option<TokenUsage>, Vec<ToolUseData>, Option<String>) {
    let mut text = String::new();
    let mut usage: Option<TokenUsage> = None;
    let mut tool_uses: Vec<ToolUseData> = Vec::new();
    let mut stop_reason: Option<String> = None;

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                match chunk.event_type {
                    StreamEventType::ContentBlockDelta => {
                        if let Some(t) = &chunk.text {
                            text.push_str(t);
                        }
                    }
                    StreamEventType::ContentBlockStop => {
                        if let Some(tu) = chunk.tool_use {
                            tool_uses.push(tu);
                        }
                    }
                    StreamEventType::MessageStart | StreamEventType::MessageDelta => {
                        if let Some(u) = chunk.usage {
                            usage = Some(u);
                        }
                        if let Some(sr) = &chunk.stop_reason {
                            stop_reason = Some(sr.clone());
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

    (text, usage, tool_uses, stop_reason)
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
