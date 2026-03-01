// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! SQLite-backed persistence for installed skills.
//!
//! [`SkillStore`] manages the `installed_skills` table created by the V5
//! migration, providing CRUD operations for skill installation, removal,
//! listing, and lookup.

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
    pub async fn install(
        &self,
        name: &str,
        version: &str,
        description: &str,
        author: Option<&str>,
        wasm_path: &str,
        manifest_toml: &str,
        capabilities_json: &str,
    ) -> Result<(), BlufioError> {
        let now = Utc::now().to_rfc3339();
        let name = name.to_string();
        let version = version.to_string();
        let description = description.to_string();
        let author = author.map(|s| s.to_string());
        let wasm_path = wasm_path.to_string();
        let manifest_toml = manifest_toml.to_string();
        let capabilities_json = capabilities_json.to_string();
        let now_clone = now.clone();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO installed_skills \
                     (name, version, description, author, wasm_path, manifest_toml, \
                      capabilities_json, verification_status, installed_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'unverified', ?8, ?9)",
                    rusqlite::params![
                        name,
                        version,
                        description,
                        author,
                        wasm_path,
                        manifest_toml,
                        capabilities_json,
                        now,
                        now_clone,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e: tokio_rusqlite::Error<rusqlite::Error>| BlufioError::Skill {
                message: format!("failed to install skill: {e}"),
                source: None,
            })
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
            .map_err(|e: tokio_rusqlite::Error<rusqlite::Error>| BlufioError::Skill {
                message: format!("failed to remove skill: {e}"),
                source: None,
            })
    }

    /// Retrieves a single installed skill by name.
    pub async fn get(&self, name: &str) -> Result<Option<InstalledSkill>, BlufioError> {
        let name = name.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT name, version, description, author, wasm_path, manifest_toml, \
                     capabilities_json, verification_status, installed_at, updated_at \
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
                        })
                    })
                    .optional()?;
                Ok(result)
            })
            .await
            .map_err(|e: tokio_rusqlite::Error<rusqlite::Error>| BlufioError::Skill {
                message: format!("failed to get skill: {e}"),
                source: None,
            })
    }

    /// Lists all installed skills.
    pub async fn list(&self) -> Result<Vec<InstalledSkill>, BlufioError> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT name, version, description, author, wasm_path, manifest_toml, \
                     capabilities_json, verification_status, installed_at, updated_at \
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
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(skills)
            })
            .await
            .map_err(|e: tokio_rusqlite::Error<rusqlite::Error>| BlufioError::Skill {
                message: format!("failed to list skills: {e}"),
                source: None,
            })
    }
}

// Use rusqlite's optional extension for query_row -> Option<T>.
use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;

    /// Create an in-memory SQLite database with the installed_skills table.
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
                    updated_at TEXT NOT NULL
                )",
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
    }

    #[tokio::test]
    async fn store_install_and_list() {
        let conn = setup_db().await;
        let store = SkillStore::new(conn);

        store
            .install("alpha", "1.0.0", "Alpha skill", None, "/a.wasm", "", "{}")
            .await
            .unwrap();
        store
            .install("beta", "2.0.0", "Beta skill", None, "/b.wasm", "", "{}")
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
            .install("temp", "0.1.0", "Temp skill", None, "/t.wasm", "", "{}")
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
            .install("skill", "1.0.0", "Version 1", None, "/v1.wasm", "", "{}")
            .await
            .unwrap();
        store
            .install("skill", "2.0.0", "Version 2", None, "/v2.wasm", "", "{}")
            .await
            .unwrap();

        let skill = store.get("skill").await.unwrap().unwrap();
        assert_eq!(skill.version, "2.0.0");
        assert_eq!(skill.description, "Version 2");
        assert_eq!(skill.wasm_path, "/v2.wasm");

        // Should still be only one entry.
        let skills = store.list().await.unwrap();
        assert_eq!(skills.len(), 1);
    }
}
