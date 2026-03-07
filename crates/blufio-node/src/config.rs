// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Node configuration re-exports.
//!
//! Config structs are defined in `blufio-config` to avoid circular dependencies.
//! This module re-exports them for convenience.

pub use blufio_config::model::{
    NodeApprovalConfig, NodeConfig, NodeHeartbeatConfig, NodeReconnectConfig,
};
