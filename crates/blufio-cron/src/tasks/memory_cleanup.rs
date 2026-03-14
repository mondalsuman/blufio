// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Memory cleanup cron task.
//!
//! Evicts lowest-scored memories exceeding the configured `max_entries` limit.

use std::sync::Arc;

use async_trait::async_trait;
use tokio_rusqlite::Connection;

use super::{CronTask, CronTaskError};

/// Memory cleanup task that evicts lowest-scored memories when over limit.
pub struct MemoryCleanupTask {
    db: Arc<Connection>,
    max_entries: usize,
}

impl MemoryCleanupTask {
    /// Create a new memory cleanup task.
    pub fn new(db: Arc<Connection>, max_entries: usize) -> Self {
        Self { db, max_entries }
    }
}

#[async_trait]
impl CronTask for MemoryCleanupTask {
    fn name(&self) -> &str {
        "memory_cleanup"
    }

    fn description(&self) -> &str {
        "Evict lowest-scored memories exceeding max_entries limit"
    }

    async fn execute(&self) -> Result<String, CronTaskError> {
        let max = self.max_entries;

        let (count, evicted) = self
            .db
            .call(move |conn| -> Result<(usize, usize), rusqlite::Error> {
                // Count active (non-deleted) memories
                let count: usize = conn.query_row(
                    "SELECT COUNT(*) FROM memories WHERE deleted_at IS NULL AND status = 'active'",
                    [],
                    |row| row.get(0),
                )?;

                if count <= max {
                    return Ok((count, 0));
                }

                // Calculate how many to evict (down to 90% of max)
                let target = (max as f64 * 0.9) as usize;
                let to_evict = count.saturating_sub(target);

                if to_evict == 0 {
                    return Ok((count, 0));
                }

                // Sync vec0 status BEFORE soft-deleting memories (rowids must still
                // be resolvable). Gracefully ignore "no such table" errors for
                // databases without vec0 enabled.
                let _ = conn.execute(
                    "UPDATE memories_vec0 SET status = 'evicted' \
                     WHERE rowid IN (\
                       SELECT rowid FROM memories \
                       WHERE id IN (\
                         SELECT id FROM memories \
                         WHERE deleted_at IS NULL AND status = 'active' \
                         ORDER BY confidence ASC, created_at ASC \
                         LIMIT ?1\
                       )\
                     )",
                    rusqlite::params![to_evict],
                );

                // Delete lowest-confidence memories (soft-delete)
                let evicted = conn.execute(
                    "UPDATE memories SET deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
                     WHERE id IN (\
                       SELECT id FROM memories \
                       WHERE deleted_at IS NULL AND status = 'active' \
                       ORDER BY confidence ASC, created_at ASC \
                       LIMIT ?1\
                     )",
                    rusqlite::params![to_evict],
                )?;

                Ok((count, evicted))
            })
            .await
            .map_err(|e| CronTaskError::DatabaseError(e.to_string()))?;

        let remaining = count.saturating_sub(evicted);
        Ok(format!(
            "Evicted {evicted} memories ({remaining} remaining, max: {})",
            self.max_entries
        ))
    }
}
