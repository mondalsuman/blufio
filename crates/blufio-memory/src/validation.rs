// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Memory validation: duplicate detection, conflict resolution, and stale cleanup.
//!
//! Runs pairwise comparison of active memory embeddings to detect duplicates
//! (>0.9 cosine similarity) and conflicts (0.7-0.9 similarity). Also flags
//! memories older than the configured stale threshold at decay floor for removal.

use std::sync::Arc;

use blufio_bus::EventBus;
use blufio_bus::events::{BusEvent, MemoryEvent, new_event_id, now_timestamp};
use blufio_config::model::MemoryConfig;
use blufio_core::error::BlufioError;
use tracing::info;

use crate::store::MemoryStore;
use crate::types::{Memory, cosine_similarity};

/// Similarity threshold above which two memories are considered duplicates.
const DEDUP_THRESHOLD: f32 = 0.9;

/// Similarity threshold for conflict detection (between duplicate and unrelated).
const CONFLICT_THRESHOLD: f32 = 0.7;

/// Result of a validation run.
#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    /// Number of duplicate pairs found and resolved.
    pub duplicates_found: usize,
    /// Number of conflict pairs found and resolved.
    pub conflicts_found: usize,
    /// Number of stale memories flagged and soft-deleted.
    pub stale_found: usize,
}

/// Run full validation: detect duplicates, conflicts, and stale memories.
///
/// - Duplicates (sim > 0.9): supersede the lower-confidence memory.
/// - Conflicts (0.7 < sim <= 0.9): supersede the older memory (newer wins).
/// - Stale: memories older than `stale_threshold_days` where decay has hit the floor.
pub async fn run_validation(
    store: &MemoryStore,
    config: &MemoryConfig,
    event_bus: &Option<Arc<EventBus>>,
) -> Result<ValidationResult, BlufioError> {
    let memories = store.get_all_active_with_embeddings().await?;
    let mut result = ValidationResult::default();

    if memories.len() < 2 {
        return Ok(result);
    }

    // Track which IDs have been superseded/deleted in this pass to avoid double-processing
    let mut resolved_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Pairwise comparison for duplicates and conflicts
    for i in 0..memories.len() {
        if resolved_ids.contains(&memories[i].id) {
            continue;
        }
        for j in (i + 1)..memories.len() {
            if resolved_ids.contains(&memories[j].id) {
                continue;
            }
            if memories[i].embedding.len() != memories[j].embedding.len()
                || memories[i].embedding.is_empty()
            {
                continue;
            }

            let sim = cosine_similarity(&memories[i].embedding, &memories[j].embedding);

            if sim > DEDUP_THRESHOLD {
                // Duplicate: supersede the lower-confidence one
                let (keep, remove) = if memories[i].confidence >= memories[j].confidence {
                    (&memories[i], &memories[j])
                } else {
                    (&memories[j], &memories[i])
                };

                store.supersede(&remove.id, &keep.id).await?;
                resolved_ids.insert(remove.id.clone());
                result.duplicates_found += 1;

                if let Some(bus) = event_bus {
                    bus.publish(BusEvent::Memory(MemoryEvent::Updated {
                        event_id: new_event_id(),
                        timestamp: now_timestamp(),
                        memory_id: remove.id.clone(),
                    }))
                    .await;
                }
            } else if sim > CONFLICT_THRESHOLD {
                // Conflict: newer wins (compare created_at lexicographically)
                let (keep, remove) = if memories[i].created_at >= memories[j].created_at {
                    (&memories[i], &memories[j])
                } else {
                    (&memories[j], &memories[i])
                };

                store.supersede(&remove.id, &keep.id).await?;
                resolved_ids.insert(remove.id.clone());
                result.conflicts_found += 1;

                if let Some(bus) = event_bus {
                    bus.publish(BusEvent::Memory(MemoryEvent::Updated {
                        event_id: new_event_id(),
                        timestamp: now_timestamp(),
                        memory_id: remove.id.clone(),
                    }))
                    .await;
                }
            }
        }
    }

    // Stale detection: memories older than threshold where decay has hit floor
    let now = chrono::Utc::now();
    let stale_threshold_days = config.stale_threshold_days as i64;

    for mem in &memories {
        if resolved_ids.contains(&mem.id) {
            continue;
        }

        let days_old = chrono::DateTime::parse_from_rfc3339(&mem.created_at)
            .or_else(|_| chrono::DateTime::parse_from_str(&mem.created_at, "%Y-%m-%dT%H:%M:%S%.fZ"))
            .map(|dt| (now - dt.with_timezone(&chrono::Utc)).num_days())
            .unwrap_or(0);

        if days_old >= stale_threshold_days {
            let decay = config.decay_factor.powf(days_old as f64);
            if decay <= config.decay_floor {
                store.soft_delete(&mem.id).await?;
                resolved_ids.insert(mem.id.clone());
                result.stale_found += 1;

                if let Some(bus) = event_bus {
                    bus.publish(BusEvent::Memory(MemoryEvent::Deleted {
                        event_id: new_event_id(),
                        timestamp: now_timestamp(),
                        memory_id: mem.id.clone(),
                    }))
                    .await;
                }
            }
        }
    }

    if result.duplicates_found > 0 || result.conflicts_found > 0 || result.stale_found > 0 {
        info!(
            duplicates = result.duplicates_found,
            conflicts = result.conflicts_found,
            stale = result.stale_found,
            "Validation complete"
        );
    }

    Ok(result)
}

/// Dry-run validation: detect issues without modifying the store.
///
/// Returns counts of duplicates, conflicts, and stale memories found.
pub fn run_validation_dry_run(memories: &[Memory], config: &MemoryConfig) -> ValidationResult {
    let mut result = ValidationResult::default();

    if memories.len() < 2 {
        // Still check for stale even with 0-1 memories
        let now = chrono::Utc::now();
        let stale_threshold_days = config.stale_threshold_days as i64;
        for mem in memories {
            let days_old = chrono::DateTime::parse_from_rfc3339(&mem.created_at)
                .or_else(|_| {
                    chrono::DateTime::parse_from_str(&mem.created_at, "%Y-%m-%dT%H:%M:%S%.fZ")
                })
                .map(|dt| (now - dt.with_timezone(&chrono::Utc)).num_days())
                .unwrap_or(0);
            if days_old >= stale_threshold_days
                && config.decay_factor.powf(days_old as f64) <= config.decay_floor
            {
                result.stale_found += 1;
            }
        }
        return result;
    }

    let mut resolved_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Pairwise comparison
    for i in 0..memories.len() {
        if resolved_ids.contains(&memories[i].id) {
            continue;
        }
        for j in (i + 1)..memories.len() {
            if resolved_ids.contains(&memories[j].id) {
                continue;
            }
            if memories[i].embedding.len() != memories[j].embedding.len()
                || memories[i].embedding.is_empty()
            {
                continue;
            }

            let sim = cosine_similarity(&memories[i].embedding, &memories[j].embedding);

            if sim > DEDUP_THRESHOLD {
                let remove_id = if memories[i].confidence >= memories[j].confidence {
                    &memories[j].id
                } else {
                    &memories[i].id
                };
                resolved_ids.insert(remove_id.clone());
                result.duplicates_found += 1;
            } else if sim > CONFLICT_THRESHOLD {
                let remove_id = if memories[i].created_at >= memories[j].created_at {
                    &memories[j].id
                } else {
                    &memories[i].id
                };
                resolved_ids.insert(remove_id.clone());
                result.conflicts_found += 1;
            }
        }
    }

    // Stale detection
    let now = chrono::Utc::now();
    let stale_threshold_days = config.stale_threshold_days as i64;

    for mem in memories {
        if resolved_ids.contains(&mem.id) {
            continue;
        }

        let days_old = chrono::DateTime::parse_from_rfc3339(&mem.created_at)
            .or_else(|_| chrono::DateTime::parse_from_str(&mem.created_at, "%Y-%m-%dT%H:%M:%S%.fZ"))
            .map(|dt| (now - dt.with_timezone(&chrono::Utc)).num_days())
            .unwrap_or(0);

        if days_old >= stale_threshold_days {
            let decay = config.decay_factor.powf(days_old as f64);
            if decay <= config.decay_floor {
                resolved_ids.insert(mem.id.clone());
                result.stale_found += 1;
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use blufio_core::classification::DataClassification;
    use tokio_rusqlite::Connection;

    use crate::types::{MemorySource, MemoryStatus};

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
        conn
    }

    fn make_memory_with_embedding(
        id: &str,
        content: &str,
        embedding: Vec<f32>,
        confidence: f64,
        days_old: i64,
    ) -> Memory {
        let created = chrono::Utc::now() - chrono::Duration::days(days_old);
        Memory {
            id: id.to_string(),
            content: content.to_string(),
            embedding,
            source: MemorySource::Extracted,
            confidence,
            status: MemoryStatus::Active,
            superseded_by: None,
            session_id: Some("test-session".to_string()),
            classification: DataClassification::default(),
            created_at: created.to_rfc3339(),
            updated_at: created.to_rfc3339(),
        }
    }

    /// Create a normalized embedding vector.
    ///
    /// `seed` determines which dimension gets maximum weight, producing orthogonal
    /// vectors for different seeds. If `seed` values are the same, the vectors
    /// will be identical (sim = 1.0).
    fn make_embedding(seed: usize, dim: usize) -> Vec<f32> {
        let mut v = vec![0.01_f32; dim];
        // Place a large value at the seed position (modulo dim)
        v[seed % dim] = 1.0;
        // Normalize
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        for x in &mut v {
            *x /= norm;
        }
        v
    }

    /// Create a conflict-range embedding pair.
    ///
    /// Returns two embeddings with cosine similarity in the 0.7-0.9 range.
    fn make_conflict_pair(dim: usize) -> (Vec<f32>, Vec<f32>) {
        // Start with same base vector and perturb some dimensions
        let mut emb1 = vec![0.1_f32; dim];
        emb1[0] = 1.0;
        emb1[1] = 0.5;
        let norm1: f32 = emb1.iter().map(|x| x * x).sum::<f32>().sqrt();
        for x in &mut emb1 {
            *x /= norm1;
        }

        let mut emb2 = emb1.clone();
        // Perturb enough dimensions to drop similarity into 0.7-0.9 range
        for val in emb2.iter_mut().take(50).skip(2) {
            *val = -*val + 0.05;
        }
        let norm2: f32 = emb2.iter().map(|x| x * x).sum::<f32>().sqrt();
        for x in &mut emb2 {
            *x /= norm2;
        }

        let sim = cosine_similarity(&emb1, &emb2);
        // If not in range, adjust iteratively
        if sim <= CONFLICT_THRESHOLD || sim > DEDUP_THRESHOLD {
            // Fallback: construct analytically
            // cos(theta) = 0.8 roughly
            let mut a = vec![0.0_f32; dim];
            let mut b = vec![0.0_f32; dim];
            a[0] = 1.0;
            // b = 0.8 * a + 0.6 * orthogonal
            b[0] = 0.8;
            b[1] = 0.6;
            return (a, b);
        }

        (emb1, emb2)
    }

    fn test_config() -> MemoryConfig {
        MemoryConfig {
            decay_factor: 0.95,
            decay_floor: 0.1,
            stale_threshold_days: 180,
            importance_boost_explicit: 1.0,
            importance_boost_extracted: 0.6,
            importance_boost_file: 0.8,
            max_entries: 10_000,
            ..Default::default()
        }
    }

    /// Helper to insert a raw memory row (bypassing save's event bus).
    async fn insert_raw(store: &MemoryStore, mem: &Memory) {
        store.save(mem).await.unwrap();
    }

    #[tokio::test]
    async fn validation_detect_duplicates_high_similarity() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        // Create two memories with identical embeddings (sim = 1.0 > 0.9)
        let emb = make_embedding(0, 384);
        let mem1 = make_memory_with_embedding("dup-1", "User likes cats", emb.clone(), 0.9, 1);
        let mem2 = make_memory_with_embedding("dup-2", "User likes cats too", emb, 0.6, 2);

        insert_raw(&store, &mem1).await;
        insert_raw(&store, &mem2).await;

        let config = test_config();
        let result = run_validation(&store, &config, &None).await.unwrap();

        assert_eq!(result.duplicates_found, 1, "should detect 1 duplicate pair");
        assert_eq!(result.conflicts_found, 0);

        // The lower-confidence one should be superseded
        let retrieved = store.get_by_id("dup-2").await.unwrap().unwrap();
        assert_eq!(retrieved.status, MemoryStatus::Superseded);
        assert_eq!(retrieved.superseded_by, Some("dup-1".to_string()));
    }

    #[tokio::test]
    async fn validation_detect_duplicates_keeps_higher_confidence() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        let emb = make_embedding(0, 384);
        let mem1 = make_memory_with_embedding("dup-low", "Fact A", emb.clone(), 0.5, 1);
        let mem2 = make_memory_with_embedding("dup-high", "Fact A variant", emb, 0.9, 2);

        insert_raw(&store, &mem1).await;
        insert_raw(&store, &mem2).await;

        let config = test_config();
        let result = run_validation(&store, &config, &None).await.unwrap();

        assert_eq!(result.duplicates_found, 1);

        // dup-low (lower confidence) should be superseded by dup-high
        let low = store.get_by_id("dup-low").await.unwrap().unwrap();
        assert_eq!(low.status, MemoryStatus::Superseded);
        assert_eq!(low.superseded_by, Some("dup-high".to_string()));

        // dup-high should remain active
        let high = store.get_by_id("dup-high").await.unwrap().unwrap();
        assert_eq!(high.status, MemoryStatus::Active);
    }

    #[tokio::test]
    async fn validation_detect_conflicts_medium_similarity() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        // Create two memories with moderate similarity (0.7 < sim <= 0.9)
        let (emb1, emb2) = make_conflict_pair(384);

        let sim = cosine_similarity(&emb1, &emb2);
        assert!(
            sim > CONFLICT_THRESHOLD && sim <= DEDUP_THRESHOLD,
            "Test embeddings should be in conflict range (0.7-0.9), got {sim}"
        );

        let mem1 = make_memory_with_embedding("conf-old", "Dog is named Max", emb1, 0.8, 10);
        let mem2 = make_memory_with_embedding("conf-new", "Dog is named Luna", emb2, 0.8, 1);

        insert_raw(&store, &mem1).await;
        insert_raw(&store, &mem2).await;

        let config = test_config();
        let result = run_validation(&store, &config, &None).await.unwrap();

        assert_eq!(result.conflicts_found, 1, "should detect 1 conflict");

        // Older memory should be superseded (newer wins)
        let old = store.get_by_id("conf-old").await.unwrap().unwrap();
        assert_eq!(old.status, MemoryStatus::Superseded);
        assert_eq!(old.superseded_by, Some("conf-new".to_string()));
    }

    #[tokio::test]
    async fn validation_detect_stale_memories() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        // Memory from 200 days ago (past stale_threshold_days=180)
        // With decay_factor=0.95, 0.95^200 = ~0.0000355 < decay_floor=0.1
        // Use orthogonal embeddings (different seed) so they won't conflict
        let emb = make_embedding(0, 384);
        let mem = make_memory_with_embedding("stale-1", "Ancient memory", emb, 0.5, 200);
        insert_raw(&store, &mem).await;

        // Fresh memory (1 day old, should NOT be stale)
        let emb2 = make_embedding(100, 384);
        let fresh = make_memory_with_embedding("fresh-1", "Recent memory", emb2, 0.5, 1);
        insert_raw(&store, &fresh).await;

        let config = test_config();
        let result = run_validation(&store, &config, &None).await.unwrap();

        assert_eq!(result.stale_found, 1, "should detect 1 stale memory");
        assert_eq!(result.duplicates_found, 0);
        assert_eq!(result.conflicts_found, 0);

        // Stale memory should be soft-deleted (forgotten)
        let stale = store.get_by_id("stale-1").await.unwrap().unwrap();
        assert_eq!(stale.status, MemoryStatus::Forgotten);
    }

    #[tokio::test]
    async fn validation_with_no_issues_does_nothing() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        // Two memories with very different embeddings (near-orthogonal)
        let emb1 = make_embedding(0, 384);
        let emb2 = make_embedding(100, 384);

        let mem1 = make_memory_with_embedding("clean-1", "User likes cats", emb1, 0.8, 1);
        let mem2 = make_memory_with_embedding("clean-2", "User works at Acme", emb2, 0.8, 1);

        insert_raw(&store, &mem1).await;
        insert_raw(&store, &mem2).await;

        let config = test_config();
        let result = run_validation(&store, &config, &None).await.unwrap();

        assert_eq!(result.duplicates_found, 0);
        assert_eq!(result.conflicts_found, 0);
        assert_eq!(result.stale_found, 0);

        // Both should still be active
        let m1 = store.get_by_id("clean-1").await.unwrap().unwrap();
        let m2 = store.get_by_id("clean-2").await.unwrap().unwrap();
        assert_eq!(m1.status, MemoryStatus::Active);
        assert_eq!(m2.status, MemoryStatus::Active);
    }

    #[tokio::test]
    async fn validation_run_validation_auto_resolves_duplicates() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        let bus = Arc::new(EventBus::new(16));
        let mut rx = bus.subscribe();
        let event_bus = Some(bus);

        let emb = make_embedding(0, 384);
        let mem1 = make_memory_with_embedding("auto-1", "Fact", emb.clone(), 0.9, 1);
        let mem2 = make_memory_with_embedding("auto-2", "Fact copy", emb, 0.6, 2);

        insert_raw(&store, &mem1).await;
        insert_raw(&store, &mem2).await;

        let config = test_config();
        let result = run_validation(&store, &config, &event_bus).await.unwrap();
        assert_eq!(result.duplicates_found, 1);

        // Should have emitted Updated event
        let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
            .await
            .expect("timeout")
            .expect("no event");

        match event {
            BusEvent::Memory(MemoryEvent::Updated { memory_id, .. }) => {
                assert_eq!(memory_id, "auto-2");
            }
            other => panic!("Expected MemoryEvent::Updated, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn validation_run_validation_auto_resolves_conflicts() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        let bus = Arc::new(EventBus::new(16));
        let mut rx = bus.subscribe();
        let event_bus = Some(bus);

        let (emb1, emb2) = make_conflict_pair(384);

        let mem_old = make_memory_with_embedding("old-fact", "Max is a dog", emb1, 0.8, 30);
        let mem_new = make_memory_with_embedding("new-fact", "Luna is a dog", emb2, 0.8, 1);

        insert_raw(&store, &mem_old).await;
        insert_raw(&store, &mem_new).await;

        let config = test_config();
        let result = run_validation(&store, &config, &event_bus).await.unwrap();
        assert_eq!(result.conflicts_found, 1);

        let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
            .await
            .expect("timeout")
            .expect("no event");

        match event {
            BusEvent::Memory(MemoryEvent::Updated { memory_id, .. }) => {
                assert_eq!(memory_id, "old-fact", "older memory should be superseded");
            }
            other => panic!("Expected MemoryEvent::Updated, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dry_run_returns_counts_without_modifications() {
        let conn = setup_test_db().await;
        let store = MemoryStore::new(conn);

        let emb = make_embedding(0, 384);
        let mem1 = make_memory_with_embedding("dry-1", "Fact A", emb.clone(), 0.9, 1);
        let mem2 = make_memory_with_embedding("dry-2", "Fact A copy", emb, 0.6, 2);

        insert_raw(&store, &mem1).await;
        insert_raw(&store, &mem2).await;

        let config = test_config();
        let memories = store.get_all_active_with_embeddings().await.unwrap();
        let result = run_validation_dry_run(&memories, &config);

        assert_eq!(result.duplicates_found, 1, "dry run should find duplicate");

        // Both memories should still be active (no modification)
        let m1 = store.get_by_id("dry-1").await.unwrap().unwrap();
        let m2 = store.get_by_id("dry-2").await.unwrap().unwrap();
        assert_eq!(m1.status, MemoryStatus::Active);
        assert_eq!(m2.status, MemoryStatus::Active);
    }
}
