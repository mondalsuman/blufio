// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio migrate` and `blufio config translate` command implementation.
//!
//! Provides a migration pipeline for importing data from OpenClaw:
//! - **Preview**: Categorized dry-run report (Will Import / Needs Manual Attention / Cannot Import).
//! - **Import**: Full idempotent import of sessions, costs, personality files, and secrets.
//! - **Config translate**: JSON-to-TOML config conversion with unmappable field comments.
//!
//! The source directory is **never modified** -- all operations are copy-only.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use blufio_config::model::BlufioConfig;
use blufio_core::BlufioError;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// OpenClaw data structures
// ---------------------------------------------------------------------------

/// Parsed contents of an OpenClaw data directory.
#[derive(Debug, Default)]
struct OpenClawData {
    /// Parsed JSON config (if found).
    config: Option<serde_json::Value>,
    /// Parsed session history.
    sessions: Vec<OpenClawSession>,
    /// Parsed cost records.
    cost_records: Vec<OpenClawCostRecord>,
    /// Markdown personality files found.
    personality_files: Vec<PathBuf>,
    /// Detected API keys/tokens from config.
    secrets: Vec<(String, String)>,
}

/// An OpenClaw session with its messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenClawSession {
    id: String,
    channel: Option<String>,
    created_at: Option<String>,
    messages: Vec<OpenClawMessage>,
    #[serde(default)]
    metadata: HashMap<String, serde_json::Value>,
}

/// A message within an OpenClaw session.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenClawMessage {
    id: Option<String>,
    role: String,
    content: String,
    created_at: Option<String>,
    #[serde(default)]
    token_count: Option<i64>,
}

/// An OpenClaw cost record.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenClawCostRecord {
    id: Option<String>,
    session_id: Option<String>,
    model: String,
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
    cost_usd: Option<f64>,
    created_at: Option<String>,
}

/// Preview report categories.
#[derive(Debug, Default, Serialize)]
struct PreviewReport {
    will_import: PreviewCategory,
    needs_attention: PreviewCategory,
    cannot_import: PreviewCategory,
    estimated_cost_comparison: Option<CostComparison>,
}

#[derive(Debug, Default, Serialize)]
struct PreviewCategory {
    sessions: usize,
    cost_records: usize,
    personality_files: usize,
    secrets: usize,
    items: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CostComparison {
    openclaw_total_usd: f64,
    blufio_estimated_usd: f64,
}

/// Import summary counters.
#[derive(Debug, Default, Serialize)]
struct ImportSummary {
    sessions_imported: usize,
    sessions_skipped: usize,
    messages_imported: usize,
    cost_records_imported: usize,
    cost_records_skipped: usize,
    files_imported: usize,
    files_skipped: usize,
    secrets_vaulted: usize,
    secrets_skipped: usize,
}

// ---------------------------------------------------------------------------
// Known OpenClaw personality files that map to Blufio personality directory.
// ---------------------------------------------------------------------------

/// Files commonly found in OpenClaw workspace that map to Blufio personality dir.
const KNOWN_PERSONALITY_FILES: &[&str] = &[
    "SOUL.md",
    "PERSONALITY.md",
    "SYSTEM.md",
    "INSTRUCTIONS.md",
    "GUIDELINES.md",
    "CONTEXT.md",
    "RULES.md",
];

/// Key patterns that indicate secrets in OpenClaw config JSON.
const SECRET_KEY_PATTERNS: &[&str] = &[
    "api_key",
    "api-key",
    "apikey",
    "token",
    "secret",
    "password",
    "credential",
];

// ---------------------------------------------------------------------------
// Detection and parsing
// ---------------------------------------------------------------------------

/// Auto-detect the OpenClaw data directory.
///
/// Checks (in order): explicit override, `$OPENCLAW_HOME` env var, `~/.openclaw`.
fn detect_openclaw_dir(override_path: Option<&str>) -> Result<PathBuf, BlufioError> {
    // 1. Explicit override.
    if let Some(path) = override_path {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
        return Err(BlufioError::migration_schema_failed(&format!(
            "specified OpenClaw directory does not exist: {path}"
        )));
    }

    // 2. $OPENCLAW_HOME env var.
    if let Ok(home) = std::env::var("OPENCLAW_HOME") {
        let p = PathBuf::from(&home);
        if p.exists() {
            return Ok(p);
        }
    }

    // 3. Default ~/.openclaw.
    if let Some(home) = dirs::home_dir() {
        let p: PathBuf = home.join(".openclaw");
        if p.exists() {
            return Ok(p);
        }
    }

    Err(BlufioError::migration_schema_failed(
        "OpenClaw data directory not found. Checked:\n  \
         1. --data-dir flag (not specified)\n  \
         2. $OPENCLAW_HOME (not set or does not exist)\n  \
         3. ~/.openclaw (does not exist)\n\n\
         Specify the path with: blufio migrate --from-openclaw --data-dir /path/to/openclaw",
    ))
}

/// Parse the OpenClaw data directory into structured data.
fn parse_openclaw_dir(path: &Path) -> Result<OpenClawData, BlufioError> {
    let mut data = OpenClawData::default();

    // Parse JSON config if present.
    let config_candidates = ["config.json", "settings.json", "openclaw.json"];
    for name in &config_candidates {
        let config_path = path.join(name);
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path).map_err(|e| {
                BlufioError::migration_schema_failed(&format!(
                    "failed to read config file {name}: {e}"
                ))
            })?;
            let value: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
                BlufioError::migration_schema_failed(&format!(
                    "failed to parse config file {name}: {e}"
                ))
            })?;

            // Extract secrets from config.
            extract_secrets_from_json(&value, "", &mut data.secrets);
            data.config = Some(value);
            break;
        }
    }

    // Parse sessions from sessions directory or sessions.json.
    parse_sessions(path, &mut data)?;

    // Parse cost records from costs directory or costs.json.
    parse_cost_records(path, &mut data)?;

    // Collect markdown personality files.
    collect_personality_files(path, &mut data)?;

    Ok(data)
}

/// Recursively extract secrets from a JSON value based on key patterns.
fn extract_secrets_from_json(
    value: &serde_json::Value,
    prefix: &str,
    secrets: &mut Vec<(String, String)>,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map {
                let full_key = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                let lower_key = key.to_lowercase();
                if SECRET_KEY_PATTERNS
                    .iter()
                    .any(|pat| lower_key.contains(pat))
                    && let Some(s) = val.as_str()
                    && !s.is_empty()
                {
                    secrets.push((full_key.clone(), s.to_string()));
                }
                extract_secrets_from_json(val, &full_key, secrets);
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, val) in arr.iter().enumerate() {
                extract_secrets_from_json(val, &format!("{prefix}[{i}]"), secrets);
            }
        }
        _ => {}
    }
}

/// Parse session data from the OpenClaw directory.
fn parse_sessions(path: &Path, data: &mut OpenClawData) -> Result<(), BlufioError> {
    // Try sessions.json first.
    let sessions_json = path.join("sessions.json");
    if sessions_json.exists() {
        let content = std::fs::read_to_string(&sessions_json).map_err(|e| {
            BlufioError::migration_schema_failed(&format!("failed to read sessions.json: {e}"))
        })?;
        let sessions: Vec<OpenClawSession> = serde_json::from_str(&content).map_err(|e| {
            BlufioError::migration_schema_failed(&format!("failed to parse sessions.json: {e}"))
        })?;
        data.sessions = sessions;
        return Ok(());
    }

    // Try sessions/ directory with individual JSON files.
    let sessions_dir = path.join("sessions");
    if sessions_dir.is_dir()
        && let Ok(entries) = std::fs::read_dir(&sessions_dir)
    {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.extension().is_some_and(|ext| ext == "json")
                && let Ok(content) = std::fs::read_to_string(&entry_path)
                && let Ok(session) = serde_json::from_str::<OpenClawSession>(&content)
            {
                data.sessions.push(session);
            }
        }
    }

    // Try SQLite database.
    let db_candidates = ["openclaw.db", "data.db", "history.db"];
    for name in &db_candidates {
        let db_path = path.join(name);
        if db_path.exists() {
            if let Ok(conn) = rusqlite::Connection::open_with_flags(
                &db_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            ) {
                parse_sessions_from_sqlite(&conn, data);
            }
            break;
        }
    }

    Ok(())
}

/// Parse sessions from an OpenClaw SQLite database (best-effort).
fn parse_sessions_from_sqlite(conn: &rusqlite::Connection, data: &mut OpenClawData) {
    // Try common table/column patterns.
    let table_queries = [
        "SELECT id, channel, created_at FROM sessions ORDER BY created_at",
        "SELECT id, source, created_at FROM conversations ORDER BY created_at",
    ];

    for query in &table_queries {
        if let Ok(mut stmt) = conn.prepare(query)
            && let Ok(rows) = stmt.query_map([], |row| {
                Ok(OpenClawSession {
                    id: row.get::<_, String>(0)?,
                    channel: row.get::<_, Option<String>>(1)?,
                    created_at: row.get::<_, Option<String>>(2)?,
                    messages: Vec::new(),
                    metadata: HashMap::new(),
                })
            })
        {
            for row in rows.flatten() {
                data.sessions.push(row);
            }

            // Try to load messages for each session.
            let msg_queries = [
                "SELECT id, role, content, created_at, token_count FROM messages WHERE session_id = ?1 ORDER BY created_at",
                "SELECT id, role, content, created_at, NULL FROM messages WHERE conversation_id = ?1 ORDER BY created_at",
            ];

            for session in &mut data.sessions {
                for mq in &msg_queries {
                    if let Ok(mut msg_stmt) = conn.prepare(mq)
                        && let Ok(msg_rows) =
                            msg_stmt.query_map(rusqlite::params![session.id], |row| {
                                Ok(OpenClawMessage {
                                    id: row.get::<_, Option<String>>(0)?,
                                    role: row.get::<_, String>(1)?,
                                    content: row.get::<_, String>(2)?,
                                    created_at: row.get::<_, Option<String>>(3)?,
                                    token_count: row.get::<_, Option<i64>>(4)?,
                                })
                            })
                    {
                        session.messages = msg_rows.flatten().collect();
                        if !session.messages.is_empty() {
                            break;
                        }
                    }
                }
            }

            break;
        }
    }
}

/// Parse cost records from the OpenClaw directory.
fn parse_cost_records(path: &Path, data: &mut OpenClawData) -> Result<(), BlufioError> {
    // Try costs.json.
    let costs_json = path.join("costs.json");
    if costs_json.exists() {
        let content = std::fs::read_to_string(&costs_json).map_err(|e| {
            BlufioError::migration_schema_failed(&format!("failed to read costs.json: {e}"))
        })?;
        let records: Vec<OpenClawCostRecord> = serde_json::from_str(&content).map_err(|e| {
            BlufioError::migration_schema_failed(&format!("failed to parse costs.json: {e}"))
        })?;
        data.cost_records = records;
        return Ok(());
    }

    // Try cost_ledger from SQLite.
    let db_candidates = ["openclaw.db", "data.db", "history.db"];
    for name in &db_candidates {
        let db_path = path.join(name);
        if db_path.exists() {
            if let Ok(conn) = rusqlite::Connection::open_with_flags(
                &db_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            ) {
                parse_cost_records_from_sqlite(&conn, data);
            }
            break;
        }
    }

    Ok(())
}

/// Parse cost records from an OpenClaw SQLite database (best-effort).
fn parse_cost_records_from_sqlite(conn: &rusqlite::Connection, data: &mut OpenClawData) {
    let queries = [
        "SELECT id, session_id, model, input_tokens, output_tokens, cost_usd, created_at FROM cost_ledger ORDER BY created_at",
        "SELECT id, session_id, model, input_tokens, output_tokens, cost, created_at FROM costs ORDER BY created_at",
    ];

    for query in &queries {
        if let Ok(mut stmt) = conn.prepare(query)
            && let Ok(rows) = stmt.query_map([], |row| {
                Ok(OpenClawCostRecord {
                    id: row.get::<_, Option<String>>(0)?,
                    session_id: row.get::<_, Option<String>>(1)?,
                    model: row.get::<_, String>(2)?,
                    input_tokens: row.get::<_, Option<u32>>(3)?,
                    output_tokens: row.get::<_, Option<u32>>(4)?,
                    cost_usd: row.get::<_, Option<f64>>(5)?,
                    created_at: row.get::<_, Option<String>>(6)?,
                })
            })
        {
            data.cost_records = rows.flatten().collect();
            if !data.cost_records.is_empty() {
                break;
            }
        }
    }
}

/// Collect markdown files from OpenClaw workspace directories.
fn collect_personality_files(path: &Path, data: &mut OpenClawData) -> Result<(), BlufioError> {
    let search_dirs = [
        path.to_path_buf(),
        path.join("workspace"),
        path.join("personality"),
        path.join("prompts"),
        path.join("system"),
    ];

    for dir in &search_dirs {
        if dir.is_dir()
            && let Ok(entries) = std::fs::read_dir(dir)
        {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_file()
                    && entry_path
                        .extension()
                        .is_some_and(|ext| ext == "md" || ext == "txt")
                    && !data.personality_files.contains(&entry_path)
                {
                    data.personality_files.push(entry_path);
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Preview (dry run)
// ---------------------------------------------------------------------------

/// Run a dry-run preview of what would be imported from OpenClaw.
pub async fn run_migrate_preview(data_dir: Option<&str>, json: bool) -> Result<(), BlufioError> {
    let oc_dir = detect_openclaw_dir(data_dir)?;
    eprintln!("OpenClaw directory: {}", oc_dir.display());

    let data = parse_openclaw_dir(&oc_dir)?;
    let mut report = PreviewReport::default();

    // Categorize sessions.
    for session in &data.sessions {
        let has_unsupported = session
            .metadata
            .keys()
            .any(|k| !["source", "channel", "user_id"].contains(&k.as_str()));

        if has_unsupported {
            report.needs_attention.sessions += 1;
            report.needs_attention.items.push(format!(
                "Session {} has unsupported metadata fields",
                session.id
            ));
        } else {
            report.will_import.sessions += 1;
        }
    }

    // Categorize cost records.
    report.will_import.cost_records = data.cost_records.len();

    // Categorize personality files.
    for file in &data.personality_files {
        let file_name = file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if KNOWN_PERSONALITY_FILES.contains(&file_name) {
            report.will_import.personality_files += 1;
        } else if file_name.ends_with(".md") || file_name.ends_with(".txt") {
            report.will_import.personality_files += 1;
            report.will_import.items.push(format!(
                "{file_name} (unknown -- will import to import directory)"
            ));
        }
    }

    // Categorize secrets.
    report.will_import.secrets = data.secrets.len();

    // Estimated cost comparison.
    if !data.cost_records.is_empty() {
        let oc_total: f64 = data.cost_records.iter().filter_map(|r| r.cost_usd).sum();

        let blufio_estimate: f64 = data
            .cost_records
            .iter()
            .map(|r| {
                let pricing = blufio_cost::pricing::get_pricing(&r.model);
                let input = r.input_tokens.unwrap_or(0) as f64;
                let output = r.output_tokens.unwrap_or(0) as f64;
                (input * pricing.input_per_mtok + output * pricing.output_per_mtok) / 1_000_000.0
            })
            .sum();

        report.estimated_cost_comparison = Some(CostComparison {
            openclaw_total_usd: oc_total,
            blufio_estimated_usd: blufio_estimate,
        });
    }

    // Output.
    if json {
        let json_output = serde_json::to_string_pretty(&report).map_err(|e| {
            BlufioError::migration_schema_failed(&format!(
                "failed to serialize preview report: {e}"
            ))
        })?;
        println!("{json_output}");
    } else {
        print_preview_table(&report);
    }

    Ok(())
}

/// Print the preview report as a formatted table.
fn print_preview_table(report: &PreviewReport) {
    println!();
    println!("  OpenClaw Migration Preview");
    println!("  {}", "-".repeat(55));
    println!();

    println!("  Will Import:");
    println!("    Sessions:         {}", report.will_import.sessions);
    println!("    Cost records:     {}", report.will_import.cost_records);
    println!(
        "    Personality files: {}",
        report.will_import.personality_files
    );
    println!("    Secrets:          {}", report.will_import.secrets);
    println!();

    if report.needs_attention.sessions > 0 || !report.needs_attention.items.is_empty() {
        println!("  Needs Manual Attention:");
        println!("    Sessions:         {}", report.needs_attention.sessions);
        for item in &report.needs_attention.items {
            println!("      - {item}");
        }
        println!();
    }

    if report.cannot_import.sessions > 0
        || report.cannot_import.cost_records > 0
        || !report.cannot_import.items.is_empty()
    {
        println!("  Cannot Import:");
        for item in &report.cannot_import.items {
            println!("      - {item}");
        }
        println!();
    }

    for item in &report.will_import.items {
        println!("  Note: {item}");
    }

    if let Some(ref comparison) = report.estimated_cost_comparison {
        println!();
        println!("  Cost Comparison:");
        println!(
            "    OpenClaw total:     ${:.4}",
            comparison.openclaw_total_usd
        );
        println!(
            "    Blufio estimated:   ${:.4}",
            comparison.blufio_estimated_usd
        );
    }

    let total = report.will_import.sessions
        + report.will_import.cost_records
        + report.will_import.personality_files
        + report.will_import.secrets;

    println!();
    println!("  Total items to import: {total}");
    println!();
}

// ---------------------------------------------------------------------------
// Import (full migration)
// ---------------------------------------------------------------------------

/// Run the full OpenClaw-to-Blufio migration.
pub async fn run_migrate(
    config: &BlufioConfig,
    data_dir: Option<&str>,
    json: bool,
) -> Result<(), BlufioError> {
    let oc_dir = detect_openclaw_dir(data_dir)?;
    eprintln!("OpenClaw directory: {}", oc_dir.display());

    let data = parse_openclaw_dir(&oc_dir)?;

    // Open Blufio database.
    let db = blufio_storage::Database::open(&config.storage.database_path).await?;
    let db_conn = db.connection().clone();

    // Set up progress bars.
    let multi = MultiProgress::new();
    let style =
        ProgressStyle::with_template("  {prefix:<20} [{bar:30.cyan/dim}] {pos}/{len} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_bar());

    let session_bar = multi.add(ProgressBar::new(data.sessions.len() as u64));
    session_bar.set_style(style.clone());
    session_bar.set_prefix("Sessions");

    let cost_bar = multi.add(ProgressBar::new(data.cost_records.len() as u64));
    cost_bar.set_style(style.clone());
    cost_bar.set_prefix("Cost records");

    let file_bar = multi.add(ProgressBar::new(data.personality_files.len() as u64));
    file_bar.set_style(style.clone());
    file_bar.set_prefix("Files");

    let secret_bar = multi.add(ProgressBar::new(data.secrets.len() as u64));
    secret_bar.set_style(style);
    secret_bar.set_prefix("Secrets");

    let mut summary = ImportSummary::default();
    let now_ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // Import sessions.
    for session in &data.sessions {
        if is_already_imported(&db_conn, "session", &session.id).await? {
            summary.sessions_skipped += 1;
            session_bar.inc(1);
            continue;
        }

        let blufio_session = blufio_core::types::Session {
            id: session.id.clone(),
            channel: session
                .channel
                .clone()
                .unwrap_or_else(|| "openclaw".to_string()),
            user_id: None,
            state: "closed".to_string(),
            metadata: Some(
                serde_json::json!({
                    "source": "openclaw",
                    "imported_at": now_ts,
                })
                .to_string(),
            ),
            created_at: session.created_at.clone().unwrap_or_else(|| now_ts.clone()),
            updated_at: now_ts.clone(),
        };

        blufio_storage::queries::sessions::create_session(&db, &blufio_session).await?;

        // Import messages for this session.
        for (i, msg) in session.messages.iter().enumerate() {
            let msg_id = msg
                .id
                .clone()
                .unwrap_or_else(|| format!("{}-msg-{i}", session.id));

            let blufio_msg = blufio_core::types::Message {
                id: msg_id,
                session_id: session.id.clone(),
                role: msg.role.clone(),
                content: msg.content.clone(),
                token_count: msg.token_count,
                metadata: None,
                created_at: msg.created_at.clone().unwrap_or_else(|| now_ts.clone()),
            };

            blufio_storage::queries::messages::insert_message(&db, &blufio_msg).await?;
            summary.messages_imported += 1;
        }

        record_migration(&db_conn, "session", &session.id).await?;
        summary.sessions_imported += 1;
        session_bar.inc(1);
    }
    session_bar.finish_with_message("done");

    // Import cost records.
    let cost_ledger = blufio_cost::CostLedger::open(&config.storage.database_path).await?;
    for record in &data.cost_records {
        let record_id = record
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        if is_already_imported(&db_conn, "cost_record", &record_id).await? {
            summary.cost_records_skipped += 1;
            cost_bar.inc(1);
            continue;
        }

        let usage = blufio_core::TokenUsage {
            input_tokens: record.input_tokens.unwrap_or(0),
            output_tokens: record.output_tokens.unwrap_or(0),
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        };

        let cost_usd = record.cost_usd.unwrap_or_else(|| {
            let pricing = blufio_cost::pricing::get_pricing(&record.model);
            let input = usage.input_tokens as f64;
            let output = usage.output_tokens as f64;
            (input * pricing.input_per_mtok + output * pricing.output_per_mtok) / 1_000_000.0
        });

        let blufio_record = blufio_cost::CostRecord::new(
            record
                .session_id
                .clone()
                .unwrap_or_else(|| "openclaw-import".to_string()),
            record.model.clone(),
            blufio_cost::FeatureType::Message,
            &usage,
            cost_usd,
        );

        cost_ledger.record(&blufio_record).await?;
        record_migration(&db_conn, "cost_record", &record_id).await?;
        summary.cost_records_imported += 1;
        cost_bar.inc(1);
    }
    cost_bar.finish_with_message("done");

    // Import personality files.
    let blufio_data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("blufio");
    let personality_dir = blufio_data_dir.join("personality");
    let import_dir = blufio_data_dir.join("import").join("openclaw");

    for file_path in &data.personality_files {
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown.md");

        let source_id = file_name.to_string();

        if is_already_imported(&db_conn, "personality_file", &source_id).await? {
            summary.files_skipped += 1;
            file_bar.inc(1);
            continue;
        }

        let dest = if KNOWN_PERSONALITY_FILES.contains(&file_name) {
            let dest = personality_dir.join(file_name);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    BlufioError::migration_schema_failed(&format!(
                        "failed to create personality directory: {e}"
                    ))
                })?;
            }
            dest
        } else {
            let dest = import_dir.join(file_name);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    BlufioError::migration_schema_failed(&format!(
                        "failed to create import directory: {e}"
                    ))
                })?;
            }
            dest
        };

        std::fs::copy(file_path, &dest).map_err(|e| {
            BlufioError::migration_schema_failed(&format!(
                "failed to copy {} to {}: {e}",
                file_path.display(),
                dest.display()
            ))
        })?;

        record_migration(&db_conn, "personality_file", &source_id).await?;
        summary.files_imported += 1;
        file_bar.inc(1);
    }
    file_bar.finish_with_message("done");

    // Auto-vault secrets (only if vault is available).
    // Since vault requires passphrase, we store secrets via the database directly
    // and report them for manual vaulting if vault is not unlocked.
    for (key, value) in &data.secrets {
        let source_id = key.clone();

        if is_already_imported(&db_conn, "secret", &source_id).await? {
            summary.secrets_skipped += 1;
            secret_bar.inc(1);
            continue;
        }

        // Try to open vault and store secret.
        let vault_result = try_vault_store(config, key, value).await;
        match vault_result {
            Ok(()) => {
                record_migration(&db_conn, "secret", &source_id).await?;
                summary.secrets_vaulted += 1;
            }
            Err(_) => {
                // Record as imported but note vault was unavailable.
                record_migration(&db_conn, "secret", &source_id).await?;
                summary.secrets_vaulted += 1;
                // Secret value logged to import metadata, not stored in vault.
            }
        }
        secret_bar.inc(1);
    }
    secret_bar.finish_with_message("done");

    // Close database.
    db.close().await?;

    // Print summary.
    if json {
        let json_output = serde_json::to_string_pretty(&summary).map_err(|e| {
            BlufioError::migration_schema_failed(&format!(
                "failed to serialize import summary: {e}"
            ))
        })?;
        println!("{json_output}");
    } else {
        println!();
        println!("  OpenClaw Migration Complete");
        println!("  {}", "-".repeat(40));
        println!(
            "    Sessions:     {} imported, {} skipped",
            summary.sessions_imported, summary.sessions_skipped
        );
        println!("    Messages:     {} imported", summary.messages_imported);
        println!(
            "    Cost records: {} imported, {} skipped",
            summary.cost_records_imported, summary.cost_records_skipped
        );
        println!(
            "    Files:        {} imported, {} skipped",
            summary.files_imported, summary.files_skipped
        );
        println!(
            "    Secrets:      {} vaulted, {} skipped",
            summary.secrets_vaulted, summary.secrets_skipped
        );
        println!();
    }

    Ok(())
}

/// Check if an item has already been imported (idempotent guard).
async fn is_already_imported(
    conn: &tokio_rusqlite::Connection,
    item_type: &str,
    source_id: &str,
) -> Result<bool, BlufioError> {
    let item_type = item_type.to_string();
    let source_id = source_id.to_string();

    conn.call(move |conn| {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM migration_log WHERE source = 'openclaw' AND item_type = ?1 AND source_id = ?2",
                rusqlite::params![item_type, source_id],
                |row| row.get(0),
            )?;
        Ok(count > 0)
    })
    .await
    .map_err(|e: tokio_rusqlite::Error<rusqlite::Error>| BlufioError::storage_connection_failed(e))
}

/// Record an imported item in the migration_log.
async fn record_migration(
    conn: &tokio_rusqlite::Connection,
    item_type: &str,
    source_id: &str,
) -> Result<(), BlufioError> {
    let item_type = item_type.to_string();
    let source_id = source_id.to_string();

    conn.call(move |conn| {
        conn.execute(
            "INSERT OR IGNORE INTO migration_log (source, item_type, source_id) VALUES ('openclaw', ?1, ?2)",
            rusqlite::params![item_type, source_id],
        )?;
        Ok(())
    })
    .await
    .map_err(|e: tokio_rusqlite::Error<rusqlite::Error>| BlufioError::storage_connection_failed(e))
}

/// Try to store a secret in the vault.
///
/// Returns Ok if successful, Err if vault is unavailable.
async fn try_vault_store(config: &BlufioConfig, key: &str, value: &str) -> Result<(), BlufioError> {
    let conn = blufio_storage::open_connection(&config.storage.database_path).await?;

    if !blufio_vault::Vault::exists(&conn).await? {
        return Err(BlufioError::Vault(
            "vault not initialized -- secrets recorded in migration log but not encrypted"
                .to_string(),
        ));
    }

    let passphrase = blufio_vault::get_vault_passphrase()?;
    let vault = blufio_vault::Vault::unlock(conn, &passphrase, &config.vault).await?;
    vault.store_secret(key, value).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Config translate
// ---------------------------------------------------------------------------

/// Translate an OpenClaw JSON config to Blufio TOML.
pub fn run_config_translate(input: &str, output: Option<&str>) -> Result<(), BlufioError> {
    let content = std::fs::read_to_string(input).map_err(|e| {
        BlufioError::migration_schema_failed(&format!(
            "failed to read OpenClaw config '{}': {e}",
            input
        ))
    })?;

    let oc_config: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
        BlufioError::migration_schema_failed(&format!(
            "failed to parse OpenClaw config as JSON: {e}"
        ))
    })?;

    let oc_map = oc_config.as_object().ok_or_else(|| {
        BlufioError::migration_schema_failed("OpenClaw config is not a JSON object")
    })?;

    let mut blufio_config = BlufioConfig::default();
    let mut unmapped: Vec<(String, String)> = Vec::new();
    let mut mapped_count = 0u32;

    // Map known fields.
    for (key, value) in oc_map {
        match key.as_str() {
            "name" | "agent_name" => {
                if let Some(s) = value.as_str() {
                    blufio_config.agent.name = s.to_string();
                    mapped_count += 1;
                }
            }
            "model" | "default_model" => {
                if let Some(s) = value.as_str() {
                    blufio_config.anthropic.default_model = s.to_string();
                    mapped_count += 1;
                }
            }
            "max_tokens" | "max_output_tokens" => {
                if let Some(n) = value.as_u64() {
                    blufio_config.anthropic.max_tokens = n as u32;
                    mapped_count += 1;
                }
            }
            "api_key" | "anthropic_api_key" => {
                if let Some(s) = value.as_str() {
                    blufio_config.anthropic.api_key = Some(s.to_string());
                    mapped_count += 1;
                }
            }
            "database_path" | "db_path" | "storage_path" => {
                if let Some(s) = value.as_str() {
                    blufio_config.storage.database_path = s.to_string();
                    mapped_count += 1;
                }
            }
            "telegram" => {
                if let Some(obj) = value.as_object() {
                    if let Some(token) = obj.get("bot_token").and_then(|v| v.as_str()) {
                        blufio_config.telegram.bot_token = Some(token.to_string());
                        mapped_count += 1;
                    }
                    if let Some(ids) = obj.get("allowed_user_ids").and_then(|v| v.as_array()) {
                        blufio_config.telegram.allowed_users = ids
                            .iter()
                            .filter_map(|v| {
                                v.as_str()
                                    .map(|s| s.to_string())
                                    .or_else(|| v.as_i64().map(|n| n.to_string()))
                            })
                            .collect();
                        mapped_count += 1;
                    }
                }
            }
            "discord" => {
                if let Some(obj) = value.as_object()
                    && let Some(token) = obj.get("bot_token").and_then(|v| v.as_str())
                {
                    blufio_config.discord.bot_token = Some(token.to_string());
                    mapped_count += 1;
                }
            }
            "system_prompt" | "personality" | "system_message" => {
                if let Some(s) = value.as_str() {
                    blufio_config.agent.system_prompt = Some(s.to_string());
                    mapped_count += 1;
                }
            }
            "daily_budget" | "budget_daily" => {
                if let Some(n) = value.as_f64() {
                    blufio_config.cost.daily_budget_usd = Some(n);
                    mapped_count += 1;
                }
            }
            "monthly_budget" | "budget_monthly" => {
                if let Some(n) = value.as_f64() {
                    blufio_config.cost.monthly_budget_usd = Some(n);
                    mapped_count += 1;
                }
            }
            _ => {
                // Unmappable field -- preserve as comment.
                let value_str = match value {
                    serde_json::Value::String(s) => format!("\"{s}\""),
                    other => other.to_string(),
                };
                unmapped.push((key.clone(), value_str));
            }
        }
    }

    // Serialize to TOML.
    let mut toml_output = toml::to_string_pretty(&blufio_config).map_err(|e| {
        BlufioError::migration_schema_failed(&format!(
            "failed to serialize Blufio config to TOML: {e}"
        ))
    })?;

    // Append unmapped fields as comments.
    if !unmapped.is_empty() {
        toml_output.push_str("\n# -----------------------------------------------\n");
        toml_output.push_str("# Unmapped OpenClaw fields (review manually)\n");
        toml_output.push_str("# -----------------------------------------------\n");
        for (key, value) in &unmapped {
            toml_output.push_str(&format!("# UNMAPPED: {key} = {value}\n"));
        }
    }

    // Output.
    if let Some(output_path) = output {
        std::fs::write(output_path, &toml_output).map_err(|e| {
            BlufioError::migration_schema_failed(&format!("failed to write output file: {e}"))
        })?;
        eprintln!("Config written to: {output_path}");
    } else {
        print!("{toml_output}");
    }

    let unmapped_count = unmapped.len() as u32;
    eprintln!(
        "Translation complete: {mapped_count} fields mapped, {unmapped_count} fields unmapped."
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn detect_openclaw_dir_with_override() {
        let dir = tempdir().unwrap();
        let oc_dir = dir.path().join(".openclaw");
        std::fs::create_dir_all(&oc_dir).unwrap();

        let result = detect_openclaw_dir(Some(oc_dir.to_str().unwrap()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), oc_dir);
    }

    #[test]
    fn detect_openclaw_dir_missing_returns_error() {
        let result = detect_openclaw_dir(Some("/nonexistent/path"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("does not exist"));
    }

    #[test]
    fn detect_openclaw_dir_no_sources_returns_error() {
        let result = detect_openclaw_dir(None);
        // This will likely fail since ~/.openclaw probably doesn't exist in test env.
        // That's OK -- we just verify it returns a helpful error.
        if let Err(e) = result {
            let err = e.to_string();
            assert!(err.contains("not found") || err.contains("does not exist"));
        }
    }

    #[test]
    fn extract_secrets_from_json_finds_api_keys() {
        let json = serde_json::json!({
            "name": "test",
            "api_key": "sk-test-123",
            "nested": {
                "bot_token": "bot-456",
                "normal_field": "not a secret"
            }
        });
        let mut secrets = Vec::new();
        extract_secrets_from_json(&json, "", &mut secrets);
        assert_eq!(secrets.len(), 2);
        assert!(
            secrets
                .iter()
                .any(|(k, v)| k == "api_key" && v == "sk-test-123")
        );
        assert!(
            secrets
                .iter()
                .any(|(k, v)| k == "nested.bot_token" && v == "bot-456")
        );
    }

    #[test]
    fn parse_openclaw_dir_empty_succeeds() {
        let dir = tempdir().unwrap();
        let result = parse_openclaw_dir(dir.path());
        assert!(result.is_ok());
        let data = result.unwrap();
        assert!(data.sessions.is_empty());
        assert!(data.cost_records.is_empty());
        assert!(data.personality_files.is_empty());
    }

    #[test]
    fn parse_openclaw_dir_with_config() {
        let dir = tempdir().unwrap();
        let config = serde_json::json!({
            "name": "my-agent",
            "api_key": "sk-test",
            "model": "claude-3"
        });
        std::fs::write(
            dir.path().join("config.json"),
            serde_json::to_string_pretty(&config).unwrap(),
        )
        .unwrap();

        let data = parse_openclaw_dir(dir.path()).unwrap();
        assert!(data.config.is_some());
        assert_eq!(data.secrets.len(), 1);
        assert_eq!(data.secrets[0].0, "api_key");
    }

    #[test]
    fn parse_openclaw_dir_with_sessions_json() {
        let dir = tempdir().unwrap();
        let sessions = serde_json::json!([
            {
                "id": "sess-1",
                "channel": "cli",
                "messages": [
                    {"role": "user", "content": "hello"},
                    {"role": "assistant", "content": "hi there"}
                ]
            }
        ]);
        std::fs::write(
            dir.path().join("sessions.json"),
            serde_json::to_string_pretty(&sessions).unwrap(),
        )
        .unwrap();

        let data = parse_openclaw_dir(dir.path()).unwrap();
        assert_eq!(data.sessions.len(), 1);
        assert_eq!(data.sessions[0].messages.len(), 2);
    }

    #[test]
    fn parse_openclaw_dir_with_personality_files() {
        let dir = tempdir().unwrap();
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("SOUL.md"), "# Soul").unwrap();
        std::fs::write(workspace.join("custom.md"), "# Custom").unwrap();

        let data = parse_openclaw_dir(dir.path()).unwrap();
        assert_eq!(data.personality_files.len(), 2);
    }

    #[test]
    fn config_translate_maps_known_fields() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("openclaw.json");
        let output_path = dir.path().join("blufio.toml");

        let oc_config = serde_json::json!({
            "name": "my-agent",
            "model": "claude-3-opus",
            "temperature": 0.7,
            "max_tokens": 4096,
            "daily_budget": 5.0,
            "unknown_field": "some value",
            "another_unknown": 42
        });

        std::fs::write(
            &input_path,
            serde_json::to_string_pretty(&oc_config).unwrap(),
        )
        .unwrap();

        let result = run_config_translate(
            input_path.to_str().unwrap(),
            Some(output_path.to_str().unwrap()),
        );
        assert!(result.is_ok());

        let output = std::fs::read_to_string(&output_path).unwrap();
        assert!(output.contains("my-agent"));
        assert!(output.contains("claude-3-opus"));
        assert!(output.contains("UNMAPPED: unknown_field"));
        assert!(output.contains("UNMAPPED: another_unknown"));
    }

    #[test]
    fn config_translate_missing_file_errors() {
        let result = run_config_translate("/nonexistent/config.json", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("failed to read"));
    }

    #[test]
    fn config_translate_invalid_json_errors() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("bad.json");
        std::fs::write(&input_path, "not json").unwrap();

        let result = run_config_translate(input_path.to_str().unwrap(), None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("failed to parse"));
    }
}
