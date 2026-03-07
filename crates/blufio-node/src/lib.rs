// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Paired device mesh with Ed25519 mutual authentication.
//!
//! The node system allows multiple Blufio instances to pair, form a trusted
//! device mesh, share sessions, and coordinate approvals. Pairing uses
//! Ed25519 mutual authentication with QR code or shared token exchange.

pub mod config;
pub mod pairing;
pub mod store;
pub mod types;

pub use pairing::{compute_pairing_fingerprint, PairingManager};
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
