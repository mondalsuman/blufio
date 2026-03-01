// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Per-session FSM that manages the lifecycle of a single conversation.
//!
//! Each session goes through states: Idle -> Receiving -> Processing -> Responding -> Idle.
//! The Draining state is used during graceful shutdown.

use std::pin::Pin;
use std::sync::Arc;

use blufio_core::error::BlufioError;
use blufio_core::types::{
    InboundMessage, Message, ProviderStreamChunk, TokenUsage,
};
use blufio_core::{ProviderAdapter, StorageAdapter};
use futures::Stream;
use tracing::debug;

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
/// - Assembling context from history
/// - Calling the LLM provider
/// - Persisting assistant responses
pub struct SessionActor {
    session_id: String,
    state: SessionState,
    storage: Arc<dyn StorageAdapter + Send + Sync>,
    provider: Arc<dyn ProviderAdapter + Send + Sync>,
    system_prompt: String,
    channel: String,
    model: String,
    max_tokens: u32,
}

impl SessionActor {
    /// Creates a new session actor.
    pub fn new(
        session_id: String,
        storage: Arc<dyn StorageAdapter + Send + Sync>,
        provider: Arc<dyn ProviderAdapter + Send + Sync>,
        system_prompt: String,
        channel: String,
        model: String,
        max_tokens: u32,
    ) -> Self {
        Self {
            session_id,
            state: SessionState::Idle,
            storage,
            provider,
            system_prompt,
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

    /// Handles an inbound message: persists it, assembles context, and starts streaming.
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

        // Assemble context from history + current message.
        let request = context::assemble_context(
            self.storage.as_ref(),
            &self.session_id,
            &self.system_prompt,
            &inbound,
            &self.model,
            self.max_tokens,
        )
        .await?;

        // Call the LLM provider for a streaming response.
        let stream = self.provider.stream(request).await?;

        // Transition: Processing -> Responding
        self.state = SessionState::Responding;

        Ok(stream)
    }

    /// Persists the full assistant response text after streaming is complete.
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
            token_count: usage.map(|u| i64::from(u.output_tokens)),
            metadata: None,
            created_at: now,
        };
        self.storage.insert_message(&msg).await?;

        debug!(
            session_id = self.session_id.as_str(),
            "persisted assistant response"
        );

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
