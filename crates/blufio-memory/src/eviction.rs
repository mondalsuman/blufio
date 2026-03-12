// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Memory eviction sweep logic.
//!
//! When the active memory count exceeds `max_entries`, eviction removes
//! the lowest-scored entries (by composite eviction score) down to 90%
//! of the configured maximum.

use std::sync::Arc;

use blufio_bus::EventBus;
use blufio_bus::events::{BusEvent, MemoryEvent, new_event_id, now_timestamp};
use blufio_config::model::MemoryConfig;
use blufio_core::error::BlufioError;
use tracing::info;

use crate::store::MemoryStore;

/// Run an eviction sweep: if active memory count exceeds `max_entries`,
/// evict the lowest-scored entries down to 90% of max.
///
/// Emits a single bulk `MemoryEvent::Evicted` event when eviction occurs.
pub async fn run_eviction_sweep(
    store: &MemoryStore,
    config: &MemoryConfig,
    event_bus: &Option<Arc<EventBus>>,
) -> Result<(), BlufioError> {
    let count = store.count_active().await?;

    if count <= config.max_entries {
        return Ok(());
    }

    // Evict down to 90% of max_entries
    let target = config.max_entries * 9 / 10;
    let evict_count = count - target;

    info!(
        active = count,
        max = config.max_entries,
        target,
        evict_count,
        "Eviction sweep: active count exceeds max, evicting lowest-scored entries"
    );

    let (deleted, lowest_score, highest_score) = store
        .batch_evict(
            evict_count,
            config.decay_factor,
            config.decay_floor,
            (
                config.importance_boost_explicit,
                config.importance_boost_extracted,
                config.importance_boost_file,
            ),
        )
        .await?;

    if deleted > 0 {
        info!(
            deleted,
            lowest_score, highest_score, "Eviction sweep complete"
        );

        if let Some(bus) = event_bus {
            bus.publish(BusEvent::Memory(MemoryEvent::Evicted {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                count: deleted as u32,
                lowest_score,
                highest_score,
            }))
            .await;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use blufio_core::classification::DataClassification;
    use tokio_rusqlite::Connection;

    use crate::types::{Memory, MemorySource, MemoryStatus};

    async fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().await.unwrap();
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
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    deleted_at TEXT
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
        conn
    }

    fn make_memory(id: &str, source: MemorySource, confidence: f64, days_old: i64) -> Memory {
        let created = chrono::Utc::now() - chrono::Duration::days(days_old);
        Memory {
            id: id.to_string(),
            content: format!("Memory {id}"),
            embedding: vec![0.1; 384],
            source,
            confidence,
            status: MemoryStatus::Active,
            superseded_by: None,
            session_id: Some("test-session".to_string()),
            classification: DataClassification::default(),
            created_at: created.to_rfc3339(),
            updated_at: created.to_rfc3339(),
        }
    }

    fn test_config(max_entries: usize) -> MemoryConfig {
        MemoryConfig {
            max_entries,
            decay_factor: 0.95,
            decay_floor: 0.1,
            importance_boost_explicit: 1.0,
            importance_boost_extracted: 0.6,
            importance_boost_file: 0.8,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn eviction_sweep_does_nothing_when_under_max() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        // Add 5 memories with max_entries=10
        for i in 0..5 {
            let mem = make_memory(&format!("mem-{i}"), MemorySource::Explicit, 0.9, 1);
            store.save(&mem).await.unwrap();
        }

        let config = test_config(10);
        run_eviction_sweep(&store, &config, &None).await.unwrap();

        // All 5 should still exist
        let count = store.count_active().await.unwrap();
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn eviction_sweep_evicts_when_over_max() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        // Add 15 memories with max_entries=10
        // Older memories should be evicted first (higher decay = lower score)
        for i in 0..15 {
            let mem = make_memory(
                &format!("mem-{i:02}"),
                MemorySource::Extracted,
                0.6,
                (i + 1) as i64,
            );
            store.save(&mem).await.unwrap();
        }

        let config = test_config(10);
        let count_before = store.count_active().await.unwrap();
        assert_eq!(count_before, 15);

        run_eviction_sweep(&store, &config, &None).await.unwrap();

        // target = 10 * 9 / 10 = 9, so evict 15 - 9 = 6
        let count_after = store.count_active().await.unwrap();
        assert_eq!(count_after, 9);
    }

    #[tokio::test]
    async fn eviction_sweep_emits_event() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        for i in 0..12 {
            let mem = make_memory(
                &format!("mem-{i:02}"),
                MemorySource::Extracted,
                0.6,
                (i + 1) as i64,
            );
            store.save(&mem).await.unwrap();
        }

        let bus = Arc::new(EventBus::new(16));
        let mut rx = bus.subscribe();
        let event_bus = Some(bus);
        let config = test_config(10);

        run_eviction_sweep(&store, &config, &event_bus)
            .await
            .unwrap();

        // Should have received exactly one Evicted event
        let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
            .await
            .expect("timeout waiting for event")
            .expect("no event received");

        match event {
            BusEvent::Memory(MemoryEvent::Evicted {
                count,
                lowest_score,
                highest_score,
                ..
            }) => {
                // 12 - 9 = 3 evicted
                assert_eq!(count, 3);
                assert!(lowest_score > 0.0);
                assert!(highest_score >= lowest_score);
            }
            other => panic!("Expected MemoryEvent::Evicted, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn eviction_only_targets_active_non_restricted() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        // Add 12 active memories
        for i in 0..12 {
            let mem = make_memory(
                &format!("mem-active-{i:02}"),
                MemorySource::Extracted,
                0.6,
                (i + 1) as i64,
            );
            store.save(&mem).await.unwrap();
        }

        // Add superseded memories (should not be counted or evicted)
        for i in 0..5 {
            let mut mem = make_memory(&format!("mem-sup-{i}"), MemorySource::Extracted, 0.6, 10);
            mem.status = MemoryStatus::Superseded;
            store.save(&mem).await.unwrap();
        }

        // Add forgotten memories
        for i in 0..3 {
            let mut mem = make_memory(&format!("mem-forg-{i}"), MemorySource::Extracted, 0.6, 10);
            mem.status = MemoryStatus::Forgotten;
            store.save(&mem).await.unwrap();
        }

        let config = test_config(10);
        run_eviction_sweep(&store, &config, &None).await.unwrap();

        // Only active non-restricted are counted: 12 active, evict to 9
        let count = store.count_active().await.unwrap();
        assert_eq!(count, 9);
    }
}
