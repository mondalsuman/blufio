// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

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

pub mod tasks;

pub use tasks::{CronTask, CronTaskError};
