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

mod serve;
mod shell;

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
    /// Manage Blufio configuration and vault secrets.
    Config {
        #[command(subcommand)]
        action: Option<ConfigCommands>,
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
            None => {
                println!("blufio config: use --help for available config commands");
            }
        },
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
