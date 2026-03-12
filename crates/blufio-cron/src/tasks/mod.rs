// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Cron task trait and error types.
//!
//! All cron job implementations satisfy [`CronTask`], enabling both built-in
//! and custom tasks to be registered with the scheduler.

use async_trait::async_trait;

/// Trait that all cron job implementations must satisfy.
///
/// Provides a uniform interface for the scheduler to discover, execute,
/// and manage cron tasks regardless of their specific implementation.
#[async_trait]
pub trait CronTask: Send + Sync {
    /// Unique name of this task (used as `job_name` in the database).
    fn name(&self) -> &str;

    /// Human-readable description for CLI display.
    fn description(&self) -> &str;

    /// Execute the task.
    ///
    /// Returns `Ok(output_string)` on success or `Err(CronTaskError)` on failure.
    /// The output string is stored (truncated) in `cron_history`.
    async fn execute(&self) -> Result<String, CronTaskError>;

    /// Default timeout for this task.
    ///
    /// The scheduler will cancel execution if it exceeds this duration.
    /// Defaults to 300 seconds (5 minutes).
    fn timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(300)
    }
}

/// Errors that can occur during cron task execution.
#[derive(Debug, thiserror::Error)]
pub enum CronTaskError {
    /// Task execution exceeded the configured timeout.
    #[error("task timed out")]
    Timeout,

    /// A database error occurred during task execution.
    #[error("database error: {0}")]
    DatabaseError(String),

    /// A general execution error occurred.
    #[error("execution error: {0}")]
    ExecutionError(String),

    /// An unclassified error.
    #[error("{0}")]
    Other(String),
}
