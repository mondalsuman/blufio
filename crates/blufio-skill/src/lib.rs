#![cfg_attr(not(test), deny(clippy::unwrap_used))]
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
pub mod provider;
pub mod sandbox;
pub mod scaffold;
pub mod signing;
pub mod store;
pub mod tool;

pub use manifest::{load_manifest, parse_manifest};
pub use provider::SkillProvider;
pub use sandbox::WasmSkillRuntime;
pub use scaffold::scaffold_skill;
pub use signing::{
    PublisherKeypair, compute_content_hash, load_private_key_from_file, load_public_key_from_file,
    save_keypair_to_file, signature_from_hex, signature_to_hex,
};
pub use store::{SkillStore, VerificationInfo};
pub use tool::{Tool, ToolOutput, ToolRegistry};
