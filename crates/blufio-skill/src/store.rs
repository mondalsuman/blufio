// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! SQLite-backed persistence for installed skills.
//!
//! [`SkillStore`] manages the `installed_skills` table created by the V5
//! migration (extended by V8 for signing), providing CRUD operations for
//! skill installation, removal, listing, lookup, update, and TOFU key management.

use std::sync::Arc;

use blufio_core::BlufioError;
use chrono::Utc;
use tokio_rusqlite::Connection;

/// Metadata for an installed skill as stored in SQLite.
#[derive(Debug, Clone)]
pub struct InstalledSkill {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: Option<String>,
    pub wasm_path: String,
    pub manifest_toml: String,
    pub capabilities_json: String,
    pub verification_status: String,
    pub installed_at: String,
    pub updated_at: String,
    pub content_hash: Option<String>,
    pub signature: Option<String>,
    pub publisher_id: Option<String>,
}

/// Verification metadata for pre-execution checks.
#[derive(Debug, Clone)]
pub struct VerificationInfo {
    pub content_hash: Option<String>,
    pub signature: Option<String>,
    pub publisher_id: Option<String>,
}

/// SQLite-backed store for installed skill metadata.
///
/// Follows the same pattern as MemoryStore and CostLedger: holds an
/// `Arc<Connection>` and delegates SQL operations via `call()`.
pub struct SkillStore {
    conn: Arc<Connection>,
}

impl SkillStore {
    /// Creates a new SkillStore using the given connection.
    pub fn new(conn: Arc<Connection>) -> Self {
        Self { conn }
    }

    /// Installs or updates a skill in the registry.
    ///
    /// Uses INSERT OR REPLACE so re-installing a skill updates its metadata.
    /// Sets `verification_status` to "verified" when a signature is present,
    /// "unverified" otherwise.
    #[allow(clippy::too_many_arguments)]
    pub async fn install(
        &self,
        name: &str,
        version: &str,
        description: &str,
        author: Option<&str>,
        wasm_path: &str,
        manifest_toml: &str,
        capabilities_json: &str,
        content_hash: Option<&str>,
        signature: Option<&str>,
        publisher_id: Option<&str>,
    ) -> Result<(), BlufioError> {
        let now = Utc::now().to_rfc3339();
        let name = name.to_string();
        let version = version.to_string();
        let description = description.to_string();
        let author = author.map(|s| s.to_string());
        let wasm_path = wasm_path.to_string();
        let manifest_toml = manifest_toml.to_string();
        let capabilities_json = capabilities_json.to_string();
        let content_hash = content_hash.map(|s| s.to_string());
        let signature = signature.map(|s| s.to_string());
        let publisher_id = publisher_id.map(|s| s.to_string());
        let verification_status = if signature.is_some() {
            "verified".to_string()
        } else {
            "unverified".to_string()
        };
        let now_clone = now.clone();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO installed_skills \
                     (name, version, description, author, wasm_path, manifest_toml, \
                      capabilities_json, verification_status, installed_at, updated_at, \
                      content_hash, signature, publisher_id) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    rusqlite::params![
                        name,
                        version,
                        description,
                        author,
                        wasm_path,
                        manifest_toml,
                        capabilities_json,
                        verification_status,
                        now,
                        now_clone,
                        content_hash,
                        signature,
                        publisher_id,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(
                |e: tokio_rusqlite::Error<rusqlite::Error>| {
                    BlufioError::skill_execution_failed(e)
                },
            )
    }

    /// Updates an existing installed skill. Errors if the skill does not exist.
    ///
    /// If the skill was previously signed, verifies that the publisher_id
    /// matches the original install (TOFU continuity).
    #[allow(clippy::too_many_arguments)]
    pub async fn update(
        &self,
        name: &str,
        version: &str,
        description: &str,
        author: Option<&str>,
        wasm_path: &str,
        manifest_toml: &str,
        capabilities_json: &str,
        content_hash: Option<&str>,
        signature: Option<&str>,
        publisher_id: Option<&str>,
    ) -> Result<(), BlufioError> {
        // Check that the skill exists.
        let existing = self.get(name).await?;
        let existing = existing.ok_or_else(|| {
            BlufioError::skill_execution_msg(&format!(
                "skill '{}' not installed -- use 'install' instead",
                name
            ))
        })?;

        // TOFU continuity: if previously signed, publisher must match.
        if let Some(ref existing_pub) = existing.publisher_id
            && let Some(new_pub) = publisher_id
            && existing_pub != new_pub
        {
            return Err(BlufioError::Security(format!(
                "skill '{}': publisher key changed (expected {}, got {}). \
                 Remove and re-install to accept a new publisher.",
                name,
                &existing_pub[..12.min(existing_pub.len())],
                &new_pub[..12.min(new_pub.len())],
            )));
        }

        self.install(
            name,
            version,
            description,
            author,
            wasm_path,
            manifest_toml,
            capabilities_json,
            content_hash,
            signature,
            publisher_id,
        )
        .await
    }

    /// Removes a skill from the registry by name.
    pub async fn remove(&self, name: &str) -> Result<(), BlufioError> {
        let name = name.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM installed_skills WHERE name = ?1",
                    rusqlite::params![name],
                )?;
                Ok(())
            })
            .await
            .map_err(
                |e: tokio_rusqlite::Error<rusqlite::Error>| {
                    BlufioError::skill_execution_failed(e)
                },
            )
    }

    /// Retrieves a single installed skill by name.
    pub async fn get(&self, name: &str) -> Result<Option<InstalledSkill>, BlufioError> {
        let name = name.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT name, version, description, author, wasm_path, manifest_toml, \
                     capabilities_json, verification_status, installed_at, updated_at, \
                     content_hash, signature, publisher_id \
                     FROM installed_skills WHERE name = ?1",
                )?;
                let result = stmt
                    .query_row(rusqlite::params![name], |row| {
                        Ok(InstalledSkill {
                            name: row.get(0)?,
                            version: row.get(1)?,
                            description: row.get(2)?,
                            author: row.get(3)?,
                            wasm_path: row.get(4)?,
                            manifest_toml: row.get(5)?,
                            capabilities_json: row.get(6)?,
                            verification_status: row.get(7)?,
                            installed_at: row.get(8)?,
                            updated_at: row.get(9)?,
                            content_hash: row.get(10)?,
                            signature: row.get(11)?,
                            publisher_id: row.get(12)?,
                        })
                    })
                    .optional()?;
                Ok(result)
            })
            .await
            .map_err(
                |e: tokio_rusqlite::Error<rusqlite::Error>| {
                    BlufioError::skill_execution_failed(e)
                },
            )
    }

    /// Retrieves verification info for a skill (for pre-execution checks).
    pub async fn get_verification_info(
        &self,
        name: &str,
    ) -> Result<Option<VerificationInfo>, BlufioError> {
        let name = name.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT content_hash, signature, publisher_id \
                     FROM installed_skills WHERE name = ?1",
                )?;
                let result = stmt
                    .query_row(rusqlite::params![name], |row| {
                        Ok(VerificationInfo {
                            content_hash: row.get(0)?,
                            signature: row.get(1)?,
                            publisher_id: row.get(2)?,
                        })
                    })
                    .optional()?;
                Ok(result)
            })
            .await
            .map_err(
                |e: tokio_rusqlite::Error<rusqlite::Error>| {
                    BlufioError::skill_execution_failed(e)
                },
            )
    }

    /// Lists all installed skills.
    pub async fn list(&self) -> Result<Vec<InstalledSkill>, BlufioError> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT name, version, description, author, wasm_path, manifest_toml, \
                     capabilities_json, verification_status, installed_at, updated_at, \
                     content_hash, signature, publisher_id \
                     FROM installed_skills ORDER BY name",
                )?;
                let skills = stmt
                    .query_map([], |row| {
                        Ok(InstalledSkill {
                            name: row.get(0)?,
                            version: row.get(1)?,
                            description: row.get(2)?,
                            author: row.get(3)?,
                            wasm_path: row.get(4)?,
                            manifest_toml: row.get(5)?,
                            capabilities_json: row.get(6)?,
                            verification_status: row.get(7)?,
                            installed_at: row.get(8)?,
                            updated_at: row.get(9)?,
                            content_hash: row.get(10)?,
                            signature: row.get(11)?,
                            publisher_id: row.get(12)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(skills)
            })
            .await
            .map_err(
                |e: tokio_rusqlite::Error<rusqlite::Error>| {
                    BlufioError::skill_execution_failed(e)
                },
            )
    }

    // ---- TOFU Publisher Key Management ----

    /// Store a publisher's public key (TOFU: trust on first use).
    pub async fn store_publisher_key(
        &self,
        publisher_id: &str,
        public_key_hex: &str,
    ) -> Result<(), BlufioError> {
        let now = Utc::now().to_rfc3339();
        let publisher_id = publisher_id.to_string();
        let public_key_hex = public_key_hex.to_string();
        let now_clone = now.clone();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO publisher_keys \
                     (publisher_id, public_key_hex, pinned, first_seen, last_used) \
                     VALUES (?1, ?2, 0, ?3, ?4)",
                    rusqlite::params![publisher_id, public_key_hex, now, now_clone],
                )?;
                Ok(())
            })
            .await
            .map_err(
                |e: tokio_rusqlite::Error<rusqlite::Error>| {
                    BlufioError::skill_execution_failed(e)
                },
            )
    }

    /// Get a publisher's stored key. Returns (public_key_hex, pinned).
    pub async fn get_publisher_key(
        &self,
        publisher_id: &str,
    ) -> Result<Option<(String, bool)>, BlufioError> {
        let publisher_id = publisher_id.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT public_key_hex, pinned FROM publisher_keys \
                     WHERE publisher_id = ?1",
                )?;
                let result = stmt
                    .query_row(rusqlite::params![publisher_id], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?))
                    })
                    .optional()?;
                Ok(result)
            })
            .await
            .map_err(
                |e: tokio_rusqlite::Error<rusqlite::Error>| {
                    BlufioError::skill_execution_failed(e)
                },
            )
    }

    /// TOFU: Check or store a publisher's key.
    ///
    /// - If new publisher: store the key (trust on first use).
    /// - If known publisher and key matches: update last_used, accept.
    /// - If known publisher and key differs: reject (key changed).
    pub async fn check_or_store_publisher_key(
        &self,
        publisher_id: &str,
        public_key_hex: &str,
    ) -> Result<(), BlufioError> {
        let existing = self.get_publisher_key(publisher_id).await?;
        match existing {
            None => {
                // First time seeing this publisher — TOFU: trust and store.
                self.store_publisher_key(publisher_id, public_key_hex).await
            }
            Some((stored_key, _pinned)) => {
                if stored_key == public_key_hex {
                    // Key matches — update last_used timestamp.
                    let now = Utc::now().to_rfc3339();
                    let pid = publisher_id.to_string();
                    self.conn
                        .call(move |conn| {
                            conn.execute(
                                "UPDATE publisher_keys SET last_used = ?1 \
                                 WHERE publisher_id = ?2",
                                rusqlite::params![now, pid],
                            )?;
                            Ok(())
                        })
                        .await
                        .map_err(
                            |e: tokio_rusqlite::Error<rusqlite::Error>| {
                                BlufioError::skill_execution_failed(e)
                            },
                        )
                } else {
                    Err(BlufioError::Security(format!(
                        "publisher '{}' key has changed. This could indicate tampering. \
                         Use 'blufio key unpin' and re-install to accept the new key.",
                        &publisher_id[..12.min(publisher_id.len())]
                    )))
                }
            }
        }
    }

    /// Pin a publisher's key (strict lockdown).
    pub async fn pin_publisher_key(&self, publisher_id: &str) -> Result<(), BlufioError> {
        let publisher_id = publisher_id.to_string();
        self.conn
            .call(move |conn| {
                let updated = conn.execute(
                    "UPDATE publisher_keys SET pinned = 1 WHERE publisher_id = ?1",
                    rusqlite::params![publisher_id],
                )?;
                if updated == 0 {
                    return Err(rusqlite::Error::QueryReturnedNoRows);
                }
                Ok(())
            })
            .await
            .map_err(
                |e: tokio_rusqlite::Error<rusqlite::Error>| {
                    BlufioError::skill_execution_failed(e)
                },
            )
    }

    /// Unpin a publisher's key.
    pub async fn unpin_publisher_key(&self, publisher_id: &str) -> Result<(), BlufioError> {
        let publisher_id = publisher_id.to_string();
        self.conn
            .call(move |conn| {
                let updated = conn.execute(
                    "UPDATE publisher_keys SET pinned = 0 WHERE publisher_id = ?1",
                    rusqlite::params![publisher_id],
                )?;
                if updated == 0 {
                    return Err(rusqlite::Error::QueryReturnedNoRows);
                }
                Ok(())
            })
            .await
            .map_err(
                |e: tokio_rusqlite::Error<rusqlite::Error>| {
                    BlufioError::skill_execution_failed(e)
                },
            )
    }

    /// Create signing-related tables if they don't exist (defensive migration).
    pub async fn ensure_signing_tables(&self) -> Result<(), BlufioError> {
        self.conn
            .call(|conn| {
                conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS publisher_keys (
                        publisher_id TEXT PRIMARY KEY,
                        public_key_hex TEXT NOT NULL,
                        pinned INTEGER NOT NULL DEFAULT 0,
                        first_seen TEXT NOT NULL,
                        last_used TEXT NOT NULL
                    )",
                )?;
                Ok(())
            })
            .await
            .map_err(
                |e: tokio_rusqlite::Error<rusqlite::Error>| {
                    BlufioError::skill_execution_failed(e)
                },
            )
    }
}

// Use rusqlite's optional extension for query_row -> Option<T>.
use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;

    /// Create an in-memory SQLite database with the installed_skills table
    /// including the V8 signing columns.
    async fn setup_db() -> Arc<Connection> {
        let conn = Connection::open_in_memory().await.unwrap();
        conn.call(|conn| {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS installed_skills (
                    name TEXT PRIMARY KEY,
                    version TEXT NOT NULL,
                    description TEXT NOT NULL,
                    author TEXT,
                    wasm_path TEXT NOT NULL,
                    manifest_toml TEXT NOT NULL,
                    capabilities_json TEXT NOT NULL,
                    verification_status TEXT NOT NULL DEFAULT 'unverified',
                    installed_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    content_hash TEXT,
                    signature TEXT,
                    publisher_id TEXT
                );
                CREATE TABLE IF NOT EXISTS publisher_keys (
                    publisher_id TEXT PRIMARY KEY,
                    public_key_hex TEXT NOT NULL,
                    pinned INTEGER NOT NULL DEFAULT 0,
                    first_seen TEXT NOT NULL,
                    last_used TEXT NOT NULL
                );",
            )?;
            Ok::<(), rusqlite::Error>(())
        })
        .await
        .unwrap();
        Arc::new(conn)
    }

    #[tokio::test]
    async fn store_install_and_get_roundtrip() {
        let conn = setup_db().await;
        let store = SkillStore::new(conn);

        store
            .install(
                "weather",
                "0.1.0",
                "Weather lookup",
                Some("Test Author"),
                "/skills/weather.wasm",
                "[skill]\nname = \"weather\"",
                r#"{"network":{"domains":["api.weather.com"]}}"#,
                None,
                None,
                None,
            )
            .await
            .unwrap();

        let skill = store.get("weather").await.unwrap().unwrap();
        assert_eq!(skill.name, "weather");
        assert_eq!(skill.version, "0.1.0");
        assert_eq!(skill.description, "Weather lookup");
        assert_eq!(skill.author.as_deref(), Some("Test Author"));
        assert_eq!(skill.wasm_path, "/skills/weather.wasm");
        assert_eq!(skill.verification_status, "unverified");
        assert!(skill.content_hash.is_none());
        assert!(skill.signature.is_none());
        assert!(skill.publisher_id.is_none());
    }

    #[tokio::test]
    async fn store_install_with_hash_and_signature() {
        let conn = setup_db().await;
        let store = SkillStore::new(conn);

        store
            .install(
                "signed-skill",
                "1.0.0",
                "Signed skill",
                None,
                "/s.wasm",
                "",
                "{}",
                Some("abc123hash"),
                Some("sig_hex_here"),
                Some("publisher_hex"),
            )
            .await
            .unwrap();

        let skill = store.get("signed-skill").await.unwrap().unwrap();
        assert_eq!(skill.verification_status, "verified");
        assert_eq!(skill.content_hash.as_deref(), Some("abc123hash"));
        assert_eq!(skill.signature.as_deref(), Some("sig_hex_here"));
        assert_eq!(skill.publisher_id.as_deref(), Some("publisher_hex"));
    }

    #[tokio::test]
    async fn store_install_unsigned_sets_unverified() {
        let conn = setup_db().await;
        let store = SkillStore::new(conn);

        store
            .install(
                "unsigned",
                "1.0.0",
                "No sig",
                None,
                "/u.wasm",
                "",
                "{}",
                Some("hash"),
                None,
                None,
            )
            .await
            .unwrap();

        let skill = store.get("unsigned").await.unwrap().unwrap();
        assert_eq!(skill.verification_status, "unverified");
        assert_eq!(skill.content_hash.as_deref(), Some("hash"));
        assert!(skill.signature.is_none());
    }

    #[tokio::test]
    async fn store_install_and_list() {
        let conn = setup_db().await;
        let store = SkillStore::new(conn);

        store
            .install(
                "alpha",
                "1.0.0",
                "Alpha skill",
                None,
                "/a.wasm",
                "",
                "{}",
                None,
                None,
                None,
            )
            .await
            .unwrap();
        store
            .install(
                "beta",
                "2.0.0",
                "Beta skill",
                None,
                "/b.wasm",
                "",
                "{}",
                None,
                None,
                None,
            )
            .await
            .unwrap();

        let skills = store.list().await.unwrap();
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].name, "alpha");
        assert_eq!(skills[1].name, "beta");
    }

    #[tokio::test]
    async fn store_remove_and_get_returns_none() {
        let conn = setup_db().await;
        let store = SkillStore::new(conn);

        store
            .install(
                "temp",
                "0.1.0",
                "Temp skill",
                None,
                "/t.wasm",
                "",
                "{}",
                None,
                None,
                None,
            )
            .await
            .unwrap();

        assert!(store.get("temp").await.unwrap().is_some());
        store.remove("temp").await.unwrap();
        assert!(store.get("temp").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn store_get_nonexistent_returns_none() {
        let conn = setup_db().await;
        let store = SkillStore::new(conn);
        assert!(store.get("nonexistent").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn store_list_empty() {
        let conn = setup_db().await;
        let store = SkillStore::new(conn);
        let skills = store.list().await.unwrap();
        assert!(skills.is_empty());
    }

    #[tokio::test]
    async fn store_reinstall_updates_metadata() {
        let conn = setup_db().await;
        let store = SkillStore::new(conn);

        store
            .install(
                "skill",
                "1.0.0",
                "Version 1",
                None,
                "/v1.wasm",
                "",
                "{}",
                None,
                None,
                None,
            )
            .await
            .unwrap();
        store
            .install(
                "skill",
                "2.0.0",
                "Version 2",
                None,
                "/v2.wasm",
                "",
                "{}",
                None,
                None,
                None,
            )
            .await
            .unwrap();

        let skill = store.get("skill").await.unwrap().unwrap();
        assert_eq!(skill.version, "2.0.0");
        assert_eq!(skill.description, "Version 2");
        assert_eq!(skill.wasm_path, "/v2.wasm");

        let skills = store.list().await.unwrap();
        assert_eq!(skills.len(), 1);
    }

    #[tokio::test]
    async fn store_update_existing_skill() {
        let conn = setup_db().await;
        let store = SkillStore::new(conn);

        store
            .install(
                "upd",
                "1.0.0",
                "V1",
                None,
                "/v1.wasm",
                "",
                "{}",
                Some("hash1"),
                None,
                None,
            )
            .await
            .unwrap();

        store
            .update(
                "upd",
                "2.0.0",
                "V2",
                None,
                "/v2.wasm",
                "",
                "{}",
                Some("hash2"),
                None,
                None,
            )
            .await
            .unwrap();

        let skill = store.get("upd").await.unwrap().unwrap();
        assert_eq!(skill.version, "2.0.0");
        assert_eq!(skill.content_hash.as_deref(), Some("hash2"));
    }

    #[tokio::test]
    async fn store_update_nonexistent_errors() {
        let conn = setup_db().await;
        let store = SkillStore::new(conn);

        let result = store
            .update(
                "missing", "1.0.0", "X", None, "/x.wasm", "", "{}", None, None, None,
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn store_update_publisher_continuity() {
        let conn = setup_db().await;
        let store = SkillStore::new(conn);

        // Install with publisher A
        store
            .install(
                "s",
                "1.0.0",
                "X",
                None,
                "/x.wasm",
                "",
                "{}",
                Some("h"),
                Some("sig"),
                Some("pub_a"),
            )
            .await
            .unwrap();

        // Update with different publisher should fail
        let result = store
            .update(
                "s",
                "2.0.0",
                "X",
                None,
                "/x.wasm",
                "",
                "{}",
                Some("h2"),
                Some("sig2"),
                Some("pub_b"),
            )
            .await;
        assert!(result.is_err());

        // Update with same publisher should succeed
        store
            .update(
                "s",
                "2.0.0",
                "X",
                None,
                "/x.wasm",
                "",
                "{}",
                Some("h2"),
                Some("sig2"),
                Some("pub_a"),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn store_get_verification_info() {
        let conn = setup_db().await;
        let store = SkillStore::new(conn);

        store
            .install(
                "v",
                "1.0.0",
                "X",
                None,
                "/x.wasm",
                "",
                "{}",
                Some("hash1"),
                Some("sig1"),
                Some("pub1"),
            )
            .await
            .unwrap();

        let info = store.get_verification_info("v").await.unwrap().unwrap();
        assert_eq!(info.content_hash.as_deref(), Some("hash1"));
        assert_eq!(info.signature.as_deref(), Some("sig1"));
        assert_eq!(info.publisher_id.as_deref(), Some("pub1"));

        assert!(
            store
                .get_verification_info("missing")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn tofu_check_or_store_new_publisher() {
        let conn = setup_db().await;
        let store = SkillStore::new(conn);

        // First time — should store.
        store
            .check_or_store_publisher_key("pub1", "key_hex_1")
            .await
            .unwrap();

        let (key, pinned) = store.get_publisher_key("pub1").await.unwrap().unwrap();
        assert_eq!(key, "key_hex_1");
        assert!(!pinned);
    }

    #[tokio::test]
    async fn tofu_check_same_key_succeeds() {
        let conn = setup_db().await;
        let store = SkillStore::new(conn);

        store
            .check_or_store_publisher_key("pub1", "key_hex_1")
            .await
            .unwrap();

        // Same key again — should succeed.
        store
            .check_or_store_publisher_key("pub1", "key_hex_1")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn tofu_check_different_key_fails() {
        let conn = setup_db().await;
        let store = SkillStore::new(conn);

        store
            .check_or_store_publisher_key("pub1", "key_hex_1")
            .await
            .unwrap();

        // Different key — should fail.
        let result = store
            .check_or_store_publisher_key("pub1", "key_hex_different")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn pin_and_unpin_publisher_key() {
        let conn = setup_db().await;
        let store = SkillStore::new(conn);

        store.store_publisher_key("pub1", "key1").await.unwrap();

        let (_, pinned) = store.get_publisher_key("pub1").await.unwrap().unwrap();
        assert!(!pinned);

        store.pin_publisher_key("pub1").await.unwrap();
        let (_, pinned) = store.get_publisher_key("pub1").await.unwrap().unwrap();
        assert!(pinned);

        store.unpin_publisher_key("pub1").await.unwrap();
        let (_, pinned) = store.get_publisher_key("pub1").await.unwrap().unwrap();
        assert!(!pinned);
    }

    #[tokio::test]
    async fn pin_nonexistent_publisher_fails() {
        let conn = setup_db().await;
        let store = SkillStore::new(conn);
        assert!(store.pin_publisher_key("missing").await.is_err());
    }
}
