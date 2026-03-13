// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Typed event definitions for the Blufio internal event bus.
//!
//! Each domain (session, channel, skill, node, webhook, batch) defines its own
//! event sub-enum. The top-level [`BusEvent`] wraps all domains.

use serde::{Deserialize, Serialize};

/// Generate a new unique event ID using UUID v4.
pub fn new_event_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Generate an ISO 8601 timestamp for the current time.
pub fn now_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Top-level event type for the Blufio internal event bus.
///
/// Each variant represents a domain of the system. Subscribers can pattern-match
/// on the variant to filter events by domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BusEvent {
    /// Session lifecycle events.
    Session(SessionEvent),
    /// Channel message events.
    Channel(ChannelEvent),
    /// Skill invocation events.
    Skill(SkillEvent),
    /// Node connection events.
    Node(NodeEvent),
    /// Webhook trigger and delivery events.
    Webhook(WebhookEvent),
    /// Batch processing events.
    Batch(BatchEvent),
    /// Resilience events (circuit breaker, degradation ladder).
    Resilience(ResilienceEvent),
    /// Data classification events (level changes, PII detection, enforcement).
    Classification(ClassificationEvent),
    /// Configuration change events (settings modified, config reloaded).
    Config(ConfigEvent),
    /// Memory CRUD events (created, updated, deleted, retrieved, evicted).
    Memory(MemoryEvent),
    /// Audit subsystem meta-events (enabled, disabled, erased).
    Audit(AuditMetaEvent),
    /// API request events (mutating HTTP requests through the gateway).
    Api(ApiEvent),
    /// Provider call events (LLM call metadata: model, tokens, cost, latency).
    Provider(ProviderEvent),
    /// Compaction lifecycle events (started, completed).
    Compaction(CompactionEvent),
    /// Security events (injection defense: detection, boundary, screening, HITL).
    Security(SecurityEvent),
    /// Cron scheduler events (job completed, job failed).
    Cron(CronEvent),
    /// Hook execution events (triggered, completed).
    Hook(HookEvent),
    /// GDPR data subject rights events (erasure, export, reporting).
    Gdpr(GdprEvent),
}

impl BusEvent {
    /// Returns the dot-separated event type string for this event.
    ///
    /// Maps each leaf variant to a `"domain.action"` string matching the
    /// `broadcast_actions` TOML config format (e.g., `"skill.invoked"`,
    /// `"session.created"`, `"channel.message_received"`).
    ///
    /// The match is exhaustive, so the compiler will catch any future variants
    /// added to `BusEvent`.
    pub fn event_type_string(&self) -> &'static str {
        match self {
            BusEvent::Session(SessionEvent::Created { .. }) => "session.created",
            BusEvent::Session(SessionEvent::Closed { .. }) => "session.closed",
            BusEvent::Channel(ChannelEvent::MessageReceived { .. }) => "channel.message_received",
            BusEvent::Channel(ChannelEvent::MessageSent { .. }) => "channel.message_sent",
            BusEvent::Channel(ChannelEvent::ConnectionLost { .. }) => "channel.connection_lost",
            BusEvent::Channel(ChannelEvent::DeliveryFailed { .. }) => "channel.delivery_failed",
            BusEvent::Skill(SkillEvent::Invoked { .. }) => "skill.invoked",
            BusEvent::Skill(SkillEvent::Completed { .. }) => "skill.completed",
            BusEvent::Node(NodeEvent::Connected { .. }) => "node.connected",
            BusEvent::Node(NodeEvent::Disconnected { .. }) => "node.disconnected",
            BusEvent::Node(NodeEvent::Paired { .. }) => "node.paired",
            BusEvent::Node(NodeEvent::PairingFailed { .. }) => "node.pairing_failed",
            BusEvent::Node(NodeEvent::Stale { .. }) => "node.stale",
            BusEvent::Webhook(WebhookEvent::Triggered { .. }) => "webhook.triggered",
            BusEvent::Webhook(WebhookEvent::DeliveryAttempted { .. }) => {
                "webhook.delivery_attempted"
            }
            BusEvent::Batch(BatchEvent::Submitted { .. }) => "batch.submitted",
            BusEvent::Batch(BatchEvent::Completed { .. }) => "batch.completed",
            BusEvent::Resilience(ResilienceEvent::CircuitBreakerStateChanged { .. }) => {
                "resilience.circuit_breaker_state_changed"
            }
            BusEvent::Resilience(ResilienceEvent::DegradationLevelChanged { .. }) => {
                "resilience.degradation_level_changed"
            }
            BusEvent::Classification(ClassificationEvent::Changed { .. }) => {
                "classification.changed"
            }
            BusEvent::Classification(ClassificationEvent::PiiDetected { .. }) => {
                "classification.pii_detected"
            }
            BusEvent::Classification(ClassificationEvent::Enforced { .. }) => {
                "classification.enforced"
            }
            BusEvent::Classification(ClassificationEvent::BulkChanged { .. }) => {
                "classification.bulk_changed"
            }
            BusEvent::Config(ConfigEvent::Changed { .. }) => "config.changed",
            BusEvent::Config(ConfigEvent::Reloaded { .. }) => "config.reloaded",
            BusEvent::Memory(MemoryEvent::Created { .. }) => "memory.created",
            BusEvent::Memory(MemoryEvent::Updated { .. }) => "memory.updated",
            BusEvent::Memory(MemoryEvent::Deleted { .. }) => "memory.deleted",
            BusEvent::Memory(MemoryEvent::Retrieved { .. }) => "memory.retrieved",
            BusEvent::Memory(MemoryEvent::Evicted { .. }) => "memory.evicted",
            BusEvent::Memory(MemoryEvent::Vec0Enabled { .. }) => "memory.vec0_enabled",
            BusEvent::Memory(MemoryEvent::Vec0FallbackTriggered { .. }) => {
                "memory.vec0_fallback_triggered"
            }
            BusEvent::Memory(MemoryEvent::Vec0PopulationComplete { .. }) => {
                "memory.vec0_population_complete"
            }
            BusEvent::Audit(AuditMetaEvent::Enabled { .. }) => "audit.enabled",
            BusEvent::Audit(AuditMetaEvent::Disabled { .. }) => "audit.disabled",
            BusEvent::Audit(AuditMetaEvent::Erased { .. }) => "audit.erased",
            BusEvent::Api(ApiEvent::Request { .. }) => "api.request",
            BusEvent::Provider(ProviderEvent::Called { .. }) => "provider.called",
            BusEvent::Compaction(CompactionEvent::Started { .. }) => "compaction.started",
            BusEvent::Compaction(CompactionEvent::Completed { .. }) => "compaction.completed",
            BusEvent::Security(SecurityEvent::InputDetection { .. }) => "security.input_detection",
            BusEvent::Security(SecurityEvent::BoundaryFailure { .. }) => {
                "security.boundary_failure"
            }
            BusEvent::Security(SecurityEvent::OutputScreening { .. }) => {
                "security.output_screening"
            }
            BusEvent::Security(SecurityEvent::HitlPrompt { .. }) => "security.hitl_prompt",
            BusEvent::Security(SecurityEvent::CanaryDetection { .. }) => {
                "security.canary_detection"
            }
            BusEvent::Cron(CronEvent::Completed { .. }) => "cron.completed",
            BusEvent::Cron(CronEvent::Failed { .. }) => "cron.failed",
            BusEvent::Hook(HookEvent::Triggered { .. }) => "hook.triggered",
            BusEvent::Hook(HookEvent::Completed { .. }) => "hook.completed",
            BusEvent::Gdpr(GdprEvent::ErasureStarted { .. }) => "gdpr.erasure_started",
            BusEvent::Gdpr(GdprEvent::ErasureCompleted { .. }) => "gdpr.erasure_completed",
            BusEvent::Gdpr(GdprEvent::ExportCompleted { .. }) => "gdpr.export_completed",
            BusEvent::Gdpr(GdprEvent::ReportGenerated { .. }) => "gdpr.report_generated",
        }
    }
}

// --- Session events ---

/// Events related to session lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionEvent {
    /// A new session was created.
    Created {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Session identifier.
        session_id: String,
        /// Channel the session was created on.
        channel: String,
    },
    /// A session was closed.
    Closed {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Session identifier.
        session_id: String,
    },
}

// --- Channel events ---

/// Events related to channel message flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelEvent {
    /// A message was received from a channel.
    MessageReceived {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Channel name (e.g., "telegram", "gateway").
        channel: String,
        /// Sender identifier from the channel.
        sender_id: String,
        /// Message content (for bridging). None if not applicable.
        #[serde(default)]
        content: Option<String>,
        /// Human-readable sender name (for bridging attribution).
        #[serde(default)]
        sender_name: Option<String>,
        /// Whether this message was forwarded by the bridge (loop prevention).
        #[serde(default)]
        is_bridged: bool,
    },
    /// A message was sent through a channel.
    MessageSent {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Channel name.
        channel: String,
    },
    /// A channel connection was lost.
    ConnectionLost {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Channel name.
        channel: String,
        /// Error details.
        error: String,
    },
    /// A message delivery failed.
    DeliveryFailed {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Channel name.
        channel: String,
        /// Recipient identifier.
        recipient: String,
        /// Error details.
        error: String,
    },
}

// --- Skill events ---

/// Events related to skill (WASM plugin) execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillEvent {
    /// A skill was invoked.
    Invoked {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Name of the skill.
        skill_name: String,
        /// Session that triggered the invocation.
        session_id: String,
    },
    /// A skill invocation completed.
    Completed {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Name of the skill.
        skill_name: String,
        /// Whether the skill returned an error.
        is_error: bool,
    },
}

// --- Node events ---

/// Events related to node connections in the fleet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeEvent {
    /// A node connected to the agent.
    Connected {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Node identifier.
        node_id: String,
    },
    /// A node disconnected from the agent.
    Disconnected {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Node identifier.
        node_id: String,
        /// Reason for disconnection.
        reason: String,
    },
    /// A new node was successfully paired.
    Paired {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Node identifier.
        node_id: String,
        /// Node display name.
        name: String,
    },
    /// A pairing attempt failed.
    PairingFailed {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Reason for failure.
        reason: String,
    },
    /// A node has become stale (missed heartbeats).
    Stale {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Node identifier.
        node_id: String,
        /// Seconds since last heartbeat.
        last_seen_secs_ago: u64,
    },
}

// --- Webhook events ---

/// Events related to webhook triggers and delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebhookEvent {
    /// A webhook was triggered.
    Triggered {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Webhook identifier.
        webhook_id: String,
        /// Type of event that triggered the webhook.
        event_type: String,
    },
    /// A webhook delivery was attempted.
    DeliveryAttempted {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Webhook identifier.
        webhook_id: String,
        /// HTTP status code from delivery attempt.
        status_code: u16,
        /// Whether delivery was successful.
        success: bool,
    },
}

// --- Batch events ---

/// Events related to batch processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BatchEvent {
    /// A batch was submitted for processing.
    Submitted {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Batch identifier.
        batch_id: String,
        /// Number of items in the batch.
        item_count: usize,
    },
    /// A batch completed processing.
    Completed {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Batch identifier.
        batch_id: String,
        /// Number of successfully processed items.
        success_count: usize,
        /// Number of items that failed.
        error_count: usize,
    },
}

// --- Classification events ---

/// Events related to data classification changes, PII detection, and enforcement.
///
/// All fields are `String` (not `DataClassification` enum) to avoid a dependency
/// from `blufio-bus` on `blufio-core`. Events carry metadata only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClassificationEvent {
    /// A classification level was changed on an entity.
    Changed {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Type of entity (e.g., "memory", "message", "session").
        entity_type: String,
        /// Entity identifier.
        entity_id: String,
        /// Previous classification level.
        old_level: String,
        /// New classification level.
        new_level: String,
        /// Who/what initiated the change (e.g., "user", "auto_pii", "bulk").
        changed_by: String,
    },
    /// PII was detected in an entity's content.
    ///
    /// Carries only PII type names and counts -- never actual PII values.
    PiiDetected {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Type of entity (e.g., "memory", "message").
        entity_type: String,
        /// Entity identifier.
        entity_id: String,
        /// Types of PII detected (e.g., "email", "phone").
        pii_types: Vec<String>,
        /// Total number of PII matches found.
        count: usize,
    },
    /// A classification enforcement action occurred (e.g., blocked export).
    Enforced {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Type of entity.
        entity_type: String,
        /// Entity identifier.
        entity_id: String,
        /// Classification level that triggered enforcement.
        level: String,
        /// The action that was blocked (e.g., "export", "include_in_context").
        action_blocked: String,
    },
    /// A bulk classification change was applied.
    BulkChanged {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Type of entities changed.
        entity_type: String,
        /// Number of entities changed.
        count: usize,
        /// Previous classification level.
        old_level: String,
        /// New classification level.
        new_level: String,
        /// Who/what initiated the change.
        changed_by: String,
    },
}

// --- Resilience events ---

/// Events related to circuit breaker and degradation ladder state changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResilienceEvent {
    /// A circuit breaker changed state.
    CircuitBreakerStateChanged {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Name of the dependency whose breaker changed.
        dependency: String,
        /// Previous state (`"closed"`, `"open"`, `"half_open"`).
        from_state: String,
        /// New state.
        to_state: String,
    },
    /// The system-wide degradation level changed.
    DegradationLevelChanged {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Previous level (0-5).
        from_level: u8,
        /// New level (0-5).
        to_level: u8,
        /// Human-readable name of previous level.
        from_name: String,
        /// Human-readable name of new level.
        to_name: String,
        /// Reason for the level change.
        reason: String,
    },
}

// --- Config events ---

/// Events related to configuration changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigEvent {
    /// A configuration value was changed.
    Changed {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Configuration key that was changed.
        key: String,
        /// Previous value (None if newly added).
        old_value: Option<String>,
        /// New value (None if removed).
        new_value: Option<String>,
    },
    /// Configuration was reloaded from source.
    Reloaded {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Source of the reload (e.g., "file", "hot_reload").
        source: String,
    },
}

// --- Memory events ---

/// Events related to memory CRUD operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryEvent {
    /// A new memory was created.
    Created {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Memory identifier.
        memory_id: String,
        /// Source of the memory (e.g., "conversation", "manual").
        source: String,
    },
    /// An existing memory was updated.
    Updated {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Memory identifier.
        memory_id: String,
    },
    /// A memory was deleted (soft-delete).
    Deleted {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Memory identifier.
        memory_id: String,
    },
    /// A memory was retrieved (read access).
    Retrieved {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Memory identifier.
        memory_id: String,
        /// Query that triggered the retrieval.
        query: String,
    },
    /// A batch of memories was evicted during a sweep.
    Evicted {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Number of memories evicted in this sweep.
        count: u32,
        /// Lowest composite score among evicted memories.
        lowest_score: f64,
        /// Highest composite score among evicted memories.
        highest_score: f64,
    },
    /// vec0 backend was enabled at startup.
    Vec0Enabled {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
    },
    /// vec0 query or registration failed; fell back to in-memory search.
    Vec0FallbackTriggered {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Reason for the fallback.
        reason: String,
    },
    /// vec0 startup population completed.
    Vec0PopulationComplete {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Number of memories populated into vec0.
        count: usize,
        /// Duration of the population in milliseconds.
        duration_ms: u64,
    },
}

// --- Audit meta-events ---

/// Meta-events about the audit subsystem itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditMetaEvent {
    /// Audit trail was enabled.
    Enabled {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
    },
    /// Audit trail was disabled.
    Disabled {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
    },
    /// GDPR erasure was performed on audit entries.
    Erased {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// SHA-256 hash of the user ID that was erased (not plaintext).
        user_id_hash: String,
    },
}

// --- API events ---

/// Events related to API requests through the gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiEvent {
    /// A mutating HTTP request was processed.
    Request {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// HTTP method (POST, PUT, DELETE).
        method: String,
        /// Request path.
        path: String,
        /// HTTP response status code.
        status: u16,
        /// Actor who made the request (e.g., "user:123", "api-key:key_id", "anonymous").
        actor: String,
    },
}

// --- Provider events ---

/// Events related to LLM provider calls.
///
/// Carries metadata only (model, tokens, cost, latency) -- never prompt/response content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProviderEvent {
    /// An LLM provider was called.
    Called {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Provider name (e.g., "anthropic", "openai").
        provider: String,
        /// Model name used for the call.
        model: String,
        /// Number of input tokens.
        input_tokens: u32,
        /// Number of output tokens.
        output_tokens: u32,
        /// Estimated cost in USD.
        cost_usd: f64,
        /// Call latency in milliseconds.
        latency_ms: u64,
        /// Whether the call succeeded.
        success: bool,
        /// Session identifier.
        session_id: String,
    },
}

// --- Compaction events ---

/// Events related to context compaction lifecycle.
///
/// All fields are `String` (not cross-crate types) following the same pattern
/// as other event sub-enums to avoid blufio-bus -> blufio-core dependency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompactionEvent {
    /// A compaction pass was started.
    Started {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Session identifier being compacted.
        session_id: String,
        /// Compaction level (e.g., "l1", "l2", "l3").
        level: String,
        /// Number of messages being compacted.
        message_count: u32,
    },
    /// A compaction pass completed.
    Completed {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Session identifier that was compacted.
        session_id: String,
        /// Compaction level (e.g., "l1", "l2", "l3").
        level: String,
        /// Quality score of the generated summary.
        quality_score: f64,
        /// Number of tokens saved by compaction.
        tokens_saved: u32,
        /// Duration of the compaction pass in milliseconds.
        duration_ms: u64,
    },
}

// --- Security events ---

/// Security events from the injection defense subsystem.
///
/// All fields are `String` (or `f64` / `Vec<String>`) following the established
/// pattern where event sub-enums avoid cross-crate type dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityEvent {
    /// L1: Input injection pattern detected.
    InputDetection {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Message-level correlation ID for cross-layer tracing.
        correlation_id: String,
        /// Input source type (`"user"`, `"mcp"`, `"wasm"`).
        source_type: String,
        /// Source name (server/skill name, empty for user input).
        source_name: String,
        /// Confidence score (0.0 - 1.0).
        score: f64,
        /// Action taken (`"clean"`, `"logged"`, `"blocked"`, `"dry_run"`).
        action: String,
        /// Matched pattern categories.
        categories: Vec<String>,
        /// Full input content for forensic analysis.
        content: String,
    },
    /// L3: HMAC boundary validation failure.
    BoundaryFailure {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Message-level correlation ID.
        correlation_id: String,
        /// Zone that failed validation (`"static"`, `"conditional"`, `"dynamic"`).
        zone: String,
        /// Zone provenance source.
        source: String,
        /// Action taken (`"stripped"`).
        action: String,
        /// Corrupted content for forensic analysis.
        content: String,
    },
    /// L4: Output screening detection.
    OutputScreening {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Message-level correlation ID.
        correlation_id: String,
        /// Detection type (`"credential_leak"`, `"injection_relay"`).
        detection_type: String,
        /// Tool whose output was screened.
        tool_name: String,
        /// Action taken (`"redacted"`, `"blocked"`).
        action: String,
        /// Screened content for forensic analysis.
        content: String,
    },
    /// L5: HITL confirmation prompt.
    HitlPrompt {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Message-level correlation ID.
        correlation_id: String,
        /// Tool requiring confirmation.
        tool_name: String,
        /// Risk level (`"low"`, `"medium"`, `"high"`).
        risk_level: String,
        /// Action taken (`"approved"`, `"denied"`, `"timeout"`).
        action: String,
        /// Session identifier.
        session_id: String,
    },
    /// Canary token leak detected in LLM output.
    CanaryDetection {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Message-level correlation ID.
        correlation_id: String,
        /// Token type that was detected: `"global"` or `"session"`.
        token_type: String,
        /// Action taken: `"blocked"`.
        action: String,
        /// Truncated output for forensic analysis (first 500 chars).
        content: String,
    },
}

// --- Cron events ---

/// Cron scheduler events.
///
/// All fields are `String` (or `u64`) following the established pattern where
/// event sub-enums avoid cross-crate type dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CronEvent {
    /// A cron job completed execution.
    Completed {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Name of the cron job.
        job_name: String,
        /// Execution status (`"success"`, `"failed"`, `"timeout"`).
        status: String,
        /// Execution duration in milliseconds.
        duration_ms: u64,
    },
    /// A cron job failed.
    Failed {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Name of the cron job.
        job_name: String,
        /// Error description.
        error: String,
    },
}

// --- Hook events ---

/// Hook execution events.
///
/// All fields are `String` (or `u32` / `u64` / `Option<String>`) following
/// the established pattern where event sub-enums avoid cross-crate type
/// dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HookEvent {
    /// A hook was triggered for execution.
    Triggered {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Name of the hook.
        hook_name: String,
        /// Event type that triggered this hook.
        trigger_event: String,
        /// Execution priority.
        priority: u32,
    },
    /// A hook completed execution.
    Completed {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// Name of the hook.
        hook_name: String,
        /// Event type that triggered this hook.
        trigger_event: String,
        /// Execution status ("success", "failed", "timeout", "skipped").
        status: String,
        /// Execution duration in milliseconds.
        duration_ms: u64,
        /// Captured stdout (None if empty).
        stdout: Option<String>,
    },
}

// --- GDPR events ---

/// GDPR data subject rights events.
///
/// All fields are `String` (or `u64`) following the established pattern where
/// event sub-enums avoid cross-crate type dependencies. User IDs are always
/// SHA-256 hashed -- never plaintext -- to prevent PII leakage through the
/// event bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GdprEvent {
    /// A GDPR erasure operation was started.
    ErasureStarted {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// SHA-256 hash of the user ID being erased (not plaintext).
        user_id_hash: String,
    },
    /// A GDPR erasure operation completed.
    ErasureCompleted {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// SHA-256 hash of the user ID that was erased (not plaintext).
        user_id_hash: String,
        /// Number of messages deleted.
        messages_deleted: u64,
        /// Number of sessions deleted.
        sessions_deleted: u64,
        /// Number of memories deleted.
        memories_deleted: u64,
        /// Number of compaction archives deleted.
        archives_deleted: u64,
        /// Number of cost records anonymized.
        cost_records_anonymized: u64,
        /// Duration of the erasure operation in milliseconds.
        duration_ms: u64,
    },
    /// A GDPR data export completed.
    ExportCompleted {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// SHA-256 hash of the user ID whose data was exported (not plaintext).
        user_id_hash: String,
        /// Export format (e.g., "json", "csv").
        format: String,
        /// Path to the export file.
        file_path: String,
        /// Size of the export file in bytes.
        size_bytes: u64,
    },
    /// A GDPR transparency report was generated.
    ReportGenerated {
        /// Unique event identifier.
        event_id: String,
        /// ISO 8601 timestamp.
        timestamp: String,
        /// SHA-256 hash of the user ID for the report (not plaintext).
        user_id_hash: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_bus_event_variants_exist() {
        let _session = BusEvent::Session(SessionEvent::Created {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            session_id: "sess-1".into(),
            channel: "telegram".into(),
        });

        let _channel = BusEvent::Channel(ChannelEvent::MessageReceived {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            channel: "gateway".into(),
            sender_id: "user-1".into(),
            content: None,
            sender_name: None,
            is_bridged: false,
        });

        let _skill = BusEvent::Skill(SkillEvent::Invoked {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            skill_name: "weather".into(),
            session_id: "sess-1".into(),
        });

        let _node = BusEvent::Node(NodeEvent::Connected {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            node_id: "node-1".into(),
        });

        let _webhook = BusEvent::Webhook(WebhookEvent::Triggered {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            webhook_id: "wh-1".into(),
            event_type: "session.created".into(),
        });

        let _batch = BusEvent::Batch(BatchEvent::Submitted {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            batch_id: "batch-1".into(),
            item_count: 10,
        });

        let _resilience = BusEvent::Resilience(ResilienceEvent::CircuitBreakerStateChanged {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            dependency: "anthropic".into(),
            from_state: "closed".into(),
            to_state: "open".into(),
        });

        let _classification = BusEvent::Classification(ClassificationEvent::Changed {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            entity_type: "memory".into(),
            entity_id: "mem-1".into(),
            old_level: "internal".into(),
            new_level: "confidential".into(),
            changed_by: "auto_pii".into(),
        });

        let _config = BusEvent::Config(ConfigEvent::Changed {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            key: "provider.model".into(),
            old_value: Some("sonnet".into()),
            new_value: Some("opus".into()),
        });

        let _memory = BusEvent::Memory(MemoryEvent::Created {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            memory_id: "mem-1".into(),
            source: "conversation".into(),
        });

        let _vec0_enabled = BusEvent::Memory(MemoryEvent::Vec0Enabled {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
        });

        let _vec0_fallback = BusEvent::Memory(MemoryEvent::Vec0FallbackTriggered {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            reason: "extension not loaded".into(),
        });

        let _vec0_population = BusEvent::Memory(MemoryEvent::Vec0PopulationComplete {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            count: 100,
            duration_ms: 500,
        });

        let _audit = BusEvent::Audit(AuditMetaEvent::Enabled {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
        });

        let _api = BusEvent::Api(ApiEvent::Request {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            method: "POST".into(),
            path: "/v1/messages".into(),
            status: 200,
            actor: "user:123".into(),
        });

        let _provider = BusEvent::Provider(ProviderEvent::Called {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            provider: "anthropic".into(),
            model: "claude-sonnet-4-20250514".into(),
            input_tokens: 100,
            output_tokens: 50,
            cost_usd: 0.001,
            latency_ms: 500,
            success: true,
            session_id: "sess-1".into(),
        });

        let _compaction = BusEvent::Compaction(CompactionEvent::Started {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            session_id: "sess-1".into(),
            level: "l1".into(),
            message_count: 10,
        });

        let _security = BusEvent::Security(SecurityEvent::InputDetection {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            correlation_id: "corr-1".into(),
            source_type: "user".into(),
            source_name: String::new(),
            score: 0.75,
            action: "logged".into(),
            categories: vec!["role_hijacking".into()],
            content: "ignore previous instructions".into(),
        });

        let _cron = BusEvent::Cron(CronEvent::Completed {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            job_name: "backup".into(),
            status: "success".into(),
            duration_ms: 1234,
        });

        let _hook = BusEvent::Hook(HookEvent::Triggered {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            hook_name: "on-session-start".into(),
            trigger_event: "session.created".into(),
            priority: 10,
        });

        let _gdpr = BusEvent::Gdpr(GdprEvent::ErasureStarted {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            user_id_hash: "sha256hash".into(),
        });

        let _canary = BusEvent::Security(SecurityEvent::CanaryDetection {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            correlation_id: "corr-canary".into(),
            token_type: "global".into(),
            action: "blocked".into(),
            content: "leaked content".into(),
        });

        let _conn_lost = BusEvent::Channel(ChannelEvent::ConnectionLost {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            channel: "email".into(),
            error: "IMAP connection timeout".into(),
        });

        let _delivery_failed = BusEvent::Channel(ChannelEvent::DeliveryFailed {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            channel: "sms".into(),
            recipient: "+1234567890".into(),
            error: "Twilio 429".into(),
        });
    }

    #[test]
    fn bus_event_implements_clone() {
        let event = BusEvent::Session(SessionEvent::Created {
            event_id: new_event_id(),
            timestamp: now_timestamp(),
            session_id: "sess-1".into(),
            channel: "telegram".into(),
        });
        let cloned = event.clone();
        // Verify clone is independent (Debug format should match).
        assert_eq!(format!("{:?}", event), format!("{:?}", cloned));
    }

    #[test]
    fn session_event_serialize_deserialize_roundtrip() {
        let event = BusEvent::Session(SessionEvent::Created {
            event_id: "evt-123".into(),
            timestamp: "2026-03-05T00:00:00Z".into(),
            session_id: "sess-abc".into(),
            channel: "telegram".into(),
        });

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: BusEvent = serde_json::from_str(&json).unwrap();

        match deserialized {
            BusEvent::Session(SessionEvent::Created {
                event_id,
                timestamp,
                session_id,
                channel,
            }) => {
                assert_eq!(event_id, "evt-123");
                assert_eq!(timestamp, "2026-03-05T00:00:00Z");
                assert_eq!(session_id, "sess-abc");
                assert_eq!(channel, "telegram");
            }
            _ => panic!("expected Session::Created"),
        }
    }

    #[test]
    fn new_event_id_is_unique() {
        let id1 = new_event_id();
        let id2 = new_event_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn now_timestamp_is_nonempty() {
        let ts = now_timestamp();
        assert!(!ts.is_empty());
    }

    #[test]
    fn event_type_string_all_variants() {
        let cases: Vec<(BusEvent, &str)> = vec![
            (
                BusEvent::Session(SessionEvent::Created {
                    event_id: String::new(),
                    timestamp: String::new(),
                    session_id: String::new(),
                    channel: String::new(),
                }),
                "session.created",
            ),
            (
                BusEvent::Session(SessionEvent::Closed {
                    event_id: String::new(),
                    timestamp: String::new(),
                    session_id: String::new(),
                }),
                "session.closed",
            ),
            (
                BusEvent::Channel(ChannelEvent::MessageReceived {
                    event_id: String::new(),
                    timestamp: String::new(),
                    channel: String::new(),
                    sender_id: String::new(),
                    content: None,
                    sender_name: None,
                    is_bridged: false,
                }),
                "channel.message_received",
            ),
            (
                BusEvent::Channel(ChannelEvent::MessageSent {
                    event_id: String::new(),
                    timestamp: String::new(),
                    channel: String::new(),
                }),
                "channel.message_sent",
            ),
            (
                BusEvent::Channel(ChannelEvent::ConnectionLost {
                    event_id: String::new(),
                    timestamp: String::new(),
                    channel: String::new(),
                    error: String::new(),
                }),
                "channel.connection_lost",
            ),
            (
                BusEvent::Channel(ChannelEvent::DeliveryFailed {
                    event_id: String::new(),
                    timestamp: String::new(),
                    channel: String::new(),
                    recipient: String::new(),
                    error: String::new(),
                }),
                "channel.delivery_failed",
            ),
            (
                BusEvent::Skill(SkillEvent::Invoked {
                    event_id: String::new(),
                    timestamp: String::new(),
                    skill_name: String::new(),
                    session_id: String::new(),
                }),
                "skill.invoked",
            ),
            (
                BusEvent::Skill(SkillEvent::Completed {
                    event_id: String::new(),
                    timestamp: String::new(),
                    skill_name: String::new(),
                    is_error: false,
                }),
                "skill.completed",
            ),
            (
                BusEvent::Node(NodeEvent::Connected {
                    event_id: String::new(),
                    timestamp: String::new(),
                    node_id: String::new(),
                }),
                "node.connected",
            ),
            (
                BusEvent::Node(NodeEvent::Disconnected {
                    event_id: String::new(),
                    timestamp: String::new(),
                    node_id: String::new(),
                    reason: String::new(),
                }),
                "node.disconnected",
            ),
            (
                BusEvent::Node(NodeEvent::Paired {
                    event_id: String::new(),
                    timestamp: String::new(),
                    node_id: String::new(),
                    name: String::new(),
                }),
                "node.paired",
            ),
            (
                BusEvent::Node(NodeEvent::PairingFailed {
                    event_id: String::new(),
                    timestamp: String::new(),
                    reason: String::new(),
                }),
                "node.pairing_failed",
            ),
            (
                BusEvent::Node(NodeEvent::Stale {
                    event_id: String::new(),
                    timestamp: String::new(),
                    node_id: String::new(),
                    last_seen_secs_ago: 0,
                }),
                "node.stale",
            ),
            (
                BusEvent::Webhook(WebhookEvent::Triggered {
                    event_id: String::new(),
                    timestamp: String::new(),
                    webhook_id: String::new(),
                    event_type: String::new(),
                }),
                "webhook.triggered",
            ),
            (
                BusEvent::Webhook(WebhookEvent::DeliveryAttempted {
                    event_id: String::new(),
                    timestamp: String::new(),
                    webhook_id: String::new(),
                    status_code: 0,
                    success: false,
                }),
                "webhook.delivery_attempted",
            ),
            (
                BusEvent::Batch(BatchEvent::Submitted {
                    event_id: String::new(),
                    timestamp: String::new(),
                    batch_id: String::new(),
                    item_count: 0,
                }),
                "batch.submitted",
            ),
            (
                BusEvent::Batch(BatchEvent::Completed {
                    event_id: String::new(),
                    timestamp: String::new(),
                    batch_id: String::new(),
                    success_count: 0,
                    error_count: 0,
                }),
                "batch.completed",
            ),
            (
                BusEvent::Resilience(ResilienceEvent::CircuitBreakerStateChanged {
                    event_id: String::new(),
                    timestamp: String::new(),
                    dependency: String::new(),
                    from_state: String::new(),
                    to_state: String::new(),
                }),
                "resilience.circuit_breaker_state_changed",
            ),
            (
                BusEvent::Resilience(ResilienceEvent::DegradationLevelChanged {
                    event_id: String::new(),
                    timestamp: String::new(),
                    from_level: 0,
                    to_level: 1,
                    from_name: String::new(),
                    to_name: String::new(),
                    reason: String::new(),
                }),
                "resilience.degradation_level_changed",
            ),
            (
                BusEvent::Classification(ClassificationEvent::Changed {
                    event_id: String::new(),
                    timestamp: String::new(),
                    entity_type: String::new(),
                    entity_id: String::new(),
                    old_level: String::new(),
                    new_level: String::new(),
                    changed_by: String::new(),
                }),
                "classification.changed",
            ),
            (
                BusEvent::Classification(ClassificationEvent::PiiDetected {
                    event_id: String::new(),
                    timestamp: String::new(),
                    entity_type: String::new(),
                    entity_id: String::new(),
                    pii_types: vec![],
                    count: 0,
                }),
                "classification.pii_detected",
            ),
            (
                BusEvent::Classification(ClassificationEvent::Enforced {
                    event_id: String::new(),
                    timestamp: String::new(),
                    entity_type: String::new(),
                    entity_id: String::new(),
                    level: String::new(),
                    action_blocked: String::new(),
                }),
                "classification.enforced",
            ),
            (
                BusEvent::Classification(ClassificationEvent::BulkChanged {
                    event_id: String::new(),
                    timestamp: String::new(),
                    entity_type: String::new(),
                    count: 0,
                    old_level: String::new(),
                    new_level: String::new(),
                    changed_by: String::new(),
                }),
                "classification.bulk_changed",
            ),
            // Config events
            (
                BusEvent::Config(ConfigEvent::Changed {
                    event_id: String::new(),
                    timestamp: String::new(),
                    key: String::new(),
                    old_value: None,
                    new_value: None,
                }),
                "config.changed",
            ),
            (
                BusEvent::Config(ConfigEvent::Reloaded {
                    event_id: String::new(),
                    timestamp: String::new(),
                    source: String::new(),
                }),
                "config.reloaded",
            ),
            // Memory events
            (
                BusEvent::Memory(MemoryEvent::Created {
                    event_id: String::new(),
                    timestamp: String::new(),
                    memory_id: String::new(),
                    source: String::new(),
                }),
                "memory.created",
            ),
            (
                BusEvent::Memory(MemoryEvent::Updated {
                    event_id: String::new(),
                    timestamp: String::new(),
                    memory_id: String::new(),
                }),
                "memory.updated",
            ),
            (
                BusEvent::Memory(MemoryEvent::Deleted {
                    event_id: String::new(),
                    timestamp: String::new(),
                    memory_id: String::new(),
                }),
                "memory.deleted",
            ),
            (
                BusEvent::Memory(MemoryEvent::Retrieved {
                    event_id: String::new(),
                    timestamp: String::new(),
                    memory_id: String::new(),
                    query: String::new(),
                }),
                "memory.retrieved",
            ),
            (
                BusEvent::Memory(MemoryEvent::Evicted {
                    event_id: String::new(),
                    timestamp: String::new(),
                    count: 0,
                    lowest_score: 0.0,
                    highest_score: 0.0,
                }),
                "memory.evicted",
            ),
            (
                BusEvent::Memory(MemoryEvent::Vec0Enabled {
                    event_id: String::new(),
                    timestamp: String::new(),
                }),
                "memory.vec0_enabled",
            ),
            (
                BusEvent::Memory(MemoryEvent::Vec0FallbackTriggered {
                    event_id: String::new(),
                    timestamp: String::new(),
                    reason: String::new(),
                }),
                "memory.vec0_fallback_triggered",
            ),
            (
                BusEvent::Memory(MemoryEvent::Vec0PopulationComplete {
                    event_id: String::new(),
                    timestamp: String::new(),
                    count: 0,
                    duration_ms: 0,
                }),
                "memory.vec0_population_complete",
            ),
            // Audit events
            (
                BusEvent::Audit(AuditMetaEvent::Enabled {
                    event_id: String::new(),
                    timestamp: String::new(),
                }),
                "audit.enabled",
            ),
            (
                BusEvent::Audit(AuditMetaEvent::Disabled {
                    event_id: String::new(),
                    timestamp: String::new(),
                }),
                "audit.disabled",
            ),
            (
                BusEvent::Audit(AuditMetaEvent::Erased {
                    event_id: String::new(),
                    timestamp: String::new(),
                    user_id_hash: String::new(),
                }),
                "audit.erased",
            ),
            // Api events
            (
                BusEvent::Api(ApiEvent::Request {
                    event_id: String::new(),
                    timestamp: String::new(),
                    method: String::new(),
                    path: String::new(),
                    status: 0,
                    actor: String::new(),
                }),
                "api.request",
            ),
            // Provider events
            (
                BusEvent::Provider(ProviderEvent::Called {
                    event_id: String::new(),
                    timestamp: String::new(),
                    provider: String::new(),
                    model: String::new(),
                    input_tokens: 0,
                    output_tokens: 0,
                    cost_usd: 0.0,
                    latency_ms: 0,
                    success: false,
                    session_id: String::new(),
                }),
                "provider.called",
            ),
            // Compaction events
            (
                BusEvent::Compaction(CompactionEvent::Started {
                    event_id: String::new(),
                    timestamp: String::new(),
                    session_id: String::new(),
                    level: String::new(),
                    message_count: 0,
                }),
                "compaction.started",
            ),
            (
                BusEvent::Compaction(CompactionEvent::Completed {
                    event_id: String::new(),
                    timestamp: String::new(),
                    session_id: String::new(),
                    level: String::new(),
                    quality_score: 0.0,
                    tokens_saved: 0,
                    duration_ms: 0,
                }),
                "compaction.completed",
            ),
            // Security events
            (
                BusEvent::Security(SecurityEvent::InputDetection {
                    event_id: String::new(),
                    timestamp: String::new(),
                    correlation_id: String::new(),
                    source_type: String::new(),
                    source_name: String::new(),
                    score: 0.0,
                    action: String::new(),
                    categories: vec![],
                    content: String::new(),
                }),
                "security.input_detection",
            ),
            (
                BusEvent::Security(SecurityEvent::BoundaryFailure {
                    event_id: String::new(),
                    timestamp: String::new(),
                    correlation_id: String::new(),
                    zone: String::new(),
                    source: String::new(),
                    action: String::new(),
                    content: String::new(),
                }),
                "security.boundary_failure",
            ),
            (
                BusEvent::Security(SecurityEvent::OutputScreening {
                    event_id: String::new(),
                    timestamp: String::new(),
                    correlation_id: String::new(),
                    detection_type: String::new(),
                    tool_name: String::new(),
                    action: String::new(),
                    content: String::new(),
                }),
                "security.output_screening",
            ),
            (
                BusEvent::Security(SecurityEvent::HitlPrompt {
                    event_id: String::new(),
                    timestamp: String::new(),
                    correlation_id: String::new(),
                    tool_name: String::new(),
                    risk_level: String::new(),
                    action: String::new(),
                    session_id: String::new(),
                }),
                "security.hitl_prompt",
            ),
            (
                BusEvent::Security(SecurityEvent::CanaryDetection {
                    event_id: String::new(),
                    timestamp: String::new(),
                    correlation_id: String::new(),
                    token_type: String::new(),
                    action: String::new(),
                    content: String::new(),
                }),
                "security.canary_detection",
            ),
            // Cron events
            (
                BusEvent::Cron(CronEvent::Completed {
                    event_id: String::new(),
                    timestamp: String::new(),
                    job_name: String::new(),
                    status: String::new(),
                    duration_ms: 0,
                }),
                "cron.completed",
            ),
            (
                BusEvent::Cron(CronEvent::Failed {
                    event_id: String::new(),
                    timestamp: String::new(),
                    job_name: String::new(),
                    error: String::new(),
                }),
                "cron.failed",
            ),
            // Hook events
            (
                BusEvent::Hook(HookEvent::Triggered {
                    event_id: String::new(),
                    timestamp: String::new(),
                    hook_name: String::new(),
                    trigger_event: String::new(),
                    priority: 0,
                }),
                "hook.triggered",
            ),
            (
                BusEvent::Hook(HookEvent::Completed {
                    event_id: String::new(),
                    timestamp: String::new(),
                    hook_name: String::new(),
                    trigger_event: String::new(),
                    status: String::new(),
                    duration_ms: 0,
                    stdout: None,
                }),
                "hook.completed",
            ),
            // GDPR events
            (
                BusEvent::Gdpr(GdprEvent::ErasureStarted {
                    event_id: String::new(),
                    timestamp: String::new(),
                    user_id_hash: String::new(),
                }),
                "gdpr.erasure_started",
            ),
            (
                BusEvent::Gdpr(GdprEvent::ErasureCompleted {
                    event_id: String::new(),
                    timestamp: String::new(),
                    user_id_hash: String::new(),
                    messages_deleted: 0,
                    sessions_deleted: 0,
                    memories_deleted: 0,
                    archives_deleted: 0,
                    cost_records_anonymized: 0,
                    duration_ms: 0,
                }),
                "gdpr.erasure_completed",
            ),
            (
                BusEvent::Gdpr(GdprEvent::ExportCompleted {
                    event_id: String::new(),
                    timestamp: String::new(),
                    user_id_hash: String::new(),
                    format: String::new(),
                    file_path: String::new(),
                    size_bytes: 0,
                }),
                "gdpr.export_completed",
            ),
            (
                BusEvent::Gdpr(GdprEvent::ReportGenerated {
                    event_id: String::new(),
                    timestamp: String::new(),
                    user_id_hash: String::new(),
                }),
                "gdpr.report_generated",
            ),
        ];

        for (event, expected) in &cases {
            assert_eq!(
                event.event_type_string(),
                *expected,
                "mismatch for {:?}",
                std::mem::discriminant(event)
            );
        }
    }

    #[test]
    fn resilience_circuit_breaker_state_changed_roundtrip() {
        let event = BusEvent::Resilience(ResilienceEvent::CircuitBreakerStateChanged {
            event_id: "evt-cb-1".into(),
            timestamp: "2026-03-09T00:00:00Z".into(),
            dependency: "anthropic".into(),
            from_state: "closed".into(),
            to_state: "open".into(),
        });

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: BusEvent = serde_json::from_str(&json).unwrap();

        match deserialized {
            BusEvent::Resilience(ResilienceEvent::CircuitBreakerStateChanged {
                event_id,
                dependency,
                from_state,
                to_state,
                ..
            }) => {
                assert_eq!(event_id, "evt-cb-1");
                assert_eq!(dependency, "anthropic");
                assert_eq!(from_state, "closed");
                assert_eq!(to_state, "open");
            }
            _ => panic!("expected Resilience::CircuitBreakerStateChanged"),
        }
    }

    #[test]
    fn classification_changed_roundtrip() {
        let event = BusEvent::Classification(ClassificationEvent::Changed {
            event_id: "evt-cls-1".into(),
            timestamp: "2026-03-10T00:00:00Z".into(),
            entity_type: "memory".into(),
            entity_id: "mem-42".into(),
            old_level: "internal".into(),
            new_level: "confidential".into(),
            changed_by: "auto_pii".into(),
        });

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: BusEvent = serde_json::from_str(&json).unwrap();

        match deserialized {
            BusEvent::Classification(ClassificationEvent::Changed {
                event_id,
                entity_type,
                entity_id,
                old_level,
                new_level,
                changed_by,
                ..
            }) => {
                assert_eq!(event_id, "evt-cls-1");
                assert_eq!(entity_type, "memory");
                assert_eq!(entity_id, "mem-42");
                assert_eq!(old_level, "internal");
                assert_eq!(new_level, "confidential");
                assert_eq!(changed_by, "auto_pii");
            }
            _ => panic!("expected Classification::Changed"),
        }
    }

    #[test]
    fn classification_pii_detected_roundtrip() {
        let event = BusEvent::Classification(ClassificationEvent::PiiDetected {
            event_id: "evt-pii-1".into(),
            timestamp: "2026-03-10T00:00:00Z".into(),
            entity_type: "message".into(),
            entity_id: "msg-99".into(),
            pii_types: vec!["email".into(), "phone".into()],
            count: 3,
        });

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: BusEvent = serde_json::from_str(&json).unwrap();

        match deserialized {
            BusEvent::Classification(ClassificationEvent::PiiDetected {
                pii_types, count, ..
            }) => {
                assert_eq!(pii_types, vec!["email", "phone"]);
                assert_eq!(count, 3);
            }
            _ => panic!("expected Classification::PiiDetected"),
        }
    }

    #[test]
    fn resilience_degradation_level_changed_roundtrip() {
        let event = BusEvent::Resilience(ResilienceEvent::DegradationLevelChanged {
            event_id: "evt-deg-1".into(),
            timestamp: "2026-03-09T00:00:00Z".into(),
            from_level: 0,
            to_level: 2,
            from_name: "FullyOperational".into(),
            to_name: "ReducedFunctionality".into(),
            reason: "Primary provider breaker opened".into(),
        });

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: BusEvent = serde_json::from_str(&json).unwrap();

        match deserialized {
            BusEvent::Resilience(ResilienceEvent::DegradationLevelChanged {
                from_level,
                to_level,
                from_name,
                to_name,
                reason,
                ..
            }) => {
                assert_eq!(from_level, 0);
                assert_eq!(to_level, 2);
                assert_eq!(from_name, "FullyOperational");
                assert_eq!(to_name, "ReducedFunctionality");
                assert_eq!(reason, "Primary provider breaker opened");
            }
            _ => panic!("expected Resilience::DegradationLevelChanged"),
        }
    }

    #[test]
    fn memory_event_vec0_enabled_roundtrip() {
        let event = BusEvent::Memory(MemoryEvent::Vec0Enabled {
            event_id: "evt-v0-1".into(),
            timestamp: "2026-03-13T00:00:00Z".into(),
        });

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: BusEvent = serde_json::from_str(&json).unwrap();

        match deserialized {
            BusEvent::Memory(MemoryEvent::Vec0Enabled {
                event_id,
                timestamp,
            }) => {
                assert_eq!(event_id, "evt-v0-1");
                assert_eq!(timestamp, "2026-03-13T00:00:00Z");
            }
            _ => panic!("expected Memory::Vec0Enabled"),
        }

        assert_eq!(event.event_type_string(), "memory.vec0_enabled");
    }

    #[test]
    fn memory_event_vec0_fallback_triggered_roundtrip() {
        let event = BusEvent::Memory(MemoryEvent::Vec0FallbackTriggered {
            event_id: "evt-v0-2".into(),
            timestamp: "2026-03-13T00:00:00Z".into(),
            reason: "extension not loaded".into(),
        });

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: BusEvent = serde_json::from_str(&json).unwrap();

        match deserialized {
            BusEvent::Memory(MemoryEvent::Vec0FallbackTriggered {
                event_id, reason, ..
            }) => {
                assert_eq!(event_id, "evt-v0-2");
                assert_eq!(reason, "extension not loaded");
            }
            _ => panic!("expected Memory::Vec0FallbackTriggered"),
        }

        assert_eq!(event.event_type_string(), "memory.vec0_fallback_triggered");
    }

    #[test]
    fn memory_event_vec0_population_complete_roundtrip() {
        let event = BusEvent::Memory(MemoryEvent::Vec0PopulationComplete {
            event_id: "evt-v0-3".into(),
            timestamp: "2026-03-13T00:00:00Z".into(),
            count: 500,
            duration_ms: 1200,
        });

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: BusEvent = serde_json::from_str(&json).unwrap();

        match deserialized {
            BusEvent::Memory(MemoryEvent::Vec0PopulationComplete {
                event_id,
                count,
                duration_ms,
                ..
            }) => {
                assert_eq!(event_id, "evt-v0-3");
                assert_eq!(count, 500);
                assert_eq!(duration_ms, 1200);
            }
            _ => panic!("expected Memory::Vec0PopulationComplete"),
        }

        assert_eq!(event.event_type_string(), "memory.vec0_population_complete");
    }
}
