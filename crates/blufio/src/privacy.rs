// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio privacy evidence-report` command implementation.
//!
//! Performs static analysis of the Blufio configuration to enumerate
//! outbound endpoints, local data stores, WASM skill permissions,
//! and data classification. No server connection is needed.

use std::fmt;

use blufio_config::model::BlufioConfig;
use blufio_core::BlufioError;
use serde::Serialize;

/// Classification of data types handled by the system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum DataType {
    /// Personally identifiable information (usernames, phone numbers).
    Pii,
    /// API keys, tokens, passwords.
    Credentials,
    /// Usage data (cost tracking, session history, message content).
    Usage,
    /// System data (config, metrics, logs).
    System,
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataType::Pii => write!(f, "PII"),
            DataType::Credentials => write!(f, "Credentials"),
            DataType::Usage => write!(f, "Usage"),
            DataType::System => write!(f, "System"),
        }
    }
}

/// Information about an outbound network endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct EndpointInfo {
    /// Human-readable name of the service.
    pub name: String,
    /// URL or host:port of the endpoint.
    pub url: String,
    /// Types of data sent to this endpoint.
    pub data_types: Vec<DataType>,
    /// Protocol used (HTTPS, WSS, TCP, etc.).
    pub protocol: String,
}

/// Information about a local data store.
#[derive(Debug, Clone, Serialize)]
pub struct StoreInfo {
    /// Human-readable name of the store.
    pub name: String,
    /// Path on disk.
    pub path: String,
    /// Types of data stored.
    pub data_types: Vec<DataType>,
    /// Retention information.
    pub retention: String,
    /// How to delete/clear this store.
    pub deletion: String,
}

/// Information about WASM skill permissions.
#[derive(Debug, Clone, Serialize)]
pub struct SkillPermissionInfo {
    /// Skill name.
    pub name: String,
    /// Granted permissions.
    pub permissions: Vec<String>,
    /// Advisory flags for potential risks.
    pub advisories: Vec<String>,
}

/// Classification distribution counts per level for a single entity type.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LevelCounts {
    /// Number of entities classified as Public.
    pub public: usize,
    /// Number of entities classified as Internal.
    pub internal: usize,
    /// Number of entities classified as Confidential.
    pub confidential: usize,
    /// Number of entities classified as Restricted.
    pub restricted: usize,
}

/// Classification distribution across entity types.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ClassificationDistribution {
    /// Counts per level for memories.
    pub memories: LevelCounts,
    /// Counts per level for messages.
    pub messages: LevelCounts,
    /// Counts per level for sessions.
    pub sessions: LevelCounts,
}

/// PII detection status information.
#[derive(Debug, Clone, Serialize)]
pub struct PiiDetectionStatus {
    /// Whether auto-classification of PII-containing content is enabled.
    pub auto_classify_pii: bool,
    /// Active PII pattern types.
    pub active_patterns: Vec<String>,
    /// Context-aware exclusion types (code blocks, inline code, URLs).
    pub context_exclusions: Vec<String>,
}

/// Complete privacy report.
#[derive(Debug, Clone, Serialize)]
pub struct PrivacyReport {
    /// Outbound network endpoints.
    pub endpoints: Vec<EndpointInfo>,
    /// Local data stores.
    pub stores: Vec<StoreInfo>,
    /// WASM skill permissions.
    pub skill_permissions: Vec<SkillPermissionInfo>,
    /// Data classification summary.
    pub data_classification: Vec<String>,
    /// Classification distribution per level per entity type (None if DB unavailable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classification_distribution: Option<ClassificationDistribution>,
    /// PII detection status.
    pub pii_detection_status: PiiDetectionStatus,
}

/// Enumerate all outbound endpoints from the configuration.
fn enumerate_outbound_endpoints(config: &BlufioConfig) -> Vec<EndpointInfo> {
    let mut endpoints = Vec::new();

    // Anthropic API
    if config.anthropic.api_key.is_some() || std::env::var("ANTHROPIC_API_KEY").is_ok() {
        endpoints.push(EndpointInfo {
            name: "Anthropic API".to_string(),
            url: "https://api.anthropic.com/v1/messages".to_string(),
            data_types: vec![DataType::Usage, DataType::Credentials],
            protocol: "HTTPS".to_string(),
        });
    }

    // OpenAI provider
    if config.providers.openai.api_key.is_some() || std::env::var("OPENAI_API_KEY").is_ok() {
        endpoints.push(EndpointInfo {
            name: "OpenAI API".to_string(),
            url: config.providers.openai.base_url.clone(),
            data_types: vec![DataType::Usage, DataType::Credentials],
            protocol: "HTTPS".to_string(),
        });
    }

    // Ollama provider (local, but still an endpoint)
    if config.providers.ollama.default_model.is_some() {
        endpoints.push(EndpointInfo {
            name: "Ollama (local)".to_string(),
            url: config.providers.ollama.base_url.clone(),
            data_types: vec![DataType::Usage],
            protocol: "HTTP".to_string(),
        });
    }

    // OpenRouter provider
    if config.providers.openrouter.api_key.is_some() || std::env::var("OPENROUTER_API_KEY").is_ok()
    {
        endpoints.push(EndpointInfo {
            name: "OpenRouter API".to_string(),
            url: "https://openrouter.ai/api/v1".to_string(),
            data_types: vec![DataType::Usage, DataType::Credentials],
            protocol: "HTTPS".to_string(),
        });
    }

    // Gemini provider
    if config.providers.gemini.api_key.is_some() || std::env::var("GEMINI_API_KEY").is_ok() {
        endpoints.push(EndpointInfo {
            name: "Google Gemini API".to_string(),
            url: "https://generativelanguage.googleapis.com".to_string(),
            data_types: vec![DataType::Usage, DataType::Credentials],
            protocol: "HTTPS".to_string(),
        });
    }

    // Custom providers
    for (name, provider) in &config.providers.custom {
        let data_types = vec![DataType::Usage, DataType::Credentials];
        let protocol = if provider.base_url.starts_with("https") {
            "HTTPS".to_string()
        } else {
            "HTTP".to_string()
        };

        endpoints.push(EndpointInfo {
            name: format!("Custom Provider: {name}"),
            url: provider.base_url.clone(),
            data_types,
            protocol,
        });
    }

    // Telegram Bot API
    if config.telegram.bot_token.is_some() {
        endpoints.push(EndpointInfo {
            name: "Telegram Bot API".to_string(),
            url: "https://api.telegram.org/bot<token>".to_string(),
            data_types: vec![DataType::Usage, DataType::Pii, DataType::Credentials],
            protocol: "HTTPS".to_string(),
        });
    }

    // Discord Gateway
    if config.discord.bot_token.is_some() {
        endpoints.push(EndpointInfo {
            name: "Discord Gateway".to_string(),
            url: "wss://gateway.discord.gg".to_string(),
            data_types: vec![DataType::Usage, DataType::Pii, DataType::Credentials],
            protocol: "WSS".to_string(),
        });
    }

    // Slack API
    if config.slack.bot_token.is_some() {
        endpoints.push(EndpointInfo {
            name: "Slack API".to_string(),
            url: "https://slack.com/api".to_string(),
            data_types: vec![DataType::Usage, DataType::Pii, DataType::Credentials],
            protocol: "HTTPS".to_string(),
        });
    }

    // WhatsApp
    if config.whatsapp.phone_number_id.is_some() || config.whatsapp.access_token.is_some() {
        endpoints.push(EndpointInfo {
            name: "WhatsApp Business API".to_string(),
            url: "https://graph.facebook.com/v18.0".to_string(),
            data_types: vec![DataType::Usage, DataType::Pii, DataType::Credentials],
            protocol: "HTTPS".to_string(),
        });
    }

    // Signal
    if config.signal.socket_path.is_some() || config.signal.host.is_some() {
        let url = if let Some(ref socket) = config.signal.socket_path {
            format!("unix://{socket}")
        } else {
            let host = config.signal.host.as_deref().unwrap_or("127.0.0.1");
            let port = config.signal.port.unwrap_or(7583);
            format!("{host}:{port}")
        };
        endpoints.push(EndpointInfo {
            name: "Signal CLI".to_string(),
            url,
            data_types: vec![DataType::Usage, DataType::Pii],
            protocol: "TCP/Unix".to_string(),
        });
    }

    // IRC
    if config.irc.server.is_some() {
        let server = config.irc.server.as_deref().unwrap_or("unknown");
        let port = config
            .irc
            .port
            .unwrap_or(if config.irc.tls { 6697 } else { 6667 });
        endpoints.push(EndpointInfo {
            name: "IRC Server".to_string(),
            url: format!("{server}:{port}"),
            data_types: vec![DataType::Usage, DataType::Pii],
            protocol: if config.irc.tls {
                "TLS".to_string()
            } else {
                "TCP".to_string()
            },
        });
    }

    // Matrix
    if config.matrix.homeserver_url.is_some() {
        let url = config
            .matrix
            .homeserver_url
            .as_deref()
            .unwrap_or("unknown")
            .to_string();
        endpoints.push(EndpointInfo {
            name: "Matrix Homeserver".to_string(),
            url,
            data_types: vec![DataType::Usage, DataType::Pii, DataType::Credentials],
            protocol: "HTTPS".to_string(),
        });
    }

    // MCP servers
    for server in &config.mcp.servers {
        let url = server.url.as_deref().unwrap_or("stdio").to_string();
        endpoints.push(EndpointInfo {
            name: format!("MCP Server: {}", server.name),
            url,
            data_types: vec![DataType::Usage, DataType::System],
            protocol: server.transport.clone(),
        });
    }

    endpoints
}

/// Enumerate local data stores from the configuration.
fn enumerate_local_stores(config: &BlufioConfig) -> Vec<StoreInfo> {
    let mut stores = Vec::new();

    // SQLite database
    stores.push(StoreInfo {
        name: "SQLite Database".to_string(),
        path: config.storage.database_path.clone(),
        data_types: vec![DataType::Usage, DataType::Pii, DataType::Credentials],
        retention: "Indefinite (manual cleanup)".to_string(),
        deletion: "blufio uninstall --purge or delete file directly".to_string(),
    });

    // Vault (encrypted within SQLite)
    stores.push(StoreInfo {
        name: "Vault (encrypted secrets)".to_string(),
        path: format!("{} (vault table)", config.storage.database_path),
        data_types: vec![DataType::Credentials],
        retention: "Until manually deleted".to_string(),
        deletion: "blufio config set-secret to overwrite, uninstall --purge to remove all"
            .to_string(),
    });

    // Skills directory
    stores.push(StoreInfo {
        name: "WASM Skills".to_string(),
        path: config.skill.skills_dir.clone(),
        data_types: vec![DataType::System],
        retention: "Until uninstalled".to_string(),
        deletion: "blufio skill remove <name>".to_string(),
    });

    // Config directory
    if let Some(config_dir) = dirs::config_dir() {
        let blufio_config = config_dir.join("blufio");
        stores.push(StoreInfo {
            name: "Configuration".to_string(),
            path: blufio_config.to_string_lossy().to_string(),
            data_types: vec![DataType::System, DataType::Credentials],
            retention: "Persistent".to_string(),
            deletion: "Delete directory manually or blufio uninstall --purge".to_string(),
        });
    }

    stores
}

/// Enumerate WASM skill permissions from the configuration.
fn enumerate_skill_permissions(config: &BlufioConfig) -> Vec<SkillPermissionInfo> {
    // Skills are registered in the database, not directly in config.
    // We can report the sandbox configuration limits.
    let mut permissions = Vec::new();

    // Report the global skill sandbox settings
    permissions.push(SkillPermissionInfo {
        name: "(global skill sandbox)".to_string(),
        permissions: vec![
            format!("fuel_limit: {}", config.skill.default_fuel),
            format!("memory_limit: {} MB", config.skill.default_memory_mb),
            format!(
                "epoch_timeout: {}s",
                config.skill.default_epoch_timeout_secs
            ),
            format!("max_in_prompt: {}", config.skill.max_skills_in_prompt),
        ],
        advisories: vec![
            "Skills run in WASM sandbox with resource limits".to_string(),
            "Skills with both network and message access could exfiltrate conversations"
                .to_string(),
        ],
    });

    permissions
}

/// Generate data classification summary.
fn generate_classification_summary(
    endpoints: &[EndpointInfo],
    stores: &[StoreInfo],
) -> Vec<String> {
    let mut summary = Vec::new();

    let pii_endpoints: Vec<&str> = endpoints
        .iter()
        .filter(|e| e.data_types.contains(&DataType::Pii))
        .map(|e| e.name.as_str())
        .collect();

    let cred_endpoints: Vec<&str> = endpoints
        .iter()
        .filter(|e| e.data_types.contains(&DataType::Credentials))
        .map(|e| e.name.as_str())
        .collect();

    if !pii_endpoints.is_empty() {
        summary.push(format!("PII sent to: {}", pii_endpoints.join(", ")));
    }

    if !cred_endpoints.is_empty() {
        summary.push(format!(
            "Credentials used by: {}",
            cred_endpoints.join(", ")
        ));
    }

    let local_pii: Vec<&str> = stores
        .iter()
        .filter(|s| s.data_types.contains(&DataType::Pii))
        .map(|s| s.name.as_str())
        .collect();

    if !local_pii.is_empty() {
        summary.push(format!("PII stored locally in: {}", local_pii.join(", ")));
    }

    summary
}

/// Format the privacy report as markdown.
fn format_markdown_report(report: &PrivacyReport) -> String {
    let mut out = String::new();

    out.push_str("# Blufio Privacy Evidence Report\n\n");
    out.push_str(&format!(
        "Generated: {}\n\n",
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ")
    ));

    // Outbound Endpoints
    out.push_str("## Outbound Endpoints\n\n");
    if report.endpoints.is_empty() {
        out.push_str("No outbound endpoints configured.\n\n");
    } else {
        out.push_str("| Service | URL | Protocol | Data Types |\n");
        out.push_str("|---------|-----|----------|------------|\n");
        for ep in &report.endpoints {
            let data_types: Vec<String> = ep.data_types.iter().map(|d| d.to_string()).collect();
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                ep.name,
                ep.url,
                ep.protocol,
                data_types.join(", ")
            ));
        }
        out.push('\n');
    }

    // Local Data Stores
    out.push_str("## Local Data Stores\n\n");
    if report.stores.is_empty() {
        out.push_str("No local data stores found.\n\n");
    } else {
        out.push_str("| Store | Path | Data Types | Retention | Deletion |\n");
        out.push_str("|-------|------|------------|-----------|----------|\n");
        for store in &report.stores {
            let data_types: Vec<String> = store.data_types.iter().map(|d| d.to_string()).collect();
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                store.name,
                store.path,
                data_types.join(", "),
                store.retention,
                store.deletion
            ));
        }
        out.push('\n');
    }

    // Skill Permissions
    out.push_str("## Skill Permissions\n\n");
    if report.skill_permissions.is_empty() {
        out.push_str("No skills configured.\n\n");
    } else {
        for skill in &report.skill_permissions {
            out.push_str(&format!("### {}\n\n", skill.name));
            out.push_str("**Permissions:**\n");
            for perm in &skill.permissions {
                out.push_str(&format!("- {perm}\n"));
            }
            if !skill.advisories.is_empty() {
                out.push_str("\n**Advisories:**\n");
                for advisory in &skill.advisories {
                    out.push_str(&format!("- [!] {advisory}\n"));
                }
            }
            out.push('\n');
        }
    }

    // Data Classification Summary
    out.push_str("## Data Classification Summary\n\n");
    if report.data_classification.is_empty() {
        out.push_str("No data classification findings.\n\n");
    } else {
        for item in &report.data_classification {
            out.push_str(&format!("- {item}\n"));
        }
        out.push('\n');
    }

    // Classification Distribution
    if let Some(ref dist) = report.classification_distribution {
        out.push_str("## Data Classification Distribution\n\n");
        out.push_str("| Entity Type | Public | Internal | Confidential | Restricted |\n");
        out.push_str("|-------------|--------|----------|--------------|------------|\n");
        out.push_str(&format!(
            "| Memories    | {} | {} | {} | {} |\n",
            dist.memories.public,
            dist.memories.internal,
            dist.memories.confidential,
            dist.memories.restricted,
        ));
        out.push_str(&format!(
            "| Messages    | {} | {} | {} | {} |\n",
            dist.messages.public,
            dist.messages.internal,
            dist.messages.confidential,
            dist.messages.restricted,
        ));
        out.push_str(&format!(
            "| Sessions    | {} | {} | {} | {} |\n",
            dist.sessions.public,
            dist.sessions.internal,
            dist.sessions.confidential,
            dist.sessions.restricted,
        ));
        out.push('\n');
    }

    // PII Detection Status
    out.push_str("## PII Detection Status\n\n");
    out.push_str(&format!(
        "- **Auto-classify PII**: {}\n",
        if report.pii_detection_status.auto_classify_pii {
            "enabled"
        } else {
            "disabled"
        }
    ));
    out.push_str(&format!(
        "- **Active patterns**: {}\n",
        report.pii_detection_status.active_patterns.join(", ")
    ));
    out.push_str(&format!(
        "- **Context-aware exclusions**: {}\n\n",
        report.pii_detection_status.context_exclusions.join(", ")
    ));

    // Retention & Deletion
    out.push_str("## Retention & Deletion\n\n");
    out.push_str("- **Database**: No automatic retention policy. Data persists until manual cleanup or `blufio uninstall --purge`.\n");
    out.push_str("- **Vault secrets**: Encrypted with Argon2id-derived key. Removed with `blufio uninstall --purge`.\n");
    out.push_str("- **Skills**: WASM files persist until `blufio skill remove <name>`.\n");
    out.push_str("- **Config**: Persists until manually deleted or `blufio uninstall --purge`.\n");

    out
}

/// Main entry point for `blufio privacy evidence-report`.
pub async fn run_privacy_report(json: bool, output: Option<&str>) -> Result<(), BlufioError> {
    let config = blufio_config::load_and_validate()
        .map_err(|errors| BlufioError::Config(format!("{} config error(s)", errors.len())))?;

    let endpoints = enumerate_outbound_endpoints(&config);
    let stores = enumerate_local_stores(&config);
    let skill_permissions = enumerate_skill_permissions(&config);
    let data_classification = generate_classification_summary(&endpoints, &stores);

    let pii_detection_status = PiiDetectionStatus {
        auto_classify_pii: config.classification.auto_classify_pii,
        active_patterns: vec![
            "email".to_string(),
            "phone".to_string(),
            "ssn".to_string(),
            "credit_card".to_string(),
        ],
        context_exclusions: vec![
            "code_blocks".to_string(),
            "inline_code".to_string(),
            "urls".to_string(),
        ],
    };

    let report = PrivacyReport {
        endpoints,
        stores,
        skill_permissions,
        data_classification,
        // Classification distribution requires DB access -- skip if unavailable.
        classification_distribution: None,
        pii_detection_status,
    };

    let content = if json {
        serde_json::to_string_pretty(&report)
            .map_err(|e| BlufioError::Internal(format!("failed to serialize report: {e}")))?
    } else {
        format_markdown_report(&report)
    };

    if let Some(path) = output {
        std::fs::write(path, &content)
            .map_err(|e| BlufioError::Internal(format!("failed to write report to {path}: {e}")))?;
        eprintln!("Privacy report written to: {path}");
    } else {
        println!("{content}");
    }

    Ok(())
}
