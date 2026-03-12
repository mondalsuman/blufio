// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Retention enforcement cron task.
//!
//! Wraps [`RetentionEnforcer`] to run two-phase retention (soft-delete +
//! permanent delete) as a scheduled cron job.

use std::sync::Arc;

use async_trait::async_trait;
use blufio_config::model::RetentionConfig;
use tokio_rusqlite::Connection;

use super::{CronTask, CronTaskError};
use crate::retention::RetentionEnforcer;

/// Retention enforcement task that runs the two-phase retention engine.
pub struct RetentionTask {
    db: Arc<Connection>,
    config: RetentionConfig,
}

impl RetentionTask {
    /// Create a new retention enforcement task.
    pub fn new(db: Arc<Connection>, config: RetentionConfig) -> Self {
        Self { db, config }
    }
}

#[async_trait]
impl CronTask for RetentionTask {
    fn name(&self) -> &str {
        "retention_enforcement"
    }

    fn description(&self) -> &str {
        "Enforce data retention policies with soft-delete and permanent removal"
    }

    async fn execute(&self) -> Result<String, CronTaskError> {
        let enforcer = RetentionEnforcer::new(Arc::clone(&self.db), self.config.clone());

        let report = enforcer
            .enforce()
            .await
            .map_err(CronTaskError::ExecutionError)?;

        Ok(report.summary())
    }

    fn timeout(&self) -> std::time::Duration {
        // Retention can take longer on large databases
        std::time::Duration::from_secs(600)
    }
}
