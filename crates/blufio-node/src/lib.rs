// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

//! Paired device mesh with Ed25519 mutual authentication.
//!
//! The node system allows multiple Blufio instances to pair, form a trusted
//! device mesh, share sessions, and coordinate approvals. Pairing uses
//! Ed25519 mutual authentication with QR code or shared token exchange.

pub mod approval;
pub mod config;
pub mod connection;
pub mod fleet;
pub mod heartbeat;
pub mod pairing;
pub mod store;
pub mod types;

pub use approval::{ApprovalOutcome, ApprovalRouter};
pub use connection::{ConnectionManager, NodeRuntimeState};
pub use fleet::{
    create_group, delete_group, exec_on_nodes, format_groups_table, format_nodes_json,
    format_nodes_table, list_groups, list_nodes,
};
pub use heartbeat::{HeartbeatMonitor, SystemMetrics, collect_metrics};
pub use pairing::{PairingManager, compute_pairing_fingerprint};
pub use store::NodeStore;
pub use types::*;

/// Errors specific to the node system.
#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    /// Storage error.
    #[error("node store error: {0}")]
    Store(String),

    /// Pairing protocol error.
    #[error("pairing error: {0}")]
    Pairing(String),

    /// WebSocket connection error.
    #[error("connection error: {0}")]
    Connection(String),

    /// Authentication error.
    #[error("auth error: {0}")]
    Auth(String),

    /// Capability not available on target node.
    #[error("capability not available: {0}")]
    CapabilityUnavailable(String),

    /// Approval was denied or timed out.
    #[error("approval {0}: {1}")]
    Approval(String, String),
}
