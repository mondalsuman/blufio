// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Cron task trait, error types, and built-in task implementations.
//!
//! All cron job implementations satisfy [`CronTask`], enabling both built-in
//! and custom tasks to be registered with the scheduler.

pub mod backup;
pub mod cost_report;
pub mod health_check;
pub mod memory_cleanup;
pub mod retention;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use blufio_config::model::{BlufioConfig, RetentionConfig};
use tokio_rusqlite::Connection;

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

/// Create all 5 built-in tasks and return the registry.
///
/// The registry maps task names to their implementations. The scheduler uses
/// these to dispatch jobs by their `task` field.
pub fn register_builtin_tasks(
    db: Arc<Connection>,
    config: &BlufioConfig,
) -> HashMap<String, Box<dyn CronTask>> {
    let mut registry: HashMap<String, Box<dyn CronTask>> = HashMap::new();

    // 1. Memory cleanup
    let mem_task = memory_cleanup::MemoryCleanupTask::new(
        Arc::clone(&db),
        config.memory.max_entries,
    );
    registry.insert(mem_task.name().to_string(), Box::new(mem_task));

    // 2. Backup
    let db_path = &config.storage.database_path;
    let backup_path = format!("{db_path}.backup");
    let backup_task = backup::BackupTask::new(Arc::clone(&db), backup_path);
    registry.insert(backup_task.name().to_string(), Box::new(backup_task));

    // 3. Cost report
    let cost_task = cost_report::CostReportTask::new(Arc::clone(&db));
    registry.insert(cost_task.name().to_string(), Box::new(cost_task));

    // 4. Health check
    let health_task = health_check::HealthCheckTask::new(Arc::clone(&db));
    registry.insert(health_task.name().to_string(), Box::new(health_task));

    // 5. Retention enforcement
    let retention_config = if config.retention.enabled {
        config.retention.clone()
    } else {
        RetentionConfig::default()
    };
    let retention_task =
        retention::RetentionTask::new(Arc::clone(&db), retention_config);
    registry.insert(retention_task.name().to_string(), Box::new(retention_task));

    registry
}
