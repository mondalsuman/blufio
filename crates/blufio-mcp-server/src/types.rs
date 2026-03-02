// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Blufio-owned types for the MCP server public API.
//!
//! These types wrap or replace rmcp types at the crate boundary,
//! ensuring no rmcp types leak into the public API.

use serde::{Deserialize, Serialize};

/// Unique identifier for an MCP protocol session.
///
/// Distinct from `blufio_core::types::SessionId` (conversation session)
/// to prevent accidental conflation at compile time. This newtype wraps
/// the MCP-level session identifier used in protocol handshakes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct McpSessionId(pub String);

impl McpSessionId {
    /// Creates a new MCP session ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for McpSessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_session_id_display() {
        let id = McpSessionId::new("mcp-session-123");
        assert_eq!(id.to_string(), "mcp-session-123");
    }

    #[test]
    fn mcp_session_id_equality() {
        let a = McpSessionId::new("abc");
        let b = McpSessionId::new("abc");
        let c = McpSessionId::new("def");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn mcp_session_id_hash_works() {
        let id1 = McpSessionId::new("test");
        let id2 = id1.clone();
        let mut set = std::collections::HashSet::new();
        set.insert(id1);
        assert!(set.contains(&id2));
    }
}
