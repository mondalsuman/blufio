// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Vault lifecycle: create, unlock, store, retrieve, list, and delete secrets.
//!
//! The vault uses a key-wrapping pattern:
//! - A random master key encrypts all secrets (stored in vault_entries).
//! - The master key itself is encrypted with a key derived from the user's
//!   passphrase via Argon2id (stored in vault_meta as wrapped_master_key).
//! - Changing the passphrase only re-wraps the master key; individual secrets
//!   are never re-encrypted.

use blufio_config::model::VaultConfig;
use blufio_core::BlufioError;
use rusqlite::params;
use secrecy::{ExposeSecret, SecretString};
use tracing::{debug, info};
use zeroize::Zeroizing;

use crate::crypto;
use crate::kdf;

/// The unlocked vault, holding the master key in memory.
///
/// Debug output intentionally omits the master key for security.
pub struct Vault {
    /// The unwrapped master key -- only in memory, never on disk.
    master_key: Zeroizing<[u8; 32]>,
    /// Database connection for vault_entries and vault_meta tables.
    conn: tokio_rusqlite::Connection,
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault")
            .field("master_key", &"[REDACTED]")
            .finish()
    }
}

impl Vault {
    /// Check if a vault exists (has a wrapped master key in vault_meta).
    pub async fn exists(conn: &tokio_rusqlite::Connection) -> Result<bool, BlufioError> {
        conn.call(|conn| -> Result<bool, rusqlite::Error> {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM vault_meta WHERE key = 'wrapped_master_key'",
                [],
                |row| row.get(0),
            )?;
            Ok(count > 0)
        })
        .await
        .map_err(map_tr_err)
    }

    /// Create a new vault with a random master key wrapped by the passphrase.
    pub async fn create(
        conn: tokio_rusqlite::Connection,
        passphrase: &SecretString,
        config: &VaultConfig,
    ) -> Result<Self, BlufioError> {
        // Generate random master key.
        let master_key = crypto::generate_random_key()?;

        // Generate salt and derive wrapping key.
        let salt = kdf::generate_salt()?;
        let wrapping_key = kdf::derive_key(
            passphrase.expose_secret().as_bytes(),
            &salt,
            config.kdf_memory_cost,
            config.kdf_iterations,
            config.kdf_parallelism,
        )?;

        // Wrap master key with the passphrase-derived key.
        let (wrapped_master_key, wrap_nonce) = crypto::seal(&wrapping_key, &master_key)?;

        // Store KDF params as JSON.
        let kdf_params = serde_json::json!({
            "memory_cost": config.kdf_memory_cost,
            "iterations": config.kdf_iterations,
            "parallelism": config.kdf_parallelism,
        });
        let kdf_params_bytes = kdf_params.to_string().into_bytes();

        // Store in vault_meta.
        let salt_vec = salt.to_vec();
        let wrap_nonce_vec = wrap_nonce.to_vec();
        conn.call(move |conn| -> Result<(), rusqlite::Error> {
            let tx = conn.transaction()?;
            tx.execute(
                "INSERT OR REPLACE INTO vault_meta (key, value) VALUES ('wrapped_master_key', ?1)",
                params![wrapped_master_key],
            )?;
            tx.execute(
                "INSERT OR REPLACE INTO vault_meta (key, value) VALUES ('master_key_nonce', ?1)",
                params![wrap_nonce_vec],
            )?;
            tx.execute(
                "INSERT OR REPLACE INTO vault_meta (key, value) VALUES ('kdf_salt', ?1)",
                params![salt_vec],
            )?;
            tx.execute(
                "INSERT OR REPLACE INTO vault_meta (key, value) VALUES ('kdf_params', ?1)",
                params![kdf_params_bytes],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
        .map_err(map_tr_err)?;

        info!("vault created");
        Ok(Self {
            master_key: Zeroizing::new(master_key),
            conn,
        })
    }

    /// Unlock an existing vault by deriving the wrapping key from the passphrase
    /// and decrypting the stored master key.
    pub async fn unlock(
        conn: tokio_rusqlite::Connection,
        passphrase: &SecretString,
        _config: &VaultConfig,
    ) -> Result<Self, BlufioError> {
        // Read vault_meta entries.
        let meta = conn
            .call(|conn| -> Result<VaultMeta, rusqlite::Error> {
                let wrapped_master_key: Vec<u8> = conn.query_row(
                    "SELECT value FROM vault_meta WHERE key = 'wrapped_master_key'",
                    [],
                    |row| row.get(0),
                )?;
                let nonce: Vec<u8> = conn.query_row(
                    "SELECT value FROM vault_meta WHERE key = 'master_key_nonce'",
                    [],
                    |row| row.get(0),
                )?;
                let salt: Vec<u8> = conn.query_row(
                    "SELECT value FROM vault_meta WHERE key = 'kdf_salt'",
                    [],
                    |row| row.get(0),
                )?;
                let kdf_params_bytes: Vec<u8> = conn.query_row(
                    "SELECT value FROM vault_meta WHERE key = 'kdf_params'",
                    [],
                    |row| row.get(0),
                )?;
                Ok(VaultMeta {
                    wrapped_master_key,
                    nonce,
                    salt,
                    kdf_params_bytes,
                })
            })
            .await
            .map_err(map_tr_err)?;

        // Parse KDF params.
        let kdf_params: serde_json::Value =
            serde_json::from_slice(&meta.kdf_params_bytes)
                .map_err(|e| BlufioError::Vault(format!("corrupted KDF params: {e}")))?;

        let memory_cost = kdf_params["memory_cost"]
            .as_u64()
            .ok_or_else(|| BlufioError::Vault("missing memory_cost in KDF params".to_string()))?
            as u32;
        let iterations = kdf_params["iterations"]
            .as_u64()
            .ok_or_else(|| BlufioError::Vault("missing iterations in KDF params".to_string()))?
            as u32;
        let parallelism = kdf_params["parallelism"]
            .as_u64()
            .ok_or_else(|| BlufioError::Vault("missing parallelism in KDF params".to_string()))?
            as u32;

        // Extract salt and nonce.
        let salt: [u8; 16] = meta
            .salt
            .try_into()
            .map_err(|_| BlufioError::Vault("corrupted salt (expected 16 bytes)".to_string()))?;
        let nonce: [u8; 12] = meta
            .nonce
            .try_into()
            .map_err(|_| BlufioError::Vault("corrupted nonce (expected 12 bytes)".to_string()))?;

        // Derive wrapping key.
        let wrapping_key = kdf::derive_key(
            passphrase.expose_secret().as_bytes(),
            &salt,
            memory_cost,
            iterations,
            parallelism,
        )?;

        // Unwrap master key.
        let master_key_bytes = crypto::open(&wrapping_key, &nonce, &meta.wrapped_master_key)
            .map_err(|_| {
                BlufioError::Vault(
                    "invalid passphrase or corrupted vault -- decryption failed".to_string(),
                )
            })?;

        let master_key: [u8; 32] = master_key_bytes.try_into().map_err(|_| {
            BlufioError::Vault("corrupted master key (expected 32 bytes)".to_string())
        })?;

        debug!("vault unlocked");
        Ok(Self {
            master_key: Zeroizing::new(master_key),
            conn,
        })
    }

    /// Store a secret in the vault, encrypted with the master key.
    pub async fn store_secret(&self, name: &str, plaintext: &str) -> Result<(), BlufioError> {
        let (ciphertext, nonce) = crypto::seal(&self.master_key, plaintext.as_bytes())?;
        let name_owned = name.to_string();
        let nonce_vec = nonce.to_vec();

        self.conn
            .call(move |conn| -> Result<(), rusqlite::Error> {
                conn.execute(
                    "INSERT OR REPLACE INTO vault_entries (name, ciphertext, nonce) VALUES (?1, ?2, ?3)",
                    params![name_owned, ciphertext, nonce_vec],
                )?;
                Ok(())
            })
            .await
            .map_err(map_tr_err)?;

        debug!(name = %name, "secret stored in vault");
        Ok(())
    }

    /// Retrieve and decrypt a secret from the vault.
    pub async fn retrieve_secret(
        &self,
        name: &str,
    ) -> Result<Option<SecretString>, BlufioError> {
        let name = name.to_string();
        type CipherNonce = (Vec<u8>, Vec<u8>);
        let entry = self
            .conn
            .call(move |conn| -> Result<Option<CipherNonce>, rusqlite::Error> {
                let mut stmt = conn.prepare(
                    "SELECT ciphertext, nonce FROM vault_entries WHERE name = ?1",
                )?;
                let result = stmt.query_row(params![name], |row| {
                    Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
                });
                match result {
                    Ok(entry) => Ok(Some(entry)),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                    Err(e) => Err(e),
                }
            })
            .await
            .map_err(map_tr_err)?;

        match entry {
            Some((ciphertext, nonce_vec)) => {
                let nonce: [u8; 12] = nonce_vec.try_into().map_err(|_| {
                    BlufioError::Vault("corrupted nonce in vault entry".to_string())
                })?;
                let plaintext = crypto::open(&self.master_key, &nonce, &ciphertext)?;
                let value = String::from_utf8(plaintext)
                    .map_err(|e| BlufioError::Vault(format!("decrypted value is not valid UTF-8: {e}")))?;
                Ok(Some(SecretString::from(value)))
            }
            None => Ok(None),
        }
    }

    /// List all secrets with masked previews.
    ///
    /// Returns `(name, masked_preview)` tuples. The preview shows the first
    /// few characters and last few characters: `"sk-...xyz"`.
    pub async fn list_secrets(&self) -> Result<Vec<(String, String)>, BlufioError> {
        let names: Vec<String> = self
            .conn
            .call(|conn| -> Result<Vec<String>, rusqlite::Error> {
                let mut stmt = conn.prepare("SELECT name FROM vault_entries ORDER BY name")?;
                let rows = stmt.query_map([], |row| row.get(0))?;
                let mut names = Vec::new();
                for row in rows {
                    names.push(row?);
                }
                Ok(names)
            })
            .await
            .map_err(map_tr_err)?;

        let mut result = Vec::new();
        for name in names {
            if let Some(secret) = self.retrieve_secret(&name).await? {
                let masked = mask_secret(secret.expose_secret());
                result.push((name, masked));
            } else {
                result.push((name, "[error: could not decrypt]".to_string()));
            }
        }

        Ok(result)
    }

    /// Delete a secret from the vault.
    pub async fn delete_secret(&self, name: &str) -> Result<(), BlufioError> {
        let name_owned = name.to_string();
        self.conn
            .call(move |conn| -> Result<(), rusqlite::Error> {
                conn.execute(
                    "DELETE FROM vault_entries WHERE name = ?1",
                    params![name_owned],
                )?;
                Ok(())
            })
            .await
            .map_err(map_tr_err)?;
        debug!(name = %name, "secret deleted from vault");
        Ok(())
    }

    /// Change the vault passphrase by re-wrapping the master key.
    ///
    /// Secrets are NOT re-encrypted -- only the master key wrapper changes.
    pub async fn change_passphrase(
        &self,
        new_passphrase: &SecretString,
        config: &VaultConfig,
    ) -> Result<(), BlufioError> {
        // Generate new salt and derive new wrapping key.
        let new_salt = kdf::generate_salt()?;
        let new_wrapping_key = kdf::derive_key(
            new_passphrase.expose_secret().as_bytes(),
            &new_salt,
            config.kdf_memory_cost,
            config.kdf_iterations,
            config.kdf_parallelism,
        )?;

        // Re-wrap master key.
        let (new_wrapped_key, new_nonce) = crypto::seal(&new_wrapping_key, &*self.master_key)?;

        // Store new KDF params.
        let kdf_params = serde_json::json!({
            "memory_cost": config.kdf_memory_cost,
            "iterations": config.kdf_iterations,
            "parallelism": config.kdf_parallelism,
        });
        let kdf_params_bytes = kdf_params.to_string().into_bytes();
        let new_salt_vec = new_salt.to_vec();
        let new_nonce_vec = new_nonce.to_vec();

        self.conn
            .call(move |conn| -> Result<(), rusqlite::Error> {
                let tx = conn.transaction()?;
                tx.execute(
                    "UPDATE vault_meta SET value = ?1 WHERE key = 'wrapped_master_key'",
                    params![new_wrapped_key],
                )?;
                tx.execute(
                    "UPDATE vault_meta SET value = ?1 WHERE key = 'master_key_nonce'",
                    params![new_nonce_vec],
                )?;
                tx.execute(
                    "UPDATE vault_meta SET value = ?1 WHERE key = 'kdf_salt'",
                    params![new_salt_vec],
                )?;
                tx.execute(
                    "UPDATE vault_meta SET value = ?1 WHERE key = 'kdf_params'",
                    params![kdf_params_bytes],
                )?;
                tx.commit()?;
                Ok(())
            })
            .await
            .map_err(map_tr_err)?;

        info!("vault passphrase changed successfully");
        Ok(())
    }

    /// Returns a reference to the underlying database connection.
    pub fn connection(&self) -> &tokio_rusqlite::Connection {
        &self.conn
    }
}

/// Internal struct for reading vault_meta entries.
struct VaultMeta {
    wrapped_master_key: Vec<u8>,
    nonce: Vec<u8>,
    salt: Vec<u8>,
    kdf_params_bytes: Vec<u8>,
}

/// Mask a secret value for display: "sk-ant-api03-abc...xyz" format.
///
/// Shows prefix (up to 4 chars) and suffix (up to 4 chars) with "..." in between.
/// Short values (< 10 chars) are fully masked as "****".
pub fn mask_secret(value: &str) -> String {
    if value.len() < 10 {
        return "****".to_string();
    }
    let prefix = &value[..4.min(value.len())];
    let suffix = &value[value.len().saturating_sub(4)..];
    format!("{prefix}...{suffix}")
}

/// Convert tokio-rusqlite errors to BlufioError::Vault.
fn map_tr_err(e: tokio_rusqlite::Error<rusqlite::Error>) -> BlufioError {
    BlufioError::Vault(format!("vault database error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Test-specific VaultConfig with low cost for fast tests.
    fn test_config() -> VaultConfig {
        VaultConfig {
            kdf_memory_cost: 32768,
            kdf_iterations: 2,
            kdf_parallelism: 1,
        }
    }

    async fn open_test_db() -> (tokio_rusqlite::Connection, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_vault.db");
        // Open the database using blufio-storage which runs migrations.
        let db = blufio_storage::Database::open(db_path.to_str().unwrap())
            .await
            .unwrap();
        let conn = db.connection().clone();
        // Don't close the Database -- just clone the connection for vault use.
        (conn, dir)
    }

    #[tokio::test]
    async fn vault_create_and_unlock_lifecycle() {
        let (conn, _dir) = open_test_db().await;
        let config = test_config();
        let passphrase = SecretString::from("test-passphrase".to_string());

        // No vault yet.
        assert!(!Vault::exists(&conn).await.unwrap());

        // Create vault.
        let vault = Vault::create(conn.clone(), &passphrase, &config)
            .await
            .unwrap();

        // Vault now exists.
        assert!(Vault::exists(&conn).await.unwrap());

        // Store a secret.
        vault
            .store_secret("api-key", "sk-ant-test-12345")
            .await
            .unwrap();

        // Drop vault (simulates process restart).
        drop(vault);

        // Unlock with correct passphrase.
        let vault2 = Vault::unlock(conn.clone(), &passphrase, &config)
            .await
            .unwrap();

        // Retrieve the secret.
        let retrieved = vault2.retrieve_secret("api-key").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().expose_secret(), "sk-ant-test-12345");
    }

    #[tokio::test]
    async fn store_and_retrieve_secret() {
        let (conn, _dir) = open_test_db().await;
        let config = test_config();
        let passphrase = SecretString::from("test-pass".to_string());

        let vault = Vault::create(conn, &passphrase, &config).await.unwrap();

        vault
            .store_secret("telegram.bot_token", "123456789:ABCdefGHI")
            .await
            .unwrap();

        let secret = vault
            .retrieve_secret("telegram.bot_token")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(secret.expose_secret(), "123456789:ABCdefGHI");
    }

    #[tokio::test]
    async fn retrieve_nonexistent_secret_returns_none() {
        let (conn, _dir) = open_test_db().await;
        let config = test_config();
        let passphrase = SecretString::from("test-pass".to_string());

        let vault = Vault::create(conn, &passphrase, &config).await.unwrap();
        let result = vault.retrieve_secret("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn list_secrets_returns_masked_previews() {
        let (conn, _dir) = open_test_db().await;
        let config = test_config();
        let passphrase = SecretString::from("test-pass".to_string());

        let vault = Vault::create(conn, &passphrase, &config).await.unwrap();
        vault
            .store_secret("anthropic.api_key", "sk-ant-api03-abc123xyz789def456")
            .await
            .unwrap();
        vault
            .store_secret("telegram.bot_token", "123456789:ABCdefGHIjklMNOpqrSTUVwxyz12345")
            .await
            .unwrap();

        let secrets = vault.list_secrets().await.unwrap();
        assert_eq!(secrets.len(), 2);

        // Sorted by name.
        assert_eq!(secrets[0].0, "anthropic.api_key");
        assert!(secrets[0].1.contains("..."));
        assert_eq!(secrets[1].0, "telegram.bot_token");
        assert!(secrets[1].1.contains("..."));
    }

    #[tokio::test]
    async fn delete_secret() {
        let (conn, _dir) = open_test_db().await;
        let config = test_config();
        let passphrase = SecretString::from("test-pass".to_string());

        let vault = Vault::create(conn, &passphrase, &config).await.unwrap();
        vault.store_secret("to-delete", "value").await.unwrap();
        assert!(vault.retrieve_secret("to-delete").await.unwrap().is_some());

        vault.delete_secret("to-delete").await.unwrap();
        assert!(vault.retrieve_secret("to-delete").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn change_passphrase_preserves_secrets() {
        let (conn, _dir) = open_test_db().await;
        let config = test_config();
        let old_pass = SecretString::from("old-passphrase".to_string());
        let new_pass = SecretString::from("new-passphrase".to_string());

        let vault = Vault::create(conn.clone(), &old_pass, &config)
            .await
            .unwrap();
        vault
            .store_secret("my-secret", "secret-value-123")
            .await
            .unwrap();

        // Change passphrase.
        vault.change_passphrase(&new_pass, &config).await.unwrap();
        drop(vault);

        // Old passphrase should fail.
        let result = Vault::unlock(conn.clone(), &old_pass, &config).await;
        assert!(result.is_err());

        // New passphrase should work.
        let vault2 = Vault::unlock(conn, &new_pass, &config).await.unwrap();
        let secret = vault2.retrieve_secret("my-secret").await.unwrap().unwrap();
        assert_eq!(secret.expose_secret(), "secret-value-123");
    }

    #[tokio::test]
    async fn wrong_passphrase_fails_with_clear_error() {
        let (conn, _dir) = open_test_db().await;
        let config = test_config();
        let correct_pass = SecretString::from("correct".to_string());
        let wrong_pass = SecretString::from("wrong".to_string());

        let _vault = Vault::create(conn.clone(), &correct_pass, &config)
            .await
            .unwrap();

        let result = Vault::unlock(conn, &wrong_pass, &config).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("invalid passphrase") || err_msg.contains("decryption failed"),
            "error should mention passphrase: {err_msg}"
        );
    }

    #[tokio::test]
    async fn store_secret_overwrites_existing() {
        let (conn, _dir) = open_test_db().await;
        let config = test_config();
        let passphrase = SecretString::from("test".to_string());

        let vault = Vault::create(conn, &passphrase, &config).await.unwrap();
        vault.store_secret("key", "value1").await.unwrap();
        vault.store_secret("key", "value2").await.unwrap();

        let secret = vault.retrieve_secret("key").await.unwrap().unwrap();
        assert_eq!(secret.expose_secret(), "value2");
    }

    #[test]
    fn mask_secret_long_value() {
        assert_eq!(mask_secret("sk-ant-api03-abcdefghijklmnop"), "sk-a...mnop");
    }

    #[test]
    fn mask_secret_short_value() {
        assert_eq!(mask_secret("short"), "****");
    }

    #[test]
    fn mask_secret_exact_boundary() {
        assert_eq!(mask_secret("1234567890"), "1234...7890");
    }
}
