// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Configuration model structs for the Blufio agent framework.
//!
//! All structs use `#[serde(deny_unknown_fields)]` to reject unrecognized
//! config keys at startup, providing actionable error messages.

use blufio_core::classification::DataClassification;
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

    /// Discord bot integration settings.
    #[serde(default)]
    pub discord: DiscordConfig,

    /// Slack app integration settings.
    #[serde(default)]
    pub slack: SlackConfig,

    /// WhatsApp channel integration settings.
    #[serde(default)]
    pub whatsapp: WhatsAppConfig,

    /// Signal channel integration settings.
    #[serde(default)]
    pub signal: SignalConfig,

    /// IRC channel integration settings.
    #[serde(default)]
    pub irc: IrcConfig,

    /// Matrix channel integration settings.
    #[serde(default)]
    pub matrix: MatrixConfig,

    /// Email channel integration settings.
    #[serde(default)]
    pub email: EmailConfig,

    /// iMessage channel integration settings (experimental, requires macOS + BlueBubbles).
    #[serde(default)]
    pub imessage: IMessageConfig,

    /// SMS channel integration settings (Twilio).
    #[serde(default)]
    pub sms: SmsConfig,

    /// Cross-channel bridge configuration.
    #[serde(default)]
    pub bridge: std::collections::HashMap<String, BridgeGroupConfig>,

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

    /// Observability settings (tracing, metrics).
    #[serde(default)]
    pub observability: ObservabilityConfig,

    /// Litestream WAL replication settings.
    #[serde(default)]
    pub litestream: LitestreamConfig,

    /// Daemon and memory management settings.
    #[serde(default)]
    pub daemon: DaemonConfig,

    /// Specialist agent definitions for multi-agent delegation.
    #[serde(default)]
    pub agents: Vec<AgentSpecConfig>,

    /// Multi-agent delegation settings.
    #[serde(default)]
    pub delegation: DelegationConfig,

    /// MCP (Model Context Protocol) settings.
    #[serde(default)]
    pub mcp: McpConfig,

    /// Provider configuration including custom provider declarations.
    #[serde(default)]
    pub providers: ProvidersConfig,

    /// Node system configuration for paired device mesh.
    #[serde(default)]
    pub node: NodeConfig,

    /// Performance tuning settings.
    #[serde(default)]
    pub performance: PerformanceConfig,

    /// Resilience settings (circuit breakers, degradation ladder).
    #[serde(default)]
    pub resilience: ResilienceConfig,

    /// Data classification settings.
    #[serde(default)]
    pub classification: ClassificationConfig,

    /// Audit trail settings.
    #[serde(default)]
    pub audit: AuditConfig,

    /// Injection defense settings.
    #[serde(default)]
    pub injection_defense: InjectionDefenseConfig,

    /// Cron scheduler settings.
    #[serde(default)]
    pub cron: CronConfig,

    /// Data retention policy settings.
    #[serde(default)]
    pub retention: RetentionConfig,

    /// Hook system settings.
    #[serde(default)]
    pub hooks: HookConfig,

    /// Hot reload settings.
    #[serde(default)]
    pub hot_reload: HotReloadConfig,

    /// GDPR data subject rights tooling settings.
    #[serde(default)]
    pub gdpr: GdprConfig,
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

/// Discord bot integration configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiscordConfig {
    /// Discord bot token. `None` disables Discord integration.
    #[serde(default)]
    pub bot_token: Option<String>,

    /// Discord application ID (for slash command registration).
    #[serde(default)]
    pub application_id: Option<u64>,

    /// List of allowed Discord user IDs.
    #[serde(default)]
    pub allowed_users: Vec<String>,
}

/// Slack app integration configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SlackConfig {
    /// Slack bot token (xoxb-*). `None` disables Slack integration.
    #[serde(default)]
    pub bot_token: Option<String>,

    /// Slack app-level token (xapp-*) for Socket Mode.
    #[serde(default)]
    pub app_token: Option<String>,

    /// List of allowed Slack user IDs.
    #[serde(default)]
    pub allowed_users: Vec<String>,
}

/// WhatsApp channel integration configuration.
///
/// Supports two variants: Cloud API (production) and Web (experimental).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WhatsAppConfig {
    /// Variant: "cloud" (default) or "web" (experimental).
    #[serde(default)]
    pub variant: Option<String>,
    /// WhatsApp Business phone number ID (Cloud API).
    #[serde(default)]
    pub phone_number_id: Option<String>,
    /// Meta Graph API access token (Cloud API).
    #[serde(default)]
    pub access_token: Option<String>,
    /// Webhook verify token for subscription validation (Cloud API).
    #[serde(default)]
    pub verify_token: Option<String>,
    /// App secret for HMAC-SHA256 webhook signature verification (Cloud API).
    #[serde(default)]
    pub app_secret: Option<String>,
    /// Path to WhatsApp Web session data (Web variant).
    #[serde(default)]
    pub session_data_path: Option<String>,
    /// List of allowed phone numbers or user IDs.
    #[serde(default)]
    pub allowed_users: Vec<String>,
}

/// Signal channel integration configuration.
///
/// Connects to an externally managed signal-cli JSON-RPC daemon.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SignalConfig {
    /// Unix domain socket path for signal-cli daemon. Takes priority over TCP.
    #[serde(default)]
    pub socket_path: Option<String>,
    /// TCP host for signal-cli daemon (default: 127.0.0.1).
    #[serde(default)]
    pub host: Option<String>,
    /// TCP port for signal-cli daemon (default: 7583).
    #[serde(default)]
    pub port: Option<u16>,
    /// Bot's own phone number (for @mention detection in groups).
    #[serde(default)]
    pub phone_number: Option<String>,
    /// List of allowed sender phone numbers.
    #[serde(default)]
    pub allowed_users: Vec<String>,
}

/// IRC channel integration configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IrcConfig {
    /// IRC server hostname.
    #[serde(default)]
    pub server: Option<String>,
    /// IRC server port (default: 6697 for TLS, 6667 without).
    #[serde(default)]
    pub port: Option<u16>,
    /// Bot nickname.
    #[serde(default)]
    pub nickname: Option<String>,
    /// Channels to join (e.g., ["#blufio", "#support"]).
    #[serde(default)]
    pub channels: Vec<String>,
    /// Enable TLS (default: true).
    #[serde(default = "default_true")]
    pub tls: bool,
    /// Authentication method: "sasl" or "nickserv".
    #[serde(default)]
    pub auth_method: Option<String>,
    /// Password for SASL or NickServ authentication.
    #[serde(default)]
    pub password: Option<String>,
    /// Rate limit between messages in milliseconds (default: 2000).
    #[serde(default = "default_irc_rate_limit")]
    pub rate_limit_ms: u64,
    /// List of allowed user nicks.
    #[serde(default)]
    pub allowed_users: Vec<String>,
}

impl Default for IrcConfig {
    fn default() -> Self {
        Self {
            server: None,
            port: None,
            nickname: None,
            channels: vec![],
            tls: true,
            auth_method: None,
            password: None,
            rate_limit_ms: default_irc_rate_limit(),
            allowed_users: vec![],
        }
    }
}

fn default_irc_rate_limit() -> u64 {
    2000
}

fn default_true() -> bool {
    true
}

/// Matrix channel integration configuration.
///
/// Uses matrix-sdk 0.11.0 (pinned). E2E encryption is deferred to EXT-06.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MatrixConfig {
    /// Matrix homeserver URL (e.g., "https://matrix.org").
    #[serde(default)]
    pub homeserver_url: Option<String>,
    /// Matrix username (localpart, e.g., "blufio-bot").
    #[serde(default)]
    pub username: Option<String>,
    /// Matrix password.
    #[serde(default)]
    pub password: Option<String>,
    /// Rooms to auto-join on startup (room IDs or aliases).
    #[serde(default)]
    pub rooms: Vec<String>,
    /// Display name for the bot.
    #[serde(default)]
    pub display_name: Option<String>,
    /// List of allowed Matrix user IDs (@user:server).
    #[serde(default)]
    pub allowed_users: Vec<String>,
}

/// Email channel integration configuration.
///
/// IMAP for incoming messages, SMTP (lettre) for outgoing.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EmailConfig {
    /// IMAP server hostname. `None` disables email integration.
    #[serde(default)]
    pub imap_host: Option<String>,
    /// IMAP server port (default: 993 for IMAPS).
    #[serde(default)]
    pub imap_port: Option<u16>,
    /// SMTP server hostname. Defaults to imap_host if not set.
    #[serde(default)]
    pub smtp_host: Option<String>,
    /// SMTP server port (default: 587 for STARTTLS).
    #[serde(default)]
    pub smtp_port: Option<u16>,
    /// Email username (used for both IMAP and SMTP by default).
    #[serde(default)]
    pub username: Option<String>,
    /// Email password (used for both IMAP and SMTP by default).
    #[serde(default)]
    pub password: Option<String>,
    /// Separate SMTP username (overrides username for SMTP if set).
    #[serde(default)]
    pub smtp_username: Option<String>,
    /// Separate SMTP password (overrides password for SMTP if set).
    #[serde(default)]
    pub smtp_password: Option<String>,
    /// From address for outgoing emails.
    #[serde(default)]
    pub from_address: Option<String>,
    /// Display name for outgoing emails (default: "Blufio").
    #[serde(default)]
    pub from_name: Option<String>,
    /// IMAP polling interval in seconds (default: 30).
    #[serde(default = "default_email_poll_interval")]
    pub poll_interval_secs: u64,
    /// IMAP folders to monitor (default: ["INBOX"]).
    #[serde(default)]
    pub folders: Vec<String>,
    /// Allow insecure (non-TLS) connections (default: false).
    #[serde(default)]
    pub allow_insecure: bool,
    /// List of allowed sender email addresses. Empty = accept all.
    #[serde(default)]
    pub allowed_senders: Vec<String>,
    /// Optional footer appended to outgoing emails.
    #[serde(default)]
    pub email_footer: Option<String>,
}

fn default_email_poll_interval() -> u64 {
    30
}

/// iMessage channel integration configuration (experimental).
///
/// Requires a BlueBubbles server running on macOS.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IMessageConfig {
    /// BlueBubbles server URL. `None` disables iMessage integration.
    #[serde(default)]
    pub bluebubbles_url: Option<String>,
    /// BlueBubbles API password.
    #[serde(default)]
    pub api_password: Option<String>,
    /// Webhook callback URL for BlueBubbles to POST incoming messages.
    #[serde(default)]
    pub webhook_callback_url: Option<String>,
    /// Shared secret for webhook endpoint validation.
    #[serde(default)]
    pub webhook_secret: Option<String>,
    /// Trigger prefix for group chats (default: "Blufio").
    #[serde(default)]
    pub group_trigger: Option<String>,
    /// List of allowed contact phone numbers or identifiers. Empty = accept all.
    #[serde(default)]
    pub allowed_contacts: Vec<String>,
}

/// SMS channel integration configuration (Twilio).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmsConfig {
    /// Twilio Account SID. `None` disables SMS integration.
    #[serde(default)]
    pub account_sid: Option<String>,
    /// Twilio Auth Token.
    #[serde(default)]
    pub auth_token: Option<String>,
    /// Twilio phone number in E.164 format (+1234567890).
    #[serde(default)]
    pub twilio_phone_number: Option<String>,
    /// Webhook URL for Twilio to POST incoming messages.
    #[serde(default)]
    pub webhook_url: Option<String>,
    /// Maximum outbound message length in characters (default: 1600).
    #[serde(default = "default_sms_max_length")]
    pub max_response_length: usize,
    /// Outbound rate limit in messages per second (default: 1).
    #[serde(default = "default_sms_rate_limit")]
    pub rate_limit_per_second: f32,
    /// List of allowed phone numbers. Empty = accept all.
    #[serde(default)]
    pub allowed_numbers: Vec<String>,
}

fn default_sms_max_length() -> usize {
    1600
}

fn default_sms_rate_limit() -> f32 {
    1.0
}

/// Cross-channel bridge group configuration.
///
/// Defines a group of channels that should have messages bridged between them.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeGroupConfig {
    /// Channels in this bridge group (e.g., ["telegram", "discord", "slack"]).
    pub channels: Vec<String>,
    /// Exclude bot messages from bridging (default: true).
    #[serde(default = "default_true")]
    pub exclude_bots: bool,
    /// Only bridge messages from these users (empty = all users).
    #[serde(default)]
    pub include_users: Vec<String>,
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
/// Controls context assembly behavior including compaction parameters,
/// quality scoring, zone budgets, and archive settings.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextConfig {
    /// Model to use for compaction summarization.
    #[serde(default = "default_compaction_model")]
    pub compaction_model: String,

    /// **Deprecated**: Use `soft_trigger` instead. Kept for backward compatibility.
    /// When present, mapped to `soft_trigger` if that field is at its default.
    #[serde(default)]
    pub compaction_threshold: Option<f64>,

    /// Context window budget in tokens.
    #[serde(default = "default_context_budget")]
    pub context_budget: u32,

    /// Enable compaction engine.
    #[serde(default = "default_true")]
    pub compaction_enabled: bool,

    /// Fraction of context budget at which soft (background) compaction triggers.
    #[serde(default = "default_soft_trigger")]
    pub soft_trigger: f64,

    /// Fraction of context budget at which hard (blocking) compaction triggers.
    #[serde(default = "default_hard_trigger")]
    pub hard_trigger: f64,

    /// Enable quality scoring of compaction summaries.
    #[serde(default = "default_true")]
    pub quality_scoring: bool,

    /// Quality gate: proceed threshold. Summaries scoring above this pass.
    #[serde(default = "default_quality_gate_proceed")]
    pub quality_gate_proceed: f64,

    /// Quality gate: retry threshold. Summaries scoring below this are retried.
    #[serde(default = "default_quality_gate_retry")]
    pub quality_gate_retry: f64,

    /// Quality weight for entity preservation.
    #[serde(default = "default_quality_weight_entity")]
    pub quality_weight_entity: f64,

    /// Quality weight for decision preservation.
    #[serde(default = "default_quality_weight_decision")]
    pub quality_weight_decision: f64,

    /// Quality weight for action item preservation.
    #[serde(default = "default_quality_weight_action")]
    pub quality_weight_action: f64,

    /// Quality weight for numerical data preservation.
    #[serde(default = "default_quality_weight_numerical")]
    pub quality_weight_numerical: f64,

    /// Maximum tokens for L1 (hot) compaction summaries.
    #[serde(default = "default_max_tokens_l1")]
    pub max_tokens_l1: u32,

    /// Maximum tokens for L2 (warm) compaction summaries.
    #[serde(default = "default_max_tokens_l2")]
    pub max_tokens_l2: u32,

    /// Maximum tokens for L3 (cold/archive) compaction summaries.
    #[serde(default = "default_max_tokens_l3")]
    pub max_tokens_l3: u32,

    /// Token budget for the static zone (system prompt, pinned content).
    #[serde(default = "default_static_zone_budget")]
    pub static_zone_budget: u32,

    /// Token budget for the conditional zone (memories, file context).
    #[serde(default = "default_conditional_zone_budget")]
    pub conditional_zone_budget: u32,

    /// Enable archiving of compaction summaries.
    #[serde(default = "default_true")]
    pub archive_enabled: bool,

    /// Maximum number of archives to retain per user.
    #[serde(default = "default_max_archives")]
    pub max_archives: u32,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            compaction_model: default_compaction_model(),
            compaction_threshold: None,
            context_budget: default_context_budget(),
            compaction_enabled: true,
            soft_trigger: default_soft_trigger(),
            hard_trigger: default_hard_trigger(),
            quality_scoring: true,
            quality_gate_proceed: default_quality_gate_proceed(),
            quality_gate_retry: default_quality_gate_retry(),
            quality_weight_entity: default_quality_weight_entity(),
            quality_weight_decision: default_quality_weight_decision(),
            quality_weight_action: default_quality_weight_action(),
            quality_weight_numerical: default_quality_weight_numerical(),
            max_tokens_l1: default_max_tokens_l1(),
            max_tokens_l2: default_max_tokens_l2(),
            max_tokens_l3: default_max_tokens_l3(),
            static_zone_budget: default_static_zone_budget(),
            conditional_zone_budget: default_conditional_zone_budget(),
            archive_enabled: true,
            max_archives: default_max_archives(),
        }
    }
}

impl ContextConfig {
    /// Returns the effective soft trigger threshold, honoring the deprecated
    /// `compaction_threshold` field for backward compatibility.
    ///
    /// If `compaction_threshold` is set and `soft_trigger` is at its default (0.50),
    /// the old threshold value is used with a deprecation warning. If both are
    /// explicitly set, `soft_trigger` wins and a warning is emitted.
    pub fn effective_soft_trigger(&self) -> f64 {
        match self.compaction_threshold {
            Some(old) if (self.soft_trigger - default_soft_trigger()).abs() < f64::EPSILON => {
                tracing::warn!(
                    "compaction_threshold is deprecated, use soft_trigger instead; \
                     using compaction_threshold={old} as soft_trigger"
                );
                old
            }
            Some(_old) => {
                tracing::warn!(
                    "both compaction_threshold and soft_trigger are set; \
                     compaction_threshold is deprecated and will be ignored"
                );
                self.soft_trigger
            }
            None => self.soft_trigger,
        }
    }
}

fn default_compaction_model() -> String {
    "claude-haiku-4-5-20250901".to_string()
}

fn default_context_budget() -> u32 {
    180_000
}

fn default_soft_trigger() -> f64 {
    0.50
}

fn default_hard_trigger() -> f64 {
    0.85
}

fn default_quality_gate_proceed() -> f64 {
    0.6
}

fn default_quality_gate_retry() -> f64 {
    0.4
}

fn default_quality_weight_entity() -> f64 {
    0.35
}

fn default_quality_weight_decision() -> f64 {
    0.25
}

fn default_quality_weight_action() -> f64 {
    0.25
}

fn default_quality_weight_numerical() -> f64 {
    0.15
}

fn default_max_tokens_l1() -> u32 {
    256
}

fn default_max_tokens_l2() -> u32 {
    1024
}

fn default_max_tokens_l3() -> u32 {
    2048
}

fn default_static_zone_budget() -> u32 {
    3000
}

fn default_conditional_zone_budget() -> u32 {
    8000
}

fn default_max_archives() -> u32 {
    10
}

/// Memory system configuration.
///
/// Controls long-term memory extraction, storage, retrieval, scoring,
/// eviction, validation, and file watching.
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

    // --- Scoring parameters ---
    /// Exponential decay factor applied per day since memory creation.
    /// `max(decay_factor^days, decay_floor)`. File-sourced memories skip decay.
    #[serde(default = "default_decay_factor")]
    pub decay_factor: f64,

    /// Minimum decay multiplier (floor). Prevents old memories from scoring zero.
    #[serde(default = "default_decay_floor")]
    pub decay_floor: f64,

    /// MMR lambda for diversity reranking (0.0 = max diversity, 1.0 = relevance only).
    #[serde(default = "default_mmr_lambda")]
    pub mmr_lambda: f64,

    /// Importance boost multiplier for explicit (user-created) memories.
    #[serde(default = "default_importance_boost_explicit")]
    pub importance_boost_explicit: f64,

    /// Importance boost multiplier for LLM-extracted memories.
    #[serde(default = "default_importance_boost_extracted")]
    pub importance_boost_extracted: f64,

    /// Importance boost multiplier for file-watcher-sourced memories.
    #[serde(default = "default_importance_boost_file")]
    pub importance_boost_file: f64,

    // --- Eviction parameters ---
    /// Maximum number of active memories before eviction triggers.
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,

    /// Interval in seconds between eviction sweeps.
    #[serde(default = "default_eviction_sweep_interval_secs")]
    pub eviction_sweep_interval_secs: u64,

    // --- Validation parameters ---
    /// Age in days after which a memory at decay floor is considered stale.
    #[serde(default = "default_stale_threshold_days")]
    pub stale_threshold_days: u64,

    // --- File watcher ---
    /// File watcher configuration for auto-indexing workspace files.
    #[serde(default)]
    pub file_watcher: FileWatcherConfig,
}

/// Configuration for the file watcher subsystem.
///
/// When `paths` is non-empty, the file watcher monitors those directories
/// for changes and auto-indexes matching files as memories.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FileWatcherConfig {
    /// Directories to watch for file changes. Empty disables the watcher.
    #[serde(default)]
    pub paths: Vec<String>,

    /// File extensions to include (e.g., ["md", "txt"]). Empty means all files.
    #[serde(default)]
    pub extensions: Vec<String>,

    /// Maximum file size in bytes. Files larger than this are skipped.
    #[serde(default = "default_max_file_size")]
    pub max_file_size: usize,
}

impl Default for FileWatcherConfig {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            extensions: Vec::new(),
            max_file_size: default_max_file_size(),
        }
    }
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
            decay_factor: default_decay_factor(),
            decay_floor: default_decay_floor(),
            mmr_lambda: default_mmr_lambda(),
            importance_boost_explicit: default_importance_boost_explicit(),
            importance_boost_extracted: default_importance_boost_extracted(),
            importance_boost_file: default_importance_boost_file(),
            max_entries: default_max_entries(),
            eviction_sweep_interval_secs: default_eviction_sweep_interval_secs(),
            stale_threshold_days: default_stale_threshold_days(),
            file_watcher: FileWatcherConfig::default(),
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

fn default_decay_factor() -> f64 {
    0.95
}

fn default_decay_floor() -> f64 {
    0.1
}

fn default_mmr_lambda() -> f64 {
    0.7
}

fn default_importance_boost_explicit() -> f64 {
    1.0
}

fn default_importance_boost_extracted() -> f64 {
    0.6
}

fn default_importance_boost_file() -> f64 {
    0.8
}

fn default_max_entries() -> usize {
    10_000
}

fn default_eviction_sweep_interval_secs() -> u64 {
    300
}

fn default_stale_threshold_days() -> u64 {
    180
}

fn default_max_file_size() -> usize {
    102_400 // 100 KB
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
    /// Allowlist of tool names accessible via the /v1/tools API.
    /// Empty = no tools accessible externally (secure default).
    #[serde(default)]
    pub api_tools_allowlist: Vec<String>,
    /// Default rate limit for new API keys (requests per minute).
    #[serde(default = "default_rate_limit")]
    pub default_rate_limit: i64,
    /// Maximum number of items allowed in a single batch request.
    #[serde(default = "default_max_batch_size")]
    pub max_batch_size: usize,
    /// OpenAPI documentation settings.
    #[serde(default)]
    pub openapi: OpenApiConfig,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            enabled: default_gateway_enabled(),
            host: default_gateway_host(),
            port: default_gateway_port(),
            bearer_token: None,
            api_tools_allowlist: Vec::new(),
            default_rate_limit: default_rate_limit(),
            max_batch_size: default_max_batch_size(),
            openapi: OpenApiConfig::default(),
        }
    }
}

fn default_rate_limit() -> i64 {
    60
}

fn default_max_batch_size() -> usize {
    100
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

/// Observability settings wrapper (tracing, metrics).
///
/// Groups tracing subsystems under a single config section.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct ObservabilityConfig {
    /// OpenTelemetry distributed tracing settings.
    pub opentelemetry: OpenTelemetryConfig,
}

/// OpenTelemetry tracing configuration.
///
/// Controls the OTel tracing pipeline: OTLP HTTP export, sampling, batching,
/// and resource attributes. Requires the `otel` feature to be compiled in.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct OpenTelemetryConfig {
    /// Enable OpenTelemetry tracing (requires `otel` feature compiled in).
    pub enabled: bool,
    /// OTLP HTTP endpoint URL.
    pub endpoint: String,
    /// Trace sampling ratio (0.0 = none, 1.0 = all).
    pub sample_ratio: f64,
    /// Service name reported in traces.
    pub service_name: String,
    /// Deployment environment label.
    pub environment: String,
    /// Batch export timeout in milliseconds.
    pub batch_timeout_ms: u64,
    /// Maximum spans per export batch.
    pub max_export_batch_size: usize,
    /// Maximum span queue size (bounded buffer).
    pub max_queue_size: usize,
    /// Custom resource attributes added to all spans.
    pub resource_attributes: HashMap<String, String>,
}

impl Default for OpenTelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "http://localhost:4318".to_string(),
            sample_ratio: 1.0,
            service_name: "blufio".to_string(),
            environment: "production".to_string(),
            batch_timeout_ms: 5000,
            max_export_batch_size: 512,
            max_queue_size: 2048,
            resource_attributes: HashMap::new(),
        }
    }
}

/// Litestream WAL replication configuration.
///
/// When enabled, sets `PRAGMA wal_autocheckpoint=0` on database open so
/// Litestream can manage WAL checkpointing for continuous replication.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct LitestreamConfig {
    /// Enable Litestream integration (sets PRAGMA wal_autocheckpoint=0).
    pub enabled: bool,
}

/// OpenAPI documentation configuration.
///
/// Controls Swagger UI availability at the `/docs` endpoint.
/// The `/openapi.json` spec endpoint is always served regardless of this setting.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct OpenApiConfig {
    /// Enable Swagger UI at `/docs` (requires `swagger-ui` feature compiled in).
    pub swagger_ui_enabled: bool,
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

/// MCP (Model Context Protocol) configuration.
///
/// Controls MCP server and client functionality. When disabled (default),
/// no MCP endpoints are exposed and no external MCP connections are made.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct McpConfig {
    /// Enable MCP functionality (server and client).
    #[serde(default)]
    pub enabled: bool,

    /// External MCP server configurations for the client.
    #[serde(default)]
    pub servers: Vec<McpServerEntry>,

    /// Tools to export via MCP server. Empty means use safe defaults.
    /// The "bash" tool is never exported regardless of this list.
    #[serde(default)]
    pub export_tools: Vec<String>,

    /// Timeout in seconds for individual tool invocations via MCP.
    /// Prevents hung WASM skills from blocking the connection.
    #[serde(default = "default_tool_timeout_secs")]
    pub tool_timeout_secs: u64,

    /// Bearer token for MCP HTTP transport authentication.
    /// Required when `enabled = true` (validated at startup).
    /// Distinct from the gateway's `bearer_token` for security isolation.
    #[serde(default)]
    pub auth_token: Option<String>,

    /// Allowed CORS origins for MCP HTTP endpoints.
    /// Empty list = reject all cross-origin requests (secure by default).
    /// Only applies to /mcp routes; existing gateway routes are unaffected.
    #[serde(default)]
    pub cors_origins: Vec<String>,

    /// Maximum concurrent MCP connections allowed.
    /// Over-limit connections receive HTTP 503 Service Unavailable.
    /// Default: 10 (conservative for personal agent use case).
    #[serde(default = "default_mcp_max_connections")]
    pub max_connections: usize,

    /// Health check interval in seconds for external MCP server ping checks.
    /// Default: 60 seconds.
    #[serde(default = "default_health_check_interval_secs")]
    pub health_check_interval_secs: u64,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            servers: Vec::new(),
            export_tools: Vec::new(),
            tool_timeout_secs: default_tool_timeout_secs(),
            auth_token: None,
            cors_origins: Vec::new(),
            max_connections: default_mcp_max_connections(),
            health_check_interval_secs: default_health_check_interval_secs(),
        }
    }
}

fn default_tool_timeout_secs() -> u64 {
    60
}

fn default_mcp_max_connections() -> usize {
    10
}

fn default_health_check_interval_secs() -> u64 {
    60
}

/// Configuration entry for an external MCP server.
///
/// Each entry represents a connection to an external MCP server that
/// Blufio can discover and invoke tools from.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct McpServerEntry {
    /// Unique name for this MCP server (used as namespace prefix).
    pub name: String,

    /// Transport type: "http" (Streamable HTTP, default) or "sse" (legacy).
    /// "stdio" is rejected at validation time (CLNT-11).
    pub transport: String,

    /// URL for HTTP/SSE transport connections.
    #[serde(default)]
    pub url: Option<String>,

    /// Command for stdio transport (rejected at validation — CLNT-11).
    #[serde(default)]
    pub command: Option<String>,

    /// Command arguments for stdio transport.
    #[serde(default)]
    pub args: Vec<String>,

    /// Optional bearer token for HTTP authentication.
    #[serde(default)]
    pub auth_token: Option<String>,

    /// Connection timeout in seconds (default: 10).
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u64,

    /// Maximum response size in characters (default: 4096).
    /// Responses exceeding this cap are truncated with a [truncated] suffix.
    #[serde(default = "default_response_size_cap")]
    pub response_size_cap: usize,

    /// Whether this server is operator-trusted. Trusted servers suppress
    /// trust zone warnings in agent prompts.
    #[serde(default)]
    pub trusted: bool,
}

fn default_connect_timeout_secs() -> u64 {
    10
}

fn default_response_size_cap() -> usize {
    4096
}

/// Provider configuration.
///
/// Contains default provider selection, per-provider config sections,
/// and custom provider declarations for third-party LLM services.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProvidersConfig {
    /// Default provider name. Valid values: "anthropic", "openai", "ollama", "openrouter", "gemini".
    #[serde(default = "default_provider")]
    pub default: String,

    /// OpenAI API configuration.
    #[serde(default)]
    pub openai: OpenAIConfig,

    /// Ollama (local) provider configuration.
    #[serde(default)]
    pub ollama: OllamaConfig,

    /// OpenRouter proxy configuration.
    #[serde(default)]
    pub openrouter: OpenRouterConfig,

    /// Google Gemini API configuration.
    #[serde(default)]
    pub gemini: GeminiConfig,

    /// Custom provider declarations.
    /// Key: provider name (e.g., "together", "groq").
    #[serde(default)]
    pub custom: HashMap<String, CustomProviderConfig>,
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            default: default_provider(),
            openai: OpenAIConfig::default(),
            ollama: OllamaConfig::default(),
            openrouter: OpenRouterConfig::default(),
            gemini: GeminiConfig::default(),
            custom: HashMap::new(),
        }
    }
}

fn default_provider() -> String {
    "anthropic".to_string()
}

/// OpenAI API configuration.
///
/// Configured via `[providers.openai]` in TOML config.
/// Supports custom `base_url` for Azure OpenAI, Together, Fireworks, etc.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAIConfig {
    /// OpenAI API key. `None` falls back to `OPENAI_API_KEY` env var.
    #[serde(default)]
    pub api_key: Option<String>,

    /// Default model to use for LLM requests.
    #[serde(default = "default_openai_model")]
    pub default_model: String,

    /// Base URL for the OpenAI-compatible API.
    #[serde(default = "default_openai_base_url")]
    pub base_url: String,

    /// Maximum tokens to generate per response.
    #[serde(default = "default_openai_max_tokens")]
    pub max_tokens: u32,
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            default_model: default_openai_model(),
            base_url: default_openai_base_url(),
            max_tokens: default_openai_max_tokens(),
        }
    }
}

fn default_openai_model() -> String {
    "gpt-4o".to_string()
}

fn default_openai_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_openai_max_tokens() -> u32 {
    4096
}

/// Ollama (local) provider configuration.
///
/// Configured via `[providers.ollama]` in TOML config.
/// No API key needed -- Ollama runs locally.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OllamaConfig {
    /// Base URL for the Ollama API.
    #[serde(default = "default_ollama_base_url")]
    pub base_url: String,

    /// Default model. No auto-pick -- user must specify.
    #[serde(default)]
    pub default_model: Option<String>,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: default_ollama_base_url(),
            default_model: None,
        }
    }
}

fn default_ollama_base_url() -> String {
    "http://localhost:11434".to_string()
}

/// OpenRouter proxy configuration.
///
/// Configured via `[providers.openrouter]` in TOML config.
/// Routes requests through OpenRouter's unified API to various providers.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpenRouterConfig {
    /// OpenRouter API key. `None` falls back to `OPENROUTER_API_KEY` env var.
    #[serde(default)]
    pub api_key: Option<String>,

    /// Default model (provider/model format).
    #[serde(default = "default_openrouter_model")]
    pub default_model: String,

    /// Application title sent via X-Title header.
    #[serde(default = "default_openrouter_x_title")]
    pub x_title: String,

    /// HTTP Referer header for OpenRouter analytics.
    #[serde(default)]
    pub http_referer: Option<String>,

    /// Preferred provider order for model routing.
    #[serde(default)]
    pub provider_order: Vec<String>,
}

impl Default for OpenRouterConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            default_model: default_openrouter_model(),
            x_title: default_openrouter_x_title(),
            http_referer: None,
            provider_order: Vec::new(),
        }
    }
}

fn default_openrouter_model() -> String {
    "anthropic/claude-sonnet-4".to_string()
}

fn default_openrouter_x_title() -> String {
    "Blufio".to_string()
}

/// Google Gemini API configuration.
///
/// Configured via `[providers.gemini]` in TOML config.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GeminiConfig {
    /// Gemini API key. `None` falls back to `GEMINI_API_KEY` env var.
    #[serde(default)]
    pub api_key: Option<String>,

    /// Default Gemini model.
    #[serde(default = "default_gemini_model")]
    pub default_model: String,
}

impl Default for GeminiConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            default_model: default_gemini_model(),
        }
    }
}

fn default_gemini_model() -> String {
    "gemini-2.0-flash".to_string()
}

/// Configuration for a custom LLM provider.
///
/// Declared via `[providers.custom.<name>]` in TOML config.
///
/// # Example
/// ```toml
/// [providers.custom.together]
/// base_url = "https://api.together.xyz/v1"
/// wire_protocol = "openai-compat"
/// api_key_env = "TOGETHER_API_KEY"
/// default_model = "meta-llama/Llama-3-70b-chat-hf"
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CustomProviderConfig {
    /// Base URL for the provider's API (e.g., "https://api.example.com/v1").
    pub base_url: String,

    /// Wire protocol for API communication.
    /// Currently supported: "openai-compat".
    pub wire_protocol: String,

    /// Environment variable name containing the API key.
    pub api_key_env: String,

    /// Default model identifier for this provider.
    #[serde(default)]
    pub default_model: Option<String>,
}

// --- Node system configuration ---

/// Node system configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NodeConfig {
    /// Whether the node system is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// User-friendly node name (defaults to hostname or generated ID).
    #[serde(default = "default_node_id")]
    pub node_id: String,

    /// WebSocket listener port for incoming node connections.
    #[serde(default = "default_node_listen_port")]
    pub listen_port: u16,

    /// Declared capabilities for this node.
    #[serde(default)]
    pub capabilities: Vec<String>,

    /// Heartbeat configuration.
    #[serde(default)]
    pub heartbeat: NodeHeartbeatConfig,

    /// Reconnection configuration.
    #[serde(default)]
    pub reconnect: NodeReconnectConfig,

    /// Approval routing configuration.
    #[serde(default)]
    pub approval: NodeApprovalConfig,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            node_id: default_node_id(),
            listen_port: default_node_listen_port(),
            capabilities: Vec::new(),
            heartbeat: NodeHeartbeatConfig::default(),
            reconnect: NodeReconnectConfig::default(),
            approval: NodeApprovalConfig::default(),
        }
    }
}

fn default_node_id() -> String {
    format!("node-{}", &uuid::Uuid::new_v4().to_string()[..8])
}

fn default_node_listen_port() -> u16 {
    9877
}

/// Node heartbeat timing configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NodeHeartbeatConfig {
    /// Interval between heartbeat sends, in seconds.
    #[serde(default = "default_node_heartbeat_interval")]
    pub interval_secs: u64,

    /// Threshold after which a node is marked stale (recommended: 3x interval).
    #[serde(default = "default_node_stale_threshold")]
    pub stale_threshold_secs: u64,
}

impl Default for NodeHeartbeatConfig {
    fn default() -> Self {
        Self {
            interval_secs: default_node_heartbeat_interval(),
            stale_threshold_secs: default_node_stale_threshold(),
        }
    }
}

fn default_node_heartbeat_interval() -> u64 {
    30
}

fn default_node_stale_threshold() -> u64 {
    90
}

/// Node WebSocket reconnection backoff configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NodeReconnectConfig {
    /// Initial delay before first reconnection attempt, in seconds.
    #[serde(default = "default_node_initial_delay")]
    pub initial_delay_secs: u64,

    /// Maximum delay between reconnection attempts, in seconds.
    #[serde(default = "default_node_max_delay")]
    pub max_delay_secs: u64,

    /// Whether to add random jitter to backoff delays.
    #[serde(default = "default_node_jitter")]
    pub jitter: bool,
}

impl Default for NodeReconnectConfig {
    fn default() -> Self {
        Self {
            initial_delay_secs: default_node_initial_delay(),
            max_delay_secs: default_node_max_delay(),
            jitter: default_node_jitter(),
        }
    }
}

fn default_node_initial_delay() -> u64 {
    1
}

fn default_node_max_delay() -> u64 {
    60
}

fn default_node_jitter() -> bool {
    true
}

/// Node approval routing configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NodeApprovalConfig {
    /// Action types that require broadcast approval.
    #[serde(default)]
    pub broadcast_actions: Vec<String>,

    /// Timeout before auto-denying a pending approval, in seconds.
    #[serde(default = "default_node_approval_timeout")]
    pub timeout_secs: u64,
}

impl Default for NodeApprovalConfig {
    fn default() -> Self {
        Self {
            broadcast_actions: Vec::new(),
            timeout_secs: default_node_approval_timeout(),
        }
    }
}

fn default_node_approval_timeout() -> u64 {
    300
}

// --- Performance tuning configuration ---

/// Performance tuning configuration.
///
/// Controls tokenizer accuracy/speed tradeoff. Set at startup, not switchable at runtime.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PerformanceConfig {
    /// Tokenizer mode: "accurate" uses real tokenizers, "fast" uses len/3.5 heuristic.
    /// Set at startup, not switchable at runtime.
    #[serde(default = "default_tokenizer_mode")]
    pub tokenizer_mode: String,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            tokenizer_mode: default_tokenizer_mode(),
        }
    }
}

fn default_tokenizer_mode() -> String {
    "accurate".to_string()
}

// ---------------------------------------------------------------------------
// Resilience configuration
// ---------------------------------------------------------------------------

/// Resilience configuration (circuit breakers, degradation ladder).
///
/// Controls circuit breaker thresholds, fallback chain, de-escalation
/// hysteresis, drain timeout, and notification deduplication.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResilienceConfig {
    /// Whether the resilience subsystem is enabled.
    #[serde(default = "default_resilience_enabled")]
    pub enabled: bool,

    /// Ordered fallback provider chain (max 2 entries).
    #[serde(default)]
    pub fallback_chain: Vec<String>,

    /// Seconds of sustained recovery before de-escalating one level.
    #[serde(default = "default_hysteresis_secs")]
    pub hysteresis_secs: u64,

    /// Seconds to drain in-flight operations during L5 shutdown.
    #[serde(default = "default_drain_timeout_secs")]
    pub drain_timeout_secs: u64,

    /// Seconds between duplicate notifications for the same level.
    #[serde(default = "default_notification_dedup_secs")]
    pub notification_dedup_secs: u64,

    /// Default circuit breaker thresholds (used when no per-dep override).
    #[serde(default)]
    pub defaults: CircuitBreakerDefaults,

    /// Per-dependency circuit breaker overrides.
    #[serde(default)]
    pub circuit_breakers: HashMap<String, CircuitBreakerOverride>,
}

impl Default for ResilienceConfig {
    fn default() -> Self {
        Self {
            enabled: default_resilience_enabled(),
            fallback_chain: Vec::new(),
            hysteresis_secs: default_hysteresis_secs(),
            drain_timeout_secs: default_drain_timeout_secs(),
            notification_dedup_secs: default_notification_dedup_secs(),
            defaults: CircuitBreakerDefaults::default(),
            circuit_breakers: HashMap::new(),
        }
    }
}

impl ResilienceConfig {
    /// Validate the resilience configuration.
    ///
    /// Returns a list of validation errors (empty if valid).
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.fallback_chain.len() > 2 {
            errors.push(format!(
                "fallback_chain has {} entries (max 2)",
                self.fallback_chain.len()
            ));
        }
        errors
    }

    /// Validate fallback chain against known provider names.
    ///
    /// Returns a list of validation errors for unknown providers.
    pub fn validate_providers(&self, known_providers: &[&str]) -> Vec<String> {
        let mut errors = self.validate();
        for name in &self.fallback_chain {
            if !known_providers.contains(&name.as_str()) {
                errors.push(format!(
                    "fallback_chain references unknown provider: {name}"
                ));
            }
        }
        errors
    }
}

fn default_resilience_enabled() -> bool {
    true
}

fn default_hysteresis_secs() -> u64 {
    120
}

fn default_drain_timeout_secs() -> u64 {
    30
}

fn default_notification_dedup_secs() -> u64 {
    60
}

/// Default circuit breaker thresholds applied to all dependencies.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CircuitBreakerDefaults {
    /// Consecutive failures before opening.
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,

    /// Seconds the breaker stays Open before probing HalfOpen.
    #[serde(default = "default_reset_timeout_secs")]
    pub reset_timeout_secs: u64,

    /// Number of consecutive successful probes to close the breaker.
    #[serde(default = "default_half_open_probes")]
    pub half_open_probes: u32,
}

impl Default for CircuitBreakerDefaults {
    fn default() -> Self {
        Self {
            failure_threshold: default_failure_threshold(),
            reset_timeout_secs: default_reset_timeout_secs(),
            half_open_probes: default_half_open_probes(),
        }
    }
}

fn default_failure_threshold() -> u32 {
    5
}

fn default_reset_timeout_secs() -> u64 {
    60
}

fn default_half_open_probes() -> u32 {
    3
}

/// Per-dependency circuit breaker override.
///
/// All fields are optional; `None` means use the global defaults.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CircuitBreakerOverride {
    /// Override for failure threshold.
    pub failure_threshold: Option<u32>,
    /// Override for reset timeout seconds.
    pub reset_timeout_secs: Option<u64>,
    /// Override for half-open probe count.
    pub half_open_probes: Option<u32>,
}

// ---------------------------------------------------------------------------
// Data Classification Config
// ---------------------------------------------------------------------------

/// Configuration for the data classification subsystem.
///
/// Controls automatic PII-based classification, default levels, and warnings.
///
/// ```toml
/// [classification]
/// enabled = true
/// auto_classify_pii = true
/// default_level = "internal"
/// warn_unencrypted = true
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClassificationConfig {
    /// Whether the classification subsystem is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Automatically classify content as Confidential when PII is detected.
    #[serde(default = "default_true")]
    pub auto_classify_pii: bool,

    /// Default classification level for new data.
    #[serde(default)]
    pub default_level: DataClassification,

    /// Warn when non-SQLCipher (plaintext) database stores Confidential data.
    #[serde(default = "default_true")]
    pub warn_unencrypted: bool,
}

impl Default for ClassificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_classify_pii: true,
            default_level: DataClassification::default(),
            warn_unencrypted: true,
        }
    }
}

/// Audit trail configuration.
///
/// Controls the tamper-evident hash-chain audit log stored in a dedicated `audit.db`.
/// When omitted from the config file, all defaults apply (enabled with all events audited).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuditConfig {
    /// Whether the audit trail is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Custom path for the audit database file.
    /// When `None`, defaults to `{data_dir}/audit.db`.
    #[serde(default)]
    pub db_path: Option<String>,

    /// Event types to audit. Supports "all", prefix matching ("session.*"),
    /// and exact matching ("session.created").
    #[serde(default = "default_audit_events")]
    pub events: Vec<String>,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            db_path: None,
            events: default_audit_events(),
        }
    }
}

fn default_audit_events() -> Vec<String> {
    vec!["all".to_string()]
}

// ---------------------------------------------------------------------------
// Injection defense config
// ---------------------------------------------------------------------------

/// Injection defense configuration.
///
/// Controls the 5-layer prompt injection defense system: L1 pattern classifier,
/// L3 HMAC boundary tokens, L4 output screening, and L5 human-in-the-loop.
///
/// Env var overrides: `BLUFIO_INJECTION_ENABLED`, `BLUFIO_INJECTION_DRY_RUN`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InjectionDefenseConfig {
    /// Whether the injection defense system is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Global dry-run mode: simulates all layers without taking action.
    #[serde(default)]
    pub dry_run: bool,

    /// L1 input detection configuration.
    #[serde(default)]
    pub input_detection: InputDetectionConfig,

    /// L3 HMAC boundary token configuration.
    #[serde(default)]
    pub hmac_boundaries: HmacBoundaryConfig,

    /// L4 output screening configuration.
    #[serde(default)]
    pub output_screening: OutputScreeningConfig,

    /// L5 human-in-the-loop configuration.
    #[serde(default)]
    pub hitl: HitlConfig,
}

impl Default for InjectionDefenseConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            dry_run: false,
            input_detection: InputDetectionConfig::default(),
            hmac_boundaries: HmacBoundaryConfig::default(),
            output_screening: OutputScreeningConfig::default(),
            hitl: HitlConfig::default(),
        }
    }
}

/// L1 input detection configuration.
///
/// Controls the regex-based pattern classifier that scans all user and
/// external input for injection signatures.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InputDetectionConfig {
    /// Detection mode: `"log"` (default) logs detections without blocking,
    /// `"block"` blocks all detections above threshold.
    #[serde(default = "default_detection_mode")]
    pub mode: String,

    /// Score threshold above which user input is blocked (default: 0.95).
    #[serde(default = "default_blocking_threshold")]
    pub blocking_threshold: f64,

    /// Score threshold above which MCP/WASM input is blocked (default: 0.98).
    #[serde(default = "default_mcp_blocking_threshold")]
    pub mcp_blocking_threshold: f64,

    /// Additional regex patterns to detect (operator-configured).
    /// Invalid patterns are logged and skipped at startup.
    #[serde(default)]
    pub custom_patterns: Vec<String>,
}

impl Default for InputDetectionConfig {
    fn default() -> Self {
        Self {
            mode: default_detection_mode(),
            blocking_threshold: default_blocking_threshold(),
            mcp_blocking_threshold: default_mcp_blocking_threshold(),
            custom_patterns: Vec::new(),
        }
    }
}

fn default_detection_mode() -> String {
    "log".to_string()
}

fn default_blocking_threshold() -> f64 {
    0.95
}

fn default_mcp_blocking_threshold() -> f64 {
    0.98
}

/// L3 HMAC boundary token configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HmacBoundaryConfig {
    /// Whether HMAC boundary tokens are enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for HmacBoundaryConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// L4 output screening configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OutputScreeningConfig {
    /// Whether output screening is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Number of screening failures in a session before escalating to HITL.
    #[serde(default = "default_escalation_threshold")]
    pub escalation_threshold: u32,
}

impl Default for OutputScreeningConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            escalation_threshold: default_escalation_threshold(),
        }
    }
}

fn default_escalation_threshold() -> u32 {
    3
}

/// L5 human-in-the-loop configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HitlConfig {
    /// Whether HITL confirmation is enabled (default: false).
    #[serde(default)]
    pub enabled: bool,

    /// Timeout in seconds before auto-denying a pending confirmation.
    #[serde(default = "default_hitl_timeout")]
    pub timeout_secs: u64,

    /// Maximum number of pending confirmations before pausing.
    #[serde(default = "default_max_pending")]
    pub max_pending: u32,

    /// Tools that are always auto-approved (no confirmation needed).
    #[serde(default = "default_safe_tools")]
    pub safe_tools: Vec<String>,
}

impl Default for HitlConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: default_hitl_timeout(),
            max_pending: default_max_pending(),
            safe_tools: default_safe_tools(),
        }
    }
}

fn default_hitl_timeout() -> u64 {
    60
}

fn default_max_pending() -> u32 {
    3
}

fn default_safe_tools() -> Vec<String> {
    vec![
        "memory_search".to_string(),
        "session_history".to_string(),
        "cost_lookup".to_string(),
        "skill_list".to_string(),
    ]
}

// ---------------------------------------------------------------------------
// Cron scheduler config
// ---------------------------------------------------------------------------

/// Cron scheduler configuration.
///
/// Controls the in-process cron scheduler that runs inside `blufio serve`.
///
/// ```toml
/// [cron]
/// enabled = true
/// job_timeout_secs = 300
///
/// [[cron.jobs]]
/// name = "nightly-backup"
/// schedule = "0 2 * * *"
/// task = "backup"
/// enabled = true
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CronConfig {
    /// Whether the cron scheduler is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// Job timeout in seconds (default: 300 = 5 minutes).
    #[serde(default = "default_job_timeout_secs")]
    pub job_timeout_secs: u64,

    /// Maximum history entries to keep per job (default: 1000).
    #[serde(default = "default_max_history")]
    pub max_history: usize,

    /// Configured cron jobs.
    #[serde(default)]
    pub jobs: Vec<CronJobConfig>,
}

impl Default for CronConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            job_timeout_secs: default_job_timeout_secs(),
            max_history: default_max_history(),
            jobs: Vec::new(),
        }
    }
}

fn default_job_timeout_secs() -> u64 {
    300
}

fn default_max_history() -> usize {
    1000
}

/// A single cron job definition.
///
/// ```toml
/// [[cron.jobs]]
/// name = "retention-sweep"
/// schedule = "0 3 * * *"
/// task = "retention_enforcement"
/// enabled = true
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CronJobConfig {
    /// Unique job name.
    pub name: String,
    /// Cron expression (5-field POSIX format).
    pub schedule: String,
    /// Task to execute (must match a registered CronTask name).
    pub task: String,
    /// Whether this job is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// Retention policy config
// ---------------------------------------------------------------------------

/// Retention policy configuration.
///
/// Controls per-type data retention with soft-delete and grace-period
/// permanent deletion.
///
/// ```toml
/// [retention]
/// enabled = true
/// grace_period_days = 7
///
/// [retention.periods]
/// messages = 90
/// sessions = 90
/// cost_records = 365
/// memories = 180
///
/// [retention.restricted]
/// messages = 30
/// sessions = 30
/// cost_records = 90
/// memories = 60
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionConfig {
    /// Whether retention enforcement is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// Grace period in days after soft-delete before permanent removal.
    #[serde(default = "default_grace_period_days")]
    pub grace_period_days: u64,

    /// Retention periods in days per data type (default classification).
    #[serde(default)]
    pub periods: RetentionPeriods,

    /// Separate retention periods for Restricted-classified data.
    #[serde(default)]
    pub restricted: RetentionPeriods,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            grace_period_days: default_grace_period_days(),
            periods: RetentionPeriods::default(),
            restricted: RetentionPeriods::default(),
        }
    }
}

fn default_grace_period_days() -> u64 {
    7
}

/// Per-type retention periods in days.
///
/// Each field specifies the number of days before records of that type
/// are soft-deleted. `None` means no retention (records kept indefinitely).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionPeriods {
    /// Days before messages are soft-deleted. None = no retention.
    #[serde(default)]
    pub messages: Option<u64>,
    /// Days before sessions are soft-deleted. None = no retention.
    #[serde(default)]
    pub sessions: Option<u64>,
    /// Days before cost records are soft-deleted. None = no retention.
    #[serde(default)]
    pub cost_records: Option<u64>,
    /// Days before memories are soft-deleted. None = no retention.
    #[serde(default)]
    pub memories: Option<u64>,
}

// ---------------------------------------------------------------------------
// Hook system config
// ---------------------------------------------------------------------------

/// Hook system configuration.
///
/// Shell-based lifecycle hooks that execute commands in response to bus events.
///
/// ```toml
/// [hooks]
/// enabled = true
/// max_recursion_depth = 3
/// default_timeout_secs = 30
/// allowed_path = "/usr/bin:/usr/local/bin"
///
/// [[hooks.definitions]]
/// name = "on-session-start"
/// event = "session.created"
/// command = "/usr/local/bin/notify.sh"
/// priority = 10
/// timeout_secs = 5
/// enabled = true
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HookConfig {
    /// Whether the hook system is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// Maximum recursion depth for hook-triggered-hook chains.
    #[serde(default = "default_max_recursion_depth")]
    pub max_recursion_depth: u32,

    /// Default timeout in seconds for hook execution.
    #[serde(default = "default_hook_timeout_secs")]
    pub default_timeout_secs: u64,

    /// Restricted PATH for hook command execution.
    #[serde(default = "default_allowed_path")]
    pub allowed_path: String,

    /// Hook definitions.
    #[serde(default)]
    pub definitions: Vec<HookDefinition>,
}

impl Default for HookConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_recursion_depth: default_max_recursion_depth(),
            default_timeout_secs: default_hook_timeout_secs(),
            allowed_path: default_allowed_path(),
            definitions: Vec::new(),
        }
    }
}

fn default_max_recursion_depth() -> u32 {
    3
}

fn default_hook_timeout_secs() -> u64 {
    30
}

fn default_allowed_path() -> String {
    "/usr/bin:/usr/local/bin".to_string()
}

/// A single hook definition.
///
/// ```toml
/// [[hooks.definitions]]
/// name = "on-session-start"
/// event = "session.created"
/// command = "/usr/local/bin/notify.sh"
/// priority = 10
/// timeout_secs = 5
/// enabled = true
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HookDefinition {
    /// Unique hook name.
    pub name: String,
    /// Event type to trigger on (matches event_type_string format, e.g., "session.created").
    pub event: String,
    /// Shell command to execute.
    pub command: String,
    /// Execution priority (lower = higher priority, default: 100).
    #[serde(default = "default_hook_priority")]
    pub priority: u32,
    /// Timeout in seconds for this hook (default: 30).
    #[serde(default = "default_hook_timeout_secs")]
    pub timeout_secs: u64,
    /// Whether this hook is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_hook_priority() -> u32 {
    100
}

// ---------------------------------------------------------------------------
// Hot reload config
// ---------------------------------------------------------------------------

/// Hot reload configuration.
///
/// Controls automatic configuration reloading via file system watching.
///
/// ```toml
/// [hot_reload]
/// enabled = true
/// debounce_ms = 500
/// watch_skills = true
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HotReloadConfig {
    /// Whether hot reload is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// Debounce duration in milliseconds for file system events.
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,

    /// Path to TLS certificate file to watch for reload.
    #[serde(default)]
    pub tls_cert_path: Option<String>,

    /// Path to TLS key file to watch for reload.
    #[serde(default)]
    pub tls_key_path: Option<String>,

    /// Whether to watch skill files for hot reload.
    #[serde(default = "default_true")]
    pub watch_skills: bool,
}

impl Default for HotReloadConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            debounce_ms: default_debounce_ms(),
            tls_cert_path: None,
            tls_key_path: None,
            watch_skills: true,
        }
    }
}

fn default_debounce_ms() -> u64 {
    500
}

// ---------------------------------------------------------------------------
// GDPR configuration
// ---------------------------------------------------------------------------

/// GDPR tooling configuration.
///
/// Controls export directory, auto-export-before-erasure behavior, and default
/// export format. All fields are optional with sensible defaults. Validated on
/// first use (when running GDPR commands), not at server startup.
///
/// # Example TOML
///
/// ```toml
/// [gdpr]
/// export_dir = "/var/lib/blufio/gdpr-exports"
/// export_before_erasure = true
/// default_format = "json"
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GdprConfig {
    /// Custom export directory. When `None`, defaults to `{data_dir}/exports/`.
    #[serde(default)]
    pub export_dir: Option<String>,

    /// Whether to automatically export user data before erasure.
    ///
    /// When `true` (the default), `blufio gdpr erase` will export the user's
    /// data before performing deletion, unless `--skip-export` is passed.
    #[serde(default = "default_export_before_erasure")]
    pub export_before_erasure: bool,

    /// Default export format (`"json"` or `"csv"`).
    #[serde(default = "default_gdpr_format")]
    pub default_format: String,
}

impl Default for GdprConfig {
    fn default() -> Self {
        Self {
            export_dir: None,
            export_before_erasure: default_export_before_erasure(),
            default_format: default_gdpr_format(),
        }
    }
}

fn default_export_before_erasure() -> bool {
    true
}

fn default_gdpr_format() -> String {
    "json".to_string()
}

#[cfg(test)]
mod providers_config_tests {
    use super::*;

    #[test]
    fn test_custom_provider_config_parses() {
        let toml_str = r#"
[providers.custom.together]
base_url = "https://api.together.xyz/v1"
wire_protocol = "openai-compat"
api_key_env = "TOGETHER_API_KEY"
default_model = "meta-llama/Llama-3-70b-chat-hf"
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.providers.custom.len(), 1);
        let together = &config.providers.custom["together"];
        assert_eq!(together.base_url, "https://api.together.xyz/v1");
        assert_eq!(together.wire_protocol, "openai-compat");
        assert_eq!(together.api_key_env, "TOGETHER_API_KEY");
        assert_eq!(
            together.default_model.as_deref(),
            Some("meta-llama/Llama-3-70b-chat-hf")
        );
    }

    #[test]
    fn test_custom_provider_empty_is_valid() {
        let toml_str = r#"
[providers]
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert!(config.providers.custom.is_empty());
    }

    #[test]
    fn test_custom_provider_rejects_unknown_fields() {
        let toml_str = r#"
[providers.custom.bad]
base_url = "https://api.example.com"
wire_protocol = "openai-compat"
api_key_env = "KEY"
unknown_field = "bad"
"#;
        let result: Result<BlufioConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_custom_provider_multiple_providers() {
        let toml_str = r#"
[providers.custom.together]
base_url = "https://api.together.xyz/v1"
wire_protocol = "openai-compat"
api_key_env = "TOGETHER_API_KEY"

[providers.custom.groq]
base_url = "https://api.groq.com/openai/v1"
wire_protocol = "openai-compat"
api_key_env = "GROQ_API_KEY"
default_model = "llama3-70b-8192"
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.providers.custom.len(), 2);
        assert!(config.providers.custom.contains_key("together"));
        assert!(config.providers.custom.contains_key("groq"));
        assert_eq!(
            config.providers.custom["groq"].default_model.as_deref(),
            Some("llama3-70b-8192")
        );
    }

    #[test]
    fn test_default_config_has_empty_providers() {
        let config = BlufioConfig::default();
        assert!(config.providers.custom.is_empty());
    }

    #[test]
    fn test_empty_toml_defaults_provider_to_anthropic() {
        let config: BlufioConfig = toml::from_str("").unwrap();
        assert_eq!(config.providers.default, "anthropic");
    }

    #[test]
    fn test_providers_default_field_parses() {
        let toml_str = r#"
[providers]
default = "openai"
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.providers.default, "openai");
    }

    #[test]
    fn test_openai_config_parses() {
        let toml_str = r#"
[providers.openai]
api_key = "sk-test"
default_model = "gpt-4o"
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.providers.openai.api_key.as_deref(), Some("sk-test"));
        assert_eq!(config.providers.openai.default_model, "gpt-4o");
    }

    #[test]
    fn test_openai_config_custom_base_url() {
        let toml_str = r#"
[providers.openai]
base_url = "https://custom.azure.com"
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.providers.openai.base_url, "https://custom.azure.com");
    }

    #[test]
    fn test_openai_config_defaults() {
        let config: BlufioConfig = toml::from_str("").unwrap();
        assert!(config.providers.openai.api_key.is_none());
        assert_eq!(config.providers.openai.default_model, "gpt-4o");
        assert_eq!(
            config.providers.openai.base_url,
            "https://api.openai.com/v1"
        );
        assert_eq!(config.providers.openai.max_tokens, 4096);
    }

    #[test]
    fn test_ollama_config_parses() {
        let toml_str = r#"
[providers.ollama]
base_url = "http://localhost:11434"
default_model = "llama3.2"
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.providers.ollama.base_url, "http://localhost:11434");
        assert_eq!(
            config.providers.ollama.default_model.as_deref(),
            Some("llama3.2")
        );
    }

    #[test]
    fn test_ollama_config_defaults() {
        let config: BlufioConfig = toml::from_str("").unwrap();
        assert_eq!(config.providers.ollama.base_url, "http://localhost:11434");
        assert!(config.providers.ollama.default_model.is_none());
    }

    #[test]
    fn test_openrouter_config_parses() {
        let toml_str = r#"
[providers.openrouter]
api_key = "or-test"
default_model = "anthropic/claude-sonnet-4"
x_title = "MyApp"
http_referer = "https://myapp.com"
provider_order = ["Anthropic", "Google"]
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.providers.openrouter.api_key.as_deref(),
            Some("or-test")
        );
        assert_eq!(
            config.providers.openrouter.default_model,
            "anthropic/claude-sonnet-4"
        );
        assert_eq!(config.providers.openrouter.x_title, "MyApp");
        assert_eq!(
            config.providers.openrouter.http_referer.as_deref(),
            Some("https://myapp.com")
        );
        assert_eq!(
            config.providers.openrouter.provider_order,
            vec!["Anthropic", "Google"]
        );
    }

    #[test]
    fn test_openrouter_config_defaults() {
        let config: BlufioConfig = toml::from_str("").unwrap();
        assert!(config.providers.openrouter.api_key.is_none());
        assert_eq!(
            config.providers.openrouter.default_model,
            "anthropic/claude-sonnet-4"
        );
        assert_eq!(config.providers.openrouter.x_title, "Blufio");
        assert!(config.providers.openrouter.http_referer.is_none());
        assert!(config.providers.openrouter.provider_order.is_empty());
    }

    #[test]
    fn test_gemini_config_parses() {
        let toml_str = r#"
[providers.gemini]
api_key = "gem-test"
default_model = "gemini-2.0-flash"
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.providers.gemini.api_key.as_deref(), Some("gem-test"));
        assert_eq!(config.providers.gemini.default_model, "gemini-2.0-flash");
    }

    #[test]
    fn test_gemini_config_defaults() {
        let config: BlufioConfig = toml::from_str("").unwrap();
        assert!(config.providers.gemini.api_key.is_none());
        assert_eq!(config.providers.gemini.default_model, "gemini-2.0-flash");
    }

    #[test]
    fn test_openai_config_rejects_unknown_fields() {
        let toml_str = r#"
[providers.openai]
api_key = "sk-test"
unknown_field = "bad"
"#;
        let result: Result<BlufioConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_ollama_config_rejects_unknown_fields() {
        let toml_str = r#"
[providers.ollama]
base_url = "http://localhost:11434"
unknown_field = "bad"
"#;
        let result: Result<BlufioConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_openrouter_config_rejects_unknown_fields() {
        let toml_str = r#"
[providers.openrouter]
api_key = "test"
unknown_field = "bad"
"#;
        let result: Result<BlufioConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_gemini_config_rejects_unknown_fields() {
        let toml_str = r#"
[providers.gemini]
api_key = "test"
unknown_field = "bad"
"#;
        let result: Result<BlufioConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod mcp_config_tests {
    use super::*;

    #[test]
    fn empty_toml_defaults_mcp_disabled() {
        let config: BlufioConfig = toml::from_str("").unwrap();
        assert!(!config.mcp.enabled);
        assert!(config.mcp.servers.is_empty());
        assert!(config.mcp.export_tools.is_empty());
    }

    #[test]
    fn mcp_section_parses_correctly() {
        let toml_str = r#"
[mcp]
enabled = true
export_tools = ["http", "file"]

[[mcp.servers]]
name = "github"
transport = "http"
url = "https://mcp.github.com"
auth_token = "ghp_xxx"

[[mcp.servers]]
name = "local"
transport = "stdio"
command = "npx"
args = ["-y", "mcp-server"]
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert!(config.mcp.enabled);
        assert_eq!(config.mcp.servers.len(), 2);
        assert_eq!(config.mcp.servers[0].name, "github");
        assert_eq!(config.mcp.servers[0].transport, "http");
        assert_eq!(
            config.mcp.servers[0].url.as_deref(),
            Some("https://mcp.github.com")
        );
        // Verify new field defaults
        assert_eq!(config.mcp.servers[0].connect_timeout_secs, 10);
        assert_eq!(config.mcp.servers[0].response_size_cap, 4096);
        assert_eq!(config.mcp.servers[1].name, "local");
        assert_eq!(config.mcp.servers[1].transport, "stdio");
        assert_eq!(config.mcp.servers[1].command.as_deref(), Some("npx"));
        assert_eq!(config.mcp.servers[1].args, vec!["-y", "mcp-server"]);
        assert_eq!(config.mcp.export_tools, vec!["http", "file"]);
    }

    #[test]
    fn mcp_server_entry_custom_timeout_and_cap() {
        let toml_str = r#"
[[mcp.servers]]
name = "custom"
transport = "http"
url = "https://example.com/mcp"
connect_timeout_secs = 30
response_size_cap = 8192
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.mcp.servers[0].connect_timeout_secs, 30);
        assert_eq!(config.mcp.servers[0].response_size_cap, 8192);
    }

    #[test]
    fn mcp_server_entry_sse_transport_parses() {
        let toml_str = r#"
[[mcp.servers]]
name = "legacy"
transport = "sse"
url = "https://sse-server.example.com/sse"
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.mcp.servers[0].transport, "sse");
        assert_eq!(
            config.mcp.servers[0].url.as_deref(),
            Some("https://sse-server.example.com/sse")
        );
    }

    #[test]
    fn mcp_section_rejects_unknown_fields() {
        let toml_str = r#"
[mcp]
enabled = true
unknown_field = "bad"
"#;
        let result: Result<BlufioConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn mcp_server_entry_rejects_unknown_fields() {
        let toml_str = r#"
[[mcp.servers]]
name = "test"
transport = "http"
unknown_field = "bad"
"#;
        let result: Result<BlufioConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn mcp_auth_token_parses_correctly() {
        let toml_str = r#"
[mcp]
enabled = true
auth_token = "mcp-secret"
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.mcp.auth_token.as_deref(), Some("mcp-secret"));
    }

    #[test]
    fn mcp_cors_origins_parses_correctly() {
        let toml_str = r#"
[mcp]
enabled = true
cors_origins = ["https://app.example.com", "https://other.example.com"]
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.mcp.cors_origins.len(), 2);
        assert_eq!(config.mcp.cors_origins[0], "https://app.example.com");
        assert_eq!(config.mcp.cors_origins[1], "https://other.example.com");
    }

    #[test]
    fn mcp_defaults_auth_token_none_cors_origins_empty() {
        let config: BlufioConfig = toml::from_str("").unwrap();
        assert!(config.mcp.auth_token.is_none());
        assert!(config.mcp.cors_origins.is_empty());
    }

    #[test]
    fn mcp_health_check_interval_defaults_to_60() {
        let config: BlufioConfig = toml::from_str("").unwrap();
        assert_eq!(config.mcp.health_check_interval_secs, 60);
    }

    #[test]
    fn mcp_health_check_interval_custom_value() {
        let toml_str = r#"
[mcp]
health_check_interval_secs = 30
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.mcp.health_check_interval_secs, 30);
    }

    #[test]
    fn mcp_server_trusted_defaults_to_false() {
        let toml_str = r#"
[[mcp.servers]]
name = "untrusted-server"
transport = "http"
url = "https://example.com/mcp"
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert!(
            !config.mcp.servers[0].trusted,
            "trusted should default to false"
        );
    }

    #[test]
    fn mcp_server_trusted_can_be_set_true() {
        let toml_str = r#"
[[mcp.servers]]
name = "trusted-server"
transport = "http"
url = "https://internal.example.com/mcp"
trusted = true
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert!(
            config.mcp.servers[0].trusted,
            "trusted should be true when set"
        );
    }
}

#[cfg(test)]
mod performance_config_tests {
    use super::*;

    #[test]
    fn performance_config_defaults_tokenizer_mode_accurate() {
        let config = PerformanceConfig::default();
        assert_eq!(config.tokenizer_mode, "accurate");
    }

    #[test]
    fn performance_config_deserializes_fast_mode() {
        let toml_str = r#"
[performance]
tokenizer_mode = "fast"
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.performance.tokenizer_mode, "fast");
    }

    #[test]
    fn blufio_config_with_performance_section_parses() {
        let toml_str = r#"
[performance]
tokenizer_mode = "accurate"
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.performance.tokenizer_mode, "accurate");
    }

    #[test]
    fn blufio_config_without_performance_section_uses_defaults() {
        let config: BlufioConfig = toml::from_str("").unwrap();
        assert_eq!(config.performance.tokenizer_mode, "accurate");
    }
}

#[cfg(test)]
mod resilience_config_tests {
    use super::*;

    #[test]
    fn resilience_defaults() {
        let config = ResilienceConfig::default();
        assert!(config.enabled);
        assert!(config.fallback_chain.is_empty());
        assert_eq!(config.hysteresis_secs, 120);
        assert_eq!(config.drain_timeout_secs, 30);
        assert_eq!(config.notification_dedup_secs, 60);
        assert_eq!(config.defaults.failure_threshold, 5);
        assert_eq!(config.defaults.reset_timeout_secs, 60);
        assert_eq!(config.defaults.half_open_probes, 3);
        assert!(config.circuit_breakers.is_empty());
    }

    #[test]
    fn resilience_parses_from_empty_section() {
        let toml_str = r#"
[resilience]
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert!(config.resilience.enabled);
        assert_eq!(config.resilience.defaults.failure_threshold, 5);
    }

    #[test]
    fn resilience_parses_without_section() {
        let config: BlufioConfig = toml::from_str("").unwrap();
        assert!(config.resilience.enabled);
    }

    #[test]
    fn resilience_parses_with_overrides() {
        let toml_str = r#"
[resilience]
enabled = true
fallback_chain = ["openai", "ollama"]
hysteresis_secs = 60
drain_timeout_secs = 15

[resilience.defaults]
failure_threshold = 10
reset_timeout_secs = 120
half_open_probes = 5

[resilience.circuit_breakers.anthropic]
failure_threshold = 3
reset_timeout_secs = 30

[resilience.circuit_breakers.telegram]
half_open_probes = 1
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert!(config.resilience.enabled);
        assert_eq!(config.resilience.fallback_chain, vec!["openai", "ollama"]);
        assert_eq!(config.resilience.hysteresis_secs, 60);
        assert_eq!(config.resilience.drain_timeout_secs, 15);
        assert_eq!(config.resilience.defaults.failure_threshold, 10);
        assert_eq!(config.resilience.defaults.reset_timeout_secs, 120);
        assert_eq!(config.resilience.defaults.half_open_probes, 5);

        let anthropic = &config.resilience.circuit_breakers["anthropic"];
        assert_eq!(anthropic.failure_threshold, Some(3));
        assert_eq!(anthropic.reset_timeout_secs, Some(30));
        assert!(anthropic.half_open_probes.is_none());

        let telegram = &config.resilience.circuit_breakers["telegram"];
        assert!(telegram.failure_threshold.is_none());
        assert!(telegram.reset_timeout_secs.is_none());
        assert_eq!(telegram.half_open_probes, Some(1));
    }

    #[test]
    fn resilience_validate_fallback_chain_max_two() {
        let config = ResilienceConfig {
            fallback_chain: vec!["a".into(), "b".into(), "c".into()],
            ..Default::default()
        };
        let errors = config.validate();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("max 2"));
    }

    #[test]
    fn resilience_validate_fallback_chain_two_ok() {
        let config = ResilienceConfig {
            fallback_chain: vec!["openai".into(), "ollama".into()],
            ..Default::default()
        };
        let errors = config.validate();
        assert!(errors.is_empty());
    }

    #[test]
    fn resilience_validate_providers_unknown() {
        let config = ResilienceConfig {
            fallback_chain: vec!["openai".into(), "unknown_provider".into()],
            ..Default::default()
        };
        let errors = config.validate_providers(&["anthropic", "openai", "ollama"]);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("unknown_provider"));
    }

    #[test]
    fn resilience_validate_providers_all_known() {
        let config = ResilienceConfig {
            fallback_chain: vec!["openai".into(), "ollama".into()],
            ..Default::default()
        };
        let errors = config.validate_providers(&["anthropic", "openai", "ollama"]);
        assert!(errors.is_empty());
    }

    #[test]
    fn resilience_disabled() {
        let toml_str = r#"
[resilience]
enabled = false
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert!(!config.resilience.enabled);
    }
}

#[cfg(test)]
mod memory_config_tests {
    use super::*;

    #[test]
    fn memory_config_default_decay_factor() {
        let config = MemoryConfig::default();
        assert!((config.decay_factor - 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn memory_config_default_decay_floor() {
        let config = MemoryConfig::default();
        assert!((config.decay_floor - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn memory_config_default_mmr_lambda() {
        let config = MemoryConfig::default();
        assert!((config.mmr_lambda - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn memory_config_default_importance_boost_explicit() {
        let config = MemoryConfig::default();
        assert!((config.importance_boost_explicit - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn memory_config_default_importance_boost_extracted() {
        let config = MemoryConfig::default();
        assert!((config.importance_boost_extracted - 0.6).abs() < f64::EPSILON);
    }

    #[test]
    fn memory_config_default_importance_boost_file() {
        let config = MemoryConfig::default();
        assert!((config.importance_boost_file - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn memory_config_default_max_entries() {
        let config = MemoryConfig::default();
        assert_eq!(config.max_entries, 10_000);
    }

    #[test]
    fn memory_config_default_eviction_sweep_interval_secs() {
        let config = MemoryConfig::default();
        assert_eq!(config.eviction_sweep_interval_secs, 300);
    }

    #[test]
    fn memory_config_default_stale_threshold_days() {
        let config = MemoryConfig::default();
        assert_eq!(config.stale_threshold_days, 180);
    }

    #[test]
    fn memory_config_file_watcher_defaults() {
        let config = MemoryConfig::default();
        assert!(config.file_watcher.paths.is_empty());
        assert!(config.file_watcher.extensions.is_empty());
        assert_eq!(config.file_watcher.max_file_size, 102_400);
    }

    #[test]
    fn memory_config_deny_unknown_fields() {
        let toml_str = r#"
[memory]
unknown_field = "bad"
"#;
        let result: Result<BlufioConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn memory_config_file_watcher_deny_unknown_fields() {
        let toml_str = r#"
[memory.file_watcher]
unknown_field = "bad"
"#;
        let result: Result<BlufioConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn memory_config_parses_scoring_fields() {
        let toml_str = r#"
[memory]
decay_factor = 0.9
decay_floor = 0.05
mmr_lambda = 0.5
importance_boost_explicit = 0.8
importance_boost_extracted = 0.4
importance_boost_file = 0.6
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert!((config.memory.decay_factor - 0.9).abs() < f64::EPSILON);
        assert!((config.memory.decay_floor - 0.05).abs() < f64::EPSILON);
        assert!((config.memory.mmr_lambda - 0.5).abs() < f64::EPSILON);
        assert!((config.memory.importance_boost_explicit - 0.8).abs() < f64::EPSILON);
        assert!((config.memory.importance_boost_extracted - 0.4).abs() < f64::EPSILON);
        assert!((config.memory.importance_boost_file - 0.6).abs() < f64::EPSILON);
    }

    #[test]
    fn memory_config_parses_eviction_fields() {
        let toml_str = r#"
[memory]
max_entries = 5000
eviction_sweep_interval_secs = 600
stale_threshold_days = 90
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.memory.max_entries, 5000);
        assert_eq!(config.memory.eviction_sweep_interval_secs, 600);
        assert_eq!(config.memory.stale_threshold_days, 90);
    }

    #[test]
    fn memory_config_parses_file_watcher() {
        let toml_str = r#"
[memory.file_watcher]
paths = ["./docs", "./notes"]
extensions = ["md", "txt"]
max_file_size = 51200
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.memory.file_watcher.paths, vec!["./docs", "./notes"]);
        assert_eq!(config.memory.file_watcher.extensions, vec!["md", "txt"]);
        assert_eq!(config.memory.file_watcher.max_file_size, 51200);
    }

    #[test]
    fn memory_config_empty_toml_uses_all_defaults() {
        let config: BlufioConfig = toml::from_str("").unwrap();
        assert!(config.memory.enabled);
        assert!((config.memory.decay_factor - 0.95).abs() < f64::EPSILON);
        assert!((config.memory.decay_floor - 0.1).abs() < f64::EPSILON);
        assert!((config.memory.mmr_lambda - 0.7).abs() < f64::EPSILON);
        assert!((config.memory.importance_boost_explicit - 1.0).abs() < f64::EPSILON);
        assert!((config.memory.importance_boost_extracted - 0.6).abs() < f64::EPSILON);
        assert!((config.memory.importance_boost_file - 0.8).abs() < f64::EPSILON);
        assert_eq!(config.memory.max_entries, 10_000);
        assert_eq!(config.memory.eviction_sweep_interval_secs, 300);
        assert_eq!(config.memory.stale_threshold_days, 180);
        assert!(config.memory.file_watcher.paths.is_empty());
    }
}
