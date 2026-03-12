// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Database backup cron task.
//!
//! Creates a backup of the database using SQLite's `VACUUM INTO` command.

use std::sync::Arc;

use async_trait::async_trait;
use tokio_rusqlite::Connection;

use super::{CronTask, CronTaskError};

/// Backup task that creates a database backup via `VACUUM INTO`.
pub struct BackupTask {
    db: Arc<Connection>,
    backup_path: String,
}

impl BackupTask {
    /// Create a new backup task.
    ///
    /// `backup_path` is the file path where the backup will be written.
    pub fn new(db: Arc<Connection>, backup_path: String) -> Self {
        Self { db, backup_path }
    }
}

#[async_trait]
impl CronTask for BackupTask {
    fn name(&self) -> &str {
        "backup"
    }

    fn description(&self) -> &str {
        "Backup database to configured path"
    }

    async fn execute(&self) -> Result<String, CronTaskError> {
        let path = self.backup_path.clone();

        self.db
            .call(move |conn| -> Result<String, rusqlite::Error> {
                conn.execute_batch(&format!("VACUUM INTO '{}'", path.replace('\'', "''")))?;
                Ok(path)
            })
            .await
            .map_err(|e| CronTaskError::DatabaseError(e.to_string()))
            .map(|path| {
                // Get backup file size
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                format!("Backup written to {path} ({size} bytes)")
            })
    }
}
