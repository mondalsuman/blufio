// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

//! Cron scheduler and retention policy engine for the Blufio agent framework.
//!
//! Provides an in-process TOML-configurable cron scheduler that runs inside
//! `blufio serve`, plus retention policy enforcement with soft-delete and
//! grace-period permanent deletion.
//!
//! # Overview
//!
//! - [`CronTask`] -- trait that all cron job implementations satisfy
//! - [`CronTaskError`] -- errors from task execution
//! - [`CronScheduler`] -- main scheduler with dispatch loop
//! - [`CronHistoryEntry`] -- job execution history entry
//! - [`RetentionEnforcer`] -- two-phase retention enforcement engine

pub mod history;
pub mod retention;
pub mod scheduler;
pub mod systemd;
pub mod tasks;

pub use history::{CronHistoryEntry, query_history};
pub use retention::RetentionEnforcer;
pub use scheduler::{CronError, CronScheduler};
pub use systemd::{CronJobRow, generate_timers};
pub use tasks::{CronTask, CronTaskError, register_builtin_tasks};
