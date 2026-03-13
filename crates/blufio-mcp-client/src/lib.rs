// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

//! MCP client implementation for Blufio.
//!
//! This crate implements the Model Context Protocol client, enabling
//! Blufio to connect to external MCP servers, discover their tools,
//! and invoke them within agent conversations.
//!
//! ## Abstraction Boundary
//!
//! The `rmcp` crate is used freely within this crate for protocol
//! handling. However, **no rmcp types appear in the public API**.
//! All public types are Blufio-owned.

pub mod external_tool;
pub mod health;
pub mod manager;
pub mod pin;
pub mod pin_store;
pub mod sanitize;
pub mod trust_zone;

pub use manager::{DiagnosticResult, McpClientManager, diagnose_server};
pub use pin_store::PinStore;
pub use trust_zone::TrustZoneProvider;
