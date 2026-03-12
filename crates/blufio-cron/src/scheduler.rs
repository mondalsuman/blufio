// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Cron scheduler with dispatch loop, single-instance locking, and EventBus integration.
//!
//! [`CronScheduler`] loads job definitions from the database, syncs them with
//! the config on startup, and dispatches due jobs every 60 seconds. Each job
//! execution acquires a single-instance lock via atomic DB update, records
//! start/finish history, and emits [`CronEvent`] on the [`EventBus`].

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use blufio_bus::EventBus;
use blufio_bus::events::{BusEvent, CronEvent, new_event_id, now_timestamp};
use blufio_config::model::CronConfig;
use chrono::Utc;
use croner::Cron;
use tokio_rusqlite::Connection;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::history;
use crate::tasks::CronTask;

/// Errors that can occur in the cron scheduler.
#[derive(Debug, thiserror::Error)]
pub enum CronError {
    /// The cron expression could not be parsed.
    #[error("invalid cron expression '{expression}': {reason}")]
    InvalidCronExpression {
        /// The invalid expression.
        expression: String,
        /// Parse error message.
        reason: String,
    },

    /// The requested job was not found.
    #[error("job not found: {0}")]
    JobNotFound(String),

    /// The job is already running (lock not acquired).
    #[error("job already running: {0}")]
    JobAlreadyRunning(String),

    /// A task-level error occurred.
    #[error("task error: {0}")]
    TaskError(String),

    /// A database error occurred.
    #[error("database error: {0}")]
    DatabaseError(String),
}

/// The main cron scheduler that dispatches due jobs on a 60-second interval.
pub struct CronScheduler {
    /// Database connection for job state and history.
    db: Arc<Connection>,
    /// Registry of task implementations keyed by task name.
    task_registry: Arc<HashMap<String, Box<dyn CronTask>>>,
    /// Optional event bus for emitting CronEvent.
    event_bus: Option<Arc<EventBus>>,
    /// Scheduler configuration.
    config: CronConfig,
}

impl CronScheduler {
    /// Create a new scheduler, syncing job definitions from config to DB.
    ///
    /// Upserts jobs from `config.jobs` into the `cron_jobs` table and
    /// disables any DB jobs not present in the config.
    pub async fn new(
        db: Arc<Connection>,
        task_registry: Arc<HashMap<String, Box<dyn CronTask>>>,
        event_bus: Option<Arc<EventBus>>,
        config: CronConfig,
    ) -> Result<Self, CronError> {
        // Sync config jobs to DB
        let jobs = config.jobs.clone();
        db.call(move |conn| -> Result<(), rusqlite::Error> {
            // Upsert each configured job
            for job in &jobs {
                conn.execute(
                    "INSERT INTO cron_jobs (name, schedule, task, enabled) \
                     VALUES (?1, ?2, ?3, ?4) \
                     ON CONFLICT(name) DO UPDATE SET \
                       schedule = excluded.schedule, \
                       task = excluded.task, \
                       enabled = excluded.enabled, \
                       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
                    rusqlite::params![
                        job.name,
                        job.schedule,
                        job.task,
                        job.enabled as i32,
                    ],
                )?;
            }

            // Disable jobs not in config (if config has jobs defined)
            if !jobs.is_empty() {
                let names: Vec<String> = jobs.iter().map(|j| j.name.clone()).collect();
                let placeholders: Vec<String> =
                    (1..=names.len()).map(|i| format!("?{i}")).collect();
                let sql = format!(
                    "UPDATE cron_jobs SET enabled = 0, \
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
                     WHERE name NOT IN ({}) AND enabled = 1",
                    placeholders.join(", ")
                );
                let params: Vec<Box<dyn rusqlite::types::ToSql>> = names
                    .iter()
                    .map(|n| Box::new(n.clone()) as Box<dyn rusqlite::types::ToSql>)
                    .collect();
                let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();
                conn.execute(&sql, param_refs.as_slice())?;
            }

            // Reset any stale running locks (from previous crashes)
            conn.execute(
                "UPDATE cron_jobs SET running = 0 WHERE running = 1",
                [],
            )?;

            Ok(())
        })
        .await
        .map_err(|e| CronError::DatabaseError(e.to_string()))?;

        info!(
            job_count = config.jobs.len(),
            "CronScheduler initialized, jobs synced to DB"
        );

        Ok(Self {
            db,
            task_registry,
            event_bus,
            config,
        })
    }

    /// Main scheduler loop. Checks all registered jobs every 60 seconds and
    /// dispatches due jobs.
    ///
    /// Respects the provided [`CancellationToken`] for graceful shutdown.
    pub async fn run(self, cancel: CancellationToken) {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        interval.tick().await; // Skip first immediate tick

        info!("CronScheduler run loop started (60s interval)");

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.dispatch_due_jobs().await;
                }
                _ = cancel.cancelled() => {
                    info!("CronScheduler shutting down");
                    break;
                }
            }
        }
    }

    /// Execute a specific job immediately, bypassing schedule check.
    ///
    /// Used by CLI `run-now` command. Follows the same lock/history/event pattern.
    pub async fn run_now(&self, job_name: &str) -> Result<String, CronError> {
        let name = job_name.to_string();

        // Verify job exists
        let job_exists = {
            let name = name.clone();
            self.db
                .call(move |conn| -> Result<bool, rusqlite::Error> {
                    let exists: bool = conn
                        .query_row(
                            "SELECT COUNT(*) > 0 FROM cron_jobs WHERE name = ?1",
                            rusqlite::params![name],
                            |row| row.get(0),
                        )
                        .unwrap_or(false);
                    Ok(exists)
                })
                .await
                .map_err(|e| CronError::DatabaseError(e.to_string()))?
        };

        if !job_exists {
            return Err(CronError::JobNotFound(name));
        }

        // Get task name for this job
        let task_name = {
            let name = name.clone();
            self.db
                .call(move |conn| -> Result<String, rusqlite::Error> {
                    conn.query_row(
                        "SELECT task FROM cron_jobs WHERE name = ?1",
                        rusqlite::params![name],
                        |row| row.get::<_, String>(0),
                    )
                })
                .await
                .map_err(|e| CronError::DatabaseError(e.to_string()))?
        };

        // Check task exists in registry
        let task = self
            .task_registry
            .get(&task_name)
            .ok_or_else(|| CronError::TaskError(format!("No task registered for '{task_name}'")))?;

        self.execute_job(&name, task.as_ref()).await
    }

    /// Dispatch all due jobs on a single tick.
    async fn dispatch_due_jobs(&self) {
        // Load all enabled jobs
        let jobs = match self.load_enabled_jobs().await {
            Ok(j) => j,
            Err(e) => {
                error!(error = %e, "Failed to load enabled jobs");
                return;
            }
        };

        for (name, schedule, task_name, last_run_at) in jobs {
            // Check if job is due
            let cron = match schedule.parse::<Cron>() {
                Ok(c) => c,
                Err(e) => {
                    warn!(
                        job = %name,
                        schedule = %schedule,
                        error = %e,
                        "Invalid cron expression, skipping job"
                    );
                    continue;
                }
            };

            if !Self::is_due(&cron, last_run_at.as_deref()) {
                continue;
            }

            // Find task in registry
            let task = match self.task_registry.get(&task_name) {
                Some(t) => t,
                None => {
                    warn!(
                        job = %name,
                        task = %task_name,
                        "No task registered, skipping"
                    );
                    continue;
                }
            };

            // Clone values for spawned task
            let db = Arc::clone(&self.db);
            let event_bus = self.event_bus.clone();
            let job_name = name.clone();
            let timeout_secs = self.config.job_timeout_secs;
            let max_history = self.config.max_history;
            let task_timeout = task.timeout();

            // Acquire lock and spawn
            let lock_acquired = match Self::acquire_lock(&db, &job_name).await {
                Ok(acquired) => acquired,
                Err(e) => {
                    error!(job = %job_name, error = %e, "Failed to acquire lock");
                    continue;
                }
            };

            if !lock_acquired {
                debug!(job = %job_name, "Job already running, skipping");
                continue;
            }

            // We need to execute the task. Since CronTask is not Clone, we call
            // it directly instead of spawning (tasks are expected to be quick).
            let effective_timeout = Duration::from_secs(timeout_secs).min(task_timeout);
            let started = std::time::Instant::now();

            // Record start
            let history_id = match history::record_start(&db, &job_name).await {
                Ok(id) => id,
                Err(e) => {
                    error!(job = %job_name, error = %e, "Failed to record start");
                    let _ = Self::release_lock(&db, &job_name).await;
                    continue;
                }
            };

            // Execute with timeout
            let result =
                tokio::time::timeout(effective_timeout, task.execute()).await;

            let duration_ms = started.elapsed().as_millis() as u64;

            match result {
                Ok(Ok(output)) => {
                    debug!(job = %job_name, duration_ms, "Job completed successfully");
                    let _ = history::record_finish(
                        &db,
                        history_id,
                        "success",
                        duration_ms,
                        Some(&output),
                    )
                    .await;
                    let _ = Self::update_last_run(&db, &job_name).await;
                    let _ = Self::release_lock(&db, &job_name).await;

                    if let Some(ref bus) = event_bus {
                        bus.publish(BusEvent::Cron(CronEvent::Completed {
                            event_id: new_event_id(),
                            timestamp: now_timestamp(),
                            job_name: job_name.clone(),
                            status: "success".into(),
                            duration_ms,
                        }))
                        .await;
                    }

                    // Cleanup old history
                    let _ = history::cleanup_old_history(&db, &job_name, max_history).await;
                }
                Ok(Err(task_err)) => {
                    warn!(job = %job_name, error = %task_err, "Job failed");
                    let _ = history::record_finish(
                        &db,
                        history_id,
                        "failed",
                        duration_ms,
                        Some(&task_err.to_string()),
                    )
                    .await;
                    let _ = Self::release_lock(&db, &job_name).await;

                    if let Some(ref bus) = event_bus {
                        bus.publish(BusEvent::Cron(CronEvent::Failed {
                            event_id: new_event_id(),
                            timestamp: now_timestamp(),
                            job_name: job_name.clone(),
                            error: task_err.to_string(),
                        }))
                        .await;
                    }
                }
                Err(_elapsed) => {
                    warn!(job = %job_name, timeout_secs = effective_timeout.as_secs(), "Job timed out");
                    let _ = history::record_finish(
                        &db,
                        history_id,
                        "timeout",
                        duration_ms,
                        Some("Task execution timed out"),
                    )
                    .await;
                    let _ = Self::release_lock(&db, &job_name).await;

                    if let Some(ref bus) = event_bus {
                        bus.publish(BusEvent::Cron(CronEvent::Failed {
                            event_id: new_event_id(),
                            timestamp: now_timestamp(),
                            job_name: job_name.clone(),
                            error: "Task execution timed out".into(),
                        }))
                        .await;
                    }
                }
            }
        }
    }

    /// Check if a job is due based on its cron expression and last run time.
    fn is_due(cron: &Cron, last_run_at: Option<&str>) -> bool {
        let now = Utc::now();

        match last_run_at {
            Some(last_run_str) => {
                // Parse last_run_at timestamp
                let last_run = match chrono::DateTime::parse_from_rfc3339(last_run_str) {
                    Ok(dt) => dt.with_timezone(&Utc),
                    Err(_) => {
                        // Try ISO 8601 with fractional seconds (SQLite format)
                        match chrono::NaiveDateTime::parse_from_str(
                            last_run_str,
                            "%Y-%m-%dT%H:%M:%S%.fZ",
                        ) {
                            Ok(ndt) => ndt.and_utc(),
                            Err(_) => return true, // Can't parse, assume due
                        }
                    }
                };

                // Find next occurrence after last run
                match cron.find_next_occurrence(&last_run, false) {
                    Ok(next) => next <= now,
                    Err(_) => false,
                }
            }
            None => true, // Never run, always due
        }
    }

    /// Load all enabled jobs from the database.
    async fn load_enabled_jobs(
        &self,
    ) -> Result<Vec<(String, String, String, Option<String>)>, CronError> {
        self.db
            .call(|conn| -> Result<Vec<(String, String, String, Option<String>)>, rusqlite::Error> {
                let mut stmt = conn.prepare(
                    "SELECT name, schedule, task, last_run_at \
                     FROM cron_jobs WHERE enabled = 1",
                )?;
                let rows = stmt
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<String>>(3)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .map_err(|e| CronError::DatabaseError(e.to_string()))
    }

    /// Acquire single-instance lock for a job via atomic DB update.
    ///
    /// Returns `true` if lock was acquired, `false` if already running.
    async fn acquire_lock(db: &Arc<Connection>, job_name: &str) -> Result<bool, CronError> {
        let name = job_name.to_string();
        db.call(move |conn| -> Result<bool, rusqlite::Error> {
            conn.execute(
                "UPDATE cron_jobs SET running = 1 WHERE name = ?1 AND running = 0",
                rusqlite::params![name],
            )
            .map(|changes| changes == 1)
        })
        .await
        .map_err(|e| CronError::DatabaseError(e.to_string()))
    }

    /// Release the single-instance lock for a job.
    async fn release_lock(db: &Arc<Connection>, job_name: &str) -> Result<(), CronError> {
        let name = job_name.to_string();
        db.call(move |conn| -> Result<(), rusqlite::Error> {
            conn.execute(
                "UPDATE cron_jobs SET running = 0 WHERE name = ?1",
                rusqlite::params![name],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| CronError::DatabaseError(e.to_string()))
    }

    /// Update last_run_at for a job after successful execution.
    async fn update_last_run(db: &Arc<Connection>, job_name: &str) -> Result<(), CronError> {
        let name = job_name.to_string();
        db.call(move |conn| -> Result<(), rusqlite::Error> {
            conn.execute(
                "UPDATE cron_jobs SET last_run_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
                 WHERE name = ?1",
                rusqlite::params![name],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| CronError::DatabaseError(e.to_string()))
    }

    /// Execute a job with lock acquisition, history recording, and event emission.
    async fn execute_job(&self, job_name: &str, task: &dyn CronTask) -> Result<String, CronError> {
        // Acquire lock
        if !Self::acquire_lock(&self.db, job_name).await? {
            return Err(CronError::JobAlreadyRunning(job_name.to_string()));
        }

        let timeout = Duration::from_secs(self.config.job_timeout_secs).min(task.timeout());
        let started = std::time::Instant::now();

        // Record start
        let history_id = history::record_start(&self.db, job_name)
            .await
            .map_err(|e| {
                CronError::DatabaseError(format!("Failed to record start: {e}"))
            })?;

        // Execute with timeout
        let result = tokio::time::timeout(timeout, task.execute()).await;
        let duration_ms = started.elapsed().as_millis() as u64;

        match result {
            Ok(Ok(output)) => {
                let _ = history::record_finish(
                    &self.db,
                    history_id,
                    "success",
                    duration_ms,
                    Some(&output),
                )
                .await;
                let _ = Self::update_last_run(&self.db, job_name).await;
                let _ = Self::release_lock(&self.db, job_name).await;

                if let Some(ref bus) = self.event_bus {
                    bus.publish(BusEvent::Cron(CronEvent::Completed {
                        event_id: new_event_id(),
                        timestamp: now_timestamp(),
                        job_name: job_name.to_string(),
                        status: "success".into(),
                        duration_ms,
                    }))
                    .await;
                }

                Ok(output)
            }
            Ok(Err(task_err)) => {
                let err_msg = task_err.to_string();
                let _ = history::record_finish(
                    &self.db,
                    history_id,
                    "failed",
                    duration_ms,
                    Some(&err_msg),
                )
                .await;
                let _ = Self::release_lock(&self.db, job_name).await;

                if let Some(ref bus) = self.event_bus {
                    bus.publish(BusEvent::Cron(CronEvent::Failed {
                        event_id: new_event_id(),
                        timestamp: now_timestamp(),
                        job_name: job_name.to_string(),
                        error: err_msg.clone(),
                    }))
                    .await;
                }

                Err(CronError::TaskError(err_msg))
            }
            Err(_elapsed) => {
                let _ = history::record_finish(
                    &self.db,
                    history_id,
                    "timeout",
                    duration_ms,
                    Some("Task execution timed out"),
                )
                .await;
                let _ = Self::release_lock(&self.db, job_name).await;

                if let Some(ref bus) = self.event_bus {
                    bus.publish(BusEvent::Cron(CronEvent::Failed {
                        event_id: new_event_id(),
                        timestamp: now_timestamp(),
                        job_name: job_name.to_string(),
                        error: "Task execution timed out".into(),
                    }))
                    .await;
                }

                Err(CronError::TaskError("Task execution timed out".into()))
            }
        }
    }
}
