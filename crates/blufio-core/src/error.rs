// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Error types for the Blufio agent framework.
//!
//! This module provides:
//! - [`BlufioError`] -- the primary error enum used across all Blufio crates
//! - Sub-enums for each subsystem (Provider, Channel, Storage, Skill, Mcp, Migration)
//! - [`ErrorContext`] -- optional metadata carried by structured error variants
//! - Classification methods: `is_retryable()`, `severity()`, `category()`,
//!   `failure_mode()`, `trips_circuit_breaker()`, `suggested_backoff()`, `user_message()`
//! - Constructor helpers for common error creation patterns
//! - [`error_log!`] macro for structured logging with classification fields

use std::borrow::Cow;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use strum::Display;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Classification enums
// ---------------------------------------------------------------------------

/// Severity level for error classification.
///
/// Ordered from most severe (Fatal) to least severe (Info).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Display,
)]
#[non_exhaustive]
pub enum Severity {
    /// Informational -- not a real error (e.g., model not found is a user mistake).
    Info,
    /// Warning -- something unexpected but non-critical (e.g., validation failure).
    Warning,
    /// Error -- a real failure that needs attention.
    Error,
    /// Fatal -- cannot continue (e.g., bad config, budget exhausted).
    Fatal,
}

/// High-level error category identifying the subsystem that produced the error.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Display,
)]
#[non_exhaustive]
pub enum ErrorCategory {
    Provider,
    Channel,
    Storage,
    Security,
    Config,
    Mcp,
    Skill,
    Migration,
    Internal,
}

/// Failure mode describing *what went wrong* irrespective of subsystem.
///
/// `is_retryable()` is derived purely from this enum.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Display,
)]
#[non_exhaustive]
pub enum FailureMode {
    Network,
    Auth,
    RateLimit,
    Timeout,
    Validation,
    Internal,
    ResourceExhausted,
    Unavailable,
}

impl FailureMode {
    /// Whether this failure mode is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            FailureMode::Network
                | FailureMode::RateLimit
                | FailureMode::Timeout
                | FailureMode::Unavailable
        )
    }
}

// ---------------------------------------------------------------------------
// Sub-enums (kind fields)
// ---------------------------------------------------------------------------

/// Specific kind of provider error.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Display,
)]
#[non_exhaustive]
pub enum ProviderErrorKind {
    RateLimited,
    AuthFailed,
    ServerError,
    Timeout,
    ModelNotFound,
}

/// Specific kind of channel error.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Display,
)]
#[non_exhaustive]
pub enum ChannelErrorKind {
    DeliveryFailed,
    ConnectionLost,
    RateLimited,
    MessageTooLarge,
    UnsupportedContent,
}

/// Specific kind of skill error.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Display,
)]
#[non_exhaustive]
pub enum SkillErrorKind {
    ExecutionFailed,
    CapabilityDenied,
    SandboxTimeout,
    CompilationFailed,
}

/// Specific kind of storage error.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Display,
)]
#[non_exhaustive]
pub enum StorageErrorKind {
    Busy,
    Corruption,
    SchemaError,
    DiskFull,
    ConnectionFailed,
}

/// Specific kind of MCP error.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Display,
)]
#[non_exhaustive]
pub enum McpErrorKind {
    ConnectionFailed,
    ToolExecutionFailed,
    Timeout,
    AuthFailed,
    ProtocolError,
}

/// Specific kind of migration error.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Display,
)]
#[non_exhaustive]
pub enum MigrationErrorKind {
    SchemaFailed,
    DataCorruption,
    VersionMismatch,
}

// ---------------------------------------------------------------------------
// ErrorContext
// ---------------------------------------------------------------------------

/// Optional metadata carried by structured error variants.
#[derive(Debug, Clone, Default)]
pub struct ErrorContext {
    /// HTTP status code from a provider or external service.
    pub http_status: Option<u16>,
    /// Name of the provider that produced the error (e.g., "anthropic").
    pub provider_name: Option<String>,
    /// Name of the channel that produced the error (e.g., "telegram").
    pub channel_name: Option<String>,
    /// How long to wait before retrying (from provider Retry-After header).
    pub retry_after: Option<Duration>,
    /// Request ID for correlation with external service logs.
    pub request_id: Option<String>,
}

// ---------------------------------------------------------------------------
// BlufioError
// ---------------------------------------------------------------------------

/// The primary error type used across all Blufio adapter traits and core operations.
#[derive(Debug, Error)]
pub enum BlufioError {
    /// LLM provider errors (API failure, token limits, model not found).
    #[error("{}: {kind}", context.provider_name.as_deref().unwrap_or("provider"))]
    Provider {
        kind: ProviderErrorKind,
        context: ErrorContext,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Channel adapter errors (connection failure, message format, rate limiting).
    #[error("{}: {kind}", context.channel_name.as_deref().unwrap_or("channel"))]
    Channel {
        kind: ChannelErrorKind,
        context: ErrorContext,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Storage backend errors (database connection, query failure, serialization).
    #[error("storage: {kind}: {source}")]
    Storage {
        kind: StorageErrorKind,
        context: ErrorContext,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Skill or tool execution errors.
    #[error("skill: {kind}{}", context.request_id.as_ref().map(|m| format!(": {m}")).unwrap_or_default())]
    Skill {
        kind: SkillErrorKind,
        context: ErrorContext,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// MCP protocol and connection errors.
    #[error("mcp: {kind}{}", context.request_id.as_ref().map(|m| format!(": {m}")).unwrap_or_default())]
    Mcp {
        kind: McpErrorKind,
        context: ErrorContext,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Migration errors (data import, format conversion, source detection).
    #[error("migration: {kind}{}", context.request_id.as_ref().map(|m| format!(": {m}")).unwrap_or_default())]
    Migration {
        kind: MigrationErrorKind,
        context: ErrorContext,
    },

    // --- Unchanged simple variants ---
    /// Configuration errors (invalid TOML, missing required fields, type mismatches).
    #[error("configuration error: {0}")]
    Config(String),

    /// Credential vault errors (decryption failure, vault locked, key derivation).
    #[error("vault error: {0}")]
    Vault(String),

    /// Security policy violations (TLS required, SSRF blocked, invalid credential).
    #[error("security violation: {0}")]
    Security(String),

    /// Signature verification errors (file not found, invalid signature, verification failed).
    #[error("signature error: {0}")]
    Signature(String),

    /// Budget cap has been reached (daily or monthly).
    #[error("budget exhausted: {message}")]
    BudgetExhausted { message: String },

    /// Adapter health check failed.
    #[error("health check failed for {name}: {source}")]
    HealthCheckFailed {
        name: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Operation timed out.
    #[error("operation timed out after {duration:?}")]
    Timeout { duration: Duration },

    /// Internal or unexpected errors.
    #[error("internal error: {0}")]
    Internal(String),

    /// Self-update errors (version check, download, swap, rollback).
    #[error("update error: {0}")]
    Update(String),

    /// Requested adapter was not found in the registry.
    #[error("adapter not found: {adapter_type}/{name}")]
    AdapterNotFound { adapter_type: String, name: String },

    /// Circuit breaker is open for this dependency (fast-fail).
    ///
    /// Not retryable (caller should use fallback) and does not trip the
    /// circuit breaker itself (avoids counting fast-fails as additional failures).
    #[error("circuit breaker open for {dependency}")]
    CircuitOpen { dependency: String },
}

// ---------------------------------------------------------------------------
// Classification methods
// ---------------------------------------------------------------------------

impl BlufioError {
    /// Returns the failure mode describing *what went wrong*.
    pub fn failure_mode(&self) -> FailureMode {
        match self {
            Self::Provider { kind, .. } => match kind {
                ProviderErrorKind::RateLimited => FailureMode::RateLimit,
                ProviderErrorKind::AuthFailed => FailureMode::Auth,
                ProviderErrorKind::ServerError => FailureMode::Unavailable,
                ProviderErrorKind::Timeout => FailureMode::Timeout,
                ProviderErrorKind::ModelNotFound => FailureMode::Validation,
            },
            Self::Channel { kind, .. } => match kind {
                ChannelErrorKind::DeliveryFailed => FailureMode::Network,
                ChannelErrorKind::ConnectionLost => FailureMode::Network,
                ChannelErrorKind::RateLimited => FailureMode::RateLimit,
                ChannelErrorKind::MessageTooLarge => FailureMode::Validation,
                ChannelErrorKind::UnsupportedContent => FailureMode::Validation,
            },
            Self::Storage { kind, .. } => match kind {
                StorageErrorKind::Busy => FailureMode::Unavailable,
                StorageErrorKind::Corruption => FailureMode::Internal,
                StorageErrorKind::SchemaError => FailureMode::Internal,
                StorageErrorKind::DiskFull => FailureMode::ResourceExhausted,
                StorageErrorKind::ConnectionFailed => FailureMode::Network,
            },
            Self::Skill { kind, .. } => match kind {
                SkillErrorKind::ExecutionFailed => FailureMode::Internal,
                SkillErrorKind::CapabilityDenied => FailureMode::Auth,
                SkillErrorKind::SandboxTimeout => FailureMode::Timeout,
                SkillErrorKind::CompilationFailed => FailureMode::Internal,
            },
            Self::Mcp { kind, .. } => match kind {
                McpErrorKind::ConnectionFailed => FailureMode::Network,
                McpErrorKind::ToolExecutionFailed => FailureMode::Internal,
                McpErrorKind::Timeout => FailureMode::Timeout,
                McpErrorKind::AuthFailed => FailureMode::Auth,
                McpErrorKind::ProtocolError => FailureMode::Internal,
            },
            Self::Migration { kind, .. } => match kind {
                MigrationErrorKind::SchemaFailed => FailureMode::Internal,
                MigrationErrorKind::DataCorruption => FailureMode::Internal,
                MigrationErrorKind::VersionMismatch => FailureMode::Validation,
            },
            Self::Config(_) => FailureMode::Validation,
            Self::Security(_) | Self::Vault(_) | Self::Signature(_) => FailureMode::Auth,
            Self::BudgetExhausted { .. } => FailureMode::ResourceExhausted,
            Self::HealthCheckFailed { .. } => FailureMode::Unavailable,
            Self::Timeout { .. } => FailureMode::Timeout,
            Self::Internal(_) => FailureMode::Internal,
            Self::Update(_) => FailureMode::Network,
            Self::AdapterNotFound { .. } => FailureMode::Validation,
            // CircuitOpen is a fast-fail signal, not a real failure mode.
            // Internal maps to is_retryable()=false and trips_circuit_breaker()=false.
            Self::CircuitOpen { .. } => FailureMode::Internal,
        }
    }

    /// Whether this error is retryable. Derived purely from [`failure_mode()`].
    pub fn is_retryable(&self) -> bool {
        self.failure_mode().is_retryable()
    }

    /// Returns the severity level for this error.
    pub fn severity(&self) -> Severity {
        match self {
            // Fatal: Config (can't start), ResourceExhausted (budget kill switch)
            Self::Config(_) => Severity::Fatal,
            Self::BudgetExhausted { .. } => Severity::Fatal,
            Self::Storage {
                kind: StorageErrorKind::DiskFull,
                ..
            } => Severity::Fatal,

            // Info: ModelNotFound
            Self::Provider {
                kind: ProviderErrorKind::ModelNotFound,
                ..
            } => Severity::Info,

            // Warning: Validation failures
            Self::Channel {
                kind: ChannelErrorKind::MessageTooLarge | ChannelErrorKind::UnsupportedContent,
                ..
            } => Severity::Warning,
            Self::Migration {
                kind: MigrationErrorKind::VersionMismatch,
                ..
            } => Severity::Warning,
            Self::AdapterNotFound { .. } => Severity::Warning,

            // Error: everything else
            _ => Severity::Error,
        }
    }

    /// Returns the high-level category identifying the subsystem.
    pub fn category(&self) -> ErrorCategory {
        match self {
            Self::Provider { .. } => ErrorCategory::Provider,
            Self::Channel { .. } => ErrorCategory::Channel,
            Self::Storage { .. } => ErrorCategory::Storage,
            Self::Skill { .. } => ErrorCategory::Skill,
            Self::Mcp { .. } => ErrorCategory::Mcp,
            Self::Migration { .. } => ErrorCategory::Migration,
            Self::Config(_) => ErrorCategory::Config,
            Self::Security(_) | Self::Vault(_) | Self::Signature(_) => ErrorCategory::Security,
            Self::BudgetExhausted { .. } => ErrorCategory::Internal,
            Self::HealthCheckFailed { .. } => ErrorCategory::Internal,
            Self::Timeout { .. } => ErrorCategory::Internal,
            Self::Internal(_) => ErrorCategory::Internal,
            Self::Update(_) => ErrorCategory::Internal,
            Self::AdapterNotFound { .. } => ErrorCategory::Internal,
            Self::CircuitOpen { .. } => ErrorCategory::Internal,
        }
    }

    /// Whether this error should trip a circuit breaker.
    ///
    /// True for server-side failure modes: Network, RateLimit, Timeout, Unavailable.
    /// False for client-side: Auth, Validation, Internal, ResourceExhausted.
    pub fn trips_circuit_breaker(&self) -> bool {
        matches!(
            self.failure_mode(),
            FailureMode::Network
                | FailureMode::RateLimit
                | FailureMode::Timeout
                | FailureMode::Unavailable
        )
    }

    /// Suggested backoff duration before retrying.
    ///
    /// Returns `None` for non-retryable errors.
    /// For `RateLimit`, uses the `retry_after` from context if available, otherwise 5s.
    pub fn suggested_backoff(&self) -> Option<Duration> {
        if !self.is_retryable() {
            return None;
        }
        match self.failure_mode() {
            FailureMode::RateLimit => {
                // Try to get retry_after from context
                let retry_after = match self {
                    Self::Provider { context, .. }
                    | Self::Channel { context, .. }
                    | Self::Storage { context, .. }
                    | Self::Skill { context, .. }
                    | Self::Mcp { context, .. } => context.retry_after,
                    Self::Migration { context, .. } => context.retry_after,
                    _ => None,
                };
                Some(retry_after.unwrap_or(Duration::from_secs(5)))
            }
            FailureMode::Network => Some(Duration::from_secs(1)),
            FailureMode::Timeout => Some(Duration::from_secs(2)),
            FailureMode::Unavailable => Some(Duration::from_secs(3)),
            _ => None,
        }
    }

    /// Returns a sanitized user-facing message with no internal details.
    ///
    /// No provider names, HTTP codes, or file paths are included.
    pub fn user_message(&self) -> Cow<'static, str> {
        match self {
            Self::Provider { kind, .. } => match kind {
                ProviderErrorKind::RateLimited => {
                    Cow::Borrowed("The AI service is busy. Please try again shortly.")
                }
                ProviderErrorKind::AuthFailed => {
                    Cow::Borrowed("Authentication with the AI service failed.")
                }
                ProviderErrorKind::ServerError => {
                    Cow::Borrowed("The AI service encountered an error. Please try again.")
                }
                ProviderErrorKind::Timeout => {
                    Cow::Borrowed("The AI service took too long to respond.")
                }
                ProviderErrorKind::ModelNotFound => {
                    Cow::Borrowed("The requested AI model was not found.")
                }
            },
            Self::Channel { kind, .. } => match kind {
                ChannelErrorKind::DeliveryFailed => Cow::Borrowed("Failed to deliver the message."),
                ChannelErrorKind::ConnectionLost => {
                    Cow::Borrowed("Connection to the messaging service was lost.")
                }
                ChannelErrorKind::RateLimited => {
                    Cow::Borrowed("Too many messages sent. Please wait a moment.")
                }
                ChannelErrorKind::MessageTooLarge => {
                    Cow::Borrowed("The message was too large to send.")
                }
                ChannelErrorKind::UnsupportedContent => {
                    Cow::Borrowed("This type of content is not supported.")
                }
            },
            Self::Storage { kind, .. } => match kind {
                StorageErrorKind::Busy => Cow::Borrowed("The database is busy. Please try again."),
                StorageErrorKind::Corruption => {
                    Cow::Borrowed("A storage error occurred. Data may need recovery.")
                }
                StorageErrorKind::SchemaError => Cow::Borrowed("A storage schema error occurred."),
                StorageErrorKind::DiskFull => Cow::Borrowed("Storage space is full."),
                StorageErrorKind::ConnectionFailed => {
                    Cow::Borrowed("Failed to connect to the database.")
                }
            },
            Self::Skill { kind, .. } => match kind {
                SkillErrorKind::ExecutionFailed => Cow::Borrowed("A skill failed to execute."),
                SkillErrorKind::CapabilityDenied => {
                    Cow::Borrowed("The skill does not have the required permissions.")
                }
                SkillErrorKind::SandboxTimeout => {
                    Cow::Borrowed("A skill timed out during execution.")
                }
                SkillErrorKind::CompilationFailed => Cow::Borrowed("A skill failed to compile."),
            },
            Self::Mcp { kind, .. } => match kind {
                McpErrorKind::ConnectionFailed => {
                    Cow::Borrowed("Failed to connect to the tool server.")
                }
                McpErrorKind::ToolExecutionFailed => Cow::Borrowed("A tool failed to execute."),
                McpErrorKind::Timeout => Cow::Borrowed("The tool server took too long to respond."),
                McpErrorKind::AuthFailed => {
                    Cow::Borrowed("Authentication with the tool server failed.")
                }
                McpErrorKind::ProtocolError => {
                    Cow::Borrowed("A communication error occurred with the tool server.")
                }
            },
            Self::Migration { kind, .. } => match kind {
                MigrationErrorKind::SchemaFailed => Cow::Borrowed("A database migration failed."),
                MigrationErrorKind::DataCorruption => {
                    Cow::Borrowed("Data corruption detected during migration.")
                }
                MigrationErrorKind::VersionMismatch => {
                    Cow::Borrowed("Incompatible data version detected.")
                }
            },
            Self::Config(_) => Cow::Borrowed("A configuration error occurred."),
            Self::Vault(_) => Cow::Borrowed("A credential vault error occurred."),
            Self::Security(_) => Cow::Borrowed("A security policy violation occurred."),
            Self::Signature(_) => Cow::Borrowed("Signature verification failed."),
            Self::BudgetExhausted { .. } => Cow::Borrowed("The usage budget has been exhausted."),
            Self::HealthCheckFailed { .. } => Cow::Borrowed("A service health check failed."),
            Self::Timeout { .. } => Cow::Borrowed("The operation timed out."),
            Self::Internal(_) => Cow::Borrowed("An internal error occurred."),
            Self::Update(_) => Cow::Borrowed("An error occurred during the update process."),
            Self::AdapterNotFound { .. } => {
                Cow::Borrowed("The requested service adapter was not found.")
            }
            Self::CircuitOpen { .. } => {
                Cow::Borrowed("The service is temporarily unavailable. Please try again later.")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP status mapping
// ---------------------------------------------------------------------------

/// Maps an HTTP status code to a [`ProviderErrorKind`].
///
/// Provider-specific overrides are supported (e.g., Anthropic 529 = RateLimited).
pub fn http_status_to_provider_error(status: u16, provider_name: &str) -> ProviderErrorKind {
    match status {
        400 | 422 => ProviderErrorKind::ModelNotFound, // Client errors -> Validation
        401 | 403 => ProviderErrorKind::AuthFailed,
        404 => ProviderErrorKind::ModelNotFound,
        408 => ProviderErrorKind::Timeout,
        429 => ProviderErrorKind::RateLimited,
        529 if provider_name == "anthropic" => ProviderErrorKind::RateLimited,
        500 | 502 | 503 | 504 | 529 => ProviderErrorKind::ServerError,
        _ => ProviderErrorKind::ServerError,
    }
}

// ---------------------------------------------------------------------------
// Constructor helpers
// ---------------------------------------------------------------------------

impl BlufioError {
    // --- Provider constructors ---

    /// Create a provider rate-limited error.
    pub fn provider_rate_limited(retry_after: Option<Duration>, provider_name: &str) -> Self {
        Self::Provider {
            kind: ProviderErrorKind::RateLimited,
            context: ErrorContext {
                retry_after,
                provider_name: Some(provider_name.to_string()),
                ..Default::default()
            },
            source: None,
        }
    }

    /// Create a provider authentication failure.
    pub fn provider_auth_failed(provider_name: &str) -> Self {
        Self::Provider {
            kind: ProviderErrorKind::AuthFailed,
            context: ErrorContext {
                provider_name: Some(provider_name.to_string()),
                ..Default::default()
            },
            source: None,
        }
    }

    /// Create a provider timeout error.
    pub fn provider_timeout(provider_name: &str) -> Self {
        Self::Provider {
            kind: ProviderErrorKind::Timeout,
            context: ErrorContext {
                provider_name: Some(provider_name.to_string()),
                ..Default::default()
            },
            source: None,
        }
    }

    /// Create a provider server error with a source.
    pub fn provider_server_error(
        provider_name: &str,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Provider {
            kind: ProviderErrorKind::ServerError,
            context: ErrorContext {
                provider_name: Some(provider_name.to_string()),
                ..Default::default()
            },
            source: Some(Box::new(source)),
        }
    }

    /// Create a provider model-not-found error.
    pub fn provider_model_not_found(model: &str, provider_name: &str) -> Self {
        Self::Provider {
            kind: ProviderErrorKind::ModelNotFound,
            context: ErrorContext {
                provider_name: Some(provider_name.to_string()),
                request_id: Some(format!("model:{model}")),
                ..Default::default()
            },
            source: None,
        }
    }

    /// Create a provider error from an HTTP status code.
    pub fn provider_from_http(
        status: u16,
        provider_name: &str,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    ) -> Self {
        let kind = http_status_to_provider_error(status, provider_name);
        Self::Provider {
            kind,
            context: ErrorContext {
                http_status: Some(status),
                provider_name: Some(provider_name.to_string()),
                ..Default::default()
            },
            source,
        }
    }

    // --- Channel constructors ---

    /// Create a channel delivery failure.
    pub fn channel_delivery_failed(
        channel_name: &str,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Channel {
            kind: ChannelErrorKind::DeliveryFailed,
            context: ErrorContext {
                channel_name: Some(channel_name.to_string()),
                ..Default::default()
            },
            source: Some(Box::new(source)),
        }
    }

    /// Create a channel connection-lost error.
    pub fn channel_connection_lost(channel_name: &str) -> Self {
        Self::Channel {
            kind: ChannelErrorKind::ConnectionLost,
            context: ErrorContext {
                channel_name: Some(channel_name.to_string()),
                ..Default::default()
            },
            source: None,
        }
    }

    /// Create a channel rate-limited error.
    pub fn channel_rate_limited(channel_name: &str, retry_after: Option<Duration>) -> Self {
        Self::Channel {
            kind: ChannelErrorKind::RateLimited,
            context: ErrorContext {
                channel_name: Some(channel_name.to_string()),
                retry_after,
                ..Default::default()
            },
            source: None,
        }
    }

    /// Create a channel message-too-large error.
    pub fn channel_message_too_large(channel_name: &str) -> Self {
        Self::Channel {
            kind: ChannelErrorKind::MessageTooLarge,
            context: ErrorContext {
                channel_name: Some(channel_name.to_string()),
                ..Default::default()
            },
            source: None,
        }
    }

    /// Create a channel unsupported-content error.
    pub fn channel_unsupported_content(channel_name: &str) -> Self {
        Self::Channel {
            kind: ChannelErrorKind::UnsupportedContent,
            context: ErrorContext {
                channel_name: Some(channel_name.to_string()),
                ..Default::default()
            },
            source: None,
        }
    }

    // --- Storage constructors ---

    /// Create a storage busy error.
    pub fn storage_busy(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Storage {
            kind: StorageErrorKind::Busy,
            context: ErrorContext::default(),
            source: Box::new(source),
        }
    }

    /// Create a storage corruption error.
    pub fn storage_corruption(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Storage {
            kind: StorageErrorKind::Corruption,
            context: ErrorContext::default(),
            source: Box::new(source),
        }
    }

    /// Create a storage connection failure.
    pub fn storage_connection_failed(
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Storage {
            kind: StorageErrorKind::ConnectionFailed,
            context: ErrorContext::default(),
            source: Box::new(source),
        }
    }

    /// Create a storage schema error.
    pub fn storage_schema_error(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Storage {
            kind: StorageErrorKind::SchemaError,
            context: ErrorContext::default(),
            source: Box::new(source),
        }
    }

    // --- MCP constructors ---

    /// Create an MCP connection failure.
    pub fn mcp_connection_failed(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Mcp {
            kind: McpErrorKind::ConnectionFailed,
            context: ErrorContext::default(),
            source: Some(Box::new(source)),
        }
    }

    /// Create an MCP tool execution failure.
    pub fn mcp_tool_failed(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Mcp {
            kind: McpErrorKind::ToolExecutionFailed,
            context: ErrorContext::default(),
            source: Some(Box::new(source)),
        }
    }

    /// Create an MCP timeout error.
    pub fn mcp_timeout(msg: &str) -> Self {
        Self::Mcp {
            kind: McpErrorKind::Timeout,
            context: ErrorContext {
                request_id: Some(msg.to_string()),
                ..Default::default()
            },
            source: None,
        }
    }

    /// Create an MCP protocol error.
    pub fn mcp_protocol_error(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Mcp {
            kind: McpErrorKind::ProtocolError,
            context: ErrorContext::default(),
            source: Some(Box::new(source)),
        }
    }

    // --- Skill constructors ---

    /// Create a skill execution failure with a source error.
    pub fn skill_execution_failed(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Skill {
            kind: SkillErrorKind::ExecutionFailed,
            context: ErrorContext::default(),
            source: Some(Box::new(source)),
        }
    }

    /// Create a skill execution failure from a message string (no source).
    pub fn skill_execution_msg(msg: &str) -> Self {
        Self::Skill {
            kind: SkillErrorKind::ExecutionFailed,
            context: ErrorContext {
                request_id: Some(msg.to_string()),
                ..Default::default()
            },
            source: None,
        }
    }

    /// Create a skill compilation failure.
    pub fn skill_compilation_failed(
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Skill {
            kind: SkillErrorKind::CompilationFailed,
            context: ErrorContext::default(),
            source: Some(Box::new(source)),
        }
    }

    /// Create a skill compilation failure from a message string (no source).
    pub fn skill_compilation_msg(msg: &str) -> Self {
        Self::Skill {
            kind: SkillErrorKind::CompilationFailed,
            context: ErrorContext {
                request_id: Some(msg.to_string()),
                ..Default::default()
            },
            source: None,
        }
    }

    /// Create a skill sandbox timeout error.
    pub fn skill_sandbox_timeout(msg: &str) -> Self {
        Self::Skill {
            kind: SkillErrorKind::SandboxTimeout,
            context: ErrorContext {
                request_id: Some(msg.to_string()),
                ..Default::default()
            },
            source: None,
        }
    }

    /// Create a skill capability denied error.
    pub fn skill_capability_denied(msg: &str) -> Self {
        Self::Skill {
            kind: SkillErrorKind::CapabilityDenied,
            context: ErrorContext {
                request_id: Some(msg.to_string()),
                ..Default::default()
            },
            source: None,
        }
    }

    // --- Migration constructors ---

    /// Create a migration schema failure from a message.
    pub fn migration_schema_failed(msg: &str) -> Self {
        Self::Migration {
            kind: MigrationErrorKind::SchemaFailed,
            context: ErrorContext {
                request_id: Some(msg.to_string()),
                ..Default::default()
            },
        }
    }

    /// Create a migration version mismatch error.
    pub fn migration_version_mismatch(msg: &str) -> Self {
        Self::Migration {
            kind: MigrationErrorKind::VersionMismatch,
            context: ErrorContext {
                request_id: Some(msg.to_string()),
                ..Default::default()
            },
        }
    }

    /// Create a migration data corruption error.
    pub fn migration_data_corruption(msg: &str) -> Self {
        Self::Migration {
            kind: MigrationErrorKind::DataCorruption,
            context: ErrorContext {
                request_id: Some(msg.to_string()),
                ..Default::default()
            },
        }
    }

    /// Create a circuit-open fast-fail error for the given dependency.
    pub fn circuit_open(dependency: impl Into<String>) -> Self {
        Self::CircuitOpen {
            dependency: dependency.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// error_log! macro
// ---------------------------------------------------------------------------

/// Log a [`BlufioError`] with structured classification fields.
///
/// Dispatches to the appropriate tracing level based on [`Severity`]:
/// - `Fatal` / `Error` -> `tracing::error!`
/// - `Warning` -> `tracing::warn!`
/// - `Info` -> `tracing::info!`
///
/// Structured fields: `error`, `category`, `failure_mode`, `retryable`.
#[macro_export]
macro_rules! error_log {
    ($error:expr) => {{
        let err = &$error;
        let severity = err.severity();
        let category = err.category();
        let failure_mode = err.failure_mode();
        let retryable = err.is_retryable();
        match severity {
            $crate::error::Severity::Fatal | $crate::error::Severity::Error => {
                tracing::error!(
                    error = %err,
                    category = %category,
                    failure_mode = %failure_mode,
                    retryable = retryable,
                    "error occurred"
                );
            }
            $crate::error::Severity::Warning => {
                tracing::warn!(
                    error = %err,
                    category = %category,
                    failure_mode = %failure_mode,
                    retryable = retryable,
                    "warning occurred"
                );
            }
            $crate::error::Severity::Info => {
                tracing::info!(
                    error = %err,
                    category = %category,
                    failure_mode = %failure_mode,
                    retryable = retryable,
                    "info error occurred"
                );
            }
        }
    }};
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- FailureMode tests --

    #[test]
    fn failure_mode_is_retryable_network() {
        assert!(FailureMode::Network.is_retryable());
    }

    #[test]
    fn failure_mode_is_retryable_rate_limit() {
        assert!(FailureMode::RateLimit.is_retryable());
    }

    #[test]
    fn failure_mode_is_retryable_timeout() {
        assert!(FailureMode::Timeout.is_retryable());
    }

    #[test]
    fn failure_mode_is_retryable_unavailable() {
        assert!(FailureMode::Unavailable.is_retryable());
    }

    #[test]
    fn failure_mode_not_retryable_auth() {
        assert!(!FailureMode::Auth.is_retryable());
    }

    #[test]
    fn failure_mode_not_retryable_validation() {
        assert!(!FailureMode::Validation.is_retryable());
    }

    #[test]
    fn failure_mode_not_retryable_internal() {
        assert!(!FailureMode::Internal.is_retryable());
    }

    #[test]
    fn failure_mode_not_retryable_resource_exhausted() {
        assert!(!FailureMode::ResourceExhausted.is_retryable());
    }

    // -- Provider classification --

    #[test]
    fn provider_rate_limited_classification() {
        let err = BlufioError::provider_rate_limited(None, "anthropic");
        assert!(err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::RateLimit);
        assert_eq!(err.severity(), Severity::Error);
        assert_eq!(err.category(), ErrorCategory::Provider);
        assert!(err.trips_circuit_breaker());
        assert!(err.suggested_backoff().is_some());
    }

    #[test]
    fn provider_auth_failed_classification() {
        let err = BlufioError::provider_auth_failed("openai");
        assert!(!err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Auth);
        assert_eq!(err.severity(), Severity::Error);
        assert_eq!(err.category(), ErrorCategory::Provider);
        assert!(!err.trips_circuit_breaker());
        assert!(err.suggested_backoff().is_none());
    }

    #[test]
    fn provider_server_error_classification() {
        let err =
            BlufioError::provider_server_error("gemini", std::io::Error::other("server error"));
        assert!(err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Unavailable);
        assert_eq!(err.severity(), Severity::Error);
        assert!(err.trips_circuit_breaker());
    }

    #[test]
    fn provider_timeout_classification() {
        let err = BlufioError::provider_timeout("anthropic");
        assert!(err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Timeout);
        assert_eq!(err.severity(), Severity::Error);
        assert!(err.trips_circuit_breaker());
        assert_eq!(err.suggested_backoff(), Some(Duration::from_secs(2)));
    }

    #[test]
    fn provider_model_not_found_classification() {
        let err = BlufioError::provider_model_not_found("gpt-5", "openai");
        assert!(!err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Validation);
        assert_eq!(err.severity(), Severity::Info);
        assert!(!err.trips_circuit_breaker());
    }

    // -- Channel classification --

    #[test]
    fn channel_delivery_failed_classification() {
        let err =
            BlufioError::channel_delivery_failed("telegram", std::io::Error::other("network"));
        assert!(err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Network);
        assert_eq!(err.severity(), Severity::Error);
        assert_eq!(err.category(), ErrorCategory::Channel);
        assert!(err.trips_circuit_breaker());
        assert_eq!(err.suggested_backoff(), Some(Duration::from_secs(1)));
    }

    #[test]
    fn channel_connection_lost_classification() {
        let err = BlufioError::channel_connection_lost("discord");
        assert!(err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Network);
    }

    #[test]
    fn channel_rate_limited_classification() {
        let retry = Duration::from_secs(10);
        let err = BlufioError::channel_rate_limited("slack", Some(retry));
        assert!(err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::RateLimit);
        assert_eq!(err.suggested_backoff(), Some(retry));
    }

    #[test]
    fn channel_message_too_large_classification() {
        let err = BlufioError::channel_message_too_large("telegram");
        assert!(!err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Validation);
        assert_eq!(err.severity(), Severity::Warning);
    }

    #[test]
    fn channel_unsupported_content_classification() {
        let err = BlufioError::channel_unsupported_content("irc");
        assert!(!err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Validation);
        assert_eq!(err.severity(), Severity::Warning);
    }

    // -- Storage classification --

    #[test]
    fn storage_busy_classification() {
        let err = BlufioError::storage_busy(std::io::Error::other("SQLITE_BUSY"));
        assert!(err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Unavailable);
        assert_eq!(err.severity(), Severity::Error);
        assert_eq!(err.category(), ErrorCategory::Storage);
        assert!(err.trips_circuit_breaker());
        assert_eq!(err.suggested_backoff(), Some(Duration::from_secs(3)));
    }

    #[test]
    fn storage_corruption_classification() {
        let err = BlufioError::storage_corruption(std::io::Error::other("corrupt"));
        assert!(!err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Internal);
        assert_eq!(err.severity(), Severity::Error);
    }

    #[test]
    fn storage_disk_full_classification() {
        let err = BlufioError::Storage {
            kind: StorageErrorKind::DiskFull,
            context: ErrorContext::default(),
            source: Box::new(std::io::Error::other("no space")),
        };
        assert!(!err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::ResourceExhausted);
        assert_eq!(err.severity(), Severity::Fatal);
    }

    #[test]
    fn storage_connection_failed_classification() {
        let err = BlufioError::Storage {
            kind: StorageErrorKind::ConnectionFailed,
            context: ErrorContext::default(),
            source: Box::new(std::io::Error::other("refused")),
        };
        assert!(err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Network);
    }

    // -- Skill classification --

    #[test]
    fn skill_execution_failed_classification() {
        let err = BlufioError::skill_execution_failed(std::io::Error::other("panic"));
        assert!(!err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Internal);
        assert_eq!(err.category(), ErrorCategory::Skill);
    }

    #[test]
    fn skill_capability_denied_classification() {
        let err = BlufioError::Skill {
            kind: SkillErrorKind::CapabilityDenied,
            context: ErrorContext::default(),
            source: None,
        };
        assert!(!err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Auth);
    }

    #[test]
    fn skill_sandbox_timeout_classification() {
        let err = BlufioError::Skill {
            kind: SkillErrorKind::SandboxTimeout,
            context: ErrorContext::default(),
            source: None,
        };
        assert!(err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Timeout);
    }

    // -- Mcp classification --

    #[test]
    fn mcp_connection_failed_classification() {
        let err = BlufioError::mcp_connection_failed(std::io::Error::other("refused"));
        assert!(err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Network);
        assert_eq!(err.category(), ErrorCategory::Mcp);
    }

    #[test]
    fn mcp_tool_failed_classification() {
        let err = BlufioError::mcp_tool_failed(std::io::Error::other("exec error"));
        assert!(!err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Internal);
    }

    #[test]
    fn mcp_timeout_classification() {
        let err = BlufioError::Mcp {
            kind: McpErrorKind::Timeout,
            context: ErrorContext::default(),
            source: None,
        };
        assert!(err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Timeout);
    }

    #[test]
    fn mcp_auth_failed_classification() {
        let err = BlufioError::Mcp {
            kind: McpErrorKind::AuthFailed,
            context: ErrorContext::default(),
            source: None,
        };
        assert!(!err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Auth);
    }

    // -- Migration classification --

    #[test]
    fn migration_schema_failed_classification() {
        let err = BlufioError::migration_schema_failed("v5 migration");
        assert!(!err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Internal);
        assert_eq!(err.category(), ErrorCategory::Migration);
    }

    #[test]
    fn migration_version_mismatch_classification() {
        let err = BlufioError::Migration {
            kind: MigrationErrorKind::VersionMismatch,
            context: ErrorContext::default(),
        };
        assert!(!err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Validation);
        assert_eq!(err.severity(), Severity::Warning);
    }

    // -- Simple variant classification --

    #[test]
    fn config_error_classification() {
        let err = BlufioError::Config("bad toml".into());
        assert!(!err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Validation);
        assert_eq!(err.severity(), Severity::Fatal);
        assert_eq!(err.category(), ErrorCategory::Config);
    }

    #[test]
    fn security_error_classification() {
        let err = BlufioError::Security("TLS required".into());
        assert!(!err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Auth);
        assert_eq!(err.category(), ErrorCategory::Security);
    }

    #[test]
    fn vault_error_classification() {
        let err = BlufioError::Vault("locked".into());
        assert!(!err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Auth);
        assert_eq!(err.category(), ErrorCategory::Security);
    }

    #[test]
    fn signature_error_classification() {
        let err = BlufioError::Signature("bad sig".into());
        assert!(!err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Auth);
        assert_eq!(err.category(), ErrorCategory::Security);
    }

    #[test]
    fn budget_exhausted_classification() {
        let err = BlufioError::BudgetExhausted {
            message: "daily cap".into(),
        };
        assert!(!err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::ResourceExhausted);
        assert_eq!(err.severity(), Severity::Fatal);
    }

    #[test]
    fn timeout_classification() {
        let err = BlufioError::Timeout {
            duration: Duration::from_secs(30),
        };
        assert!(err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Timeout);
    }

    #[test]
    fn internal_error_classification() {
        let err = BlufioError::Internal("unexpected".into());
        assert!(!err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Internal);
    }

    #[test]
    fn update_error_classification() {
        let err = BlufioError::Update("download failed".into());
        assert!(err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Network);
    }

    #[test]
    fn adapter_not_found_classification() {
        let err = BlufioError::AdapterNotFound {
            adapter_type: "Channel".into(),
            name: "test".into(),
        };
        assert!(!err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Validation);
        assert_eq!(err.severity(), Severity::Warning);
    }

    #[test]
    fn health_check_failed_classification() {
        let err = BlufioError::HealthCheckFailed {
            name: "db".into(),
            source: Box::new(std::io::Error::other("unreachable")),
        };
        assert!(err.is_retryable());
        assert_eq!(err.failure_mode(), FailureMode::Unavailable);
    }

    // -- HTTP status mapping --

    #[test]
    fn http_401_maps_to_auth_failed() {
        assert_eq!(
            http_status_to_provider_error(401, "openai"),
            ProviderErrorKind::AuthFailed
        );
    }

    #[test]
    fn http_403_maps_to_auth_failed() {
        assert_eq!(
            http_status_to_provider_error(403, "openai"),
            ProviderErrorKind::AuthFailed
        );
    }

    #[test]
    fn http_404_maps_to_model_not_found() {
        assert_eq!(
            http_status_to_provider_error(404, "openai"),
            ProviderErrorKind::ModelNotFound
        );
    }

    #[test]
    fn http_408_maps_to_timeout() {
        assert_eq!(
            http_status_to_provider_error(408, "openai"),
            ProviderErrorKind::Timeout
        );
    }

    #[test]
    fn http_429_maps_to_rate_limited() {
        assert_eq!(
            http_status_to_provider_error(429, "openai"),
            ProviderErrorKind::RateLimited
        );
    }

    #[test]
    fn http_529_anthropic_maps_to_rate_limited() {
        assert_eq!(
            http_status_to_provider_error(529, "anthropic"),
            ProviderErrorKind::RateLimited
        );
    }

    #[test]
    fn http_529_non_anthropic_maps_to_server_error() {
        assert_eq!(
            http_status_to_provider_error(529, "openai"),
            ProviderErrorKind::ServerError
        );
    }

    #[test]
    fn http_500_maps_to_server_error() {
        assert_eq!(
            http_status_to_provider_error(500, "openai"),
            ProviderErrorKind::ServerError
        );
    }

    #[test]
    fn http_502_maps_to_server_error() {
        assert_eq!(
            http_status_to_provider_error(502, "openai"),
            ProviderErrorKind::ServerError
        );
    }

    #[test]
    fn http_503_maps_to_server_error() {
        assert_eq!(
            http_status_to_provider_error(503, "openai"),
            ProviderErrorKind::ServerError
        );
    }

    #[test]
    fn http_504_maps_to_server_error() {
        assert_eq!(
            http_status_to_provider_error(504, "openai"),
            ProviderErrorKind::ServerError
        );
    }

    #[test]
    fn http_unknown_maps_to_server_error() {
        assert_eq!(
            http_status_to_provider_error(418, "openai"),
            ProviderErrorKind::ServerError
        );
    }

    // -- suggested_backoff --

    #[test]
    fn backoff_rate_limit_with_retry_after() {
        let err = BlufioError::provider_rate_limited(Some(Duration::from_secs(30)), "anthropic");
        assert_eq!(err.suggested_backoff(), Some(Duration::from_secs(30)));
    }

    #[test]
    fn backoff_rate_limit_default() {
        let err = BlufioError::provider_rate_limited(None, "anthropic");
        assert_eq!(err.suggested_backoff(), Some(Duration::from_secs(5)));
    }

    #[test]
    fn backoff_network() {
        let err =
            BlufioError::channel_delivery_failed("telegram", std::io::Error::other("timeout"));
        assert_eq!(err.suggested_backoff(), Some(Duration::from_secs(1)));
    }

    #[test]
    fn backoff_timeout() {
        let err = BlufioError::provider_timeout("anthropic");
        assert_eq!(err.suggested_backoff(), Some(Duration::from_secs(2)));
    }

    #[test]
    fn backoff_unavailable() {
        let err = BlufioError::storage_busy(std::io::Error::other("SQLITE_BUSY"));
        assert_eq!(err.suggested_backoff(), Some(Duration::from_secs(3)));
    }

    #[test]
    fn backoff_non_retryable_is_none() {
        let err = BlufioError::provider_auth_failed("openai");
        assert_eq!(err.suggested_backoff(), None);
    }

    // -- user_message sanitization --

    #[test]
    fn user_message_no_provider_name() {
        let err = BlufioError::provider_rate_limited(None, "anthropic");
        let msg = err.user_message();
        assert!(
            !msg.contains("anthropic"),
            "user_message should not contain provider name"
        );
    }

    #[test]
    fn user_message_no_http_code() {
        let err = BlufioError::provider_from_http(429, "openai", None);
        let msg = err.user_message();
        assert!(
            !msg.contains("429"),
            "user_message should not contain HTTP status code"
        );
    }

    #[test]
    fn user_message_no_channel_name() {
        let err = BlufioError::channel_connection_lost("discord");
        let msg = err.user_message();
        assert!(
            !msg.contains("discord"),
            "user_message should not contain channel name"
        );
    }

    // -- Display format --

    #[test]
    fn provider_error_display_includes_provider_name() {
        let err = BlufioError::provider_rate_limited(None, "anthropic");
        let display = err.to_string();
        assert!(display.contains("anthropic"));
        assert!(display.contains("RateLimited"));
    }

    #[test]
    fn channel_error_display_includes_channel_name() {
        let err = BlufioError::channel_connection_lost("telegram");
        let display = err.to_string();
        assert!(display.contains("telegram"));
        assert!(display.contains("ConnectionLost"));
    }

    // -- Invariant: is_retryable consistency --

    #[test]
    fn is_retryable_consistent_with_failure_mode() {
        // Test representative errors from every failure mode
        let errors: Vec<BlufioError> = vec![
            BlufioError::provider_rate_limited(None, "test"),
            BlufioError::provider_auth_failed("test"),
            BlufioError::provider_timeout("test"),
            BlufioError::provider_model_not_found("gpt-5", "test"),
            BlufioError::provider_server_error("test", std::io::Error::other("err")),
            BlufioError::channel_delivery_failed("test", std::io::Error::other("err")),
            BlufioError::channel_connection_lost("test"),
            BlufioError::channel_rate_limited("test", None),
            BlufioError::channel_message_too_large("test"),
            BlufioError::channel_unsupported_content("test"),
            BlufioError::storage_busy(std::io::Error::other("err")),
            BlufioError::storage_corruption(std::io::Error::other("err")),
            BlufioError::skill_execution_failed(std::io::Error::other("err")),
            BlufioError::mcp_connection_failed(std::io::Error::other("err")),
            BlufioError::mcp_tool_failed(std::io::Error::other("err")),
            BlufioError::migration_schema_failed("test"),
            BlufioError::Config("test".into()),
            BlufioError::Security("test".into()),
            BlufioError::Vault("test".into()),
            BlufioError::Signature("test".into()),
            BlufioError::BudgetExhausted {
                message: "test".into(),
            },
            BlufioError::Timeout {
                duration: Duration::from_secs(1),
            },
            BlufioError::Internal("test".into()),
            BlufioError::Update("test".into()),
            BlufioError::AdapterNotFound {
                adapter_type: "Channel".into(),
                name: "test".into(),
            },
            BlufioError::HealthCheckFailed {
                name: "test".into(),
                source: Box::new(std::io::Error::other("err")),
            },
        ];

        for err in &errors {
            assert_eq!(
                err.is_retryable(),
                err.failure_mode().is_retryable(),
                "is_retryable() != failure_mode().is_retryable() for: {:?}",
                err
            );
        }
    }

    // -- error_log! macro test --

    #[test]
    fn error_log_macro_compiles_and_runs() {
        // This test verifies the macro compiles and can be invoked.
        // Structured field verification requires tracing-test (covered by proptest module).
        let err = BlufioError::provider_rate_limited(None, "test");
        // Just ensure it doesn't panic -- tracing subscriber not installed in basic test
        error_log!(err);
    }

    // -- Typed constructor tests (replaced deprecated fallbacks) --

    #[test]
    fn storage_connection_failed_constructor() {
        let err = BlufioError::storage_connection_failed(std::io::Error::other("test"));
        assert_eq!(err.category(), ErrorCategory::Storage);
        assert_eq!(err.failure_mode(), FailureMode::Network);
    }

    #[test]
    fn skill_execution_msg_constructor() {
        let err = BlufioError::skill_execution_msg("test message");
        assert_eq!(err.category(), ErrorCategory::Skill);
        assert!(err.to_string().contains("test message"));
    }

    #[test]
    fn mcp_timeout_constructor() {
        let err = BlufioError::mcp_timeout("test timeout");
        assert_eq!(err.category(), ErrorCategory::Mcp);
        assert!(err.to_string().contains("test timeout"));
    }

    #[test]
    fn migration_schema_failed_includes_message() {
        let err = BlufioError::migration_schema_failed("test schema");
        assert_eq!(err.category(), ErrorCategory::Migration);
        assert!(err.to_string().contains("test schema"));
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    fn arb_provider_error_kind() -> impl Strategy<Value = ProviderErrorKind> {
        prop_oneof![
            Just(ProviderErrorKind::RateLimited),
            Just(ProviderErrorKind::AuthFailed),
            Just(ProviderErrorKind::ServerError),
            Just(ProviderErrorKind::Timeout),
            Just(ProviderErrorKind::ModelNotFound),
        ]
    }

    fn arb_channel_error_kind() -> impl Strategy<Value = ChannelErrorKind> {
        prop_oneof![
            Just(ChannelErrorKind::DeliveryFailed),
            Just(ChannelErrorKind::ConnectionLost),
            Just(ChannelErrorKind::RateLimited),
            Just(ChannelErrorKind::MessageTooLarge),
            Just(ChannelErrorKind::UnsupportedContent),
        ]
    }

    fn arb_storage_error_kind() -> impl Strategy<Value = StorageErrorKind> {
        prop_oneof![
            Just(StorageErrorKind::Busy),
            Just(StorageErrorKind::Corruption),
            Just(StorageErrorKind::SchemaError),
            Just(StorageErrorKind::DiskFull),
            Just(StorageErrorKind::ConnectionFailed),
        ]
    }

    fn arb_skill_error_kind() -> impl Strategy<Value = SkillErrorKind> {
        prop_oneof![
            Just(SkillErrorKind::ExecutionFailed),
            Just(SkillErrorKind::CapabilityDenied),
            Just(SkillErrorKind::SandboxTimeout),
            Just(SkillErrorKind::CompilationFailed),
        ]
    }

    fn arb_mcp_error_kind() -> impl Strategy<Value = McpErrorKind> {
        prop_oneof![
            Just(McpErrorKind::ConnectionFailed),
            Just(McpErrorKind::ToolExecutionFailed),
            Just(McpErrorKind::Timeout),
            Just(McpErrorKind::AuthFailed),
            Just(McpErrorKind::ProtocolError),
        ]
    }

    fn arb_migration_error_kind() -> impl Strategy<Value = MigrationErrorKind> {
        prop_oneof![
            Just(MigrationErrorKind::SchemaFailed),
            Just(MigrationErrorKind::DataCorruption),
            Just(MigrationErrorKind::VersionMismatch),
        ]
    }

    fn arb_failure_mode() -> impl Strategy<Value = FailureMode> {
        prop_oneof![
            Just(FailureMode::Network),
            Just(FailureMode::Auth),
            Just(FailureMode::RateLimit),
            Just(FailureMode::Timeout),
            Just(FailureMode::Validation),
            Just(FailureMode::Internal),
            Just(FailureMode::ResourceExhausted),
            Just(FailureMode::Unavailable),
        ]
    }

    // Build a BlufioError from sub-enum kinds for property testing.
    fn arb_blufio_error() -> impl Strategy<Value = BlufioError> {
        prop_oneof![
            arb_provider_error_kind().prop_map(|kind| BlufioError::Provider {
                kind,
                context: ErrorContext::default(),
                source: None,
            }),
            arb_channel_error_kind().prop_map(|kind| BlufioError::Channel {
                kind,
                context: ErrorContext::default(),
                source: None,
            }),
            arb_storage_error_kind().prop_map(|kind| BlufioError::Storage {
                kind,
                context: ErrorContext::default(),
                source: Box::new(std::io::Error::other("test")),
            }),
            arb_skill_error_kind().prop_map(|kind| BlufioError::Skill {
                kind,
                context: ErrorContext::default(),
                source: None,
            }),
            arb_mcp_error_kind().prop_map(|kind| BlufioError::Mcp {
                kind,
                context: ErrorContext::default(),
                source: None,
            }),
            arb_migration_error_kind().prop_map(|kind| BlufioError::Migration {
                kind,
                context: ErrorContext::default(),
            }),
            Just("test-dep".to_string()).prop_map(|dep| BlufioError::CircuitOpen {
                dependency: dep,
            }),
        ]
    }

    proptest! {
        #[test]
        fn is_retryable_matches_failure_mode(err in arb_blufio_error()) {
            prop_assert_eq!(err.is_retryable(), err.failure_mode().is_retryable());
        }

        #[test]
        fn trips_circuit_breaker_implies_server_side(err in arb_blufio_error()) {
            if err.trips_circuit_breaker() {
                prop_assert!(matches!(
                    err.failure_mode(),
                    FailureMode::Network | FailureMode::RateLimit | FailureMode::Timeout | FailureMode::Unavailable
                ));
            }
        }

        #[test]
        fn severity_at_least_info(err in arb_blufio_error()) {
            prop_assert!(err.severity() >= Severity::Info);
        }

        #[test]
        fn failure_mode_retryable_consistency(mode in arb_failure_mode()) {
            let retryable = mode.is_retryable();
            let expected = matches!(
                mode,
                FailureMode::Network | FailureMode::RateLimit | FailureMode::Timeout | FailureMode::Unavailable
            );
            prop_assert_eq!(retryable, expected);
        }
    }
}

#[cfg(test)]
mod tracing_tests {
    use super::*;
    use tracing_test::traced_test;

    #[traced_test]
    #[test]
    fn error_log_emits_structured_fields_for_error() {
        let err = BlufioError::provider_rate_limited(None, "anthropic");
        error_log!(err);
        assert!(logs_contain("category=Provider"));
        assert!(logs_contain("failure_mode=RateLimit"));
        assert!(logs_contain("retryable=true"));
    }

    #[traced_test]
    #[test]
    fn error_log_emits_warn_for_warning_severity() {
        let err = BlufioError::channel_message_too_large("telegram");
        error_log!(err);
        assert!(logs_contain("WARN"));
        assert!(logs_contain("warning occurred"));
    }

    #[traced_test]
    #[test]
    fn error_log_emits_info_for_info_severity() {
        let err = BlufioError::provider_model_not_found("gpt-5", "openai");
        error_log!(err);
        assert!(logs_contain("INFO"));
        assert!(logs_contain("info error occurred"));
    }
}
