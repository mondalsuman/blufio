// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Job execution history persistence.
//!
//! Provides read/write operations for the `cron_history` table, including
//! start/finish recording, querying, and cleanup of excess entries.

use std::sync::Arc;

use tokio_rusqlite::Connection;

/// Maximum output string length stored in cron_history.
const MAX_OUTPUT_LENGTH: usize = 4096;

/// A single cron job execution history entry.
#[derive(Debug, Clone)]
pub struct CronHistoryEntry {
    /// Row ID.
    pub id: i64,
    /// Name of the job that was executed.
    pub job_name: String,
    /// ISO 8601 timestamp when execution started.
    pub started_at: String,
    /// ISO 8601 timestamp when execution finished (None if still running).
    pub finished_at: Option<String>,
    /// Execution status: "running", "success", "failed", "timeout".
    pub status: String,
    /// Execution duration in milliseconds (None if still running).
    pub duration_ms: Option<i64>,
    /// Task output or error message (truncated to 4096 chars).
    pub output: Option<String>,
}

/// Record the start of a job execution.
///
/// Inserts a new row into `cron_history` with status `"running"` and returns
/// the row ID for later `record_finish`.
pub async fn record_start(conn: &Arc<Connection>, job_name: &str) -> Result<i64, String> {
    let name = job_name.to_string();
    conn.call(move |conn| -> Result<i64, rusqlite::Error> {
        conn.execute(
            "INSERT INTO cron_history (job_name, started_at, status) \
             VALUES (?1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), 'running')",
            rusqlite::params![name],
        )?;
        Ok(conn.last_insert_rowid())
    })
    .await
    .map_err(|e| format!("Failed to record start: {e}"))
}

/// Record the finish of a job execution.
///
/// Updates the history entry with finished_at, status, duration_ms, and
/// output (truncated to 4096 characters).
pub async fn record_finish(
    conn: &Arc<Connection>,
    history_id: i64,
    status: &str,
    duration_ms: u64,
    output: Option<&str>,
) -> Result<(), String> {
    let status = status.to_string();
    let output = output.map(|s| {
        if s.len() > MAX_OUTPUT_LENGTH {
            format!("{}... (truncated)", &s[..MAX_OUTPUT_LENGTH])
        } else {
            s.to_string()
        }
    });
    let duration = duration_ms as i64;

    conn.call(move |conn| -> Result<(), rusqlite::Error> {
        conn.execute(
            "UPDATE cron_history SET \
             finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), \
             status = ?1, \
             duration_ms = ?2, \
             output = ?3 \
             WHERE id = ?4",
            rusqlite::params![status, duration, output, history_id],
        )?;
        Ok(())
    })
    .await
    .map_err(|e| format!("Failed to record finish: {e}"))
}

/// Query job execution history.
///
/// Returns entries ordered by `started_at DESC`, limited to `limit` rows.
/// If `job_name` is `Some`, filters to that specific job.
pub async fn query_history(
    conn: &Arc<Connection>,
    job_name: Option<&str>,
    limit: usize,
) -> Result<Vec<CronHistoryEntry>, String> {
    let name = job_name.map(|s| s.to_string());
    let lim = limit as i64;

    conn.call(
        move |conn| -> Result<Vec<CronHistoryEntry>, rusqlite::Error> {
            let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match name {
                Some(ref n) => (
                    "SELECT id, job_name, started_at, finished_at, status, duration_ms, output \
                 FROM cron_history WHERE job_name = ?1 \
                 ORDER BY started_at DESC LIMIT ?2"
                        .to_string(),
                    vec![
                        Box::new(n.clone()) as Box<dyn rusqlite::types::ToSql>,
                        Box::new(lim),
                    ],
                ),
                None => (
                    "SELECT id, job_name, started_at, finished_at, status, duration_ms, output \
                 FROM cron_history \
                 ORDER BY started_at DESC LIMIT ?1"
                        .to_string(),
                    vec![Box::new(lim) as Box<dyn rusqlite::types::ToSql>],
                ),
            };

            let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();

            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map(param_refs.as_slice(), |row| {
                    Ok(CronHistoryEntry {
                        id: row.get(0)?,
                        job_name: row.get(1)?,
                        started_at: row.get(2)?,
                        finished_at: row.get(3)?,
                        status: row.get(4)?,
                        duration_ms: row.get(5)?,
                        output: row.get(6)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;

            Ok(rows)
        },
    )
    .await
    .map_err(|e| format!("Failed to query history: {e}"))
}

/// Delete excess history entries per job beyond `max_per_job`.
///
/// Keeps the most recent `max_per_job` entries for the given job, deleting
/// any older entries. Returns the number of rows deleted.
pub async fn cleanup_old_history(
    conn: &Arc<Connection>,
    job_name: &str,
    max_per_job: usize,
) -> Result<u64, String> {
    let name = job_name.to_string();
    let max = max_per_job as i64;

    conn.call(move |conn| -> Result<u64, rusqlite::Error> {
        let deleted = conn.execute(
            "DELETE FROM cron_history WHERE job_name = ?1 AND id NOT IN (\
             SELECT id FROM cron_history WHERE job_name = ?1 \
             ORDER BY started_at DESC LIMIT ?2)",
            rusqlite::params![name, max],
        )? as u64;
        Ok(deleted)
    })
    .await
    .map_err(|e| format!("Failed to cleanup history: {e}"))
}
