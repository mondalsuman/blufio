// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Core types for the Blufio node system.
//!
//! Defines node identity, capabilities, message protocol, and pairing types.

use serde::{Deserialize, Serialize};

/// Unique identifier for a node in the mesh.
///
/// Format: `node-{uuid4}` generated at first pairing.
pub type NodeId = String;

/// Capabilities a node can declare.
///
/// Core capabilities are enum variants; plugins can extend via `Custom(String)`.
/// Capabilities are declared in TOML config and enforced at request routing time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum NodeCapability {
    /// Camera access (photo/video capture).
    Camera,
    /// Screen capture and sharing.
    Screen,
    /// GPS/location reporting.
    Location,
    /// Shell command execution and Blufio operation routing.
    Exec,
    /// Plugin-defined custom capability.
    #[serde(untagged)]
    Custom(String),
}

impl std::fmt::Display for NodeCapability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Camera => write!(f, "camera"),
            Self::Screen => write!(f, "screen"),
            Self::Location => write!(f, "location"),
            Self::Exec => write!(f, "exec"),
            Self::Custom(s) => write!(f, "{s}"),
        }
    }
}

/// Connection status of a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    /// Node is connected and sending heartbeats.
    Online,
    /// Node has not sent a heartbeat within the stale threshold.
    Stale,
    /// Node has disconnected or was never connected this session.
    Offline,
}

impl std::fmt::Display for NodeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Online => write!(f, "online"),
            Self::Stale => write!(f, "stale"),
            Self::Offline => write!(f, "offline"),
        }
    }
}

/// Information about a paired node, combining stored and runtime state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Unique node identifier.
    pub node_id: NodeId,
    /// Human-friendly display name.
    pub name: String,
    /// Hex-encoded Ed25519 public key.
    pub public_key_hex: String,
    /// Declared capabilities.
    pub capabilities: Vec<NodeCapability>,
    /// WebSocket endpoint for reconnection (e.g., ws://host:port/nodes/ws).
    pub endpoint: Option<String>,
    /// When the node was paired (ISO 8601).
    pub paired_at: String,
    /// Last heartbeat timestamp (ISO 8601). None if never connected.
    pub last_seen: Option<String>,
    /// Current connection status (runtime, not persisted).
    #[serde(default = "default_offline")]
    pub status: NodeStatus,
    /// Last reported battery percentage (0-100). None if unavailable.
    pub battery_percent: Option<u8>,
    /// Last reported memory usage in MB.
    pub memory_used_mb: Option<u64>,
    /// Last reported total memory in MB.
    pub memory_total_mb: Option<u64>,
}

fn default_offline() -> NodeStatus {
    NodeStatus::Offline
}

/// A pairing token for initiating node pairing.
#[derive(Debug, Clone)]
pub struct PairingToken {
    /// Random 32-byte token, hex-encoded (64 characters).
    pub value: String,
    /// When this token expires (15 minutes from creation).
    pub expires_at: std::time::Instant,
    /// Whether this token has been consumed.
    pub used: bool,
}

impl PairingToken {
    /// Generate a new pairing token with 15-minute expiry.
    pub fn generate() -> Self {
        use rand::RngCore;
        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        Self {
            value: hex::encode(bytes),
            expires_at: std::time::Instant::now() + std::time::Duration::from_secs(15 * 60),
            used: false,
        }
    }

    /// Check if the token is still valid (not expired, not used).
    pub fn is_valid(&self) -> bool {
        !self.used && std::time::Instant::now() < self.expires_at
    }

    /// Mark the token as consumed.
    pub fn consume(&mut self) {
        self.used = true;
    }
}

/// JSON-serialized messages exchanged over WebSocket between nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeMessage {
    // --- Pairing ---
    /// Initiator sends token and public key.
    PairRequest {
        token: String,
        public_key: String,
    },
    /// Responder sends public key and signature over challenge.
    PairResponse {
        public_key: String,
        signature: String,
    },
    /// Both sides confirm the fingerprint match.
    PairConfirm {
        fingerprint: String,
        confirmed: bool,
    },

    // --- Connection lifecycle ---
    /// Initial hello after connection, declaring identity and capabilities.
    Hello {
        node_id: String,
        capabilities: Vec<String>,
        version: String,
    },
    /// Periodic heartbeat with system metrics.
    Heartbeat {
        node_id: String,
        battery_percent: Option<u8>,
        memory_used_mb: u64,
        memory_total_mb: u64,
        uptime_secs: u64,
    },

    // --- Approval routing ---
    /// Request approval from operator devices.
    ApprovalRequest {
        request_id: String,
        action_type: String,
        description: String,
        timeout_secs: u64,
    },
    /// Approval or denial from a device.
    ApprovalResponse {
        request_id: String,
        approved: bool,
        responder_node: String,
    },
    /// Notification that a request was already handled.
    ApprovalHandled {
        request_id: String,
        handled_by: String,
    },

    // --- Exec routing ---
    /// Request command execution on a remote node.
    ExecRequest {
        request_id: String,
        command: String,
        args: Vec<String>,
    },
    /// Streaming output from command execution.
    ExecOutput {
        request_id: String,
        node_id: String,
        /// "stdout" or "stderr".
        stream: String,
        data: String,
    },
    /// Command execution completed.
    ExecComplete {
        request_id: String,
        node_id: String,
        exit_code: i32,
    },
}

/// Status of a pending approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Denied,
    Expired,
}

impl std::fmt::Display for ApprovalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Approved => write!(f, "approved"),
            Self::Denied => write!(f, "denied"),
            Self::Expired => write!(f, "expired"),
        }
    }
}
