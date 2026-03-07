// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! SQLite persistence for node pairings, groups, and approval state.
//!
//! Uses tokio-rusqlite for async access, matching the blufio-storage patterns.

use tokio_rusqlite::Connection;
use tracing::debug;

use crate::types::{ApprovalStatus, NodeCapability, NodeInfo, NodeStatus};

/// Async store for node system data.
pub struct NodeStore {
    conn: Connection,
}

impl NodeStore {
    /// Create a new store using the given connection.
    ///
    /// The V9 migration must have already been applied to this database.
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Save a new pairing to the database.
    pub async fn save_pairing(&self, info: &NodeInfo) -> Result<(), crate::NodeError> {
        let node_id = info.node_id.clone();
        let name = info.name.clone();
        let public_key_hex = info.public_key_hex.clone();
        let capabilities =
            serde_json::to_string(&info.capabilities).unwrap_or_else(|_| "[]".to_string());
        let paired_at = info.paired_at.clone();
        let endpoint = info.endpoint.clone();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO node_pairings (node_id, name, public_key_hex, capabilities, paired_at, endpoint)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(node_id) DO UPDATE SET
                       name = excluded.name,
                       public_key_hex = excluded.public_key_hex,
                       capabilities = excluded.capabilities,
                       endpoint = excluded.endpoint",
                    rusqlite::params![node_id, name, public_key_hex, capabilities, paired_at, endpoint],
                )?;
                Ok(())
            })
            .await
            .map_err(|e: tokio_rusqlite::Error<rusqlite::Error>| crate::NodeError::Store(format!("save pairing: {e}")))?;

        debug!(node_id = %info.node_id, "saved pairing to store");
        Ok(())
    }

    /// Load all paired nodes from the database.
    pub async fn list_pairings(&self) -> Result<Vec<NodeInfo>, crate::NodeError> {
        self.conn
            .call(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT node_id, name, public_key_hex, capabilities, paired_at, last_seen, endpoint
                     FROM node_pairings ORDER BY paired_at",
                )?;
                let rows = stmt.query_map([], |row| {
                    let caps_json: String = row.get(3)?;
                    let capabilities: Vec<NodeCapability> =
                        serde_json::from_str(&caps_json).unwrap_or_default();
                    Ok(NodeInfo {
                        node_id: row.get(0)?,
                        name: row.get(1)?,
                        public_key_hex: row.get(2)?,
                        capabilities,
                        paired_at: row.get(4)?,
                        last_seen: row.get(5)?,
                        endpoint: row.get(6)?,
                        status: NodeStatus::Offline,
                        battery_percent: None,
                        memory_used_mb: None,
                        memory_total_mb: None,
                    })
                })?;
                let mut nodes = Vec::new();
                for row in rows {
                    nodes.push(row?);
                }
                Ok(nodes)
            })
            .await
            .map_err(|e: tokio_rusqlite::Error<rusqlite::Error>| crate::NodeError::Store(format!("list pairings: {e}")))
    }

    /// Get a single paired node by ID.
    pub async fn get_pairing(&self, node_id: &str) -> Result<Option<NodeInfo>, crate::NodeError> {
        let nid = node_id.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT node_id, name, public_key_hex, capabilities, paired_at, last_seen, endpoint
                     FROM node_pairings WHERE node_id = ?1",
                )?;
                let result = stmt.query_row(rusqlite::params![nid], |row| {
                    let caps_json: String = row.get(3)?;
                    let capabilities: Vec<NodeCapability> =
                        serde_json::from_str(&caps_json).unwrap_or_default();
                    Ok(NodeInfo {
                        node_id: row.get(0)?,
                        name: row.get(1)?,
                        public_key_hex: row.get(2)?,
                        capabilities,
                        paired_at: row.get(4)?,
                        last_seen: row.get(5)?,
                        endpoint: row.get(6)?,
                        status: NodeStatus::Offline,
                        battery_percent: None,
                        memory_used_mb: None,
                        memory_total_mb: None,
                    })
                });
                match result {
                    Ok(info) => Ok(Some(info)),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                    Err(e) => Err(e),
                }
            })
            .await
            .map_err(|e: tokio_rusqlite::Error<rusqlite::Error>| crate::NodeError::Store(format!("get pairing: {e}")))
    }

    /// Remove a pairing by node ID.
    pub async fn remove_pairing(&self, node_id: &str) -> Result<bool, crate::NodeError> {
        let nid = node_id.to_string();
        self.conn
            .call(move |conn| {
                let affected = conn.execute(
                    "DELETE FROM node_pairings WHERE node_id = ?1",
                    rusqlite::params![nid],
                )?;
                Ok(affected > 0)
            })
            .await
            .map_err(|e: tokio_rusqlite::Error<rusqlite::Error>| {
                crate::NodeError::Store(format!("remove pairing: {e}"))
            })
    }

    /// Update last_seen timestamp for a node.
    pub async fn update_last_seen(
        &self,
        node_id: &str,
        timestamp: &str,
    ) -> Result<(), crate::NodeError> {
        let nid = node_id.to_string();
        let ts = timestamp.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE node_pairings SET last_seen = ?1 WHERE node_id = ?2",
                    rusqlite::params![ts, nid],
                )?;
                Ok(())
            })
            .await
            .map_err(|e: tokio_rusqlite::Error<rusqlite::Error>| {
                crate::NodeError::Store(format!("update last_seen: {e}"))
            })
    }

    // --- Group operations ---

    /// Add a node to a named group.
    pub async fn add_to_group(
        &self,
        group_name: &str,
        node_id: &str,
    ) -> Result<(), crate::NodeError> {
        let gn = group_name.to_string();
        let nid = node_id.to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR IGNORE INTO node_groups (group_name, node_id, created_at) VALUES (?1, ?2, ?3)",
                    rusqlite::params![gn, nid, now],
                )?;
                Ok(())
            })
            .await
            .map_err(|e: tokio_rusqlite::Error<rusqlite::Error>| crate::NodeError::Store(format!("add to group: {e}")))
    }

    /// Remove a node from a group.
    pub async fn remove_from_group(
        &self,
        group_name: &str,
        node_id: &str,
    ) -> Result<(), crate::NodeError> {
        let gn = group_name.to_string();
        let nid = node_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM node_groups WHERE group_name = ?1 AND node_id = ?2",
                    rusqlite::params![gn, nid],
                )?;
                Ok(())
            })
            .await
            .map_err(|e: tokio_rusqlite::Error<rusqlite::Error>| {
                crate::NodeError::Store(format!("remove from group: {e}"))
            })
    }

    /// List all groups.
    pub async fn list_groups(&self) -> Result<Vec<(String, Vec<String>)>, crate::NodeError> {
        self.conn
            .call(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT group_name, node_id FROM node_groups ORDER BY group_name, node_id",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;
                let mut groups: std::collections::HashMap<String, Vec<String>> =
                    std::collections::HashMap::new();
                for row in rows {
                    let (group, node) = row?;
                    groups.entry(group).or_default().push(node);
                }
                let mut result: Vec<_> = groups.into_iter().collect();
                result.sort_by(|a, b| a.0.cmp(&b.0));
                Ok(result)
            })
            .await
            .map_err(|e: tokio_rusqlite::Error<rusqlite::Error>| {
                crate::NodeError::Store(format!("list groups: {e}"))
            })
    }

    /// Get all node IDs in a group.
    pub async fn get_group_nodes(&self, group_name: &str) -> Result<Vec<String>, crate::NodeError> {
        let gn = group_name.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT node_id FROM node_groups WHERE group_name = ?1 ORDER BY node_id",
                )?;
                let rows = stmt.query_map(rusqlite::params![gn], |row| row.get(0))?;
                let mut nodes = Vec::new();
                for row in rows {
                    nodes.push(row?);
                }
                Ok(nodes)
            })
            .await
            .map_err(|e: tokio_rusqlite::Error<rusqlite::Error>| {
                crate::NodeError::Store(format!("get group nodes: {e}"))
            })
    }

    /// Delete a group entirely.
    pub async fn delete_group(&self, group_name: &str) -> Result<bool, crate::NodeError> {
        let gn = group_name.to_string();
        self.conn
            .call(move |conn| {
                let affected = conn.execute(
                    "DELETE FROM node_groups WHERE group_name = ?1",
                    rusqlite::params![gn],
                )?;
                Ok(affected > 0)
            })
            .await
            .map_err(|e: tokio_rusqlite::Error<rusqlite::Error>| {
                crate::NodeError::Store(format!("delete group: {e}"))
            })
    }

    // --- Approval operations ---

    /// Save a new pending approval.
    pub async fn save_approval(
        &self,
        request_id: &str,
        action_type: &str,
        description: &str,
        timeout_secs: u64,
    ) -> Result<(), crate::NodeError> {
        let rid = request_id.to_string();
        let at = action_type.to_string();
        let desc = description.to_string();
        let now = chrono::Utc::now();
        let created = now.to_rfc3339();
        let expires = (now + chrono::Duration::seconds(timeout_secs as i64)).to_rfc3339();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO pending_approvals (request_id, action_type, description, status, created_at, expires_at)
                     VALUES (?1, ?2, ?3, 'pending', ?4, ?5)",
                    rusqlite::params![rid, at, desc, created, expires],
                )?;
                Ok(())
            })
            .await
            .map_err(|e: tokio_rusqlite::Error<rusqlite::Error>| crate::NodeError::Store(format!("save approval: {e}")))
    }

    /// Resolve an approval (approve, deny, or expire).
    pub async fn resolve_approval(
        &self,
        request_id: &str,
        status: ApprovalStatus,
        handled_by: Option<&str>,
    ) -> Result<bool, crate::NodeError> {
        let rid = request_id.to_string();
        let status_str = status.to_string();
        let handler = handled_by.map(String::from);
        let now = chrono::Utc::now().to_rfc3339();

        self.conn
            .call(move |conn| {
                let affected = conn.execute(
                    "UPDATE pending_approvals SET status = ?1, handled_by = ?2, resolved_at = ?3
                     WHERE request_id = ?4 AND status = 'pending'",
                    rusqlite::params![status_str, handler, now, rid],
                )?;
                Ok(affected > 0)
            })
            .await
            .map_err(|e: tokio_rusqlite::Error<rusqlite::Error>| {
                crate::NodeError::Store(format!("resolve approval: {e}"))
            })
    }
}
