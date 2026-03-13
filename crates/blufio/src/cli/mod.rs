// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! CLI subcommand handler modules for `blufio`.
//!
//! Each module handles the implementation for a group of related subcommands,
//! keeping main.rs focused on argument parsing and dispatch.

pub(crate) mod audit_cmd;
pub(crate) mod config_cmd;
pub(crate) mod injection_cmd;
pub(crate) mod memory_cmd;
pub(crate) mod nodes_cmd;
pub(crate) mod plugin_cmd;
pub(crate) mod skill_cmd;
