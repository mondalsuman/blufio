// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Health check cron task.
//!
//! Runs basic health checks: DB connectivity, disk space (if applicable).

use std::sync::Arc;

use async_trait::async_trait;
use tokio_rusqlite::Connection;

use super::{CronTask, CronTaskError};

/// Health check task that verifies DB connectivity and reports system status.
pub struct HealthCheckTask {
    db: Arc<Connection>,
}

impl HealthCheckTask {
    /// Create a new health check task.
    pub fn new(db: Arc<Connection>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl CronTask for HealthCheckTask {
    fn name(&self) -> &str {
        "health_check"
    }

    fn description(&self) -> &str {
        "Run basic health checks"
    }

    async fn execute(&self) -> Result<String, CronTaskError> {
        // Check DB connectivity
        let db_ok = self
            .db
            .call(|conn| -> Result<bool, rusqlite::Error> {
                conn.query_row("SELECT 1", [], |row| row.get::<_, i32>(0))?;
                Ok(true)
            })
            .await
            .map_err(|e| CronTaskError::DatabaseError(e.to_string()))?;

        if !db_ok {
            return Err(CronTaskError::ExecutionError(
                "Database connectivity check failed".into(),
            ));
        }

        // Get memory and session counts for basic telemetry
        let (memory_count, session_count) = self
            .db
            .call(|conn| -> Result<(i64, i64), rusqlite::Error> {
                let memories: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM memories WHERE deleted_at IS NULL",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);

                let sessions: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM sessions WHERE deleted_at IS NULL",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);

                Ok((memories, sessions))
            })
            .await
            .map_err(|e| CronTaskError::DatabaseError(e.to_string()))?;

        Ok(format!(
            "Health OK: DB connected, {memory_count} memories, {session_count} sessions"
        ))
    }

    fn timeout(&self) -> std::time::Duration {
        // Health checks should be quick
        std::time::Duration::from_secs(30)
    }
}
