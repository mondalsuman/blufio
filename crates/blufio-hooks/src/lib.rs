// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Shell-based lifecycle hook system for the Blufio agent framework.
//!
//! Provides shell command execution with JSON stdin, stdout capture,
//! configurable timeout, PATH restriction, and recursion guard to
//! prevent hook-triggered-hook infinite loops.

pub mod executor;
pub mod recursion;

pub use executor::{execute_hook, HookError, HookResult};
pub use recursion::RecursionGuard;
