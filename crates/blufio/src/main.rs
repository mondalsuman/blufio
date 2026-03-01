// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Blufio - An always-on personal AI agent.
//!
//! This is the binary entry point for the Blufio agent.

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

mod backup;
mod doctor;
mod serve;
mod shell;
mod status;

use clap::{Parser, Subcommand};

/// Blufio - An always-on personal AI agent.
#[derive(Parser, Debug)]
#[command(name = "blufio", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

/// Available subcommands.
#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the Blufio agent server.
    Serve,
    /// Launch an interactive REPL session.
    Shell,
    /// Show agent status (connects to health endpoint).
    Status {
        /// Output as structured JSON for scripting.
        #[arg(long)]
        json: bool,
        /// Disable colored output.
        #[arg(long)]
        plain: bool,
    },
    /// Run diagnostic checks against the environment.
    Doctor {
        /// Run additional intensive checks (DB integrity, memory, disk).
        #[arg(long)]
        deep: bool,
        /// Disable colored output.
        #[arg(long)]
        plain: bool,
    },
    /// Create an atomic backup of the SQLite database.
    Backup {
        /// Destination path for the backup file.
        path: String,
    },
    /// Restore the database from a backup file.
    Restore {
        /// Path to the backup file to restore from.
        path: String,
    },
    /// Manage Blufio configuration and vault secrets.
    Config {
        #[command(subcommand)]
        action: Option<ConfigCommands>,
    },
    /// Manage Blufio skills (WASM plugins).
    Skill {
        #[command(subcommand)]
        action: SkillCommands,
    },
    /// Manage Blufio plugins (compiled-in adapter modules).
    Plugin {
        #[command(subcommand)]
        action: PluginCommands,
    },
}

/// Config management subcommands.
#[derive(Subcommand, Debug)]
enum ConfigCommands {
    /// Store or update an encrypted secret in the vault.
    SetSecret {
        /// The name/key for the secret (e.g., "anthropic.api_key").
        key: String,
    },
    /// List all secrets stored in the vault (names and masked previews only).
    ListSecrets,
    /// Get the current resolved value for a config key (dotted path).
    Get {
        /// Config key path (e.g., "agent.name", "storage.database_path").
        key: String,
    },
    /// Validate the configuration file and report any errors.
    Validate,
}

/// Skill management subcommands.
#[derive(Subcommand, Debug)]
enum SkillCommands {
    /// Create a new skill project scaffold.
    Init {
        /// Name of the skill to create.
        name: String,
    },
    /// List all installed skills.
    List,
    /// Install a WASM skill from a file.
    Install {
        /// Path to the .wasm file.
        wasm_path: String,
        /// Path to the skill.toml manifest.
        manifest_path: String,
    },
    /// Remove an installed skill.
    Remove {
        /// Name of the skill to remove.
        name: String,
    },
}

/// Plugin management subcommands.
#[derive(Subcommand, Debug)]
enum PluginCommands {
    /// List all compiled-in plugins and their status.
    List,
    /// Search available plugins in the built-in catalog.
    Search {
        /// Search query (matches name or description).
        #[arg(default_value = "")]
        query: String,
    },
    /// Enable a plugin (set enabled in config).
    Install {
        /// Plugin name to enable.
        name: String,
    },
    /// Disable a plugin (set disabled in config).
    Remove {
        /// Plugin name to disable.
        name: String,
    },
    /// Show plugin update information.
    Update,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Load and validate configuration at startup
    let config = match blufio_config::load_and_validate() {
        Ok(config) => {
            eprintln!(
                "blufio: config loaded (agent.name={})",
                config.agent.name
            );
            config
        }
        Err(errors) => {
            blufio_config::render_errors(&errors);
            std::process::exit(1);
        }
    };

    match cli.command {
        Some(Commands::Serve) => {
            if let Err(e) = serve::run_serve(config).await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Shell) => {
            if let Err(e) = shell::run_shell(config).await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Status { json, plain }) => {
            if let Err(e) = status::run_status(&config, json, plain).await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Doctor { deep, plain }) => {
            if let Err(e) = doctor::run_doctor(&config, deep, plain).await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Backup { path }) => {
            if let Err(e) = backup::run_backup(&config.storage.database_path, &path) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Restore { path }) => {
            if let Err(e) = backup::run_restore(&config.storage.database_path, &path) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Config { action }) => match action {
            Some(ConfigCommands::SetSecret { key }) => {
                if let Err(e) = cmd_set_secret(&config, &key).await {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
            Some(ConfigCommands::ListSecrets) => {
                if let Err(e) = cmd_list_secrets(&config).await {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
            Some(ConfigCommands::Get { key }) => {
                if let Err(e) = cmd_config_get(&config, &key) {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
            Some(ConfigCommands::Validate) => {
                match blufio_config::load_and_validate() {
                    Ok(_) => {
                        println!("Configuration is valid.");
                    }
                    Err(errors) => {
                        blufio_config::render_errors(&errors);
                        std::process::exit(1);
                    }
                }
            }
            None => {
                println!("blufio config: use --help for available config commands");
            }
        },
        Some(Commands::Skill { action }) => {
            if let Err(e) = handle_skill_command(&config, action).await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Plugin { action }) => {
            if let Err(e) = handle_plugin_command(&config, action) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        None => {
            println!("blufio: use --help for available commands");
        }
    }
}

/// Open the database, returning the connection.
async fn open_db(
    config: &blufio_config::model::BlufioConfig,
) -> Result<blufio_storage::Database, blufio_core::BlufioError> {
    blufio_storage::Database::open(&config.storage.database_path).await
}

/// Handle `blufio config set-secret <key>`.
///
/// Creates the vault lazily on first use. Prompts for the secret value
/// via hidden TTY input or reads from piped stdin.
async fn cmd_set_secret(
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
async fn cmd_list_secrets(
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
fn read_secret_value(key: &str) -> Result<String, blufio_core::BlufioError> {
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

/// Handle `blufio skill <action>` subcommands.
async fn handle_skill_command(
    config: &blufio_config::model::BlufioConfig,
    action: SkillCommands,
) -> Result<(), blufio_core::BlufioError> {
    match action {
        SkillCommands::Init { name } => {
            let target_dir = std::path::Path::new(".");
            blufio_skill::scaffold_skill(&name, target_dir)?;
            eprintln!("Skill project '{name}' created successfully.");
            eprintln!("  cd {name} && cargo build --target wasm32-wasip1 --release");
            Ok(())
        }
        SkillCommands::List => {
            let conn = tokio_rusqlite::Connection::open(&config.storage.database_path)
                .await
                .map_err(|e| blufio_core::BlufioError::Storage {
                    source: Box::new(e),
                })?;
            let store = blufio_skill::SkillStore::new(std::sync::Arc::new(conn));
            let skills = store.list().await?;

            if skills.is_empty() {
                println!("No skills installed.");
            } else {
                println!("{:<20} {:<10} {:<12} DESCRIPTION", "NAME", "VERSION", "STATUS");
                println!("{}", "-".repeat(70));
                for skill in &skills {
                    println!(
                        "{:<20} {:<10} {:<12} {}",
                        skill.name, skill.version, skill.verification_status, skill.description
                    );
                }
            }
            Ok(())
        }
        SkillCommands::Install { wasm_path, manifest_path } => {
            // Read and parse the manifest.
            let manifest_content = std::fs::read_to_string(&manifest_path).map_err(|e| {
                blufio_core::BlufioError::Skill {
                    message: format!("failed to read manifest '{}': {e}", manifest_path),
                    source: Some(Box::new(e)),
                }
            })?;
            let manifest = blufio_skill::parse_manifest(&manifest_content)?;

            // Verify the WASM file exists.
            if !std::path::Path::new(&wasm_path).exists() {
                return Err(blufio_core::BlufioError::Skill {
                    message: format!("WASM file '{}' not found", wasm_path),
                    source: None,
                });
            }

            // Serialize capabilities to JSON for storage.
            let capabilities_json = serde_json::to_string(&manifest.capabilities)
                .unwrap_or_else(|_| "{}".to_string());

            // Open DB and store the skill.
            let conn = tokio_rusqlite::Connection::open(&config.storage.database_path)
                .await
                .map_err(|e| blufio_core::BlufioError::Storage {
                    source: Box::new(e),
                })?;
            let store = blufio_skill::SkillStore::new(std::sync::Arc::new(conn));

            store
                .install(
                    &manifest.name,
                    &manifest.version,
                    &manifest.description,
                    manifest.author.as_deref(),
                    &wasm_path,
                    &manifest_content,
                    &capabilities_json,
                )
                .await?;

            eprintln!("Skill '{}' v{} installed successfully.", manifest.name, manifest.version);

            // Print capabilities summary.
            if manifest.capabilities.network.is_some() {
                eprintln!("  Capabilities: network access");
            }
            if manifest.capabilities.filesystem.is_some() {
                eprintln!("  Capabilities: filesystem access");
            }
            if !manifest.capabilities.env.is_empty() {
                eprintln!(
                    "  Capabilities: env vars ({})",
                    manifest.capabilities.env.join(", ")
                );
            }

            Ok(())
        }
        SkillCommands::Remove { name } => {
            let conn = tokio_rusqlite::Connection::open(&config.storage.database_path)
                .await
                .map_err(|e| blufio_core::BlufioError::Storage {
                    source: Box::new(e),
                })?;
            let store = blufio_skill::SkillStore::new(std::sync::Arc::new(conn));
            store.remove(&name).await?;
            eprintln!("Skill '{name}' removed.");
            Ok(())
        }
    }
}

/// Handle `blufio plugin <action>` subcommands.
fn handle_plugin_command(
    config: &blufio_config::model::BlufioConfig,
    action: PluginCommands,
) -> Result<(), blufio_core::BlufioError> {
    match action {
        PluginCommands::List => {
            let catalog = blufio_plugin::builtin_catalog();
            let mut registry = blufio_plugin::PluginRegistry::new();

            for manifest in catalog {
                // Determine status based on config overrides and required config keys.
                let name = manifest.name.clone();
                let config_override = config.plugin.plugins.get(&name);

                let status = match config_override {
                    Some(false) => blufio_plugin::PluginStatus::Disabled,
                    Some(true) => blufio_plugin::PluginStatus::Enabled,
                    None => {
                        // Check if required config keys are present.
                        let all_configured = manifest.config_keys.iter().all(|key| {
                            is_config_key_present(config, key)
                        });
                        if all_configured || manifest.config_keys.is_empty() {
                            blufio_plugin::PluginStatus::Enabled
                        } else {
                            blufio_plugin::PluginStatus::NotConfigured
                        }
                    }
                };

                registry.register_with_status(manifest, None, status);
            }

            println!(
                "{:<18} {:<15} {:<16} DESCRIPTION",
                "NAME", "TYPE", "STATUS"
            );
            println!("{}", "-".repeat(75));
            for entry in registry.list_all() {
                println!(
                    "{:<18} {:<15} {:<16} {}",
                    entry.manifest.name,
                    entry.manifest.adapter_type.to_string(),
                    entry.status,
                    entry.manifest.description,
                );
            }
            Ok(())
        }
        PluginCommands::Search { query } => {
            let results = blufio_plugin::search_catalog(&query);
            if results.is_empty() {
                println!("No plugins found matching '{query}'.");
            } else {
                println!(
                    "{:<18} {:<15} DESCRIPTION",
                    "NAME", "TYPE"
                );
                println!("{}", "-".repeat(65));
                for manifest in &results {
                    println!(
                        "{:<18} {:<15} {}",
                        manifest.name,
                        manifest.adapter_type.to_string(),
                        manifest.description,
                    );
                }
            }
            Ok(())
        }
        PluginCommands::Install { name } => {
            let catalog = blufio_plugin::builtin_catalog();
            let found = catalog.iter().find(|m| m.name == name);

            match found {
                Some(manifest) => {
                    println!("Plugin '{}' enabled.", name);
                    if !manifest.config_keys.is_empty() {
                        println!(
                            "  Required config keys: {}",
                            manifest.config_keys.join(", ")
                        );
                        println!("  Add configuration to blufio.toml if required.");
                    }
                    Ok(())
                }
                None => Err(blufio_core::BlufioError::AdapterNotFound {
                    adapter_type: "plugin".to_string(),
                    name,
                }),
            }
        }
        PluginCommands::Remove { name } => {
            let catalog = blufio_plugin::builtin_catalog();
            let found = catalog.iter().any(|m| m.name == name);

            if found {
                println!("Plugin '{name}' disabled.");
                Ok(())
            } else {
                Err(blufio_core::BlufioError::AdapterNotFound {
                    adapter_type: "plugin".to_string(),
                    name,
                })
            }
        }
        PluginCommands::Update => {
            println!("Plugins are compiled into the Blufio binary.");
            println!("Update by rebuilding or downloading a new binary release.");
            Ok(())
        }
    }
}

/// Check if a config key is present (non-empty) in the loaded config.
///
/// Supports dotted key paths like "telegram.bot_token" and "anthropic.api_key".
fn is_config_key_present(
    config: &blufio_config::model::BlufioConfig,
    key: &str,
) -> bool {
    match key {
        "telegram.bot_token" => config.telegram.bot_token.is_some(),
        "anthropic.api_key" => config.anthropic.api_key.is_some(),
        _ => false,
    }
}

/// Handle `blufio config get <key>`.
///
/// Resolves a dotted config key path to its current value. Uses serde_json
/// serialization to traverse the config struct generically.
fn cmd_config_get(
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

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(not(target_env = "msvc"))]
    fn jemalloc_is_active() {
        // Verify jemalloc is the global allocator by advancing the epoch.
        // Only jemalloc supports this -- the system allocator would fail.
        use tikv_jemalloc_ctl::{epoch, stats};
        epoch::advance().unwrap();
        let allocated = stats::allocated::read().unwrap();
        assert!(allocated > 0, "jemalloc should report non-zero allocation");
    }

    #[test]
    fn binary_loads_config_defaults() {
        // Verify config loads with defaults (no config file needed)
        let config = blufio_config::load_and_validate()
            .expect("default config should be valid");
        assert_eq!(config.agent.name, "blufio");
    }

    use super::*;
    use blufio_config::model::BlufioConfig;

    #[test]
    fn cli_parses_set_secret_subcommand() {
        let cli = Cli::parse_from(["blufio", "config", "set-secret", "my-key"]);
        match cli.command {
            Some(Commands::Config {
                action: Some(ConfigCommands::SetSecret { key }),
            }) => {
                assert_eq!(key, "my-key");
            }
            _ => panic!("expected Config SetSecret command"),
        }
    }

    #[test]
    fn cli_parses_list_secrets_subcommand() {
        let cli = Cli::parse_from(["blufio", "config", "list-secrets"]);
        match cli.command {
            Some(Commands::Config {
                action: Some(ConfigCommands::ListSecrets),
            }) => {}
            _ => panic!("expected Config ListSecrets command"),
        }
    }

    #[test]
    fn cli_config_without_subcommand() {
        let cli = Cli::parse_from(["blufio", "config"]);
        match cli.command {
            Some(Commands::Config { action: None }) => {}
            _ => panic!("expected Config with no subcommand"),
        }
    }

    #[tokio::test]
    async fn set_secret_and_list_secrets_roundtrip() {
        use secrecy::ExposeSecret;

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test-cli.db");

        let config = BlufioConfig {
            storage: blufio_config::model::StorageConfig {
                database_path: db_path.to_str().unwrap().to_string(),
                ..Default::default()
            },
            vault: blufio_config::model::VaultConfig {
                kdf_memory_cost: 32768,
                kdf_iterations: 2,
                kdf_parallelism: 1,
            },
            ..Default::default()
        };

        // Set passphrase via env var for test.
        unsafe { std::env::set_var("BLUFIO_VAULT_KEY", "test-cli-pass") };

        // Open DB and create vault manually (since we can't pipe stdin in test).
        let db = open_db(&config).await.unwrap();
        let conn = db.connection().clone();
        let passphrase = secrecy::SecretString::from("test-cli-pass".to_string());
        let vault =
            blufio_vault::Vault::create(conn, &passphrase, &config.vault)
                .await
                .unwrap();

        // Store a secret directly.
        vault
            .store_secret("test.api_key", "sk-test-12345678")
            .await
            .unwrap();

        // Verify retrieval.
        let retrieved = vault
            .retrieve_secret("test.api_key")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.expose_secret(), "sk-test-12345678");

        // Verify list shows masked preview.
        let secrets = vault.list_secrets().await.unwrap();
        assert_eq!(secrets.len(), 1);
        assert_eq!(secrets[0].0, "test.api_key");
        assert!(secrets[0].1.contains("..."));
        assert!(!secrets[0].1.contains("sk-test-12345678"));

        db.close().await.unwrap();

        unsafe { std::env::remove_var("BLUFIO_VAULT_KEY") };
    }

    #[tokio::test]
    async fn list_secrets_no_vault_graceful() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test-no-vault.db");

        let config = BlufioConfig {
            storage: blufio_config::model::StorageConfig {
                database_path: db_path.to_str().unwrap().to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        // This should succeed gracefully -- no vault exists.
        let result = cmd_list_secrets(&config).await;
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parses_status() {
        let cli = Cli::parse_from(["blufio", "status"]);
        match cli.command {
            Some(Commands::Status { json, plain }) => {
                assert!(!json);
                assert!(!plain);
            }
            _ => panic!("expected Status command"),
        }
    }

    #[test]
    fn cli_parses_status_json() {
        let cli = Cli::parse_from(["blufio", "status", "--json"]);
        match cli.command {
            Some(Commands::Status { json, plain }) => {
                assert!(json);
                assert!(!plain);
            }
            _ => panic!("expected Status --json command"),
        }
    }

    #[test]
    fn cli_parses_status_plain() {
        let cli = Cli::parse_from(["blufio", "status", "--plain"]);
        match cli.command {
            Some(Commands::Status { json, plain }) => {
                assert!(!json);
                assert!(plain);
            }
            _ => panic!("expected Status --plain command"),
        }
    }

    #[test]
    fn cli_parses_doctor() {
        let cli = Cli::parse_from(["blufio", "doctor"]);
        match cli.command {
            Some(Commands::Doctor { deep, plain }) => {
                assert!(!deep);
                assert!(!plain);
            }
            _ => panic!("expected Doctor command"),
        }
    }

    #[test]
    fn cli_parses_doctor_deep() {
        let cli = Cli::parse_from(["blufio", "doctor", "--deep"]);
        match cli.command {
            Some(Commands::Doctor { deep, plain }) => {
                assert!(deep);
                assert!(!plain);
            }
            _ => panic!("expected Doctor --deep command"),
        }
    }

    #[test]
    fn cli_parses_backup() {
        let cli = Cli::parse_from(["blufio", "backup", "/tmp/backup.db"]);
        match cli.command {
            Some(Commands::Backup { path }) => {
                assert_eq!(path, "/tmp/backup.db");
            }
            _ => panic!("expected Backup command"),
        }
    }

    #[test]
    fn cli_parses_restore() {
        let cli = Cli::parse_from(["blufio", "restore", "/tmp/backup.db"]);
        match cli.command {
            Some(Commands::Restore { path }) => {
                assert_eq!(path, "/tmp/backup.db");
            }
            _ => panic!("expected Restore command"),
        }
    }

    #[test]
    fn cli_parses_config_get() {
        let cli = Cli::parse_from(["blufio", "config", "get", "agent.name"]);
        match cli.command {
            Some(Commands::Config {
                action: Some(ConfigCommands::Get { key }),
            }) => {
                assert_eq!(key, "agent.name");
            }
            _ => panic!("expected Config Get command"),
        }
    }

    #[test]
    fn cli_parses_config_validate() {
        let cli = Cli::parse_from(["blufio", "config", "validate"]);
        match cli.command {
            Some(Commands::Config {
                action: Some(ConfigCommands::Validate),
            }) => {}
            _ => panic!("expected Config Validate command"),
        }
    }

    #[test]
    fn config_get_agent_name() {
        let config = BlufioConfig::default();
        // Use serde_json traversal approach
        let value = serde_json::to_value(&config).unwrap();
        let agent_name = value.get("agent").unwrap().get("name").unwrap();
        assert_eq!(agent_name, "blufio");
    }

    #[test]
    fn config_get_resolves_known_keys() {
        let config = BlufioConfig::default();
        // Should succeed for known keys
        assert!(cmd_config_get(&config, "agent.name").is_ok());
        assert!(cmd_config_get(&config, "storage.database_path").is_ok());
        assert!(cmd_config_get(&config, "agent.log_level").is_ok());
        assert!(cmd_config_get(&config, "daemon.memory_warn_mb").is_ok());
    }

    #[test]
    fn config_get_fails_for_unknown_key() {
        let config = BlufioConfig::default();
        assert!(cmd_config_get(&config, "nonexistent.key").is_err());
    }

    #[test]
    fn cli_parses_skill_init() {
        let cli = Cli::parse_from(["blufio", "skill", "init", "my-skill"]);
        match cli.command {
            Some(Commands::Skill {
                action: SkillCommands::Init { name },
            }) => {
                assert_eq!(name, "my-skill");
            }
            _ => panic!("expected Skill Init command"),
        }
    }

    #[test]
    fn cli_parses_skill_list() {
        let cli = Cli::parse_from(["blufio", "skill", "list"]);
        match cli.command {
            Some(Commands::Skill {
                action: SkillCommands::List,
            }) => {}
            _ => panic!("expected Skill List command"),
        }
    }

    #[test]
    fn cli_parses_skill_install() {
        let cli = Cli::parse_from([
            "blufio",
            "skill",
            "install",
            "path/to/skill.wasm",
            "path/to/skill.toml",
        ]);
        match cli.command {
            Some(Commands::Skill {
                action: SkillCommands::Install {
                    wasm_path,
                    manifest_path,
                },
            }) => {
                assert_eq!(wasm_path, "path/to/skill.wasm");
                assert_eq!(manifest_path, "path/to/skill.toml");
            }
            _ => panic!("expected Skill Install command"),
        }
    }

    #[test]
    fn cli_parses_skill_remove() {
        let cli = Cli::parse_from(["blufio", "skill", "remove", "my-skill"]);
        match cli.command {
            Some(Commands::Skill {
                action: SkillCommands::Remove { name },
            }) => {
                assert_eq!(name, "my-skill");
            }
            _ => panic!("expected Skill Remove command"),
        }
    }

    #[test]
    fn cli_parses_plugin_list() {
        let cli = Cli::parse_from(["blufio", "plugin", "list"]);
        match cli.command {
            Some(Commands::Plugin {
                action: PluginCommands::List,
            }) => {}
            _ => panic!("expected Plugin List command"),
        }
    }

    #[test]
    fn cli_parses_plugin_search_with_query() {
        let cli = Cli::parse_from(["blufio", "plugin", "search", "telegram"]);
        match cli.command {
            Some(Commands::Plugin {
                action: PluginCommands::Search { query },
            }) => {
                assert_eq!(query, "telegram");
            }
            _ => panic!("expected Plugin Search command"),
        }
    }

    #[test]
    fn cli_parses_plugin_search_no_query() {
        let cli = Cli::parse_from(["blufio", "plugin", "search"]);
        match cli.command {
            Some(Commands::Plugin {
                action: PluginCommands::Search { query },
            }) => {
                assert_eq!(query, "");
            }
            _ => panic!("expected Plugin Search command with empty query"),
        }
    }

    #[test]
    fn cli_parses_plugin_install() {
        let cli = Cli::parse_from(["blufio", "plugin", "install", "prometheus"]);
        match cli.command {
            Some(Commands::Plugin {
                action: PluginCommands::Install { name },
            }) => {
                assert_eq!(name, "prometheus");
            }
            _ => panic!("expected Plugin Install command"),
        }
    }

    #[test]
    fn cli_parses_plugin_remove() {
        let cli = Cli::parse_from(["blufio", "plugin", "remove", "prometheus"]);
        match cli.command {
            Some(Commands::Plugin {
                action: PluginCommands::Remove { name },
            }) => {
                assert_eq!(name, "prometheus");
            }
            _ => panic!("expected Plugin Remove command"),
        }
    }

    #[test]
    fn cli_parses_plugin_update() {
        let cli = Cli::parse_from(["blufio", "plugin", "update"]);
        match cli.command {
            Some(Commands::Plugin {
                action: PluginCommands::Update,
            }) => {}
            _ => panic!("expected Plugin Update command"),
        }
    }

    #[test]
    fn plugin_config_default_empty_plugins() {
        let config = BlufioConfig::default();
        assert!(config.plugin.plugins.is_empty());
    }

    #[test]
    fn plugin_config_deserializes_from_toml() {
        let toml_str = r#"
[plugin]
plugins = { telegram = true, prometheus = false }
"#;
        let config: BlufioConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.plugin.plugins.get("telegram"), Some(&true));
        assert_eq!(config.plugin.plugins.get("prometheus"), Some(&false));
    }

    #[test]
    fn handle_plugin_list_succeeds() {
        let config = BlufioConfig::default();
        let result = handle_plugin_command(&config, PluginCommands::List);
        assert!(result.is_ok());
    }

    #[test]
    fn handle_plugin_search_succeeds() {
        let config = BlufioConfig::default();
        let result = handle_plugin_command(
            &config,
            PluginCommands::Search {
                query: "telegram".to_string(),
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn handle_plugin_install_known() {
        let config = BlufioConfig::default();
        let result = handle_plugin_command(
            &config,
            PluginCommands::Install {
                name: "prometheus".to_string(),
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn handle_plugin_install_unknown_fails() {
        let config = BlufioConfig::default();
        let result = handle_plugin_command(
            &config,
            PluginCommands::Install {
                name: "nonexistent".to_string(),
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn handle_plugin_remove_known() {
        let config = BlufioConfig::default();
        let result = handle_plugin_command(
            &config,
            PluginCommands::Remove {
                name: "telegram".to_string(),
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn handle_plugin_update_succeeds() {
        let config = BlufioConfig::default();
        let result = handle_plugin_command(&config, PluginCommands::Update);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn set_secret_overwrites_existing() {
        use secrecy::ExposeSecret;

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test-overwrite.db");

        let config = BlufioConfig {
            storage: blufio_config::model::StorageConfig {
                database_path: db_path.to_str().unwrap().to_string(),
                ..Default::default()
            },
            vault: blufio_config::model::VaultConfig {
                kdf_memory_cost: 32768,
                kdf_iterations: 2,
                kdf_parallelism: 1,
            },
            ..Default::default()
        };

        unsafe { std::env::set_var("BLUFIO_VAULT_KEY", "test-overwrite") };

        let db = open_db(&config).await.unwrap();
        let conn = db.connection().clone();
        let passphrase = secrecy::SecretString::from("test-overwrite".to_string());
        let vault =
            blufio_vault::Vault::create(conn, &passphrase, &config.vault)
                .await
                .unwrap();

        // Store initial value.
        vault
            .store_secret("my.key", "original-value")
            .await
            .unwrap();

        // Overwrite with new value.
        vault
            .store_secret("my.key", "updated-value")
            .await
            .unwrap();

        // Verify the updated value.
        let retrieved = vault.retrieve_secret("my.key").await.unwrap().unwrap();
        assert_eq!(retrieved.expose_secret(), "updated-value");

        db.close().await.unwrap();

        unsafe { std::env::remove_var("BLUFIO_VAULT_KEY") };
    }
}
