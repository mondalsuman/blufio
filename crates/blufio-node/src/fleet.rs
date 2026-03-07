// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fleet management operations for the node CLI.
//!
//! Provides the backing logic for `blufio nodes list/group/exec` commands.

use tracing::{info, warn};

use crate::connection::ConnectionManager;
use crate::store::NodeStore;
use crate::types::{NodeId, NodeInfo, NodeMessage};

/// List all nodes with current runtime state.
///
/// Returns enriched node info with status, battery, memory merged from runtime state.
pub async fn list_nodes(
    conn_manager: &ConnectionManager,
) -> Result<Vec<NodeInfo>, crate::NodeError> {
    conn_manager.list_nodes_with_state().await
}

/// Format nodes as a human-readable table.
pub fn format_nodes_table(nodes: &[NodeInfo]) -> String {
    if nodes.is_empty() {
        return "No paired nodes.".to_string();
    }

    let mut output = String::new();

    // Header
    output.push_str(&format!(
        "{:<20} {:<10} {:<30} {:<10} {:<15}\n",
        "Name", "Status", "Capabilities", "Battery", "Memory"
    ));
    output.push_str(&"-".repeat(85));
    output.push('\n');

    for node in nodes {
        let caps = node
            .capabilities
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let battery = node
            .battery_percent
            .map(|b| format!("{}%", b))
            .unwrap_or_else(|| "n/a".to_string());
        let memory = match (node.memory_used_mb, node.memory_total_mb) {
            (Some(used), Some(total)) => format!("{}/{}MB", used, total),
            _ => "n/a".to_string(),
        };

        output.push_str(&format!(
            "{:<20} {:<10} {:<30} {:<10} {:<15}\n",
            &node.name,
            node.status.to_string(),
            &caps,
            &battery,
            &memory,
        ));
    }

    output
}

/// Format nodes as JSON.
pub fn format_nodes_json(nodes: &[NodeInfo]) -> Result<String, crate::NodeError> {
    serde_json::to_string_pretty(nodes)
        .map_err(|e| crate::NodeError::Store(format!("JSON serialization: {e}")))
}

/// Execute a command on one or more target nodes, streaming output.
///
/// Targets can be individual node IDs or group names (resolved via store).
pub async fn exec_on_nodes(
    conn_manager: &ConnectionManager,
    store: &NodeStore,
    targets: &[String],
    command: &str,
    args: &[String],
) -> Result<(), crate::NodeError> {
    // Resolve targets to node IDs (could be group names)
    let mut node_ids: Vec<NodeId> = Vec::new();
    for target in targets {
        let group_nodes = store.get_group_nodes(target).await?;
        if group_nodes.is_empty() {
            // Not a group, treat as node_id
            node_ids.push(target.clone());
        } else {
            node_ids.extend(group_nodes);
        }
    }

    // Deduplicate
    node_ids.sort();
    node_ids.dedup();

    if node_ids.is_empty() {
        return Err(crate::NodeError::Connection(
            "no target nodes resolved".to_string(),
        ));
    }

    // Check capabilities
    for nid in &node_ids {
        if !conn_manager.has_capability(nid, "exec").await? {
            return Err(crate::NodeError::CapabilityUnavailable(format!(
                "node {nid} does not have 'exec' capability"
            )));
        }
    }

    // Send exec requests
    let request_id = uuid::Uuid::new_v4().to_string();
    for nid in &node_ids {
        let msg = NodeMessage::ExecRequest {
            request_id: request_id.clone(),
            command: command.to_string(),
            args: args.to_vec(),
        };
        match conn_manager.send_to(nid, msg).await {
            Ok(_) => info!(node_id = %nid, "exec request sent"),
            Err(e) => warn!(node_id = %nid, "exec send failed: {e}"),
        }
    }

    Ok(())
}

/// Create a named group.
pub async fn create_group(
    store: &NodeStore,
    group_name: &str,
    node_ids: &[String],
) -> Result<(), crate::NodeError> {
    for nid in node_ids {
        store.add_to_group(group_name, nid).await?;
    }
    info!(group = %group_name, nodes = node_ids.len(), "group created");
    Ok(())
}

/// Delete a named group.
pub async fn delete_group(store: &NodeStore, group_name: &str) -> Result<bool, crate::NodeError> {
    store.delete_group(group_name).await
}

/// List all groups with their node IDs.
pub async fn list_groups(
    store: &NodeStore,
) -> Result<Vec<(String, Vec<String>)>, crate::NodeError> {
    store.list_groups().await
}

/// Format groups as a human-readable table.
pub fn format_groups_table(groups: &[(String, Vec<String>)]) -> String {
    if groups.is_empty() {
        return "No groups defined.".to_string();
    }

    let mut output = String::new();
    output.push_str(&format!("{:<20} {}\n", "Group", "Nodes"));
    output.push_str(&"-".repeat(60));
    output.push('\n');

    for (name, nodes) in groups {
        output.push_str(&format!("{:<20} {}\n", name, nodes.join(", ")));
    }

    output
}
