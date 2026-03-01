// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Post-deserialization validation for configuration values.
//!
//! Validates semantic constraints that cannot be expressed via serde attributes,
//! such as valid IP addresses, non-empty paths, and non-negative budgets.

use std::collections::HashSet;

use crate::diagnostic::ConfigError;
use crate::model::BlufioConfig;

/// Validate a deserialized configuration for semantic correctness.
///
/// Returns `Ok(())` if all validations pass, or `Err(Vec<ConfigError>)` with
/// all collected validation errors (does not fail fast).
pub fn validate_config(config: &BlufioConfig) -> Result<(), Vec<ConfigError>> {
    let mut errors = Vec::new();

    // Validate bind_address is not empty
    if config.security.bind_address.trim().is_empty() {
        errors.push(ConfigError::Validation {
            message: "security.bind_address must not be empty".to_string(),
        });
    }

    // Validate bind_address looks like a valid IP or hostname
    if !config.security.bind_address.trim().is_empty() {
        let addr = config.security.bind_address.trim();
        // Accept valid IPv4, IPv6, or hostname patterns
        let is_valid_ip = addr.parse::<std::net::IpAddr>().is_ok();
        let is_valid_hostname = addr
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == ':');
        if !is_valid_ip && !is_valid_hostname {
            errors.push(ConfigError::Validation {
                message: format!(
                    "security.bind_address `{addr}` is not a valid IP address or hostname"
                ),
            });
        }
    }

    // Validate database_path is not empty
    if config.storage.database_path.trim().is_empty() {
        errors.push(ConfigError::Validation {
            message: "storage.database_path must not be empty".to_string(),
        });
    }

    // Validate budget values are non-negative if set
    if let Some(daily) = config.cost.daily_budget_usd
        && daily < 0.0
    {
        errors.push(ConfigError::Validation {
            message: format!("cost.daily_budget_usd must be non-negative, got {daily}"),
        });
    }

    if let Some(monthly) = config.cost.monthly_budget_usd
        && monthly < 0.0
    {
        errors.push(ConfigError::Validation {
            message: format!("cost.monthly_budget_usd must be non-negative, got {monthly}"),
        });
    }

    // Validate vault KDF parameters
    if config.vault.kdf_memory_cost < 32768 {
        errors.push(ConfigError::Validation {
            message: format!(
                "vault.kdf_memory_cost must be at least 32768 (32 MiB), got {}",
                config.vault.kdf_memory_cost
            ),
        });
    }

    if config.vault.kdf_iterations < 2 {
        errors.push(ConfigError::Validation {
            message: format!(
                "vault.kdf_iterations must be at least 2, got {}",
                config.vault.kdf_iterations
            ),
        });
    }

    if config.vault.kdf_parallelism < 1 {
        errors.push(ConfigError::Validation {
            message: format!(
                "vault.kdf_parallelism must be at least 1, got {}",
                config.vault.kdf_parallelism
            ),
        });
    }

    // Validate no duplicate agent names
    let mut seen_names = HashSet::new();
    for agent in &config.agents {
        if !seen_names.insert(&agent.name) {
            errors.push(ConfigError::Validation {
                message: format!(
                    "duplicate agent name `{}` in [[agents]] array",
                    agent.name
                ),
            });
        }
    }

    // Validate agent names are non-empty
    for (i, agent) in config.agents.iter().enumerate() {
        if agent.name.trim().is_empty() {
            errors.push(ConfigError::Validation {
                message: format!("agents[{i}].name must not be empty"),
            });
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_validates() {
        let config = BlufioConfig::default();
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn empty_database_path_fails_validation() {
        let mut config = BlufioConfig::default();
        config.storage.database_path = "".to_string();
        let errors = validate_config(&config).unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, ConfigError::Validation { message } if message.contains("database_path"))));
    }

    #[test]
    fn negative_budget_fails_validation() {
        let mut config = BlufioConfig::default();
        config.cost.daily_budget_usd = Some(-5.0);
        let errors = validate_config(&config).unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, ConfigError::Validation { message } if message.contains("daily_budget_usd"))));
    }

    #[test]
    fn valid_custom_config_passes() {
        let mut config = BlufioConfig::default();
        config.security.bind_address = "0.0.0.0".to_string();
        config.storage.database_path = "/tmp/test.db".to_string();
        config.cost.daily_budget_usd = Some(10.0);
        config.cost.monthly_budget_usd = Some(100.0);
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn empty_agents_array_defaults_correctly() {
        let toml_str = r#"
[agent]
name = "test"
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert!(config.agents.is_empty());
    }

    #[test]
    fn agents_array_deserializes_correctly() {
        let toml_str = r#"
[agent]
name = "test"

[[agents]]
name = "summarizer"
system_prompt = "You summarize text"
model = "claude-haiku-4-5-20250901"
allowed_skills = ["web_search"]

[[agents]]
name = "coder"
system_prompt = "You write code"
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.agents.len(), 2);
        assert_eq!(config.agents[0].name, "summarizer");
        assert_eq!(config.agents[0].system_prompt, "You summarize text");
        assert_eq!(config.agents[0].model, "claude-haiku-4-5-20250901");
        assert_eq!(config.agents[0].allowed_skills, vec!["web_search"]);
        assert_eq!(config.agents[1].name, "coder");
        assert_eq!(config.agents[1].system_prompt, "You write code");
        // model defaults to claude-sonnet-4-20250514
        assert_eq!(config.agents[1].model, "claude-sonnet-4-20250514");
        assert!(config.agents[1].allowed_skills.is_empty());
    }

    #[test]
    fn agents_deny_unknown_fields() {
        let toml_str = r#"
[agent]
name = "test"

[[agents]]
name = "summarizer"
system_prompt = "You summarize text"
unknown_field = "bad"
"#;
        let result = toml::from_str::<BlufioConfig>(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn delegation_timeout_defaults_to_60() {
        let toml_str = r#"
[agent]
name = "test"

[delegation]
enabled = true
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert!(config.delegation.enabled);
        assert_eq!(config.delegation.timeout_secs, 60);
    }

    #[test]
    fn delegation_defaults_when_not_specified() {
        let config = BlufioConfig::default();
        assert!(!config.delegation.enabled);
        assert_eq!(config.delegation.timeout_secs, 60);
    }

    #[test]
    fn duplicate_agent_names_fails_validation() {
        use crate::model::AgentSpecConfig;
        let mut config = BlufioConfig::default();
        config.agents = vec![
            AgentSpecConfig {
                name: "summarizer".to_string(),
                system_prompt: "prompt1".to_string(),
                model: "claude-sonnet-4-20250514".to_string(),
                allowed_skills: vec![],
            },
            AgentSpecConfig {
                name: "summarizer".to_string(),
                system_prompt: "prompt2".to_string(),
                model: "claude-sonnet-4-20250514".to_string(),
                allowed_skills: vec![],
            },
        ];
        let errors = validate_config(&config).unwrap_err();
        assert!(errors.iter().any(
            |e| matches!(e, ConfigError::Validation { message } if message.contains("duplicate agent name"))
        ));
    }
}
