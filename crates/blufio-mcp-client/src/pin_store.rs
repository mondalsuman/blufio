// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! SQLite-backed storage for MCP tool hash pins (CLNT-07).
//!
//! Each tool definition is SHA-256 hashed at discovery time. On subsequent
//! discoveries, the hash is compared to detect schema mutations (rug-pull
//! attacks). Pins are stored in an `mcp_tool_pins` table keyed by
//! (server_name, tool_name).

use rusqlite::OptionalExtension;
use tokio_rusqlite::Connection;
use tracing::{error, info};

/// SQLite-backed storage for MCP tool hash pins.
///
/// Each tool definition is SHA-256 hashed at discovery time.
/// On subsequent discoveries, the hash is compared to detect
/// schema mutations (rug pulls).
pub struct PinStore {
    conn: Connection,
}

/// Result of verifying a pin during tool re-discovery.
#[derive(Debug, PartialEq, Eq)]
pub enum PinVerification {
    /// First discovery: no stored pin, pin was stored.
    FirstSeen,
    /// Pin matches stored value: tool is unchanged.
    Verified,
    /// Pin mismatch: schema has mutated (potential rug pull).
    Mismatch {
        /// The stored (expected) hash.
        stored: String,
        /// The newly computed hash.
        computed: String,
    },
}

impl PinStore {
    /// Open or create the pin store, creating the table if needed.
    pub async fn open(db_path: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let conn = blufio_storage::open_connection(db_path)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        conn.call(|conn| {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS mcp_tool_pins (
                    server_name TEXT NOT NULL,
                    tool_name TEXT NOT NULL,
                    pin_hash TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (datetime('now')),
                    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                    PRIMARY KEY (server_name, tool_name)
                )",
            )?;
            Ok::<(), rusqlite::Error>(())
        })
        .await?;
        Ok(Self { conn })
    }

    /// Store a hash pin for a tool (upsert).
    pub async fn store_pin(
        &self,
        server: &str,
        tool: &str,
        hash: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let server = server.to_string();
        let tool = tool.to_string();
        let hash = hash.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO mcp_tool_pins (server_name, tool_name, pin_hash)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(server_name, tool_name)
                     DO UPDATE SET pin_hash = ?3, updated_at = datetime('now')",
                    rusqlite::params![server, tool, hash],
                )?;
                Ok::<(), rusqlite::Error>(())
            })
            .await?;
        Ok(())
    }

    /// Get the stored pin for a tool, if any.
    pub async fn get_pin(
        &self,
        server: &str,
        tool: &str,
    ) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        let server = server.to_string();
        let tool = tool.to_string();
        let result = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT pin_hash FROM mcp_tool_pins
                     WHERE server_name = ?1 AND tool_name = ?2",
                )?;
                let hash = stmt
                    .query_row(rusqlite::params![server, tool], |row| {
                        row.get::<_, String>(0)
                    })
                    .optional()?;
                Ok::<Option<String>, rusqlite::Error>(hash)
            })
            .await?;
        Ok(result)
    }

    /// Delete a pin (used by `blufio mcp re-pin` CLI).
    ///
    /// Returns `true` if a pin was found and deleted.
    pub async fn delete_pin(
        &self,
        server: &str,
        tool: &str,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let server = server.to_string();
        let tool = tool.to_string();
        let deleted = self
            .conn
            .call(move |conn| {
                let count = conn.execute(
                    "DELETE FROM mcp_tool_pins
                     WHERE server_name = ?1 AND tool_name = ?2",
                    rusqlite::params![server, tool],
                )?;
                Ok::<bool, rusqlite::Error>(count > 0)
            })
            .await?;
        Ok(deleted)
    }

    /// List all pins for a server (for diagnostics).
    pub async fn list_pins(
        &self,
        server: &str,
    ) -> Result<Vec<(String, String)>, Box<dyn std::error::Error + Send + Sync>> {
        let server = server.to_string();
        let pins = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT tool_name, pin_hash FROM mcp_tool_pins
                     WHERE server_name = ?1
                     ORDER BY tool_name",
                )?;
                let rows = stmt
                    .query_map(rusqlite::params![server], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok::<Vec<(String, String)>, rusqlite::Error>(rows)
            })
            .await?;
        Ok(pins)
    }

    /// Verify a computed pin against the stored pin.
    ///
    /// - If no stored pin exists, stores the computed pin (first discovery).
    /// - If stored pin matches, returns `Verified`.
    /// - If stored pin does not match, returns `Mismatch` (rug pull detected).
    pub async fn verify_or_store(
        &self,
        server: &str,
        tool: &str,
        computed_hash: &str,
    ) -> Result<PinVerification, Box<dyn std::error::Error + Send + Sync>> {
        match self.get_pin(server, tool).await? {
            None => {
                // First discovery: store the pin.
                self.store_pin(server, tool, computed_hash).await?;
                info!(
                    server = server,
                    tool = tool,
                    "MCP tool pin stored (first discovery)"
                );
                Ok(PinVerification::FirstSeen)
            }
            Some(stored) if stored == computed_hash => Ok(PinVerification::Verified),
            Some(stored) => {
                error!(
                    server = server,
                    tool = tool,
                    old_pin = stored.as_str(),
                    new_pin = computed_hash,
                    "SECURITY: MCP tool schema mutated (rug pull detected). \
                     Tool disabled. Use 'blufio mcp re-pin {server} {tool}' to re-trust."
                );
                Ok(PinVerification::Mismatch {
                    stored,
                    computed: computed_hash.to_string(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn open_memory_store() -> PinStore {
        PinStore::open(":memory:").await.expect("open in-memory db")
    }

    #[tokio::test]
    async fn store_and_get_pin() {
        let store = open_memory_store().await;
        store.store_pin("github", "search", "abc123").await.unwrap();
        let pin = store.get_pin("github", "search").await.unwrap();
        assert_eq!(pin, Some("abc123".to_string()));
    }

    #[tokio::test]
    async fn get_nonexistent_pin_returns_none() {
        let store = open_memory_store().await;
        let pin = store.get_pin("github", "search").await.unwrap();
        assert_eq!(pin, None);
    }

    #[tokio::test]
    async fn store_pin_upserts() {
        let store = open_memory_store().await;
        store
            .store_pin("github", "search", "hash_v1")
            .await
            .unwrap();
        store
            .store_pin("github", "search", "hash_v2")
            .await
            .unwrap();
        let pin = store.get_pin("github", "search").await.unwrap();
        assert_eq!(pin, Some("hash_v2".to_string()));
    }

    #[tokio::test]
    async fn delete_pin_removes_entry() {
        let store = open_memory_store().await;
        store.store_pin("github", "search", "abc123").await.unwrap();
        let deleted = store.delete_pin("github", "search").await.unwrap();
        assert!(deleted);
        let pin = store.get_pin("github", "search").await.unwrap();
        assert_eq!(pin, None);
    }

    #[tokio::test]
    async fn delete_nonexistent_pin_returns_false() {
        let store = open_memory_store().await;
        let deleted = store.delete_pin("github", "search").await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn list_pins_returns_all_for_server() {
        let store = open_memory_store().await;
        store.store_pin("github", "search", "hash1").await.unwrap();
        store
            .store_pin("github", "create_issue", "hash2")
            .await
            .unwrap();
        store
            .store_pin("other", "list_items", "hash3")
            .await
            .unwrap();

        let pins = store.list_pins("github").await.unwrap();
        assert_eq!(pins.len(), 2);
        assert_eq!(pins[0].0, "create_issue"); // sorted by name
        assert_eq!(pins[1].0, "search");
    }

    #[tokio::test]
    async fn list_pins_empty_for_unknown_server() {
        let store = open_memory_store().await;
        let pins = store.list_pins("unknown").await.unwrap();
        assert!(pins.is_empty());
    }

    #[tokio::test]
    async fn verify_or_store_first_seen() {
        let store = open_memory_store().await;
        let result = store
            .verify_or_store("github", "search", "hash123")
            .await
            .unwrap();
        assert_eq!(result, PinVerification::FirstSeen);

        // Verify it was stored.
        let pin = store.get_pin("github", "search").await.unwrap();
        assert_eq!(pin, Some("hash123".to_string()));
    }

    #[tokio::test]
    async fn verify_or_store_verified() {
        let store = open_memory_store().await;
        store
            .store_pin("github", "search", "hash123")
            .await
            .unwrap();

        let result = store
            .verify_or_store("github", "search", "hash123")
            .await
            .unwrap();
        assert_eq!(result, PinVerification::Verified);
    }

    #[tokio::test]
    async fn verify_or_store_mismatch_detects_rug_pull() {
        let store = open_memory_store().await;
        store
            .store_pin("github", "search", "original_hash")
            .await
            .unwrap();

        let result = store
            .verify_or_store("github", "search", "mutated_hash")
            .await
            .unwrap();
        assert_eq!(
            result,
            PinVerification::Mismatch {
                stored: "original_hash".to_string(),
                computed: "mutated_hash".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn multiple_servers_independent() {
        let store = open_memory_store().await;
        store
            .store_pin("server_a", "tool1", "hash_a")
            .await
            .unwrap();
        store
            .store_pin("server_b", "tool1", "hash_b")
            .await
            .unwrap();

        let pin_a = store.get_pin("server_a", "tool1").await.unwrap();
        let pin_b = store.get_pin("server_b", "tool1").await.unwrap();
        assert_eq!(pin_a, Some("hash_a".to_string()));
        assert_eq!(pin_b, Some("hash_b".to_string()));
        assert_ne!(pin_a, pin_b);
    }
}
