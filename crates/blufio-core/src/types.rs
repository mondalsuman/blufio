// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Common types used across adapter traits and the Blufio framework.

use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use crate::classification::{Classifiable, DataClassification};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize)]
pub enum AdapterType {
    Channel,
    Provider,
    Storage,
    Embedding,
    Observability,
    Auth,
    SkillRuntime,
    Tts,
    Transcription,
    ImageGen,
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

/// How a channel supports streaming message updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, Default)]
#[non_exhaustive]
pub enum StreamingType {
    /// No streaming support -- messages are sent as a whole.
    #[default]
    None,
    /// Messages can be edited in place (e.g., Telegram, Discord, Slack).
    EditBased,
    /// Messages are appended (e.g., IRC, SSE).
    AppendOnly,
}

/// Level of text formatting a channel supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, Default)]
#[non_exhaustive]
pub enum FormattingSupport {
    /// Plain text only.
    #[default]
    PlainText,
    /// Bold, italic, links (e.g., WhatsApp).
    BasicMarkdown,
    /// Full GitHub-Flavored Markdown (e.g., Discord, Slack).
    FullMarkdown,
    /// HTML rendering (e.g., Matrix).
    HTML,
}

/// Rate limit information for a channel.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct RateLimit {
    /// Maximum messages per second.
    pub messages_per_second: Option<f32>,
    /// Maximum burst size.
    pub burst_limit: Option<u32>,
    /// Maximum messages per day.
    pub daily_limit: Option<u32>,
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
    /// Whether the channel supports rich embeds (Discord embeds, Slack blocks).
    pub supports_embeds: bool,
    /// Whether the channel supports emoji reactions.
    pub supports_reactions: bool,
    /// Whether the channel supports threaded replies.
    pub supports_threads: bool,
    /// How the channel handles streaming message updates.
    pub streaming_type: StreamingType,
    /// Level of text formatting the channel supports.
    pub formatting_support: FormattingSupport,
    /// Rate limit information for the channel.
    pub rate_limit: Option<RateLimit>,
    /// Whether the channel supports code blocks (fenced with backticks).
    pub supports_code_blocks: bool,
    /// Whether the channel supports interactive user confirmation (HITL prompts).
    /// Default true for messaging channels (Telegram, Discord, Slack, IRC, Matrix, Signal, WhatsApp, iMessage).
    /// False for non-interactive channels (Email IMAP polling, SMS webhooks).
    pub supports_interactive: bool,
}

impl Default for ChannelCapabilities {
    fn default() -> Self {
        Self {
            supports_edit: false,
            supports_typing: false,
            supports_images: false,
            supports_documents: false,
            supports_voice: false,
            max_message_length: None,
            supports_embeds: false,
            supports_reactions: false,
            supports_threads: false,
            streaming_type: StreamingType::default(),
            formatting_support: FormattingSupport::default(),
            rate_limit: None,
            supports_code_blocks: false,
            supports_interactive: true,
        }
    }
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
    /// Tool definitions to send to the provider.
    /// When present, the LLM may respond with tool_use content blocks.
    pub tools: Option<Vec<ToolDefinition>>,
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

/// Provider-agnostic tool definition.
///
/// Each LLM provider serializes this to its own wire format.
/// Replaces the previous `serde_json::Value` tool representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name (unique identifier).
    pub name: String,
    /// Human-readable description of what the tool does.
    pub description: String,
    /// JSON Schema describing the tool's input parameters.
    pub input_schema: serde_json::Value,
}

impl ToolDefinition {
    /// Convert to a raw JSON value for backward compatibility or generic serialization.
    pub fn to_json_value(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "description": self.description,
            "input_schema": self.input_schema,
        })
    }
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
    /// Optional session ID for event correlation.
    pub session_id: Option<String>,
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
    /// Data classification level for this session.
    #[serde(default)]
    pub classification: DataClassification,
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
    /// Data classification level for this message.
    #[serde(default)]
    pub classification: DataClassification,
}

impl Classifiable for Message {
    fn classification(&self) -> DataClassification {
        self.classification
    }
    fn set_classification(&mut self, level: DataClassification) {
        self.classification = level;
    }
}

impl Classifiable for Session {
    fn classification(&self) -> DataClassification {
        self.classification
    }
    fn set_classification(&mut self, level: DataClassification) {
        self.classification = level;
    }
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

// --- TTS types ---

/// A request to a text-to-speech provider.
#[derive(Debug, Clone)]
pub struct TtsRequest {
    /// Text to synthesize to audio.
    pub text: String,
    /// Voice identifier (provider-specific).
    pub voice: String,
    /// Output audio format (e.g., "mp3", "wav", "opus").
    pub output_format: String,
    /// Speaking speed multiplier (1.0 = normal).
    pub speed: f32,
}

/// A response from a text-to-speech provider.
#[derive(Debug, Clone)]
pub struct TtsResponse {
    /// Raw audio bytes in the requested format.
    pub audio_data: Vec<u8>,
    /// MIME type of the audio (e.g., "audio/mpeg").
    pub content_type: String,
    /// Duration of the generated audio in seconds.
    pub duration_secs: Option<f32>,
}

// --- Transcription types ---

/// A request to a transcription (speech-to-text) provider.
#[derive(Debug, Clone)]
pub struct TranscriptionRequest {
    /// Raw audio bytes to transcribe.
    pub audio_data: Vec<u8>,
    /// MIME type of the audio (e.g., "audio/wav", "audio/mpeg").
    pub content_type: String,
    /// Optional language hint (ISO 639-1 code, e.g., "en").
    pub language: Option<String>,
}

/// A response from a transcription provider.
#[derive(Debug, Clone)]
pub struct TranscriptionResponse {
    /// Transcribed text.
    pub text: String,
    /// Detected language (ISO 639-1 code).
    pub language: Option<String>,
    /// Duration of the audio in seconds.
    pub duration_secs: Option<f32>,
}

// --- Image generation types ---

/// A request to an image generation provider.
#[derive(Debug, Clone)]
pub struct ImageRequest {
    /// Text prompt describing the image to generate.
    pub prompt: String,
    /// Image dimensions (width x height).
    pub size: (u32, u32),
    /// Number of images to generate.
    pub n: u32,
    /// Output format (e.g., "png", "jpeg").
    pub output_format: String,
}

/// A response from an image generation provider.
#[derive(Debug, Clone)]
pub struct ImageResponse {
    /// Generated images as raw bytes.
    pub images: Vec<Vec<u8>>,
    /// MIME type of the images (e.g., "image/png").
    pub content_type: String,
}
