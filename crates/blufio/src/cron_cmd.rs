// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! CLI handler for `blufio cron` subcommands.
//!
//! Provides the operator interface for managing cron jobs: listing, adding,
//! removing, running, viewing history, and generating systemd timer files.
//! Uses `open_connection_sync` for direct DB access in CLI context (Phase 54 decision).

use std::path::Path;
use std::sync::Arc;

use crate::CronCommands;
use blufio_core::BlufioError;

/// Handle a `blufio cron` subcommand.
pub async fn handle_cron_command(action: CronCommands, db_path: &str) -> Result<(), BlufioError> {
    match action {
        CronCommands::List { json } => cmd_list(db_path, json),
        CronCommands::Add {
            name,
            schedule,
            task,
        } => cmd_add(db_path, &name, &schedule, &task),
        CronCommands::Remove { name } => cmd_remove(db_path, &name),
        CronCommands::RunNow { name } => cmd_run_now(db_path, &name).await,
        CronCommands::History { job, limit, json } => {
            cmd_history(db_path, job.as_deref(), limit, json).await
        }
        CronCommands::GenerateTimers { output_dir } => cmd_generate_timers(db_path, &output_dir),
    }
}

/// `blufio cron list` -- show all configured cron jobs with next-run times.
fn cmd_list(db_path: &str, json: bool) -> Result<(), BlufioError> {
    let conn = open_db(db_path)?;

    let mut stmt = conn
        .prepare(
            "SELECT name, schedule, task, enabled, last_run_at \
             FROM cron_jobs ORDER BY name",
        )
        .map_err(|e| BlufioError::Internal(format!("Failed to prepare query: {e}")))?;

    let rows: Vec<(String, String, String, bool, Option<String>)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(|e| BlufioError::Internal(format!("Failed to query jobs: {e}")))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| BlufioError::Internal(format!("Failed to read rows: {e}")))?;

    if json {
        let entries: Vec<serde_json::Value> = rows
            .iter()
            .map(|(name, schedule, task, enabled, last_run)| {
                let next_run = compute_next_run(schedule);
                serde_json::json!({
                    "name": name,
                    "schedule": schedule,
                    "task": task,
                    "enabled": enabled,
                    "last_run": last_run,
                    "next_run": next_run,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".to_string())
        );
    } else {
        if rows.is_empty() {
            println!("No cron jobs configured.");
            return Ok(());
        }
        println!(
            "{:<20} {:<20} {:<20} {:<8} {:<24} {}",
            "NAME", "SCHEDULE", "TASK", "ENABLED", "NEXT RUN", "LAST STATUS"
        );
        println!("{}", "-".repeat(116));
        for (name, schedule, task, enabled, last_run) in &rows {
            let next_run = compute_next_run(schedule);
            let enabled_str = if *enabled { "yes" } else { "no" };
            let last_str = last_run.as_deref().unwrap_or("-");
            println!(
                "{:<20} {:<20} {:<20} {:<8} {:<24} {}",
                name,
                schedule,
                task,
                enabled_str,
                next_run.as_deref().unwrap_or("-"),
                last_str
            );
        }
    }
    Ok(())
}

/// `blufio cron add` -- create a new cron job.
fn cmd_add(db_path: &str, name: &str, schedule: &str, task: &str) -> Result<(), BlufioError> {
    // Validate cron expression before inserting.
    schedule
        .parse::<croner::Cron>()
        .map_err(|e| BlufioError::Config(format!("Invalid cron expression '{schedule}': {e}")))?;

    let conn = open_db(db_path)?;

    conn.execute(
        "INSERT INTO cron_jobs (name, schedule, task, enabled) VALUES (?1, ?2, ?3, 1)",
        rusqlite::params![name, schedule, task],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            BlufioError::Config(format!("Job '{name}' already exists"))
        } else {
            BlufioError::Internal(format!("Failed to insert job: {e}"))
        }
    })?;

    println!("Added cron job '{name}' ({schedule}) -> task '{task}'");
    Ok(())
}

/// `blufio cron remove` -- delete a cron job.
fn cmd_remove(db_path: &str, name: &str) -> Result<(), BlufioError> {
    let conn = open_db(db_path)?;

    let changes = conn
        .execute(
            "DELETE FROM cron_jobs WHERE name = ?1",
            rusqlite::params![name],
        )
        .map_err(|e| BlufioError::Internal(format!("Failed to delete job: {e}")))?;

    if changes == 0 {
        println!("Job '{name}' not found.");
    } else {
        println!("Removed cron job '{name}'.");
    }
    Ok(())
}

/// `blufio cron run-now` -- execute a job immediately.
async fn cmd_run_now(db_path: &str, name: &str) -> Result<(), BlufioError> {
    // Open async DB connection for history recording and task execution.
    let conn = Arc::new(
        tokio_rusqlite::Connection::open(db_path)
            .await
            .map_err(|e| BlufioError::Internal(format!("Failed to open database: {e}")))?,
    );

    // Verify the job exists and look up the task name.
    let job_name = name.to_string();
    let task_name: String = {
        let jn = job_name.clone();
        conn.call(move |c| -> Result<String, rusqlite::Error> {
            c.query_row(
                "SELECT task FROM cron_jobs WHERE name = ?1",
                rusqlite::params![jn],
                |row| row.get(0),
            )
        })
        .await
        .map_err(|e| BlufioError::Internal(format!("Job '{name}' not found: {e}")))?
    };

    println!("Running job '{name}' (task: {task_name})...");

    // Create task registry with built-in tasks.
    let config = blufio_config::load_and_validate().map_err(|errors| {
        BlufioError::Config(format!("Config validation failed: {:?}", errors))
    })?;
    let registry = blufio_cron::register_builtin_tasks(Arc::clone(&conn), &config);

    // Look up the task in the registry.
    let task = registry.get(&task_name);

    // Record start in history.
    let history_id = blufio_cron::history::record_start(&conn, name)
        .await
        .map_err(|e| BlufioError::Internal(format!("Failed to record start: {e}")))?;

    let start = std::time::Instant::now();

    // Execute the task if found, otherwise report not available.
    let result: Result<String, String> = match task {
        Some(t) => {
            let timeout = std::time::Duration::from_secs(config.cron.job_timeout_secs);
            match tokio::time::timeout(timeout, t.execute()).await {
                Ok(Ok(output)) => Ok(output),
                Ok(Err(task_err)) => Err(task_err.to_string()),
                Err(_) => Err("Task execution timed out".to_string()),
            }
        }
        None => Err(format!(
            "Task '{}' not found in registry. Available tasks: {}",
            task_name,
            registry.keys().cloned().collect::<Vec<_>>().join(", ")
        )),
    };

    let duration_ms = start.elapsed().as_millis() as u64;

    match &result {
        Ok(output) => {
            let _ = blufio_cron::history::record_finish(
                &conn,
                history_id,
                "success",
                duration_ms,
                Some(output),
            )
            .await;
            println!("Job '{name}' completed successfully ({duration_ms}ms).");
            println!("{output}");
        }
        Err(msg) => {
            let _ = blufio_cron::history::record_finish(
                &conn,
                history_id,
                "failed",
                duration_ms,
                Some(msg),
            )
            .await;
            println!("Job '{name}' failed ({duration_ms}ms): {msg}");
        }
    }

    Ok(())
}

/// `blufio cron history` -- show job execution history.
async fn cmd_history(
    db_path: &str,
    job: Option<&str>,
    limit: usize,
    json: bool,
) -> Result<(), BlufioError> {
    let conn = Arc::new(
        tokio_rusqlite::Connection::open(db_path)
            .await
            .map_err(|e| BlufioError::Internal(format!("Failed to open database: {e}")))?,
    );

    let entries = blufio_cron::query_history(&conn, job, limit)
        .await
        .map_err(|e| BlufioError::Internal(format!("Failed to query history: {e}")))?;

    if json {
        let json_entries: Vec<serde_json::Value> = entries
            .iter()
            .map(|e| {
                serde_json::json!({
                    "id": e.id,
                    "job_name": e.job_name,
                    "started_at": e.started_at,
                    "finished_at": e.finished_at,
                    "status": e.status,
                    "duration_ms": e.duration_ms,
                    "output": e.output,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json_entries).unwrap_or_else(|_| "[]".to_string())
        );
    } else {
        if entries.is_empty() {
            println!("No execution history found.");
            return Ok(());
        }
        println!(
            "{:<20} {:<24} {:<10} {:<12} {}",
            "JOB", "STARTED", "STATUS", "DURATION", "OUTPUT"
        );
        println!("{}", "-".repeat(90));
        for entry in &entries {
            let duration_str = entry
                .duration_ms
                .map(|ms| format!("{ms}ms"))
                .unwrap_or_else(|| "-".to_string());
            let output_preview = entry
                .output
                .as_deref()
                .map(|s| {
                    let first_line = s.lines().next().unwrap_or("");
                    if first_line.len() > 40 {
                        format!("{}...", &first_line[..37])
                    } else {
                        first_line.to_string()
                    }
                })
                .unwrap_or_else(|| "-".to_string());
            println!(
                "{:<20} {:<24} {:<10} {:<12} {}",
                entry.job_name, entry.started_at, entry.status, duration_str, output_preview
            );
        }
    }
    Ok(())
}

/// `blufio cron generate-timers` -- write systemd .timer/.service files.
fn cmd_generate_timers(db_path: &str, output_dir: &str) -> Result<(), BlufioError> {
    let conn = open_db(db_path)?;

    let mut stmt = conn
        .prepare("SELECT name, schedule, task, enabled FROM cron_jobs")
        .map_err(|e| BlufioError::Internal(format!("Failed to prepare query: {e}")))?;

    let jobs: Vec<blufio_cron::CronJobRow> = stmt
        .query_map([], |row| {
            Ok(blufio_cron::CronJobRow {
                name: row.get(0)?,
                schedule: row.get(1)?,
                task: row.get(2)?,
                enabled: row.get(3)?,
            })
        })
        .map_err(|e| BlufioError::Internal(format!("Failed to query jobs: {e}")))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| BlufioError::Internal(format!("Failed to read rows: {e}")))?;

    if jobs.is_empty() {
        println!("No cron jobs found. Nothing to generate.");
        return Ok(());
    }

    // Detect blufio binary path.
    let blufio_path = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "blufio".to_string());

    let output_path = Path::new(output_dir);
    if !output_path.exists() {
        std::fs::create_dir_all(output_path).map_err(|e| {
            BlufioError::Internal(format!("Failed to create output directory: {e}"))
        })?;
    }

    let created =
        blufio_cron::generate_timers(&jobs, &blufio_path, output_path).map_err(|e| {
            BlufioError::Internal(format!("Failed to generate timer files: {e}"))
        })?;

    if created.is_empty() {
        println!("No enabled cron jobs. Nothing generated.");
    } else {
        println!("Generated {} systemd unit files:", created.len());
        for path in &created {
            println!("  {}", path.display());
        }
        println!(
            "\nTo install, copy to /etc/systemd/system/ and run:\n  \
             sudo systemctl daemon-reload\n  \
             sudo systemctl enable --now blufio-cron-*.timer"
        );
    }
    Ok(())
}

/// Open a sync SQLite connection for CLI usage (following Phase 54 convention).
fn open_db(db_path: &str) -> Result<rusqlite::Connection, BlufioError> {
    blufio_storage::open_connection_sync(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
            | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
}

/// Compute the next run time for a cron expression.
fn compute_next_run(schedule: &str) -> Option<String> {
    let cron: croner::Cron = schedule.parse().ok()?;
    let now = chrono::Utc::now();
    cron.find_next_occurrence(&now, false)
        .ok()
        .map(|dt: chrono::DateTime<chrono::Utc>| dt.format("%Y-%m-%d %H:%M UTC").to_string())
}
