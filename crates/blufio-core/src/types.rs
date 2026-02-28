// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Common types used across adapter traits and the Blufio framework.

use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

/// Unique identifier for a conversation session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

/// Unique identifier for a message.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub String);

/// Health status reported by adapter health checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    /// Adapter is fully operational.
    Healthy,
    /// Adapter is operational but experiencing issues.
    Degraded(String),
    /// Adapter is not operational.
    Unhealthy(String),
}

/// Identifies the type of adapter in the plugin registry.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize,
)]
pub enum AdapterType {
    Channel,
    Provider,
    Storage,
    Embedding,
    Observability,
    Auth,
    SkillRuntime,
}

// --- Channel placeholder types ---

/// An inbound message received from a channel adapter.
#[derive(Debug, Clone)]
pub struct InboundMessage {
    pub _placeholder: (),
}

/// An outbound message to be sent via a channel adapter.
#[derive(Debug, Clone)]
pub struct OutboundMessage {
    pub _placeholder: (),
}

/// Capabilities reported by a channel adapter.
#[derive(Debug, Clone)]
pub struct ChannelCapabilities {
    pub _placeholder: (),
}

// --- Provider placeholder types ---

/// A request to an LLM provider.
#[derive(Debug, Clone)]
pub struct ProviderRequest {
    pub _placeholder: (),
}

/// A response from an LLM provider.
#[derive(Debug, Clone)]
pub struct ProviderResponse {
    pub _placeholder: (),
}

/// A single chunk from a streaming LLM provider response.
#[derive(Debug, Clone)]
pub struct ProviderStreamChunk {
    pub _placeholder: (),
}

// --- Embedding placeholder types ---

/// Input for an embedding adapter.
#[derive(Debug, Clone)]
pub struct EmbeddingInput {
    pub _placeholder: (),
}

/// Output from an embedding adapter.
#[derive(Debug, Clone)]
pub struct EmbeddingOutput {
    pub _placeholder: (),
}

// --- Auth placeholder types ---

/// An authentication token to be verified.
#[derive(Debug, Clone)]
pub struct AuthToken {
    pub _placeholder: (),
}

/// A verified identity from an auth adapter.
#[derive(Debug, Clone)]
pub struct AuthIdentity {
    pub _placeholder: (),
}

// --- Skill placeholder types ---

/// Manifest describing a skill's capabilities and requirements.
#[derive(Debug, Clone)]
pub struct SkillManifest {
    pub _placeholder: (),
}

/// An invocation request for a skill.
#[derive(Debug, Clone)]
pub struct SkillInvocation {
    pub _placeholder: (),
}

/// The result of a skill invocation.
#[derive(Debug, Clone)]
pub struct SkillResult {
    pub _placeholder: (),
}

// --- Observability placeholder types ---

/// A metric or telemetry event.
#[derive(Debug, Clone)]
pub struct MetricEvent {
    pub _placeholder: (),
}

// --- Storage domain types ---

/// A conversation session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier.
    pub id: String,
    /// Channel the session originates from (e.g., "telegram", "cli").
    pub channel: String,
    /// Optional user identifier from the channel.
    pub user_id: Option<String>,
    /// Session state: "active", "paused", "closed".
    pub state: String,
    /// Optional JSON metadata blob.
    pub metadata: Option<String>,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last-update timestamp.
    pub updated_at: String,
}

/// A single message within a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique message identifier.
    pub id: String,
    /// Session this message belongs to.
    pub session_id: String,
    /// Role: "user", "assistant", "system", or "tool".
    pub role: String,
    /// Message content (text or JSON for tool results).
    pub content: String,
    /// Token count for cost tracking (populated after LLM response).
    pub token_count: Option<i64>,
    /// Optional JSON metadata blob.
    pub metadata: Option<String>,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
}

/// A crash-safe message queue entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueEntry {
    /// Auto-increment queue entry ID.
    pub id: i64,
    /// Queue name for routing (e.g., "inbound", "outbound").
    pub queue_name: String,
    /// JSON payload.
    pub payload: String,
    /// Status: "pending", "processing", "completed", "failed".
    pub status: String,
    /// Number of processing attempts so far.
    pub attempts: i32,
    /// Maximum allowed attempts before permanent failure.
    pub max_attempts: i32,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last-update timestamp.
    pub updated_at: String,
    /// ISO 8601 timestamp until which this entry is locked for processing.
    pub locked_until: Option<String>,
}
