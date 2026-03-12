// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Phase 2: Permanently delete past-grace records.
//!
//! For each table, permanently removes records whose `deleted_at` timestamp
//! is older than the configured grace period.
//!
//! CRITICAL for memories table: Delete from `memories` table directly (not
//! `memories_fts`). The FTS5 AFTER DELETE trigger handles FTS cleanup
//! automatically.
//!
//! CRITICAL: Never touches audit.db (architectural isolation per RETN-04).

use std::sync::Arc;

use tokio_rusqlite::Connection;

use super::TableBreakdown;

/// Tables subject to permanent deletion.
const TABLES: &[&str] = &["messages", "sessions", "cost_ledger", "memories"];

/// Permanently delete records past the grace period.
///
/// For each table, deletes records where `deleted_at` is not null and
/// `deleted_at` is older than `grace_period_days` ago.
///
/// Returns per-table counts of permanently deleted records.
pub async fn run_permanent_delete(
    conn: &Arc<Connection>,
    grace_period_days: u64,
) -> Result<TableBreakdown, String> {
    conn.call(move |conn| -> Result<TableBreakdown, rusqlite::Error> {
        let mut breakdown = TableBreakdown::default();

        for table in TABLES {
            let deleted = conn.execute(
                &format!(
                    "DELETE FROM {} \
                     WHERE deleted_at IS NOT NULL \
                       AND deleted_at < datetime('now', '-{} days')",
                    table, grace_period_days
                ),
                [],
            )? as u64;

            match *table {
                "messages" => breakdown.messages = deleted,
                "sessions" => breakdown.sessions = deleted,
                "cost_ledger" => breakdown.cost_records = deleted,
                "memories" => breakdown.memories = deleted,
                _ => {}
            }
        }

        Ok(breakdown)
    })
    .await
    .map_err(|e| format!("Permanent delete failed: {e}"))
}
