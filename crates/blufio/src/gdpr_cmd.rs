// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! CLI handler for `blufio gdpr` subcommands.
//!
//! Provides the operator interface for GDPR data subject rights:
//! erasure (Art. 17), data portability (Art. 20), and transparency (Art. 15).
//! Orchestrates the business logic from `blufio-gdpr` with safety guards,
//! colored output, interactive confirmation, and Prometheus metrics.

use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use colored::Colorize;
use sha2::{Digest, Sha256};

use blufio_config::model::BlufioConfig;
use blufio_core::BlufioError;
use blufio_gdpr::{
    ExportMetadata, FilterCriteria, GdprError, ReportData, apply_redaction, check_active_sessions,
    cleanup_memory_index, collect_user_data, count_user_data, erase_audit_trail, execute_erasure,
    find_user_sessions, resolve_export_path, write_csv_export, write_json_export, write_manifest,
};

use crate::GdprCommands;

/// Handle a `blufio gdpr` subcommand.
pub async fn handle_gdpr_command(
    action: GdprCommands,
    config: &BlufioConfig,
) -> Result<(), BlufioError> {
    match action {
        GdprCommands::Erase {
            user,
            yes,
            dry_run,
            skip_export,
            force,
            timeout,
        } => cmd_erase(config, &user, yes, dry_run, skip_export, force, timeout).await,
        GdprCommands::Report { user, json } => cmd_report(config, &user, json).await,
        GdprCommands::Export {
            user,
            format,
            session,
            since,
            until,
            r#type,
            redact,
            output,
        } => {
            cmd_export(
                config,
                &user,
                &format,
                session.as_deref(),
                since.as_deref(),
                until.as_deref(),
                r#type,
                redact,
                output.as_deref(),
            )
            .await
        }
        GdprCommands::ListUsers { json } => cmd_list_users(config, json).await,
    }
}

// ---------------------------------------------------------------------------
// Erase
// ---------------------------------------------------------------------------

/// `blufio gdpr erase --user <id>` -- delete all user data with safety guards.
async fn cmd_erase(
    config: &BlufioConfig,
    user: &str,
    yes: bool,
    dry_run: bool,
    skip_export: bool,
    force: bool,
    timeout: u64,
) -> Result<(), BlufioError> {
    // a. Fail early if DB encrypted and key not set
    let db_path = &config.storage.database_path;
    let path = std::path::Path::new(db_path);
    if path.exists()
        && !blufio_storage::is_plaintext_sqlite(path).unwrap_or(true)
        && std::env::var("BLUFIO_DB_KEY").is_err()
    {
        return Err(BlufioError::Gdpr(
            "database is encrypted but BLUFIO_DB_KEY is not set".to_string(),
        ));
    }

    // b. Open main DB connection, find user sessions
    let conn = open_main_db(config).await?;
    let sessions = find_user_sessions(&conn, user)
        .await
        .map_err(|e| BlufioError::Gdpr(e.to_string()))?;

    // c. No data found
    if sessions.is_empty() {
        println!("{}", format!("No data found for user {user}").cyan());
        return Ok(());
    }

    let session_ids: Vec<String> = sessions.iter().map(|s| s.id.clone()).collect();

    // d. Check active sessions
    let active = check_active_sessions(&sessions);
    if active > 0 && !force {
        eprintln!(
            "{}",
            format!("User has {active} active session(s). Close them first or pass --force.").red()
        );
        return Err(BlufioError::Gdpr(
            GdprError::ActiveSessionsExist(active).to_string(),
        ));
    }

    // Get preview counts (used for dry_run and confirmation prompt)
    let audit_conn = open_audit_db(config).await?;
    let report = count_user_data(&conn, audit_conn.as_ref(), &session_ids, user)
        .await
        .map_err(|e| BlufioError::Gdpr(e.to_string()))?;

    // e. Dry run: show preview and return
    if dry_run {
        println!("{}", "Dry run -- no data will be deleted.".yellow());
        println!();
        print_preview_counts(&report);
        return Ok(());
    }

    // f. Interactive confirmation
    if !yes {
        print_preview_counts(&report);
        println!();
        print!("Type YES to confirm erasure for user {user}: ");
        std::io::stdout().flush().ok();
        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .map_err(|e| BlufioError::Internal(format!("failed to read input: {e}")))?;
        if input.trim() != "YES" {
            println!("{}", "Erasure aborted.".yellow());
            return Ok(());
        }
    }

    let start = Instant::now();

    // g. Export before erasure (if configured)
    let mut export_path: Option<String> = None;
    if !skip_export && config.gdpr.export_before_erasure {
        println!("{}", "Exporting user data before erasure...".cyan());
        match run_export_for_erasure(config, &conn, user, &session_ids).await {
            Ok(path) => {
                println!(
                    "{}",
                    format!("Pre-erasure export saved: {}", path.display()).green()
                );
                export_path = Some(path.to_string_lossy().to_string());
            }
            Err(e) => {
                eprintln!(
                    "{}",
                    format!("Export failed: {e}. Aborting erasure for data safety.").red()
                );
                return Err(BlufioError::Gdpr(format!("pre-erasure export failed: {e}")));
            }
        }
    }

    // h. Execute erasure with timeout
    let erasure_result = tokio::time::timeout(
        std::time::Duration::from_secs(timeout),
        execute_erasure(&conn, &session_ids, user),
    )
    .await;

    let mut manifest = match erasure_result {
        Ok(Ok(m)) => m,
        Ok(Err(e)) => {
            return Err(BlufioError::Gdpr(format!("erasure failed: {e}")));
        }
        Err(_) => {
            return Err(BlufioError::Gdpr(format!(
                "erasure timed out after {timeout} seconds"
            )));
        }
    };

    // i. Write manifest
    let export_dir = resolve_export_dir(config);
    if let Err(e) = write_manifest(&manifest, &export_dir) {
        eprintln!(
            "{}",
            format!("Warning: manifest write failed: {e}").yellow()
        );
    }

    // j. Erase audit trail (best-effort)
    let mut audit_warning: Option<String> = None;
    if let Some(ref ac) = audit_conn {
        match erase_audit_trail(ac, user).await {
            Ok(count) => {
                manifest.audit_entries_redacted = count;
            }
            Err(warning) => {
                audit_warning = Some(warning);
            }
        }
    }

    // k. Cleanup memory index (synchronous)
    if let Err(e) = cleanup_memory_index(&conn, &session_ids).await {
        eprintln!("{}", format!("Warning: FTS5 cleanup failed: {e}").yellow());
    }

    // k2. Emit audit trail entry recording the erasure event.
    if let Some(ref ac) = audit_conn {
        let user_id_hash = hex::encode(Sha256::digest(user.as_bytes()));
        let details = serde_json::json!({
            "user_id_hash": user_id_hash,
            "records_affected": {
                "messages": manifest.messages_deleted,
                "sessions": manifest.sessions_deleted,
                "memories": manifest.memories_deleted,
                "archives": manifest.archives_deleted,
                "cost_records": manifest.cost_records_anonymized,
                "audit_entries": manifest.audit_entries_redacted,
            }
        });
        let details_json = details.to_string();
        let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let entry_id = uuid::Uuid::new_v4().to_string();
        let ac_clone = ac.clone();

        let audit_result = ac_clone
            .call(move |db| -> Result<(), rusqlite::Error> {
                // Retrieve the last entry hash for chain continuity.
                let prev_hash: String = db
                    .query_row(
                        "SELECT entry_hash FROM audit_entries ORDER BY id DESC LIMIT 1",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap_or_else(|_| blufio_audit::GENESIS_HASH.to_string());

                let entry_hash = blufio_audit::compute_entry_hash(
                    &prev_hash,
                    &timestamp,
                    "gdpr.erasure",
                    "erase",
                    "user",
                    &entry_id,
                );

                db.execute(
                    "INSERT INTO audit_entries \
                     (entry_hash, prev_hash, timestamp, event_type, action, \
                      resource_type, resource_id, actor, session_id, details_json, pii_marker) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    rusqlite::params![
                        entry_hash,
                        prev_hash,
                        timestamp,
                        "gdpr.erasure",
                        "erase",
                        "user",
                        entry_id,
                        "cli",
                        "",
                        details_json,
                        1,
                    ],
                )?;
                Ok(())
            })
            .await;

        match audit_result {
            Ok(()) => {
                tracing::debug!("GDPR erasure audit entry written");
            }
            Err(e) => {
                eprintln!(
                    "{}",
                    format!("Warning: audit trail entry for erasure failed: {e}").yellow()
                );
            }
        }
    } else {
        eprintln!(
            "{}",
            "Audit database not found; erasure event not logged. Run `blufio serve` at least once to initialize audit.db."
                .yellow()
        );
    }

    // l. Record Prometheus metrics
    let duration = start.elapsed();
    let duration_secs = duration.as_secs_f64();
    metrics::counter!("blufio_gdpr_erasures_total", "status" => "success").increment(1);
    metrics::histogram!("blufio_gdpr_erasure_duration_seconds").record(duration_secs);
    metrics::counter!("blufio_gdpr_records_erased_total", "type" => "messages")
        .increment(manifest.messages_deleted);
    metrics::counter!("blufio_gdpr_records_erased_total", "type" => "sessions")
        .increment(manifest.sessions_deleted);
    metrics::counter!("blufio_gdpr_records_erased_total", "type" => "memories")
        .increment(manifest.memories_deleted);
    metrics::counter!("blufio_gdpr_records_erased_total", "type" => "archives")
        .increment(manifest.archives_deleted);
    metrics::counter!("blufio_gdpr_records_erased_total", "type" => "cost_records")
        .increment(manifest.cost_records_anonymized);

    // m. Print success summary
    println!();
    println!(
        "{}",
        format!("Erasure complete for user {user} ({:.1}s)", duration_secs).green()
    );
    println!("  Messages deleted:       {}", manifest.messages_deleted);
    println!("  Sessions deleted:       {}", manifest.sessions_deleted);
    println!("  Memories deleted:       {}", manifest.memories_deleted);
    println!("  Archives deleted:       {}", manifest.archives_deleted);
    println!(
        "  Cost records anonymized: {}",
        manifest.cost_records_anonymized
    );
    println!(
        "  Audit entries redacted: {}",
        manifest.audit_entries_redacted
    );
    if let Some(ref path) = export_path {
        println!("  Pre-erasure export:     {path}");
    }

    // n. Print audit warning if any
    if let Some(ref warning) = audit_warning {
        println!();
        println!("{}", format!("Warning: {warning}").yellow());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

/// `blufio gdpr report --user <id>` -- show transparency report.
async fn cmd_report(config: &BlufioConfig, user: &str, json: bool) -> Result<(), BlufioError> {
    let conn = open_main_db(config).await?;
    let sessions = find_user_sessions(&conn, user)
        .await
        .map_err(|e| BlufioError::Gdpr(e.to_string()))?;

    if sessions.is_empty() {
        println!("{}", format!("No data found for user {user}").cyan());
        return Ok(());
    }

    let session_ids: Vec<String> = sessions.iter().map(|s| s.id.clone()).collect();
    let audit_conn = open_audit_db(config).await?;

    let report = count_user_data(&conn, audit_conn.as_ref(), &session_ids, user)
        .await
        .map_err(|e| BlufioError::Gdpr(e.to_string()))?;

    // Record Prometheus metric
    metrics::counter!("blufio_gdpr_reports_total", "status" => "success").increment(1);

    if json {
        let json_str = serde_json::to_string_pretty(&report)
            .map_err(|e| BlufioError::Internal(format!("JSON serialization failed: {e}")))?;
        println!("{json_str}");
    } else {
        println!();
        println!(
            "  {}",
            format!("Transparency Report for user: {user}").cyan()
        );
        println!("  {}", "\u{2500}".repeat(40));
        println!("  Messages:      {}", report.messages);
        println!("  Sessions:      {}", report.sessions);
        println!("  Memories:      {}", report.memories);
        println!("  Archives:      {}", report.archives);
        println!("  Cost Records:  {}", report.cost_records);
        println!(
            "  Audit Entries: {} (not deletable per retention policy)",
            report.audit_entries
        );
        println!();
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// `blufio gdpr export --user <id>` -- export user data.
#[allow(clippy::too_many_arguments)]
async fn cmd_export(
    config: &BlufioConfig,
    user: &str,
    format: &str,
    session: Option<&str>,
    since: Option<&str>,
    until: Option<&str>,
    data_types: Option<Vec<String>>,
    redact: bool,
    output: Option<&str>,
) -> Result<(), BlufioError> {
    // a. Validate format
    if format != "json" && format != "csv" {
        return Err(BlufioError::Gdpr(format!(
            "unsupported format '{format}' -- use 'json' or 'csv'"
        )));
    }

    // b. Open DB, find sessions
    let conn = open_main_db(config).await?;
    let sessions = find_user_sessions(&conn, user)
        .await
        .map_err(|e| BlufioError::Gdpr(e.to_string()))?;

    // c. No data
    if sessions.is_empty() {
        println!("{}", format!("No data found for user {user}").cyan());
        return Ok(());
    }

    let session_ids: Vec<String> = sessions.iter().map(|s| s.id.clone()).collect();

    // d. Build filter criteria
    let filters = FilterCriteria {
        session_id: session.map(|s| s.to_string()),
        since: since.map(|s| s.to_string()),
        until: until.map(|s| s.to_string()),
        data_types,
        redacted: redact,
    };

    // e. Collect user data
    let mut data = collect_user_data(&conn, &session_ids, &filters)
        .await
        .map_err(|e| BlufioError::Gdpr(e.to_string()))?;

    // f. Redact if requested
    if redact {
        apply_redaction(&mut data);
    }

    // g. Resolve export path
    let data_dir = resolve_data_dir(config);
    let export_path = resolve_export_path(&config.gdpr, user, format, output, &data_dir);

    if let Some(parent) = export_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| BlufioError::Gdpr(format!("cannot create export directory: {e}")))?;
    }

    // h. Write export
    let metadata = ExportMetadata {
        timestamp: chrono::Utc::now().to_rfc3339(),
        user_id: user.to_string(),
        blufio_version: env!("CARGO_PKG_VERSION").to_string(),
        filter_criteria: filters,
    };

    let size = match format {
        "json" => write_json_export(&data, &metadata, &export_path)
            .map_err(|e| BlufioError::Gdpr(e.to_string()))?,
        "csv" => write_csv_export(&data, &metadata, &export_path)
            .map_err(|e| BlufioError::Gdpr(e.to_string()))?,
        _ => unreachable!(),
    };

    // i. Print restricted warning
    if data.restricted_excluded > 0 {
        println!(
            "{}",
            format!(
                "{} restricted record(s) excluded from export",
                data.restricted_excluded
            )
            .yellow()
        );
    }

    // j. Record Prometheus metrics
    metrics::counter!("blufio_gdpr_exports_total", "status" => "success").increment(1);
    metrics::histogram!("blufio_gdpr_export_size_bytes").record(size as f64);

    // k. Print success
    let size_kb = size as f64 / 1024.0;
    println!(
        "{}",
        format!(
            "Export saved: {} ({:.1} KB)",
            export_path.display(),
            size_kb
        )
        .green()
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// List Users
// ---------------------------------------------------------------------------

/// `blufio gdpr list-users` -- show all user IDs with record counts.
async fn cmd_list_users(config: &BlufioConfig, json: bool) -> Result<(), BlufioError> {
    let conn = open_main_db(config).await?;

    // Query distinct user_ids with session counts
    let users: Vec<(String, i64)> = conn
        .call(|conn| -> Result<Vec<(String, i64)>, rusqlite::Error> {
            let mut stmt = conn.prepare(
                "SELECT user_id, COUNT(*) as session_count \
                 FROM sessions \
                 WHERE user_id IS NOT NULL AND deleted_at IS NULL \
                 GROUP BY user_id \
                 ORDER BY user_id",
            )?;
            let rows = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
        .map_err(|e| BlufioError::Gdpr(format!("query failed: {e}")))?;

    if users.is_empty() {
        println!("{}", "No users with data found.".cyan());
        return Ok(());
    }

    // For each user, also count messages, memories, cost_records
    let mut user_data: Vec<UserRow> = Vec::new();
    for (user_id, session_count) in &users {
        let uid = user_id.clone();
        let counts = conn
            .call(move |conn| -> Result<(i64, i64, i64), rusqlite::Error> {
                let messages: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM messages m \
                         JOIN sessions s ON m.session_id = s.id \
                         WHERE s.user_id = ?1 AND m.deleted_at IS NULL AND s.deleted_at IS NULL",
                        rusqlite::params![uid],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);

                let memories: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM memories mem \
                         JOIN sessions s ON mem.session_id = s.id \
                         WHERE s.user_id = ?1 AND s.deleted_at IS NULL",
                        rusqlite::params![uid],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);

                let cost_records: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM cost_ledger cl \
                         JOIN sessions s ON cl.session_id = s.id \
                         WHERE s.user_id = ?1 AND s.deleted_at IS NULL",
                        rusqlite::params![uid],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);

                Ok((messages, memories, cost_records))
            })
            .await
            .unwrap_or((0, 0, 0));

        user_data.push(UserRow {
            user_id: user_id.clone(),
            sessions: *session_count,
            messages: counts.0,
            memories: counts.1,
            cost_records: counts.2,
        });
    }

    if json {
        let json_array: Vec<serde_json::Value> = user_data
            .iter()
            .map(|u| {
                serde_json::json!({
                    "user_id": u.user_id,
                    "sessions": u.sessions,
                    "messages": u.messages,
                    "memories": u.memories,
                    "cost_records": u.cost_records,
                })
            })
            .collect();
        let json_str = serde_json::to_string_pretty(&json_array)
            .map_err(|e| BlufioError::Internal(format!("JSON serialization failed: {e}")))?;
        println!("{json_str}");
    } else {
        println!();
        println!(
            "  {:<30} {:>10} {:>10} {:>10} {:>12}",
            "User ID".cyan(),
            "Sessions".cyan(),
            "Messages".cyan(),
            "Memories".cyan(),
            "Cost Records".cyan(),
        );
        println!("  {}", "\u{2500}".repeat(76));
        for u in &user_data {
            println!(
                "  {:<30} {:>10} {:>10} {:>10} {:>12}",
                u.user_id, u.sessions, u.messages, u.memories, u.cost_records
            );
        }
        println!();
        println!("  {} user(s) found.", user_data.len());
        println!();
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Row data for list-users output.
struct UserRow {
    user_id: String,
    sessions: i64,
    messages: i64,
    memories: i64,
    cost_records: i64,
}

/// Open the main database connection.
async fn open_main_db(config: &BlufioConfig) -> Result<tokio_rusqlite::Connection, BlufioError> {
    blufio_storage::open_connection(&config.storage.database_path)
        .await
        .map_err(|e| BlufioError::Gdpr(format!("cannot open database: {e}")))
}

/// Open the audit database connection (returns None if not found).
async fn open_audit_db(
    config: &BlufioConfig,
) -> Result<Option<tokio_rusqlite::Connection>, BlufioError> {
    let audit_db_path = config.audit.db_path.clone().unwrap_or_else(|| {
        let db = std::path::Path::new(&config.storage.database_path);
        db.parent()
            .unwrap_or(std::path::Path::new("."))
            .join("audit.db")
            .to_string_lossy()
            .to_string()
    });

    let path = std::path::Path::new(&audit_db_path);
    if !path.exists() {
        return Ok(None);
    }

    match blufio_storage::open_connection(&audit_db_path).await {
        Ok(conn) => Ok(Some(conn)),
        Err(_) => Ok(None), // Audit DB is optional
    }
}

/// Resolve the GDPR export directory.
fn resolve_export_dir(config: &BlufioConfig) -> PathBuf {
    config
        .gdpr
        .export_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let db = std::path::Path::new(&config.storage.database_path);
            db.parent()
                .unwrap_or(std::path::Path::new("."))
                .join("exports")
        })
}

/// Resolve the data directory from config (parent of the database path).
fn resolve_data_dir(config: &BlufioConfig) -> String {
    let db = std::path::Path::new(&config.storage.database_path);
    db.parent()
        .unwrap_or(std::path::Path::new("."))
        .to_string_lossy()
        .to_string()
}

/// Print preview counts table.
fn print_preview_counts(report: &ReportData) {
    println!("  Would delete for user {}:", report.user_id);
    println!("    Messages:      {}", report.messages);
    println!("    Sessions:      {}", report.sessions);
    println!("    Memories:      {}", report.memories);
    println!("    Archives:      {}", report.archives);
    println!("    Cost Records:  {} (anonymized)", report.cost_records);
    println!(
        "    Audit Entries: {} (redacted, not deleted)",
        report.audit_entries
    );
}

/// Run export as part of export-before-erasure safety net.
async fn run_export_for_erasure(
    config: &BlufioConfig,
    conn: &tokio_rusqlite::Connection,
    user: &str,
    session_ids: &[String],
) -> Result<PathBuf, BlufioError> {
    let filters = FilterCriteria {
        session_id: None,
        since: None,
        until: None,
        data_types: None,
        redacted: false,
    };

    let data = collect_user_data(conn, session_ids, &filters)
        .await
        .map_err(|e| BlufioError::Gdpr(e.to_string()))?;

    let data_dir = resolve_data_dir(config);
    let format = &config.gdpr.default_format;
    let export_path = resolve_export_path(&config.gdpr, user, format, None, &data_dir);

    if let Some(parent) = export_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| BlufioError::Gdpr(format!("cannot create export directory: {e}")))?;
    }

    let metadata = ExportMetadata {
        timestamp: chrono::Utc::now().to_rfc3339(),
        user_id: user.to_string(),
        blufio_version: env!("CARGO_PKG_VERSION").to_string(),
        filter_criteria: filters,
    };

    match format.as_str() {
        "csv" => write_csv_export(&data, &metadata, &export_path)
            .map_err(|e| BlufioError::Gdpr(e.to_string()))?,
        _ => write_json_export(&data, &metadata, &export_path)
            .map_err(|e| BlufioError::Gdpr(e.to_string()))?,
    };

    Ok(export_path)
}
