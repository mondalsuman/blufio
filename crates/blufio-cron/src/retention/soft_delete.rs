// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Phase 1: Soft-delete expired records.
//!
//! For each table (messages, sessions, cost_ledger, memories):
//! - Sets `deleted_at` on records older than the configured retention period
//! - Applies separate retention periods for restricted-classified data
//!
//! CRITICAL: Never touches audit.db. Retention operates ONLY on the main
//! database connection (architectural isolation per RETN-04).

use std::sync::Arc;

use blufio_config::model::RetentionPeriods;
use tokio_rusqlite::Connection;

use super::TableBreakdown;

/// Table metadata for soft-delete operations.
struct TableConfig {
    /// SQL table name.
    name: &'static str,
    /// Whether this table has a `classification` column for restricted filtering.
    has_classification: bool,
}

/// All tables subject to retention (never audit tables).
const TABLES: &[TableConfig] = &[
    TableConfig {
        name: "messages",
        has_classification: true,
    },
    TableConfig {
        name: "sessions",
        has_classification: true,
    },
    TableConfig {
        name: "cost_ledger",
        has_classification: false,
    },
    TableConfig {
        name: "memories",
        has_classification: true,
    },
];

/// Get the retention days for a given table from the periods config.
fn retention_days(periods: &RetentionPeriods, table: &str) -> Option<u64> {
    match table {
        "messages" => periods.messages,
        "sessions" => periods.sessions,
        "cost_ledger" => periods.cost_records,
        "memories" => periods.memories,
        _ => None,
    }
}

/// Run soft-delete across all retention-managed tables.
///
/// For each table, marks records as soft-deleted when:
/// - Non-restricted records exceed `periods` retention days
/// - Restricted records exceed `restricted` retention days (separate period)
///
/// Returns per-table counts of newly soft-deleted records.
pub async fn run_soft_delete(
    conn: &Arc<Connection>,
    periods: &RetentionPeriods,
    restricted: &RetentionPeriods,
) -> Result<TableBreakdown, String> {
    let periods = periods.clone();
    let restricted = restricted.clone();

    conn.call(move |conn| -> Result<TableBreakdown, rusqlite::Error> {
        let mut breakdown = TableBreakdown::default();

        for table in TABLES {
            let mut count: u64 = 0;

            // Default/non-restricted records
            if let Some(days) = retention_days(&periods, table.name) {
                if table.has_classification {
                    // Soft-delete non-restricted records past retention period
                    count += conn.execute(
                        &format!(
                            "UPDATE {} SET deleted_at = datetime('now') \
                             WHERE deleted_at IS NULL \
                               AND created_at < datetime('now', '-{} days') \
                               AND (classification IS NULL OR classification != 'restricted')",
                            table.name, days
                        ),
                        [],
                    )? as u64;
                } else {
                    // No classification column (e.g., cost_ledger)
                    count += conn.execute(
                        &format!(
                            "UPDATE {} SET deleted_at = datetime('now') \
                             WHERE deleted_at IS NULL \
                               AND created_at < datetime('now', '-{} days')",
                            table.name, days
                        ),
                        [],
                    )? as u64;
                }
            }

            // Restricted records (separate retention period)
            if table.has_classification
                && let Some(restricted_days) = retention_days(&restricted, table.name)
            {
                count += conn.execute(
                    &format!(
                        "UPDATE {} SET deleted_at = datetime('now') \
                         WHERE deleted_at IS NULL \
                           AND classification = 'restricted' \
                           AND created_at < datetime('now', '-{} days')",
                        table.name, restricted_days
                    ),
                    [],
                )? as u64;
            }

            // Record per-table count
            match table.name {
                "messages" => breakdown.messages = count,
                "sessions" => breakdown.sessions = count,
                "cost_ledger" => breakdown.cost_records = count,
                "memories" => breakdown.memories = count,
                _ => {}
            }
        }

        Ok(breakdown)
    })
    .await
    .map_err(|e| format!("Soft-delete failed: {e}"))
}
