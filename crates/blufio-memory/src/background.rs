// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Combined background task for memory eviction and validation.
//!
//! Runs eviction sweeps on a configurable interval (default: 5 minutes)
//! and validation (duplicate/stale/conflict detection) daily.

use std::sync::Arc;

use blufio_bus::EventBus;
use blufio_config::model::MemoryConfig;
use tokio::time::{Duration, interval};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::eviction;
use crate::store::MemoryStore;
use crate::validation;

/// Spawn a combined background task that runs eviction and validation on separate timers.
///
/// - Eviction: runs every `config.eviction_sweep_interval_secs` (default 300s = 5min).
/// - Validation: runs every 86400 seconds (daily).
///
/// Both timers skip their first immediate tick. The task respects the provided
/// `CancellationToken` for graceful shutdown.
pub async fn spawn_background_task(
    store: Arc<MemoryStore>,
    config: MemoryConfig,
    event_bus: Option<Arc<EventBus>>,
    cancel: CancellationToken,
) {
    let eviction_secs = config.eviction_sweep_interval_secs;
    let mut eviction_interval = interval(Duration::from_secs(eviction_secs));
    eviction_interval.tick().await; // Skip first immediate tick

    let mut validation_interval = interval(Duration::from_secs(86400));
    validation_interval.tick().await; // Skip first immediate tick

    info!(
        eviction_interval_secs = eviction_secs,
        validation_interval_secs = 86400,
        "Memory background task started"
    );

    loop {
        tokio::select! {
            _ = eviction_interval.tick() => {
                if let Err(e) = eviction::run_eviction_sweep(&store, &config, &event_bus).await {
                    warn!(error = %e, "Eviction sweep failed");
                }
            }
            _ = validation_interval.tick() => {
                if let Err(e) = validation::run_validation(&store, &config, &event_bus).await {
                    warn!(error = %e, "Validation run failed");
                }
            }
            _ = cancel.cancelled() => {
                info!("Memory background task shutting down");
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn background_task_respects_cancellation_token() {
        let conn = tokio_rusqlite::Connection::open_in_memory().await.unwrap();
        conn.call(|conn| -> Result<(), rusqlite::Error> {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS memories (
                    id TEXT PRIMARY KEY NOT NULL,
                    content TEXT NOT NULL,
                    embedding BLOB NOT NULL,
                    source TEXT NOT NULL,
                    confidence REAL NOT NULL DEFAULT 0.5,
                    status TEXT NOT NULL DEFAULT 'active',
                    superseded_by TEXT,
                    session_id TEXT,
                    classification TEXT NOT NULL DEFAULT 'internal',
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );

                CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                    content,
                    content='memories',
                    content_rowid='rowid'
                );

                CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
                    INSERT INTO memories_fts(rowid, content) VALUES (new.rowid, new.content);
                END;

                CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
                    INSERT INTO memories_fts(memories_fts, rowid, content)
                        VALUES('delete', old.rowid, old.content);
                END;

                CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
                    INSERT INTO memories_fts(memories_fts, rowid, content)
                        VALUES('delete', old.rowid, old.content);
                    INSERT INTO memories_fts(rowid, content) VALUES (new.rowid, new.content);
                END;

                CREATE INDEX IF NOT EXISTS idx_memories_status ON memories(status);
                CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at);",
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let store = Arc::new(MemoryStore::new(conn));
        let config = MemoryConfig {
            eviction_sweep_interval_secs: 1, // 1 second for testing
            ..Default::default()
        };
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        let handle = tokio::spawn(async move {
            spawn_background_task(store, config, None, cancel_clone).await;
        });

        // Let it run for a brief moment
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Cancel
        cancel.cancel();

        // Should complete within a reasonable time
        let result = tokio::time::timeout(Duration::from_secs(5), handle).await;
        assert!(
            result.is_ok(),
            "Background task should exit after cancellation"
        );
        assert!(result.unwrap().is_ok(), "Background task should not panic");
    }
}
