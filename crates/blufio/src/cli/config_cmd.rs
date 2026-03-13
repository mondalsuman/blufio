// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Config CLI handlers for `blufio config` subcommands.
//!
//! Includes vault secret management, config get, and recipe generation.

/// Open the database, returning the connection.
pub(crate) async fn open_db(
    config: &blufio_config::model::BlufioConfig,
) -> Result<blufio_storage::Database, blufio_core::BlufioError> {
    blufio_storage::Database::open(&config.storage.database_path).await
}

/// Handle `blufio config set-secret <key>`.
///
/// Creates the vault lazily on first use. Prompts for the secret value
/// via hidden TTY input or reads from piped stdin.
pub(crate) async fn cmd_set_secret(
    config: &blufio_config::model::BlufioConfig,
    key: &str,
) -> Result<(), blufio_core::BlufioError> {
    let db = open_db(config).await?;
    let conn = db.connection().clone();

    // Get or create vault.
    let vault = if blufio_vault::Vault::exists(&conn).await? {
        let passphrase = blufio_vault::get_vault_passphrase()?;
        blufio_vault::Vault::unlock(conn, &passphrase, &config.vault).await?
    } else {
        eprintln!("No vault found. Creating a new vault.");
        let passphrase = blufio_vault::prompt::get_vault_passphrase_with_confirm()?;
        blufio_vault::Vault::create(conn, &passphrase, &config.vault).await?
    };

    // Read secret value.
    let value = read_secret_value(key)?;

    // Store in vault.
    vault.store_secret(key, &value).await?;
    eprintln!("Secret '{}' stored in vault.", key);

    // Clean close with WAL checkpoint.
    db.close().await?;
    Ok(())
}

/// Handle `blufio config list-secrets`.
///
/// Lists all vault secrets with masked previews. Values are never fully shown.
pub(crate) async fn cmd_list_secrets(
    config: &blufio_config::model::BlufioConfig,
) -> Result<(), blufio_core::BlufioError> {
    let db = open_db(config).await?;
    let conn = db.connection().clone();

    if !blufio_vault::Vault::exists(&conn).await? {
        println!("No vault found. Use 'blufio config set-secret' to create one.");
        db.close().await?;
        return Ok(());
    }

    let passphrase = blufio_vault::get_vault_passphrase()?;
    let vault = blufio_vault::Vault::unlock(conn, &passphrase, &config.vault).await?;

    let secrets = vault.list_secrets().await?;
    if secrets.is_empty() {
        println!("No secrets stored.");
    } else {
        for (name, masked) in &secrets {
            println!("{name}: {masked}");
        }
    }

    db.close().await?;
    Ok(())
}

/// Read a secret value from interactive TTY (hidden input) or piped stdin.
pub(crate) fn read_secret_value(key: &str) -> Result<String, blufio_core::BlufioError> {
    if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        eprint!("Secret value for '{key}': ");
        let value = rpassword::read_password().map_err(|e| {
            blufio_core::BlufioError::Vault(format!("failed to read secret value: {e}"))
        })?;
        if value.is_empty() {
            return Err(blufio_core::BlufioError::Vault(
                "empty secret value not allowed".to_string(),
            ));
        }
        Ok(value)
    } else {
        // Read from piped stdin for scripting support.
        let mut line = String::new();
        std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line).map_err(|e| {
            blufio_core::BlufioError::Vault(format!("failed to read secret from stdin: {e}"))
        })?;
        let value = line.trim_end_matches('\n').trim_end_matches('\r');
        if value.is_empty() {
            return Err(blufio_core::BlufioError::Vault(
                "empty secret value not allowed".to_string(),
            ));
        }
        Ok(value.to_string())
    }
}

/// Handle `blufio config get <key>`.
///
/// Resolves a dotted config key path to its current value. Uses serde_json
/// serialization to traverse the config struct generically.
pub(crate) fn cmd_config_get(
    config: &blufio_config::model::BlufioConfig,
    key: &str,
) -> Result<(), blufio_core::BlufioError> {
    // Serialize the full config to a JSON Value for generic traversal.
    let value = serde_json::to_value(config).map_err(|e| {
        blufio_core::BlufioError::Internal(format!("failed to serialize config: {e}"))
    })?;

    // Walk the dotted key path.
    let parts: Vec<&str> = key.split('.').collect();
    let mut current = &value;

    for part in &parts {
        match current.get(part) {
            Some(v) => current = v,
            None => {
                return Err(blufio_core::BlufioError::Config(format!(
                    "unknown config key: {key}"
                )));
            }
        }
    }

    // Print the resolved value.
    match current {
        serde_json::Value::String(s) => println!("{s}"),
        serde_json::Value::Null => println!("null"),
        other => println!("{other}"),
    }

    Ok(())
}

/// Generate a config recipe template for a specific preset.
pub(crate) fn generate_config_recipe(preset: &str) -> Result<String, blufio_core::BlufioError> {
    let content = match preset {
        "personal" => {
            r#"# Blufio Configuration: Personal Use
# Generated by: blufio config recipe personal
#
# Minimal setup for personal use with a single chat channel.

[agent]
name = "blufio"
max_sessions = 3
log_level = "info"
# system_prompt = "You are a helpful personal assistant."

[telegram]
# bot_token = "<your-telegram-bot-token>"
# allowed_users = ["your_telegram_id"]

[anthropic]
# api_key = "<your-anthropic-api-key>"
# Or set ANTHROPIC_API_KEY environment variable
default_model = "claude-sonnet-4-20250514"
max_tokens = 4096

[storage]
# database_path = "~/.local/share/blufio/blufio.db"

[cost]
# daily_limit_usd = 5.0
# monthly_limit_usd = 50.0
"#
        }
        "team" => {
            r#"# Blufio Configuration: Team Use
# Generated by: blufio config recipe team
#
# Setup for a small team with Slack integration and cost controls.

[agent]
name = "team-blufio"
max_sessions = 10
log_level = "info"
# system_prompt_file = "/etc/blufio/system-prompt.md"

[slack]
# bot_token = "<xoxb-your-slack-bot-token>"
# app_token = "<xapp-your-slack-app-token>"
# allowed_users = ["U12345", "U67890"]

[anthropic]
# api_key = "<your-anthropic-api-key>"
default_model = "claude-sonnet-4-20250514"
max_tokens = 4096

[storage]
# database_path = "/var/lib/blufio/blufio.db"

[cost]
# daily_limit_usd = 20.0
# monthly_limit_usd = 200.0

[security]
# require_tls = true

[gateway]
enabled = true
host = "127.0.0.1"
port = 3000
# bearer_token = "<generate-a-strong-token>"
"#
        }
        "production" => {
            r##"# Blufio Configuration: Production
# Generated by: blufio config recipe production
#
# Full production setup with all channels, security, monitoring, and rate limits.

[agent]
name = "blufio-prod"
max_sessions = 50
log_level = "warn"
# system_prompt_file = "/etc/blufio/system-prompt.md"

[telegram]
# bot_token = "<your-telegram-bot-token>"
# allowed_users = []

[discord]
# bot_token = "<your-discord-bot-token>"
# application_id = 0
# allowed_users = []

[slack]
# bot_token = "<xoxb-your-slack-bot-token>"
# app_token = "<xapp-your-slack-app-token>"
# allowed_users = []

[irc]
# server = "irc.libera.chat"
# port = 6697
# nickname = "blufio-bot"
# channels = ["#your-channel"]
# tls = true

[matrix]
# homeserver_url = "https://matrix.org"
# username = "blufio-bot"
# password = "<your-matrix-password>"
# rooms = ["#your-room:matrix.org"]

[anthropic]
# api_key = "<your-anthropic-api-key>"
default_model = "claude-sonnet-4-20250514"
max_tokens = 4096

[providers]
default = "anthropic"

[providers.openai]
# api_key = "<your-openai-api-key>"
# default_model = "gpt-4o"

[storage]
database_path = "/var/lib/blufio/blufio.db"
wal_mode = true

[cost]
# daily_limit_usd = 100.0
# monthly_limit_usd = 1000.0

[security]
# require_tls = true

[vault]
# session_timeout_secs = 900

[gateway]
enabled = true
host = "0.0.0.0"
port = 3000
# bearer_token = "<generate-a-strong-token>"
# default_rate_limit = 60

[prometheus]
enabled = true
port = 9090

[daemon]
memory_warn_mb = 256
memory_limit_mb = 512
# health_port = 3000
"##
        }
        "iot" => {
            r#"# Blufio Configuration: IoT / Embedded
# Generated by: blufio config recipe iot
#
# Minimal configuration for resource-constrained IoT devices.

[agent]
name = "blufio-iot"
max_sessions = 1
log_level = "warn"

[anthropic]
# api_key = "<your-anthropic-api-key>"
default_model = "claude-sonnet-4-20250514"
max_tokens = 1024

[storage]
# database_path = "/var/lib/blufio/blufio.db"

[daemon]
memory_warn_mb = 64
memory_limit_mb = 128

[skill]
default_fuel = 500000
default_memory_mb = 16
default_epoch_timeout_secs = 5
max_skills_in_prompt = 3

[gateway]
enabled = false
"#
        }
        _ => {
            return Err(blufio_core::BlufioError::Config(format!(
                "unknown recipe preset: \"{preset}\". Available: personal, team, production, iot"
            )));
        }
    };

    Ok(content.to_string())
}
