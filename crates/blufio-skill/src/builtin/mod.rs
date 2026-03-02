// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Built-in tools for the Blufio agent.
//!
//! These tools are always available without any plugin or WASM installation.

pub mod bash;
pub mod file;
pub mod http;

pub use bash::BashTool;
pub use file::FileTool;
pub use http::HttpTool;

use crate::ToolRegistry;
use std::sync::Arc;

/// Registers all built-in tools into the given registry.
///
/// Built-in tools are marked with [`ToolRegistry::register_builtin`] so they
/// always win on collision with external MCP tools.
pub fn register_builtins(registry: &mut ToolRegistry) {
    registry
        .register_builtin(Arc::new(BashTool))
        .expect("register built-in: bash");
    registry
        .register_builtin(Arc::new(HttpTool::new()))
        .expect("register built-in: http");
    registry
        .register_builtin(Arc::new(FileTool))
        .expect("register built-in: file");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_builtins_registers_exactly_3_tools() {
        let mut registry = ToolRegistry::new();
        register_builtins(&mut registry);
        assert_eq!(registry.len(), 3);
        assert!(registry.get("bash").is_some());
        assert!(registry.get("http").is_some());
        assert!(registry.get("file").is_some());
    }
}
