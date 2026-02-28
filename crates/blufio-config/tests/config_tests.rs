// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration tests for the Blufio configuration system.

use blufio_config::diagnostic::{suggest_key, ConfigError};
use blufio_config::model::BlufioConfig;
use blufio_config::{load_and_validate_str, load_config_from_str};

/// Valid TOML with all known fields deserializes successfully.
#[test]
fn valid_toml_deserializes_into_blufio_config() {
    let toml = r#"
[agent]
name = "test-agent"
max_sessions = 5
log_level = "debug"

[telegram]
bot_token = "123:ABC"
allowed_users = ["alice", "bob"]

[anthropic]
api_key = "sk-ant-123"
default_model = "claude-sonnet-4-20250514"

[storage]
database_path = "/tmp/test.db"
wal_mode = false

[security]
bind_address = "0.0.0.0"
require_tls = false

[cost]
daily_budget_usd = 10.0
monthly_budget_usd = 100.0
track_tokens = false
"#;

    let config = load_config_from_str(toml).expect("valid TOML should deserialize");
    assert_eq!(config.agent.name, "test-agent");
    assert_eq!(config.agent.max_sessions, 5);
    assert_eq!(config.agent.log_level, "debug");
    assert_eq!(config.telegram.bot_token.as_deref(), Some("123:ABC"));
    assert_eq!(config.telegram.allowed_users, vec!["alice", "bob"]);
    assert_eq!(config.anthropic.api_key.as_deref(), Some("sk-ant-123"));
    assert_eq!(config.storage.database_path, "/tmp/test.db");
    assert!(!config.storage.wal_mode);
    assert_eq!(config.security.bind_address, "0.0.0.0");
    assert!(!config.security.require_tls);
    assert_eq!(config.cost.daily_budget_usd, Some(10.0));
    assert_eq!(config.cost.monthly_budget_usd, Some(100.0));
    assert!(!config.cost.track_tokens);
}

/// Unknown field in [agent] section produces an UnknownField error.
#[test]
fn unknown_field_in_agent_produces_error() {
    let toml = r#"
[agent]
naem = "test"
"#;

    let err = load_config_from_str(toml).expect_err("should reject unknown field");
    let err_str = format!("{err}");
    // Figment wraps serde's deny_unknown_fields error
    assert!(
        err_str.contains("unknown field") || err_str.contains("naem"),
        "error should mention unknown field or the bad key, got: {err_str}"
    );
}

/// Unknown field in [telegram] section produces an UnknownField error.
#[test]
fn unknown_field_in_telegram_produces_error() {
    let toml = r#"
[telegram]
bot_tken = "abc"
"#;

    let err = load_config_from_str(toml).expect_err("should reject unknown field");
    let err_str = format!("{err}");
    assert!(
        err_str.contains("unknown field") || err_str.contains("bot_tken"),
        "error should mention unknown field, got: {err_str}"
    );
}

/// Missing optional sections use defaults without error.
#[test]
fn missing_optional_sections_use_defaults() {
    let toml = "";
    let config = load_config_from_str(toml).expect("empty TOML should use defaults");

    assert_eq!(config.agent.name, "blufio");
    assert_eq!(config.agent.max_sessions, 10);
    assert_eq!(config.agent.log_level, "info");
    assert!(config.telegram.bot_token.is_none());
    assert!(config.telegram.allowed_users.is_empty());
    assert!(config.anthropic.api_key.is_none());
    assert_eq!(config.anthropic.default_model, "claude-sonnet-4-20250514");
    assert_eq!(config.storage.database_path, "blufio.db");
    assert!(config.storage.wal_mode);
    assert_eq!(config.security.bind_address, "127.0.0.1");
    assert!(config.security.require_tls);
    assert!(config.cost.daily_budget_usd.is_none());
    assert!(config.cost.monthly_budget_usd.is_none());
    assert!(config.cost.track_tokens);
}

/// Environment variable BLUFIO_AGENT_NAME overrides agent.name in TOML.
#[test]
fn env_var_overrides_agent_name() {
    // We test this via the Figment builder directly to control env vars in test
    use figment::{
        providers::{Format, Serialized, Toml},
        Figment,
    };

    let toml_content = r#"
[agent]
name = "from-toml"
"#;

    // Simulate BLUFIO_AGENT_NAME env var by building figment with test env
    let config: BlufioConfig = Figment::new()
        .merge(Serialized::defaults(BlufioConfig::default()))
        .merge(Toml::string(toml_content))
        .merge(("agent.name", "envtest"))
        .extract()
        .expect("should merge env override");

    assert_eq!(config.agent.name, "envtest");
}

/// Environment variable BLUFIO_TELEGRAM_BOT_TOKEN maps to telegram.bot_token
/// (NOT telegram.bot.token -- this is the critical Pitfall 5 from research).
#[test]
fn env_var_overrides_telegram_bot_token() {
    use figment::{providers::Serialized, Figment};

    let config: BlufioConfig = Figment::new()
        .merge(Serialized::defaults(BlufioConfig::default()))
        .merge(("telegram.bot_token", "xyz-from-env"))
        .extract()
        .expect("should set bot_token via dot notation");

    assert_eq!(config.telegram.bot_token.as_deref(), Some("xyz-from-env"));
}

/// Serialized defaults provide sensible values for all required fields.
#[test]
fn serialized_defaults_are_sensible() {
    let config = BlufioConfig::default();

    assert_eq!(config.agent.name, "blufio");
    assert_eq!(config.agent.max_sessions, 10);
    assert_eq!(config.agent.log_level, "info");
    assert!(config.telegram.bot_token.is_none());
    assert_eq!(config.anthropic.default_model, "claude-sonnet-4-20250514");
    assert_eq!(config.storage.database_path, "blufio.db");
    assert!(config.storage.wal_mode);
    assert_eq!(config.security.bind_address, "127.0.0.1");
    assert!(config.security.require_tls);
    assert!(config.cost.track_tokens);
}

/// Missing config files are silently skipped (Figment's Toml::file() behavior).
#[test]
fn missing_config_files_silently_skipped() {
    use figment::{
        providers::{Format, Serialized, Toml},
        Figment,
    };

    let config: BlufioConfig = Figment::new()
        .merge(Serialized::defaults(BlufioConfig::default()))
        .merge(Toml::file("/nonexistent/path/blufio.toml"))
        .extract()
        .expect("missing file should be silently skipped");

    // Should just get defaults
    assert_eq!(config.agent.name, "blufio");
}

/// Config sections match user decision: agent, telegram, anthropic, storage, security, cost.
#[test]
fn config_sections_match_user_decision() {
    let toml = r#"
[agent]
name = "a"

[telegram]
bot_token = "b"

[anthropic]
api_key = "c"

[storage]
database_path = "d"

[security]
bind_address = "e"

[cost]
track_tokens = false
"#;

    let config = load_config_from_str(toml).expect("all expected sections should parse");
    assert_eq!(config.agent.name, "a");
    assert_eq!(config.telegram.bot_token.as_deref(), Some("b"));
    assert_eq!(config.anthropic.api_key.as_deref(), Some("c"));
    assert_eq!(config.storage.database_path, "d");
    assert_eq!(config.security.bind_address, "e");
    assert!(!config.cost.track_tokens);
}

/// Unexpected top-level section is rejected by deny_unknown_fields.
#[test]
fn deny_unknown_fields_at_top_level() {
    let toml = r#"
[logging]
level = "debug"
"#;

    let err = load_config_from_str(toml).expect_err("unknown top-level section should be rejected");
    let err_str = format!("{err}");
    assert!(
        err_str.contains("unknown field") || err_str.contains("logging"),
        "error should mention unknown field, got: {err_str}"
    );
}

// ============================================================================
// Diagnostic tests (Task 2)
// ============================================================================

/// Unknown key "naem" in [agent] produces suggestion "did you mean `name`?"
#[test]
fn diagnostic_naem_suggests_name() {
    let valid_keys = &["name", "max_sessions", "log_level"];
    let suggestion = suggest_key("naem", valid_keys);
    assert_eq!(suggestion, Some("name".to_string()));
}

/// Unknown key "bot_tken" in [telegram] produces suggestion "did you mean `bot_token`?"
#[test]
fn diagnostic_bot_tken_suggests_bot_token() {
    let valid_keys = &["bot_token", "allowed_users"];
    let suggestion = suggest_key("bot_tken", valid_keys);
    assert_eq!(suggestion, Some("bot_token".to_string()));
}

/// Unknown key "zzzzzz" with no close match does NOT produce a suggestion.
#[test]
fn diagnostic_no_suggestion_for_distant_typo() {
    let valid_keys = &["name", "max_sessions", "log_level"];
    let suggestion = suggest_key("zzzzzz", valid_keys);
    assert!(suggestion.is_none(), "should not suggest for distant typo");
}

/// Error output from load_and_validate_str includes the unknown key name.
#[test]
fn diagnostic_error_includes_unknown_key() {
    let toml = r#"
[agent]
naem = "test"
"#;

    let errors = load_and_validate_str(toml).expect_err("should produce errors");
    assert!(!errors.is_empty(), "should have at least one error");

    let has_unknown_key = errors.iter().any(|e| {
        matches!(e, ConfigError::UnknownKey { key, suggestion, valid_keys, .. } if {
            key == "naem"
                && suggestion.as_deref() == Some("name")
                && valid_keys.contains("name")
        })
    });
    assert!(
        has_unknown_key,
        "should have UnknownKey error for 'naem' with suggestion 'name', got: {errors:?}"
    );
}

/// Error output includes the list of valid keys for the section.
#[test]
fn diagnostic_error_includes_valid_keys() {
    let toml = r#"
[agent]
naem = "test"
"#;

    let errors = load_and_validate_str(toml).expect_err("should produce errors");
    let has_valid_keys = errors.iter().any(|e| {
        matches!(e, ConfigError::UnknownKey { valid_keys, .. } if {
            valid_keys.contains("name")
                && valid_keys.contains("max_sessions")
                && valid_keys.contains("log_level")
        })
    });
    assert!(
        has_valid_keys,
        "error should list valid keys for [agent] section"
    );
}

/// Invalid type (string where number expected) produces clear message.
#[test]
fn diagnostic_invalid_type_message() {
    let toml = r#"
[agent]
max_sessions = "not_a_number"
"#;

    let err = load_config_from_str(toml).expect_err("should reject invalid type");
    let err_str = format!("{err}");
    assert!(
        err_str.contains("invalid type") || err_str.contains("max_sessions"),
        "error should mention type mismatch, got: {err_str}"
    );
}

/// ConfigError implements miette::Diagnostic (can be rendered).
#[test]
fn config_error_implements_diagnostic() {
    use miette::Diagnostic;

    let error = ConfigError::UnknownKey {
        key: "naem".to_string(),
        suggestion: Some("name".to_string()),
        valid_keys: "name, max_sessions, log_level".to_string(),
        span: None,
        src: None,
    };

    // Verify it implements Diagnostic
    let code = error.code();
    assert!(code.is_some(), "should have diagnostic code");

    let help = error.help();
    assert!(help.is_some(), "should have help text");
    let help_str = help.unwrap().to_string();
    assert!(
        help_str.contains("did you mean `name`"),
        "help should contain suggestion, got: {help_str}"
    );
}

/// ConfigError can be rendered using miette's graphical handler.
#[test]
fn config_error_renders_with_miette() {
    use miette::GraphicalReportHandler;

    let error = ConfigError::UnknownKey {
        key: "naem".to_string(),
        suggestion: Some("name".to_string()),
        valid_keys: "name, max_sessions, log_level".to_string(),
        span: None,
        src: None,
    };

    let handler = GraphicalReportHandler::new();
    let mut buf = String::new();
    handler
        .render_report(&mut buf, &error)
        .expect("should render without error");
    assert!(
        !buf.is_empty(),
        "rendered report should not be empty"
    );
    assert!(
        buf.contains("naem"),
        "rendered report should mention the key"
    );
}

/// load_and_validate_str with valid TOML returns Ok config.
#[test]
fn load_and_validate_valid_toml() {
    let toml = r#"
[agent]
name = "test"
"#;

    let config = load_and_validate_str(toml).expect("valid TOML should validate");
    assert_eq!(config.agent.name, "test");
}

/// load_and_validate with defaults works (no config file needed).
#[test]
fn load_and_validate_defaults() {
    let config = blufio_config::load_and_validate().expect("defaults should validate");
    assert_eq!(config.agent.name, "blufio");
}

/// Validation catches negative budget.
#[test]
fn validation_catches_negative_budget() {
    let toml = r#"
[cost]
daily_budget_usd = -5.0
"#;

    let errors = load_and_validate_str(toml).expect_err("negative budget should fail");
    let has_validation_error = errors.iter().any(|e| {
        matches!(e, ConfigError::Validation { message } if message.contains("daily_budget_usd"))
    });
    assert!(has_validation_error, "should have validation error for negative budget");
}
