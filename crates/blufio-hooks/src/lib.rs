// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

//! Shell-based lifecycle hook system for the Blufio agent framework.
//!
//! Provides shell command execution with JSON stdin, stdout capture,
//! configurable timeout, PATH restriction, and recursion guard to
//! prevent hook-triggered-hook infinite loops.

pub mod executor;
pub mod manager;
pub mod recursion;

pub use executor::{HookError, HookResult, execute_hook};
pub use manager::HookManager;
pub use recursion::RecursionGuard;
