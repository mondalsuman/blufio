// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Re-exports injection defense configuration types from [`blufio_config`].
//!
//! The canonical definitions live in `blufio-config/src/model.rs` (following the
//! established pattern where `ClassificationConfig`, `AuditConfig`, etc. are all
//! defined inline in the config crate).

pub use blufio_config::model::{
    HitlConfig, HmacBoundaryConfig, InjectionDefenseConfig, InputDetectionConfig,
    OutputScreeningConfig,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injection_defense_config_default_enabled_true() {
        let config = InjectionDefenseConfig::default();
        assert!(config.enabled);
        assert!(!config.dry_run);
    }

    #[test]
    fn input_detection_config_default_blocking_threshold() {
        let config = InputDetectionConfig::default();
        assert_eq!(config.mode, "log");
        assert!((config.blocking_threshold - 0.95).abs() < f64::EPSILON);
        assert!((config.mcp_blocking_threshold - 0.98).abs() < f64::EPSILON);
        assert!(config.custom_patterns.is_empty());
    }

    #[test]
    fn hmac_boundary_config_default_enabled() {
        let config = HmacBoundaryConfig::default();
        assert!(config.enabled);
    }

    #[test]
    fn output_screening_config_default_values() {
        let config = OutputScreeningConfig::default();
        assert!(config.enabled);
        assert_eq!(config.escalation_threshold, 3);
    }

    #[test]
    fn hitl_config_default_disabled() {
        let config = HitlConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.timeout_secs, 60);
        assert_eq!(config.max_pending, 3);
        assert_eq!(
            config.safe_tools,
            vec!["memory_search", "session_history", "cost_lookup", "skill_list"]
        );
    }

    #[test]
    fn injection_defense_config_deserializes_from_toml() {
        let toml_str = r#"
enabled = true
dry_run = true

[input_detection]
mode = "block"
blocking_threshold = 0.90
mcp_blocking_threshold = 0.95
custom_patterns = ["(?i)hack\\s+the\\s+planet"]

[hmac_boundaries]
enabled = false

[output_screening]
enabled = true
escalation_threshold = 5

[hitl]
enabled = true
timeout_secs = 30
max_pending = 5
safe_tools = ["memory_search", "cost_lookup"]
"#;

        let config: InjectionDefenseConfig = toml::from_str(toml_str).unwrap();
        assert!(config.enabled);
        assert!(config.dry_run);
        assert_eq!(config.input_detection.mode, "block");
        assert!((config.input_detection.blocking_threshold - 0.90).abs() < f64::EPSILON);
        assert!((config.input_detection.mcp_blocking_threshold - 0.95).abs() < f64::EPSILON);
        assert_eq!(config.input_detection.custom_patterns.len(), 1);
        assert!(!config.hmac_boundaries.enabled);
        assert!(config.output_screening.enabled);
        assert_eq!(config.output_screening.escalation_threshold, 5);
        assert!(config.hitl.enabled);
        assert_eq!(config.hitl.timeout_secs, 30);
        assert_eq!(config.hitl.max_pending, 5);
        assert_eq!(config.hitl.safe_tools, vec!["memory_search", "cost_lookup"]);
    }

    #[test]
    fn injection_defense_config_deserializes_empty_toml() {
        // Empty TOML should use all defaults
        let config: InjectionDefenseConfig = toml::from_str("").unwrap();
        assert!(config.enabled);
        assert!(!config.dry_run);
        assert_eq!(config.input_detection.mode, "log");
        assert!((config.input_detection.blocking_threshold - 0.95).abs() < f64::EPSILON);
    }
}
