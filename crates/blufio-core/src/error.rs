// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Error types for the Blufio agent framework.

use thiserror::Error;

/// The primary error type used across all Blufio adapter traits and core operations.
#[derive(Debug, Error)]
pub enum BlufioError {
    /// Configuration errors (invalid TOML, missing required fields, type mismatches).
    #[error("configuration error: {0}")]
    Config(String),

    /// Storage backend errors (database connection, query failure, serialization).
    #[error("storage error: {source}")]
    Storage {
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Channel adapter errors (connection failure, message format, rate limiting).
    #[error("channel error: {message}")]
    Channel {
        message: String,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// LLM provider errors (API failure, token limits, model not found).
    #[error("provider error: {message}")]
    Provider {
        message: String,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Requested adapter was not found in the registry.
    #[error("adapter not found: {adapter_type}/{name}")]
    AdapterNotFound { adapter_type: String, name: String },

    /// Adapter health check failed.
    #[error("health check failed for {name}: {source}")]
    HealthCheckFailed {
        name: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Operation timed out.
    #[error("operation timed out after {duration:?}")]
    Timeout { duration: std::time::Duration },

    /// Internal or unexpected errors.
    #[error("internal error: {0}")]
    Internal(String),
}
