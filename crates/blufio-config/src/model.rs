// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Configuration model structs for the Blufio agent framework.
//!
//! All structs use `#[serde(deny_unknown_fields)]` to reject unrecognized
//! config keys at startup, providing actionable error messages.

use serde::{Deserialize, Serialize};

/// Top-level Blufio configuration.
///
/// Loaded from TOML files following XDG hierarchy, with environment variable overrides.
/// All sections are optional and default to sensible values.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BlufioConfig {
    /// Agent identity and behavior settings.
    #[serde(default)]
    pub agent: AgentConfig,

    /// Telegram bot integration settings.
    #[serde(default)]
    pub telegram: TelegramConfig,

    /// Anthropic API settings.
    #[serde(default)]
    pub anthropic: AnthropicConfig,

    /// Storage backend settings.
    #[serde(default)]
    pub storage: StorageConfig,

    /// Network and TLS security settings.
    #[serde(default)]
    pub security: SecurityConfig,

    /// Cost tracking and budget settings.
    #[serde(default)]
    pub cost: CostConfig,
}

impl Default for BlufioConfig {
    fn default() -> Self {
        Self {
            agent: AgentConfig::default(),
            telegram: TelegramConfig::default(),
            anthropic: AnthropicConfig::default(),
            storage: StorageConfig::default(),
            security: SecurityConfig::default(),
            cost: CostConfig::default(),
        }
    }
}

/// Agent identity and behavior configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentConfig {
    /// Display name of the agent.
    #[serde(default = "default_agent_name")]
    pub name: String,

    /// Maximum number of concurrent sessions.
    #[serde(default = "default_max_sessions")]
    pub max_sessions: usize,

    /// Logging level (trace, debug, info, warn, error).
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: default_agent_name(),
            max_sessions: default_max_sessions(),
            log_level: default_log_level(),
        }
    }
}

fn default_agent_name() -> String {
    "blufio".to_string()
}

fn default_max_sessions() -> usize {
    10
}

fn default_log_level() -> String {
    "info".to_string()
}

/// Telegram bot integration configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TelegramConfig {
    /// Telegram Bot API token. `None` disables Telegram integration.
    #[serde(default)]
    pub bot_token: Option<String>,

    /// List of allowed Telegram user IDs or usernames.
    #[serde(default)]
    pub allowed_users: Vec<String>,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            bot_token: None,
            allowed_users: Vec::new(),
        }
    }
}

/// Anthropic API configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AnthropicConfig {
    /// Anthropic API key. `None` requires environment variable.
    #[serde(default)]
    pub api_key: Option<String>,

    /// Default model to use for LLM requests.
    #[serde(default = "default_model")]
    pub default_model: String,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            default_model: default_model(),
        }
    }
}

fn default_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}

/// Storage backend configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    /// Path to the SQLite database file.
    #[serde(default = "default_database_path")]
    pub database_path: String,

    /// Enable WAL (Write-Ahead Logging) mode for SQLite.
    #[serde(default = "default_wal_mode")]
    pub wal_mode: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            database_path: default_database_path(),
            wal_mode: default_wal_mode(),
        }
    }
}

fn default_database_path() -> String {
    "blufio.db".to_string()
}

fn default_wal_mode() -> bool {
    true
}

/// Network and TLS security configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityConfig {
    /// Address to bind the server to.
    #[serde(default = "default_bind_address")]
    pub bind_address: String,

    /// Require TLS for all connections.
    #[serde(default = "default_require_tls")]
    pub require_tls: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            bind_address: default_bind_address(),
            require_tls: default_require_tls(),
        }
    }
}

fn default_bind_address() -> String {
    "127.0.0.1".to_string()
}

fn default_require_tls() -> bool {
    true
}

/// Cost tracking and budget configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CostConfig {
    /// Maximum daily spending limit in USD. `None` means no limit.
    #[serde(default)]
    pub daily_budget_usd: Option<f64>,

    /// Maximum monthly spending limit in USD. `None` means no limit.
    #[serde(default)]
    pub monthly_budget_usd: Option<f64>,

    /// Whether to track token usage for cost estimation.
    #[serde(default = "default_track_tokens")]
    pub track_tokens: bool,
}

impl Default for CostConfig {
    fn default() -> Self {
        Self {
            daily_budget_usd: None,
            monthly_budget_usd: None,
            track_tokens: default_track_tokens(),
        }
    }
}

fn default_track_tokens() -> bool {
    true
}
