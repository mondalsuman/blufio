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
