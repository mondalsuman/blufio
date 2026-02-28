// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Post-deserialization validation for configuration values.
//!
//! Validates semantic constraints that cannot be expressed via serde attributes,
//! such as valid IP addresses, non-empty paths, and non-negative budgets.

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
    if let Some(daily) = config.cost.daily_budget_usd {
        if daily < 0.0 {
            errors.push(ConfigError::Validation {
                message: format!(
                    "cost.daily_budget_usd must be non-negative, got {daily}"
                ),
            });
        }
    }

    if let Some(monthly) = config.cost.monthly_budget_usd {
        if monthly < 0.0 {
            errors.push(ConfigError::Validation {
                message: format!(
                    "cost.monthly_budget_usd must be non-negative, got {monthly}"
                ),
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
}
