// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tool trait, registry, and built-in tools for the Blufio agent framework.
//!
//! This crate provides the unified [`Tool`] trait that both built-in tools
//! and WASM skill sandboxes implement. The [`ToolRegistry`] manages tool
//! lookup and generates Anthropic-format tool definitions for the LLM.
//!
//! Built-in tools include:
//! - [`builtin::BashTool`] -- Execute shell commands
//! - [`builtin::HttpTool`] -- Make HTTP requests
//! - [`builtin::FileTool`] -- Read and write files

pub mod builtin;
pub mod manifest;
pub mod sandbox;
pub mod scaffold;
pub mod store;
pub mod tool;

pub use manifest::{load_manifest, parse_manifest};
pub use sandbox::WasmSkillRuntime;
pub use scaffold::scaffold_skill;
pub use store::SkillStore;
pub use tool::{Tool, ToolOutput, ToolRegistry};
