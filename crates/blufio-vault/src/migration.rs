// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Auto-migration of plaintext secrets from TOML config into the encrypted vault.
//!
//! Detects known secret fields in the config, stores them in the vault, then
//! rewrites the config file with the secrets removed.

use std::path::Path;

use blufio_config::model::BlufioConfig;
use blufio_core::BlufioError;
use tracing::{info, warn};

use crate::vault::Vault;

/// Report of what the migration did.
#[derive(Debug, Default)]
pub struct MigrationReport {
    /// Names of secrets that were migrated to vault.
    pub migrated: Vec<String>,
    /// Names of secrets already in vault (skipped).
    pub skipped: Vec<String>,
    /// Non-fatal warnings (e.g., config rewrite failure).
    pub warnings: Vec<String>,
}

/// Scan config for plaintext secrets and migrate them to the vault.
///
/// This is called on startup after the vault is unlocked. Known secret fields:
/// - `telegram.bot_token`
/// - `anthropic.api_key`
///
/// Returns a report of migrated and skipped secrets.
pub async fn migrate_plaintext_secrets(
    config: &BlufioConfig,
    config_path: &Path,
    vault: &Vault,
) -> Result<MigrationReport, BlufioError> {
    let mut report = MigrationReport::default();

    // Collect secrets to migrate.
    let mut secrets_to_migrate: Vec<(&str, String)> = Vec::new();

    if let Some(ref token) = config.telegram.bot_token
        && !token.is_empty()
    {
        secrets_to_migrate.push(("telegram.bot_token", token.clone()));
    }

    if let Some(ref key) = config.anthropic.api_key
        && !key.is_empty()
    {
        secrets_to_migrate.push(("anthropic.api_key", key.clone()));
    }

    if secrets_to_migrate.is_empty() {
        info!("no plaintext secrets found in config -- nothing to migrate");
        return Ok(report);
    }

    // Migrate each secret.
    for (name, value) in &secrets_to_migrate {
        // Check if already in vault.
        let existing = vault.retrieve_secret(name).await?;
        if existing.is_some() {
            report.skipped.push(name.to_string());
            info!(name = %name, "secret already in vault -- skipping migration");
            continue;
        }

        // Store in vault.
        vault.store_secret(name, value).await?;
        report.migrated.push(name.to_string());
        warn!(name = %name, "migrated plaintext secret from config to vault");
    }

    // Rewrite config file to remove secrets.
    if !report.migrated.is_empty()
        && let Err(e) = rewrite_config_without_secrets(config_path, &report.migrated)
    {
        let warning = format!(
            "failed to rewrite config file to remove plaintext secrets: {e}. \
             Secrets are safely stored in the vault and will be caught again on next startup."
        );
        warn!("{}", warning);
        report.warnings.push(warning);
    }

    Ok(report)
}

/// Rewrite the TOML config file, removing the specified secret fields.
fn rewrite_config_without_secrets(
    config_path: &Path,
    secret_names: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(config_path)?;
    let mut doc: toml::Value = content.parse()?;

    for name in secret_names {
        // Parse "section.field" format.
        let parts: Vec<&str> = name.split('.').collect();
        if parts.len() == 2
            && let Some(table) = doc.get_mut(parts[0]).and_then(|v| v.as_table_mut())
        {
            table.remove(parts[1]);
        }
    }

    let new_content = toml::to_string_pretty(&doc)?;
    std::fs::write(config_path, new_content)?;
    info!(path = %config_path.display(), "config file rewritten without plaintext secrets");

    Ok(())
}

/// Check vault state and prepare for agent startup.
///
/// - If no vault exists: return `Ok(None)` (vault created lazily on first `set-secret`).
/// - If vault exists and passphrase available: unlock and return `Some(Vault)`.
/// - If vault exists but no passphrase: return error.
pub async fn vault_startup_check(
    conn: tokio_rusqlite::Connection,
    config: &blufio_config::model::VaultConfig,
) -> Result<Option<Vault>, BlufioError> {
    if !Vault::exists(&conn).await? {
        info!("no vault found -- vault will be created on first set-secret");
        return Ok(None);
    }

    // Vault exists -- need passphrase.
    let passphrase = crate::prompt::get_vault_passphrase()?;
    let vault = Vault::unlock(conn, &passphrase, config).await?;
    info!("vault unlocked successfully");
    Ok(Some(vault))
}

#[cfg(test)]
mod tests {
    use super::*;
    use blufio_config::model::VaultConfig;
    use secrecy::SecretString;
    use tempfile::tempdir;

    fn test_config() -> VaultConfig {
        VaultConfig {
            kdf_memory_cost: 32768,
            kdf_iterations: 2,
            kdf_parallelism: 1,
        }
    }

    async fn open_test_db() -> (tokio_rusqlite::Connection, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("migration_test.db");
        let db = blufio_storage::Database::open(db_path.to_str().unwrap())
            .await
            .unwrap();
        let conn = db.connection().clone();
        (conn, dir)
    }

    #[tokio::test]
    async fn migrate_detects_and_stores_secrets() {
        let (conn, dir) = open_test_db().await;
        let vault_config = test_config();
        let passphrase = SecretString::from("test".to_string());
        let vault = Vault::create(conn, &passphrase, &vault_config)
            .await
            .unwrap();

        // Create a minimal config with a plaintext bot token.
        let config_path = dir.path().join("blufio.toml");
        std::fs::write(
            &config_path,
            r#"
[telegram]
bot_token = "123456:ABCdef"

[anthropic]
api_key = "sk-ant-test-key"
"#,
        )
        .unwrap();

        let config = BlufioConfig {
            telegram: blufio_config::model::TelegramConfig {
                bot_token: Some("123456:ABCdef".to_string()),
                ..Default::default()
            },
            anthropic: blufio_config::model::AnthropicConfig {
                api_key: Some("sk-ant-test-key".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };

        let report = migrate_plaintext_secrets(&config, &config_path, &vault)
            .await
            .unwrap();

        assert_eq!(report.migrated.len(), 2);
        assert!(report.migrated.contains(&"telegram.bot_token".to_string()));
        assert!(report.migrated.contains(&"anthropic.api_key".to_string()));

        // Verify secrets are in vault.
        use secrecy::ExposeSecret;
        let token = vault
            .retrieve_secret("telegram.bot_token")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(token.expose_secret(), "123456:ABCdef");
    }

    #[tokio::test]
    async fn migrate_is_idempotent() {
        let (conn, dir) = open_test_db().await;
        let vault_config = test_config();
        let passphrase = SecretString::from("test".to_string());
        let vault = Vault::create(conn, &passphrase, &vault_config)
            .await
            .unwrap();

        let config_path = dir.path().join("blufio.toml");
        std::fs::write(&config_path, "[telegram]\nbot_token = \"abc\"\n").unwrap();

        let config = BlufioConfig {
            telegram: blufio_config::model::TelegramConfig {
                bot_token: Some("abc".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };

        // First migration.
        let report1 = migrate_plaintext_secrets(&config, &config_path, &vault)
            .await
            .unwrap();
        assert_eq!(report1.migrated.len(), 1);

        // Second migration -- should skip.
        let report2 = migrate_plaintext_secrets(&config, &config_path, &vault)
            .await
            .unwrap();
        assert_eq!(report2.migrated.len(), 0);
        assert_eq!(report2.skipped.len(), 1);
    }

    #[tokio::test]
    async fn vault_startup_check_no_vault_returns_none() {
        let (conn, _dir) = open_test_db().await;
        let vault_config = test_config();

        let result = vault_startup_check(conn, &vault_config).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn vault_startup_check_with_env_var() {
        let (conn, _dir) = open_test_db().await;
        let vault_config = test_config();
        let passphrase = SecretString::from("test-startup".to_string());

        // Create vault first.
        let _vault = Vault::create(conn.clone(), &passphrase, &vault_config)
            .await
            .unwrap();

        // Set env var and check startup.
        unsafe { std::env::set_var(crate::prompt::VAULT_KEY_ENV_VAR, "test-startup") };
        let result = vault_startup_check(conn, &vault_config).await;
        unsafe { std::env::remove_var(crate::prompt::VAULT_KEY_ENV_VAR) };

        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[tokio::test]
    async fn vault_startup_check_vault_exists_no_passphrase_fails() {
        let (conn, _dir) = open_test_db().await;
        let vault_config = test_config();
        let passphrase = SecretString::from("test-fail".to_string());

        // Create vault.
        let _vault = Vault::create(conn.clone(), &passphrase, &vault_config)
            .await
            .unwrap();

        // Remove env var and ensure not a terminal.
        unsafe { std::env::remove_var(crate::prompt::VAULT_KEY_ENV_VAR) };

        let result = vault_startup_check(conn, &vault_config).await;
        assert!(result.is_err());
    }
}
