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

// --- Channel types ---

/// Content types that can be received from a channel.
#[derive(Debug, Clone)]
pub enum MessageContent {
    /// Plain text message.
    Text(String),
    /// Image with raw bytes and metadata.
    Image {
        data: Vec<u8>,
        mime_type: String,
        caption: Option<String>,
    },
    /// Document/file with raw bytes and metadata.
    Document {
        data: Vec<u8>,
        filename: String,
        mime_type: String,
    },
    /// Voice message with raw audio bytes.
    Voice {
        data: Vec<u8>,
        duration_secs: Option<f32>,
    },
}

/// An inbound message received from a channel adapter.
#[derive(Debug, Clone)]
pub struct InboundMessage {
    /// Message ID from the channel.
    pub id: String,
    /// Session this message belongs to (resolved by agent loop).
    pub session_id: Option<String>,
    /// Channel identifier (e.g., "telegram", "cli").
    pub channel: String,
    /// Sender identifier from the channel.
    pub sender_id: String,
    /// Message content.
    pub content: MessageContent,
    /// ISO 8601 timestamp.
    pub timestamp: String,
    /// Optional JSON metadata blob.
    pub metadata: Option<String>,
}

/// An outbound message to be sent via a channel adapter.
#[derive(Debug, Clone)]
pub struct OutboundMessage {
    /// Session this message belongs to.
    pub session_id: Option<String>,
    /// Channel identifier.
    pub channel: String,
    /// Text content to send.
    pub content: String,
    /// Message ID to reply to (channel-specific).
    pub reply_to: Option<String>,
    /// Parse mode for formatting (e.g., "MarkdownV2").
    pub parse_mode: Option<String>,
    /// Optional JSON metadata blob.
    pub metadata: Option<String>,
}

/// Capabilities reported by a channel adapter.
#[derive(Debug, Clone)]
pub struct ChannelCapabilities {
    /// Whether the channel supports editing sent messages.
    pub supports_edit: bool,
    /// Whether the channel supports typing indicators.
    pub supports_typing: bool,
    /// Whether the channel supports image messages.
    pub supports_images: bool,
    /// Whether the channel supports document messages.
    pub supports_documents: bool,
    /// Whether the channel supports voice messages.
    pub supports_voice: bool,
    /// Maximum message length in characters (None = unlimited).
    pub max_message_length: Option<usize>,
}

// --- Provider types ---

/// A content block within a provider message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    /// Text content block.
    #[serde(rename = "text")]
    Text { text: String },
    /// Image content block (base64 encoded).
    #[serde(rename = "image")]
    Image {
        source_type: String,
        media_type: String,
        data: String,
    },
    /// Tool use content block (assistant requests tool execution).
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Tool result content block (user provides tool execution result).
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

/// A single message in a provider conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMessage {
    /// Role: "user", "assistant", or "system".
    pub role: String,
    /// Content blocks for this message.
    pub content: Vec<ContentBlock>,
}

/// A request to an LLM provider.
#[derive(Debug, Clone)]
pub struct ProviderRequest {
    /// Model identifier (e.g., "claude-sonnet-4-20250514").
    pub model: String,
    /// System prompt (injected as system parameter, not a message).
    pub system_prompt: Option<String>,
    /// Structured system prompt blocks (provider-specific formatting).
    /// When set, takes precedence over system_prompt.
    pub system_blocks: Option<serde_json::Value>,
    /// Conversation messages.
    pub messages: Vec<ProviderMessage>,
    /// Maximum tokens to generate.
    pub max_tokens: u32,
    /// Whether to stream the response.
    pub stream: bool,
    /// Tool definitions to send to the provider (Anthropic format).
    /// When present, the LLM may respond with tool_use content blocks.
    pub tools: Option<Vec<serde_json::Value>>,
}

/// Token usage statistics from a provider response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Number of input tokens consumed.
    pub input_tokens: u32,
    /// Number of output tokens generated.
    pub output_tokens: u32,
    /// Number of tokens read from cache (prompt caching).
    #[serde(default)]
    pub cache_read_tokens: u32,
    /// Number of tokens written to cache (prompt caching).
    #[serde(default)]
    pub cache_creation_tokens: u32,
}

/// A response from an LLM provider.
#[derive(Debug, Clone)]
pub struct ProviderResponse {
    /// Response ID from the provider.
    pub id: String,
    /// Generated text content.
    pub content: String,
    /// Model that generated the response.
    pub model: String,
    /// Reason the generation stopped (e.g., "end_turn", "max_tokens").
    pub stop_reason: Option<String>,
    /// Token usage statistics.
    pub usage: TokenUsage,
}

/// Event types in a streaming provider response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEventType {
    MessageStart,
    ContentBlockStart,
    ContentBlockDelta,
    ContentBlockStop,
    MessageDelta,
    MessageStop,
    Ping,
    Error,
}

/// Data for a tool_use content block parsed from a streaming response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseData {
    /// Unique ID for this tool use (links to tool_result).
    pub id: String,
    /// Name of the tool being invoked.
    pub name: String,
    /// Parsed JSON input for the tool.
    pub input: serde_json::Value,
}

/// A single chunk from a streaming LLM provider response.
#[derive(Debug, Clone)]
pub struct ProviderStreamChunk {
    /// Type of streaming event.
    pub event_type: StreamEventType,
    /// Text content (for ContentBlockDelta with text_delta).
    pub text: Option<String>,
    /// Token usage (for MessageDelta).
    pub usage: Option<TokenUsage>,
    /// Error message (for Error events).
    pub error: Option<String>,
    /// Tool use data (for ContentBlockStop on a tool_use block).
    pub tool_use: Option<ToolUseData>,
    /// Stop reason from the provider (e.g., "end_turn", "tool_use").
    pub stop_reason: Option<String>,
}

// --- Embedding types ---

/// Input for an embedding adapter.
#[derive(Debug, Clone)]
pub struct EmbeddingInput {
    /// Text strings to embed.
    pub texts: Vec<String>,
}

/// Output from an embedding adapter.
#[derive(Debug, Clone)]
pub struct EmbeddingOutput {
    /// One embedding vector per input text.
    pub embeddings: Vec<Vec<f32>>,
    /// Dimensionality of each embedding vector.
    pub dimensions: usize,
}

// --- Auth types ---

/// An authentication token to be verified.
#[derive(Debug, Clone)]
pub struct AuthToken {
    /// The raw bearer token string.
    pub token: String,
}

/// A verified identity from an auth adapter.
#[derive(Debug, Clone)]
pub struct AuthIdentity {
    /// Unique identifier for the authenticated entity.
    pub id: String,
    /// Human-readable label (e.g., device name).
    pub label: Option<String>,
}

// --- Skill types ---

/// Manifest describing a skill's capabilities and requirements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    /// Unique name of the skill (alphanumeric + hyphens).
    pub name: String,
    /// Semantic version string.
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// Optional author identifier.
    #[serde(default)]
    pub author: Option<String>,
    /// Capabilities the skill declares it needs.
    #[serde(default)]
    pub capabilities: SkillCapabilities,
    /// Resource limits for the WASM sandbox.
    #[serde(default)]
    pub resources: SkillResources,
    /// WASM binary filename (relative to skill directory).
    #[serde(default = "default_wasm_entry")]
    pub wasm_entry: String,
}

fn default_wasm_entry() -> String {
    "skill.wasm".to_string()
}

/// Capabilities a skill declares it needs.
///
/// Each capability must be explicitly declared in the skill manifest.
/// The WASM sandbox only exposes host functions matching declared capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillCapabilities {
    /// Network access capability (domain-scoped).
    #[serde(default)]
    pub network: Option<NetworkCapability>,
    /// Filesystem access capability (path-scoped).
    #[serde(default)]
    pub filesystem: Option<FilesystemCapability>,
    /// Environment variables the skill may read.
    #[serde(default)]
    pub env: Vec<String>,
}

/// Network capability: which domains the skill may access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkCapability {
    /// Allowed domain names (e.g., ["api.example.com"]).
    pub domains: Vec<String>,
}

/// Filesystem capability: which paths the skill may read/write.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemCapability {
    /// Paths the skill may read from.
    #[serde(default)]
    pub read: Vec<String>,
    /// Paths the skill may write to.
    #[serde(default)]
    pub write: Vec<String>,
}

/// Resource limits for a WASM skill sandbox.
///
/// Conservative defaults: 1B fuel (~1s of compute), 16MB memory, 5s wall-clock timeout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillResources {
    /// Fuel limit for WASM execution (default: 1,000,000,000).
    #[serde(default = "default_fuel")]
    pub fuel: u64,
    /// Maximum memory in megabytes (default: 16).
    #[serde(default = "default_memory_mb")]
    pub memory_mb: u32,
    /// Epoch-based wall-clock timeout in seconds (default: 5).
    #[serde(default = "default_epoch_timeout")]
    pub epoch_timeout_secs: u64,
}

fn default_fuel() -> u64 {
    1_000_000_000
}
fn default_memory_mb() -> u32 {
    16
}
fn default_epoch_timeout() -> u64 {
    5
}

impl Default for SkillResources {
    fn default() -> Self {
        Self {
            fuel: default_fuel(),
            memory_mb: default_memory_mb(),
            epoch_timeout_secs: default_epoch_timeout(),
        }
    }
}

/// An invocation request for a skill.
#[derive(Debug, Clone)]
pub struct SkillInvocation {
    /// Name of the skill to invoke.
    pub skill_name: String,
    /// JSON input to pass to the skill.
    pub input: serde_json::Value,
}

/// The result of a skill invocation.
#[derive(Debug, Clone)]
pub struct SkillResult {
    /// Text content returned by the skill.
    pub content: String,
    /// Whether the invocation resulted in an error.
    pub is_error: bool,
}

// --- Observability types ---

/// A metric or telemetry event.
#[derive(Debug, Clone)]
pub enum MetricEvent {
    /// Increment a counter.
    Counter {
        name: String,
        value: u64,
        labels: Vec<(String, String)>,
    },
    /// Set a gauge value.
    Gauge {
        name: String,
        value: f64,
        labels: Vec<(String, String)>,
    },
    /// Record a histogram observation.
    Histogram {
        name: String,
        value: f64,
        labels: Vec<(String, String)>,
    },
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
