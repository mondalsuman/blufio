// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! MCP server implementation for Blufio.
//!
//! This crate implements the Model Context Protocol server, allowing
//! MCP clients (like Claude Desktop) to discover and invoke Blufio
//! tools, read resources, and use prompt templates.
//!
//! ## Abstraction Boundary
//!
//! The `rmcp` crate is used freely within this crate for protocol
//! handling. However, **no rmcp types appear in the public API**.
//! All public types are Blufio-owned, defined in [`types`].

pub mod bridge;
pub mod handler;
pub mod types;

// Re-export public types for convenience.
pub use handler::BlufioMcpHandler;
pub use types::McpSessionId;
