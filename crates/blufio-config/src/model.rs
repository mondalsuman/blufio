// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Configuration model structs for the Blufio agent framework.
//!
//! All structs use `#[serde(deny_unknown_fields)]` to reject unrecognized
//! config keys at startup, providing actionable error messages.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level Blufio configuration.
///
/// Loaded from TOML files following XDG hierarchy, with environment variable overrides.
/// All sections are optional and default to sensible values.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
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

    /// Credential vault settings.
    #[serde(default)]
    pub vault: VaultConfig,

    /// Cost tracking and budget settings.
    #[serde(default)]
    pub cost: CostConfig,

    /// Context engine settings.
    #[serde(default)]
    pub context: ContextConfig,

    /// Memory system settings.
    #[serde(default)]
    pub memory: MemoryConfig,

    /// Model routing settings.
    #[serde(default)]
    pub routing: RoutingConfig,

    /// Smart heartbeat settings.
    #[serde(default)]
    pub heartbeat: HeartbeatConfig,

    /// WASM skill sandbox settings.
    #[serde(default)]
    pub skill: SkillConfig,

    /// Plugin system settings.
    #[serde(default)]
    pub plugin: PluginConfig,

    /// HTTP/WebSocket gateway settings.
    #[serde(default)]
    pub gateway: GatewayConfig,

    /// Prometheus metrics settings.
    #[serde(default)]
    pub prometheus: PrometheusConfig,

    /// Daemon and memory management settings.
    #[serde(default)]
    pub daemon: DaemonConfig,

    /// Specialist agent definitions for multi-agent delegation.
    #[serde(default)]
    pub agents: Vec<AgentSpecConfig>,

    /// Multi-agent delegation settings.
    #[serde(default)]
    pub delegation: DelegationConfig,
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

    /// Inline system prompt string. Overridden by `system_prompt_file` if both set.
    #[serde(default)]
    pub system_prompt: Option<String>,

    /// Path to a markdown file containing the system prompt.
    /// Takes precedence over `system_prompt` if both are set.
    #[serde(default)]
    pub system_prompt_file: Option<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: default_agent_name(),
            max_sessions: default_max_sessions(),
            log_level: default_log_level(),
            system_prompt: None,
            system_prompt_file: None,
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
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TelegramConfig {
    /// Telegram Bot API token. `None` disables Telegram integration.
    #[serde(default)]
    pub bot_token: Option<String>,

    /// List of allowed Telegram user IDs or usernames.
    #[serde(default)]
    pub allowed_users: Vec<String>,
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

    /// Maximum tokens to generate per response.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,

    /// Anthropic API version string.
    #[serde(default = "default_api_version")]
    pub api_version: String,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            default_model: default_model(),
            max_tokens: default_max_tokens(),
            api_version: default_api_version(),
        }
    }
}

fn default_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}

fn default_max_tokens() -> u32 {
    4096
}

fn default_api_version() -> String {
    "2023-06-01".to_string()
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
    dirs::data_dir()
        .map(|p| p.join("blufio").join("blufio.db"))
        .unwrap_or_else(|| std::path::PathBuf::from("blufio.db"))
        .to_string_lossy()
        .into_owned()
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

    /// Private IP addresses allowed for SSRF exemption (e.g., local services).
    #[serde(default)]
    pub allowed_private_ips: Vec<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            bind_address: default_bind_address(),
            require_tls: default_require_tls(),
            allowed_private_ips: Vec::new(),
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

/// Credential vault configuration.
///
/// Controls Argon2id key derivation parameters used to protect the vault
/// master key. Defaults follow OWASP recommendations.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VaultConfig {
    /// Argon2id memory cost in KiB (default: 65536 = 64 MiB).
    #[serde(default = "default_kdf_memory_cost")]
    pub kdf_memory_cost: u32,

    /// Argon2id iteration count (default: 3).
    #[serde(default = "default_kdf_iterations")]
    pub kdf_iterations: u32,

    /// Argon2id parallelism lanes (default: 4).
    #[serde(default = "default_kdf_parallelism")]
    pub kdf_parallelism: u32,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            kdf_memory_cost: default_kdf_memory_cost(),
            kdf_iterations: default_kdf_iterations(),
            kdf_parallelism: default_kdf_parallelism(),
        }
    }
}

fn default_kdf_memory_cost() -> u32 {
    65536 // 64 MiB per OWASP recommendation
}

fn default_kdf_iterations() -> u32 {
    3
}

fn default_kdf_parallelism() -> u32 {
    4
}

/// Context engine configuration.
///
/// Controls context assembly behavior including compaction parameters.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextConfig {
    /// Model to use for compaction summarization.
    #[serde(default = "default_compaction_model")]
    pub compaction_model: String,

    /// Compaction threshold as fraction of context window (0.0-1.0).
    /// When estimated tokens exceed this fraction, compaction triggers.
    #[serde(default = "default_compaction_threshold")]
    pub compaction_threshold: f64,

    /// Context window budget in tokens.
    #[serde(default = "default_context_budget")]
    pub context_budget: u32,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            compaction_model: default_compaction_model(),
            compaction_threshold: default_compaction_threshold(),
            context_budget: default_context_budget(),
        }
    }
}

fn default_compaction_model() -> String {
    "claude-haiku-4-5-20250901".to_string()
}

fn default_compaction_threshold() -> f64 {
    0.70
}

fn default_context_budget() -> u32 {
    180_000
}

/// Memory system configuration.
///
/// Controls long-term memory extraction, storage, and retrieval.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryConfig {
    /// Enable the memory system. When false, no memory operations occur.
    #[serde(default = "default_memory_enabled")]
    pub enabled: bool,

    /// Minimum cosine similarity threshold for memory retrieval (0.0-1.0).
    /// Memories below this threshold are not loaded into context.
    #[serde(default = "default_similarity_threshold")]
    pub similarity_threshold: f64,

    /// Name of the embedding model to use.
    #[serde(default = "default_model_name")]
    pub model_name: String,

    /// Model to use for memory extraction (Haiku for cost efficiency).
    #[serde(default = "default_extraction_model")]
    pub extraction_model: String,

    /// Seconds of idle time before triggering memory extraction.
    #[serde(default = "default_idle_timeout_secs")]
    pub idle_timeout_secs: u64,

    /// Maximum number of candidate results per search method (pre-RRF).
    #[serde(default = "default_max_retrieval_results")]
    pub max_retrieval_results: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enabled: default_memory_enabled(),
            similarity_threshold: default_similarity_threshold(),
            model_name: default_model_name(),
            extraction_model: default_extraction_model(),
            idle_timeout_secs: default_idle_timeout_secs(),
            max_retrieval_results: default_max_retrieval_results(),
        }
    }
}

fn default_memory_enabled() -> bool {
    true
}

fn default_similarity_threshold() -> f64 {
    0.35
}

fn default_model_name() -> String {
    "all-MiniLM-L6-v2".to_string()
}

fn default_extraction_model() -> String {
    "claude-haiku-4-5-20250901".to_string()
}

fn default_idle_timeout_secs() -> u64 {
    300 // 5 minutes
}

fn default_max_retrieval_results() -> usize {
    50
}

/// Model routing configuration.
///
/// Controls automatic query complexity classification and model tier selection.
/// When enabled, the agent routes user-facing messages to Haiku (simple),
/// Sonnet (standard), or Opus (complex) based on heuristic classification.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoutingConfig {
    /// Enable model routing. When false, uses anthropic.default_model for all messages.
    #[serde(default = "default_routing_enabled")]
    pub enabled: bool,

    /// Force all messages to a specific model, bypassing classification.
    /// Example: "claude-sonnet-4-20250514"
    #[serde(default)]
    pub force_model: Option<String>,

    /// Model identifier for simple queries (Haiku tier).
    #[serde(default = "default_simple_model")]
    pub simple_model: String,

    /// Model identifier for standard queries (Sonnet tier).
    #[serde(default = "default_standard_model")]
    pub standard_model: String,

    /// Model identifier for complex queries (Opus tier).
    #[serde(default = "default_complex_model")]
    pub complex_model: String,

    /// Max tokens for simple tier responses.
    #[serde(default = "default_simple_max_tokens")]
    pub simple_max_tokens: u32,

    /// Max tokens for standard tier responses.
    #[serde(default = "default_standard_max_tokens")]
    pub standard_max_tokens: u32,

    /// Max tokens for complex tier responses.
    #[serde(default = "default_complex_max_tokens")]
    pub complex_max_tokens: u32,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            enabled: default_routing_enabled(),
            force_model: None,
            simple_model: default_simple_model(),
            standard_model: default_standard_model(),
            complex_model: default_complex_model(),
            simple_max_tokens: default_simple_max_tokens(),
            standard_max_tokens: default_standard_max_tokens(),
            complex_max_tokens: default_complex_max_tokens(),
        }
    }
}

fn default_routing_enabled() -> bool {
    true
}

fn default_simple_model() -> String {
    "claude-haiku-4-5-20250901".to_string()
}

fn default_standard_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}

fn default_complex_model() -> String {
    "claude-opus-4-20250514".to_string()
}

fn default_simple_max_tokens() -> u32 {
    1024
}

fn default_standard_max_tokens() -> u32 {
    4096
}

fn default_complex_max_tokens() -> u32 {
    8192
}

/// Smart heartbeat configuration.
///
/// Controls proactive check-in behavior. Heartbeats run on Haiku
/// with their own dedicated budget, separate from conversation costs.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HeartbeatConfig {
    /// Enable smart heartbeats. Opt-in feature.
    #[serde(default = "default_heartbeat_enabled")]
    pub enabled: bool,

    /// Heartbeat check interval in seconds.
    #[serde(default = "default_heartbeat_interval_secs")]
    pub interval_secs: u64,

    /// Delivery mode: "immediate" sends Telegram message directly,
    /// "on_next_message" stores insight for next user interaction.
    #[serde(default = "default_heartbeat_delivery")]
    pub delivery: String,

    /// Monthly budget cap for heartbeats in USD. Separate from conversation budget.
    #[serde(default = "default_heartbeat_monthly_budget_usd")]
    pub monthly_budget_usd: f64,

    /// Model to use for heartbeat LLM calls.
    #[serde(default = "default_heartbeat_model")]
    pub model: String,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            enabled: default_heartbeat_enabled(),
            interval_secs: default_heartbeat_interval_secs(),
            delivery: default_heartbeat_delivery(),
            monthly_budget_usd: default_heartbeat_monthly_budget_usd(),
            model: default_heartbeat_model(),
        }
    }
}

fn default_heartbeat_enabled() -> bool {
    false
}

fn default_heartbeat_interval_secs() -> u64 {
    3600 // 1 hour
}

fn default_heartbeat_delivery() -> String {
    "on_next_message".to_string()
}

fn default_heartbeat_monthly_budget_usd() -> f64 {
    10.0
}

fn default_heartbeat_model() -> String {
    "claude-haiku-4-5-20250901".to_string()
}

/// WASM skill sandbox configuration.
///
/// Controls skill installation directory, default resource limits for WASM
/// sandboxes, and the maximum number of skill tool definitions included
/// in LLM prompts.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SkillConfig {
    /// Directory where installed skill WASM bundles are stored.
    #[serde(default = "default_skills_dir")]
    pub skills_dir: String,

    /// Default fuel limit for WASM execution (overridden by skill manifest).
    #[serde(default = "default_skill_fuel")]
    pub default_fuel: u64,

    /// Default memory limit in megabytes for WASM execution.
    #[serde(default = "default_skill_memory_mb")]
    pub default_memory_mb: u32,

    /// Default epoch timeout in seconds for WASM wall-clock limit.
    #[serde(default = "default_skill_epoch_timeout")]
    pub default_epoch_timeout_secs: u64,

    /// Maximum number of skill tool definitions included in LLM prompts.
    #[serde(default = "default_max_skills_in_prompt")]
    pub max_skills_in_prompt: usize,

    /// Enable the skill system. When false, no skills are loaded or executed.
    #[serde(default = "default_skill_enabled")]
    pub enabled: bool,
}

impl Default for SkillConfig {
    fn default() -> Self {
        Self {
            skills_dir: default_skills_dir(),
            default_fuel: default_skill_fuel(),
            default_memory_mb: default_skill_memory_mb(),
            default_epoch_timeout_secs: default_skill_epoch_timeout(),
            max_skills_in_prompt: default_max_skills_in_prompt(),
            enabled: default_skill_enabled(),
        }
    }
}

fn default_skills_dir() -> String {
    dirs::data_dir()
        .map(|p| p.join("blufio").join("skills"))
        .unwrap_or_else(|| std::path::PathBuf::from("skills"))
        .to_string_lossy()
        .into_owned()
}

fn default_skill_fuel() -> u64 {
    1_000_000_000
}

fn default_skill_memory_mb() -> u32 {
    16
}

fn default_skill_epoch_timeout() -> u64 {
    5
}

fn default_max_skills_in_prompt() -> usize {
    20
}

fn default_skill_enabled() -> bool {
    false
}

/// Plugin system configuration.
///
/// Controls which compiled-in adapters are enabled/disabled.
/// Each entry in the `plugins` map overrides the default enabled state.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginConfig {
    /// Per-plugin enable/disable overrides.
    /// Key: plugin name (e.g., "telegram", "anthropic").
    /// Value: true = enabled, false = disabled.
    #[serde(default)]
    pub plugins: HashMap<String, bool>,
}

/// HTTP/WebSocket gateway configuration.
///
/// Controls the API gateway server for programmatic access alongside
/// channel-based messaging (e.g., Telegram).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayConfig {
    /// Enable the HTTP/WebSocket gateway.
    #[serde(default = "default_gateway_enabled")]
    pub enabled: bool,
    /// Host address to bind the gateway server.
    #[serde(default = "default_gateway_host")]
    pub host: String,
    /// Port for the gateway server.
    #[serde(default = "default_gateway_port")]
    pub port: u16,
    /// Bearer token for API authentication. If empty, auth is disabled.
    #[serde(default)]
    pub bearer_token: Option<String>,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            enabled: default_gateway_enabled(),
            host: default_gateway_host(),
            port: default_gateway_port(),
            bearer_token: None,
        }
    }
}

fn default_gateway_enabled() -> bool {
    false
}

fn default_gateway_host() -> String {
    "127.0.0.1".to_string()
}

fn default_gateway_port() -> u16 {
    3000
}

/// Prometheus metrics configuration.
///
/// Controls Prometheus metrics collection and export via the gateway /metrics endpoint.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrometheusConfig {
    /// Enable Prometheus metrics collection and export.
    #[serde(default = "default_prometheus_enabled")]
    pub enabled: bool,
}

impl Default for PrometheusConfig {
    fn default() -> Self {
        Self {
            enabled: default_prometheus_enabled(),
        }
    }
}

fn default_prometheus_enabled() -> bool {
    false
}

/// Daemon and memory management configuration.
///
/// Controls memory monitoring thresholds, health endpoint settings,
/// and cache shedding behavior for production deployment.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DaemonConfig {
    /// Heap memory warning threshold in MB. When jemalloc allocated bytes
    /// exceed this, a warning is logged and caches are proactively shed.
    #[serde(default = "default_memory_warn_mb")]
    pub memory_warn_mb: u64,

    /// Heap memory limit in MB. When exceeded, new sessions are rejected
    /// to prevent OOM on constrained VPS deployments.
    #[serde(default = "default_memory_limit_mb")]
    pub memory_limit_mb: u64,

    /// Port for the health endpoint. Defaults to the gateway port.
    #[serde(default = "default_health_port")]
    pub health_port: u16,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            memory_warn_mb: default_memory_warn_mb(),
            memory_limit_mb: default_memory_limit_mb(),
            health_port: default_health_port(),
        }
    }
}

fn default_memory_warn_mb() -> u64 {
    150
}

fn default_memory_limit_mb() -> u64 {
    200
}

fn default_health_port() -> u16 {
    3000
}

/// Configuration for a specialist agent used in multi-agent delegation.
///
/// Defined via `[[agents]]` TOML array entries. Each specialist agent
/// has its own system prompt, model, and allowed skill set.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentSpecConfig {
    /// Unique name for this specialist agent.
    pub name: String,

    /// System prompt that defines the specialist's behavior.
    pub system_prompt: String,

    /// LLM model to use for this specialist.
    #[serde(default = "default_specialist_model")]
    pub model: String,

    /// List of tool/skill names this specialist is allowed to use.
    #[serde(default)]
    pub allowed_skills: Vec<String>,
}

fn default_specialist_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}

/// Multi-agent delegation configuration.
///
/// Controls whether delegation is enabled and how long to wait
/// for specialist responses before timing out.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DelegationConfig {
    /// Enable multi-agent delegation.
    #[serde(default)]
    pub enabled: bool,

    /// Timeout in seconds for specialist responses.
    #[serde(default = "default_delegation_timeout")]
    pub timeout_secs: u64,
}

impl Default for DelegationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: default_delegation_timeout(),
        }
    }
}

fn default_delegation_timeout() -> u64 {
    60
}
