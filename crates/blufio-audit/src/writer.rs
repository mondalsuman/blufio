// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Async background writer for audit trail entries.
//!
//! [`AuditWriter`] owns a bounded mpsc channel and a background tokio task.
//! Callers submit [`PendingEntry`] values via [`try_send`](AuditWriter::try_send),
//! which never blocks. The background task drains the channel, computes
//! SHA-256 hashes to maintain the chain, and batch-flushes entries to
//! `audit.db` in a single SQLite transaction.
//!
//! Flush triggers:
//! - **Batch size:** 64 entries accumulated
//! - **Time interval:** 1 second since last flush with pending entries
//! - **Explicit:** [`flush()`](AuditWriter::flush) via oneshot channel
//! - **Shutdown:** [`shutdown()`](AuditWriter::shutdown) flushes remaining entries

use std::time::Duration;

use metrics::counter;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, error, warn};

use crate::chain::{GENESIS_HASH, compute_entry_hash};
use crate::models::{AuditError, PendingEntry};

/// Maximum entries buffered before a flush is triggered.
const BATCH_SIZE: usize = 64;

/// Time interval for flushing pending entries.
const FLUSH_INTERVAL: Duration = Duration::from_secs(1);

/// Channel capacity for the audit writer.
const CHANNEL_CAPACITY: usize = 1024;

/// Commands sent to the background writer task.
pub enum AuditCommand {
    /// Write a pending entry to the audit trail.
    Write(PendingEntry),
    /// Flush all pending entries immediately and signal completion.
    Flush(oneshot::Sender<Result<(), AuditError>>),
    /// Shut down the background task after flushing remaining entries.
    Shutdown,
}

/// Async background writer for the audit trail.
///
/// Accepts [`PendingEntry`] values via [`try_send`](Self::try_send), batches them,
/// and flushes to `audit.db` with correct hash chain maintenance.
pub struct AuditWriter {
    tx: mpsc::Sender<AuditCommand>,
    task_handle: Option<JoinHandle<()>>,
}

impl AuditWriter {
    /// Create a new `AuditWriter` that writes to the given database path.
    ///
    /// This will:
    /// 1. Open (or create) the audit database via `open_connection`
    /// 2. Run embedded migrations
    /// 3. Recover the chain head from the last entry (or use GENESIS_HASH)
    /// 4. Spawn a background task to drain the channel and batch-flush entries
    pub async fn new(db_path: &str) -> Result<Self, AuditError> {
        let conn = blufio_storage::database::open_connection(db_path)
            .await
            .map_err(|e| AuditError::DbUnavailable(e.to_string()))?;

        // Run migrations
        conn.call(|conn| {
            crate::migrations::run(conn).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::other(e.to_string())),
                )
            })
        })
        .await
        .map_err(|e| AuditError::DbUnavailable(format!("migration failed: {e}")))?;

        // Apply PRAGMAs
        conn.call(|conn| -> Result<(), rusqlite::Error> {
            conn.execute_batch("PRAGMA journal_mode = WAL;")?;
            conn.execute_batch(
                "PRAGMA synchronous = NORMAL;\
                 PRAGMA foreign_keys = ON;",
            )?;
            Ok(())
        })
        .await
        .map_err(|e| AuditError::DbUnavailable(format!("pragma setup failed: {e}")))?;

        // Recover chain head
        let chain_head: String = conn
            .call(|conn| -> Result<String, rusqlite::Error> {
                match conn.query_row(
                    "SELECT entry_hash FROM audit_entries ORDER BY id DESC LIMIT 1",
                    [],
                    |row| row.get(0),
                ) {
                    Ok(hash) => Ok(hash),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(GENESIS_HASH.to_string()),
                    Err(e) => Err(e),
                }
            })
            .await
            .map_err(|e| AuditError::DbUnavailable(format!("chain head recovery failed: {e}")))?;

        debug!(chain_head = %chain_head, "audit writer initialized");

        let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);

        let task_handle = tokio::spawn(Self::background_task(conn, rx, chain_head));

        Ok(Self {
            tx,
            task_handle: Some(task_handle),
        })
    }

    /// The background task that owns the database connection and processes commands.
    async fn background_task(
        conn: tokio_rusqlite::Connection,
        mut rx: mpsc::Receiver<AuditCommand>,
        mut chain_head: String,
    ) {
        let mut pending: Vec<PendingEntry> = Vec::with_capacity(BATCH_SIZE);
        let mut flush_interval = tokio::time::interval(FLUSH_INTERVAL);
        // Skip the immediate first tick
        flush_interval.tick().await;

        loop {
            tokio::select! {
                cmd = rx.recv() => {
                    match cmd {
                        Some(AuditCommand::Write(entry)) => {
                            pending.push(entry);
                            if pending.len() >= BATCH_SIZE {
                                chain_head = Self::flush_batch(&conn, &mut pending, &chain_head).await;
                            }
                        }
                        Some(AuditCommand::Flush(reply)) => {
                            chain_head = Self::flush_batch(&conn, &mut pending, &chain_head).await;
                            let _ = reply.send(Ok(()));
                        }
                        Some(AuditCommand::Shutdown) => {
                            let _ = Self::flush_batch(&conn, &mut pending, &chain_head).await;
                            debug!("audit writer shutting down");
                            break;
                        }
                        None => {
                            // Channel closed -- flush and exit
                            if !pending.is_empty() {
                                Self::flush_batch(&conn, &mut pending, &chain_head).await;
                            }
                            debug!("audit writer channel closed");
                            break;
                        }
                    }
                }
                _ = flush_interval.tick() => {
                    if !pending.is_empty() {
                        chain_head = Self::flush_batch(&conn, &mut pending, &chain_head).await;
                    }
                }
            }
        }
    }

    /// Flush a batch of pending entries to the database in a single transaction.
    ///
    /// Computes entry hashes and maintains the chain head. Returns the new chain head.
    async fn flush_batch(
        conn: &tokio_rusqlite::Connection,
        pending: &mut Vec<PendingEntry>,
        chain_head: &str,
    ) -> String {
        if pending.is_empty() {
            return chain_head.to_string();
        }

        let batch = std::mem::take(pending);
        let batch_size = batch.len();
        let head = chain_head.to_string();
        let start = std::time::Instant::now();

        let result = conn
            .call(move |conn| -> Result<String, rusqlite::Error> {
                let tx = conn.transaction()?;
                let mut current_head = head;

                for entry in &batch {
                    let entry_hash = compute_entry_hash(
                        &current_head,
                        &entry.timestamp,
                        &entry.event_type,
                        &entry.action,
                        &entry.resource_type,
                        &entry.resource_id,
                    );

                    tx.execute(
                        "INSERT INTO audit_entries \
                         (entry_hash, prev_hash, timestamp, event_type, action, \
                          resource_type, resource_id, actor, session_id, details_json) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                        rusqlite::params![
                            entry_hash,
                            current_head,
                            entry.timestamp,
                            entry.event_type,
                            entry.action,
                            entry.resource_type,
                            entry.resource_id,
                            entry.actor,
                            entry.session_id,
                            entry.details_json,
                        ],
                    )?;

                    current_head = entry_hash;
                }

                tx.commit()?;
                Ok(current_head)
            })
            .await;

        let elapsed = start.elapsed();

        match result {
            Ok(new_head) => {
                counter!("blufio_audit_entries_total").increment(batch_size as u64);
                counter!("blufio_audit_batch_flush_total").increment(1);
                debug!(
                    batch_size = batch_size,
                    duration_ms = elapsed.as_millis(),
                    "audit batch flushed"
                );
                new_head
            }
            Err(e) => {
                counter!("blufio_audit_errors_total").increment(1);
                error!(error = %e, batch_size = batch_size, "audit batch flush failed");
                // Do NOT update chain head on failure
                chain_head.to_string()
            }
        }
    }

    /// Submit a pending entry to the audit writer.
    ///
    /// Returns `Ok(())` if the entry was accepted into the channel.
    /// Returns `Err` if the channel is full (the entry is dropped with a warning).
    pub fn try_send(&self, entry: PendingEntry) -> Result<(), AuditError> {
        match self.tx.try_send(AuditCommand::Write(entry)) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                counter!("blufio_audit_dropped_total").increment(1);
                warn!("audit channel full, entry dropped");
                Err(AuditError::FlushFailed(
                    "audit channel full, entry dropped".to_string(),
                ))
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                Err(AuditError::DbUnavailable("audit writer closed".to_string()))
            }
        }
    }

    /// Flush all pending entries immediately and wait for completion.
    pub async fn flush(&self) -> Result<(), AuditError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(AuditCommand::Flush(tx))
            .await
            .map_err(|_| AuditError::DbUnavailable("audit writer closed".to_string()))?;
        rx.await
            .map_err(|_| AuditError::FlushFailed("flush reply dropped".to_string()))?
    }

    /// Flush remaining entries and shut down the background task.
    pub async fn shutdown(mut self) {
        let _ = self.tx.send(AuditCommand::Shutdown).await;
        if let Some(handle) = self.task_handle.take() {
            let _ = handle.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::{GENESIS_HASH, verify_chain};
    use tempfile::tempdir;

    /// Helper to create a PendingEntry with minimal data.
    fn make_entry(event_type: &str, action: &str) -> PendingEntry {
        PendingEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            event_type: event_type.to_string(),
            action: action.to_string(),
            resource_type: "test".to_string(),
            resource_id: "t1".to_string(),
            actor: "system".to_string(),
            session_id: "test-session".to_string(),
            details_json: "{}".to_string(),
        }
    }

    #[tokio::test]
    async fn writer_creates_db_and_table() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("audit.db");
        let writer = AuditWriter::new(db_path.to_str().unwrap()).await.unwrap();
        assert!(db_path.exists());
        writer.shutdown().await;
    }

    #[tokio::test]
    async fn write_and_flush_produces_valid_chain() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("audit.db");
        let writer = AuditWriter::new(db_path.to_str().unwrap()).await.unwrap();

        // Send 3 entries
        writer
            .try_send(make_entry("session.created", "create"))
            .unwrap();
        writer
            .try_send(make_entry("memory.created", "create"))
            .unwrap();
        writer
            .try_send(make_entry("session.closed", "close"))
            .unwrap();

        // Flush and verify
        writer.flush().await.unwrap();

        // Open a sync connection to verify
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let report = verify_chain(&conn).unwrap();
        assert!(report.ok);
        assert_eq!(report.verified, 3);

        writer.shutdown().await;
    }

    #[tokio::test]
    async fn batch_size_triggers_flush() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("audit.db");
        let writer = AuditWriter::new(db_path.to_str().unwrap()).await.unwrap();

        // Send exactly BATCH_SIZE entries
        for i in 0..BATCH_SIZE {
            writer
                .try_send(make_entry(&format!("event.{i}"), "test"))
                .unwrap();
        }

        // Give the background task a moment to process
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify entries were flushed (batch size trigger)
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM audit_entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, BATCH_SIZE as i64);

        let report = verify_chain(&conn).unwrap();
        assert!(report.ok);

        writer.shutdown().await;
    }

    #[tokio::test]
    async fn chain_head_recovery_after_restart() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("audit.db");

        // First writer: write some entries
        {
            let writer = AuditWriter::new(db_path.to_str().unwrap()).await.unwrap();
            writer
                .try_send(make_entry("session.created", "create"))
                .unwrap();
            writer
                .try_send(make_entry("memory.created", "create"))
                .unwrap();
            writer.flush().await.unwrap();
            writer.shutdown().await;
        }

        // Second writer: should recover chain head
        {
            let writer = AuditWriter::new(db_path.to_str().unwrap()).await.unwrap();
            writer
                .try_send(make_entry("session.closed", "close"))
                .unwrap();
            writer.flush().await.unwrap();
            writer.shutdown().await;
        }

        // Verify the full chain is valid across both writers
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let report = verify_chain(&conn).unwrap();
        assert!(report.ok);
        assert_eq!(report.verified, 3);
    }

    #[tokio::test]
    async fn empty_db_uses_genesis_hash() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("audit.db");
        let writer = AuditWriter::new(db_path.to_str().unwrap()).await.unwrap();

        writer
            .try_send(make_entry("session.created", "create"))
            .unwrap();
        writer.flush().await.unwrap();

        // Check that the first entry has GENESIS_HASH as prev_hash
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let prev_hash: String = conn
            .query_row(
                "SELECT prev_hash FROM audit_entries WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(prev_hash, GENESIS_HASH);

        writer.shutdown().await;
    }

    #[tokio::test]
    async fn flush_via_oneshot_completes_writes() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("audit.db");
        let writer = AuditWriter::new(db_path.to_str().unwrap()).await.unwrap();

        for i in 0..10 {
            writer
                .try_send(make_entry(&format!("event.{i}"), "test"))
                .unwrap();
        }

        // flush() returns Ok after all entries are written
        writer.flush().await.unwrap();

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM audit_entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 10);

        writer.shutdown().await;
    }

    #[tokio::test]
    async fn shutdown_flushes_remaining_entries() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("audit.db");
        let writer = AuditWriter::new(db_path.to_str().unwrap()).await.unwrap();

        for i in 0..5 {
            writer
                .try_send(make_entry(&format!("event.{i}"), "test"))
                .unwrap();
        }

        // Shutdown should flush remaining entries
        writer.shutdown().await;

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM audit_entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 5);
    }
}
