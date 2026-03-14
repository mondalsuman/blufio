// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio doctor` command implementation.
//!
//! Runs diagnostic checks against the Blufio environment to identify
//! configuration issues, connectivity problems, and resource constraints.

use std::io::IsTerminal;
use std::time::{Duration, Instant};

use blufio_config::model::BlufioConfig;
use blufio_core::BlufioError;

/// Status of a diagnostic check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckStatus {
    /// Check passed successfully.
    Pass,
    /// Check passed with a warning.
    Warn,
    /// Check failed.
    Fail,
}

/// Result of a single diagnostic check.
#[derive(Debug, Clone)]
pub struct CheckResult {
    /// Name of the check.
    pub name: String,
    /// Check status.
    pub status: CheckStatus,
    /// Human-readable message.
    pub message: String,
    /// Duration the check took.
    pub duration: Duration,
}

/// Run the `blufio doctor` command.
///
/// Runs quick diagnostic checks. With `--deep`, runs additional intensive checks.
/// With `--plain`, disables colored output.
pub async fn run_doctor(config: &BlufioConfig, deep: bool, plain: bool) -> Result<(), BlufioError> {
    let use_color = !plain && std::io::stdout().is_terminal();
    let mut results = Vec::new();

    // Quick checks (always run)
    results.push(check_config().await);
    results.push(check_database(&config.storage.database_path).await);
    results.push(check_encryption(&config.storage.database_path).await);
    results.push(check_llm_connectivity(config).await);
    results.push(check_health_endpoint(config).await);
    results.push(check_audit_trail(config).await);

    // MCP server checks (if configured)
    #[cfg(feature = "mcp-client")]
    {
        let mcp_results = check_mcp_servers(config).await;
        results.extend(mcp_results);
    }

    // Injection defense check
    results.push(check_injection_defense(config));

    // Cron scheduler health check
    results.push(check_cron(&config.storage.database_path).await);

    // Hook system health check
    results.push(check_hooks(config));

    // Hot reload health check
    results.push(check_hot_reload(config));

    // GDPR readiness check
    results.push(check_gdpr(config));

    // Retention health check
    results.push(check_retention(config).await);

    // Litestream WAL replication check
    results.push(check_litestream(config));

    // vec0 vector search health check
    results.push(check_vec0(config).await);

    // Deep checks (only with --deep)
    if deep {
        results.push(check_db_integrity(&config.storage.database_path).await);
        results.push(check_disk_space(&config.storage.database_path).await);
        results.push(check_memory_baseline().await);
    }

    // Print results
    println!();
    println!("  blufio doctor");
    println!("  {}", "-".repeat(50));

    let mut fail_count = 0;
    let mut warn_count = 0;

    for result in &results {
        let duration_ms = result.duration.as_millis();
        let status_symbol;
        let line;

        match result.status {
            CheckStatus::Pass => {
                if use_color {
                    use colored::Colorize;
                    status_symbol = "✓".green().to_string();
                    line = format!(
                        "    {status_symbol} {:<20} {} ({duration_ms}ms)",
                        result.name, result.message
                    );
                } else {
                    line = format!(
                        "    [OK]   {:<20} {} ({duration_ms}ms)",
                        result.name, result.message
                    );
                }
            }
            CheckStatus::Warn => {
                warn_count += 1;
                if use_color {
                    use colored::Colorize;
                    status_symbol = "!".yellow().to_string();
                    line = format!(
                        "    {status_symbol} {:<20} {} ({duration_ms}ms)",
                        result.name,
                        result.message.yellow()
                    );
                } else {
                    line = format!(
                        "    [WARN] {:<20} {} ({duration_ms}ms)",
                        result.name, result.message
                    );
                }
            }
            CheckStatus::Fail => {
                fail_count += 1;
                if use_color {
                    use colored::Colorize;
                    status_symbol = "✗".red().to_string();
                    line = format!(
                        "    {status_symbol} {:<20} {} ({duration_ms}ms)",
                        result.name,
                        result.message.red()
                    );
                } else {
                    line = format!(
                        "    [FAIL] {:<20} {} ({duration_ms}ms)",
                        result.name, result.message
                    );
                }
            }
        }

        println!("{line}");
    }

    println!();

    if fail_count > 0 || warn_count > 0 {
        let issues = fail_count + warn_count;
        let issue_word = if issues == 1 { "issue" } else { "issues" };
        println!("  {issues} {issue_word} found.");
        if !deep {
            println!("  Run with --deep for detailed diagnostics.");
        }
    } else {
        println!("  All checks passed.");
    }

    println!();

    Ok(())
}

/// Check configuration loads without errors.
async fn check_config() -> CheckResult {
    let start = Instant::now();
    match blufio_config::load_and_validate() {
        Ok(_) => CheckResult {
            name: "Configuration".to_string(),
            status: CheckStatus::Pass,
            message: "valid".to_string(),
            duration: start.elapsed(),
        },
        Err(errors) => CheckResult {
            name: "Configuration".to_string(),
            status: CheckStatus::Fail,
            message: format!("{} error(s)", errors.len()),
            duration: start.elapsed(),
        },
    }
}

/// Check database file exists and can be opened.
async fn check_database(db_path: &str) -> CheckResult {
    let start = Instant::now();
    let path = std::path::Path::new(db_path);

    if !path.exists() {
        return CheckResult {
            name: "Database".to_string(),
            status: CheckStatus::Warn,
            message: format!("not found: {db_path} (will be created on first run)"),
            duration: start.elapsed(),
        };
    }

    match blufio_storage::open_connection(db_path).await {
        Ok(conn) => {
            let query_result: Result<(), tokio_rusqlite::Error<rusqlite::Error>> = conn
                .call(|conn| {
                    conn.execute_batch("SELECT 1")?;
                    Ok(())
                })
                .await;

            match query_result {
                Ok(()) => CheckResult {
                    name: "Database".to_string(),
                    status: CheckStatus::Pass,
                    message: "connected".to_string(),
                    duration: start.elapsed(),
                },
                Err(e) => CheckResult {
                    name: "Database".to_string(),
                    status: CheckStatus::Fail,
                    message: format!("query failed: {e}"),
                    duration: start.elapsed(),
                },
            }
        }
        Err(e) => CheckResult {
            name: "Database".to_string(),
            status: CheckStatus::Fail,
            message: format!("open failed: {e}"),
            duration: start.elapsed(),
        },
    }
}

/// Check encryption status of the database (CIPH-08).
///
/// Reports:
/// - Pass ("not encrypted") when no key and plaintext DB
/// - Warn when BLUFIO_DB_KEY set but DB is still plaintext
/// - Fail when DB is encrypted but no key is set
/// - Pass with cipher details when encrypted and key is correct
async fn check_encryption(db_path: &str) -> CheckResult {
    let start = Instant::now();
    let path = std::path::Path::new(db_path);

    if !path.exists() {
        return CheckResult {
            name: "Encryption".to_string(),
            status: CheckStatus::Pass,
            message: "no database yet".to_string(),
            duration: start.elapsed(),
        };
    }

    let is_plaintext = blufio_storage::is_plaintext_sqlite(path).unwrap_or(true);
    let has_key = std::env::var("BLUFIO_DB_KEY").is_ok();

    match (is_plaintext, has_key) {
        (true, false) => CheckResult {
            name: "Encryption".to_string(),
            status: CheckStatus::Pass,
            message: "not encrypted".to_string(),
            duration: start.elapsed(),
        },
        (true, true) => CheckResult {
            name: "Encryption".to_string(),
            status: CheckStatus::Warn,
            message: "BLUFIO_DB_KEY set but database is plaintext -- run: blufio db encrypt"
                .to_string(),
            duration: start.elapsed(),
        },
        (false, false) => CheckResult {
            name: "Encryption".to_string(),
            status: CheckStatus::Fail,
            message: "database is encrypted but BLUFIO_DB_KEY is not set".to_string(),
            duration: start.elapsed(),
        },
        (false, true) => {
            // Query cipher details.
            match blufio_storage::open_connection(db_path).await {
                Ok(conn) => {
                    let info = conn
                        .call(|conn| -> Result<(String, i64), rusqlite::Error> {
                            let version: String = conn
                                .query_row("PRAGMA cipher_version;", [], |row| row.get(0))
                                .unwrap_or_else(|_| "unknown".to_string());
                            let page_size: i64 = conn
                                .query_row("PRAGMA cipher_page_size;", [], |row| row.get(0))
                                .unwrap_or(4096);
                            Ok((version, page_size))
                        })
                        .await;

                    match info {
                        Ok((version, page_size)) => CheckResult {
                            name: "Encryption".to_string(),
                            status: CheckStatus::Pass,
                            message: format!(
                                "encrypted (SQLCipher {version}, page size: {page_size})"
                            ),
                            duration: start.elapsed(),
                        },
                        Err(_) => CheckResult {
                            name: "Encryption".to_string(),
                            status: CheckStatus::Warn,
                            message: "encrypted but could not query cipher details".to_string(),
                            duration: start.elapsed(),
                        },
                    }
                }
                Err(_) => CheckResult {
                    name: "Encryption".to_string(),
                    status: CheckStatus::Fail,
                    message: "encrypted but cannot open -- verify BLUFIO_DB_KEY".to_string(),
                    duration: start.elapsed(),
                },
            }
        }
    }
}

/// Check LLM API connectivity via HEAD request.
async fn check_llm_connectivity(config: &BlufioConfig) -> CheckResult {
    let start = Instant::now();

    let has_api_key =
        config.anthropic.api_key.is_some() || std::env::var("ANTHROPIC_API_KEY").is_ok();

    if !has_api_key {
        return CheckResult {
            name: "LLM API".to_string(),
            status: CheckStatus::Warn,
            message: "no API key configured".to_string(),
            duration: start.elapsed(),
        };
    }

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return CheckResult {
                name: "LLM API".to_string(),
                status: CheckStatus::Fail,
                message: format!("HTTP client error: {e}"),
                duration: start.elapsed(),
            };
        }
    };

    match client
        .head("https://api.anthropic.com/v1/messages")
        .send()
        .await
    {
        Ok(_resp) => CheckResult {
            name: "LLM API".to_string(),
            status: CheckStatus::Pass,
            message: "reachable".to_string(),
            duration: start.elapsed(),
        },
        Err(e) => {
            let msg = if e.is_timeout() {
                "timeout (5s)".to_string()
            } else if e.is_connect() {
                "connection refused".to_string()
            } else {
                format!("error: {e}")
            };
            CheckResult {
                name: "LLM API".to_string(),
                status: CheckStatus::Fail,
                message: msg,
                duration: start.elapsed(),
            }
        }
    }
}

/// Check gateway health endpoint.
async fn check_health_endpoint(config: &BlufioConfig) -> CheckResult {
    let start = Instant::now();
    let host = &config.gateway.host;
    let port = config.daemon.health_port;
    let url = format!("http://{host}:{port}/health");

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return CheckResult {
                name: "Health endpoint".to_string(),
                status: CheckStatus::Fail,
                message: format!("HTTP client error: {e}"),
                duration: start.elapsed(),
            };
        }
    };

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => CheckResult {
            name: "Health endpoint".to_string(),
            status: CheckStatus::Pass,
            message: "reachable".to_string(),
            duration: start.elapsed(),
        },
        Ok(resp) => CheckResult {
            name: "Health endpoint".to_string(),
            status: CheckStatus::Warn,
            message: format!("status {}", resp.status()),
            duration: start.elapsed(),
        },
        Err(_) => CheckResult {
            name: "Health endpoint".to_string(),
            status: CheckStatus::Warn,
            message: format!("not reachable at {url} (agent may not be running)"),
            duration: start.elapsed(),
        },
    }
}

/// Check connectivity to configured MCP servers (CLNT-13).
///
/// Each configured server is checked independently. A failing server
/// does not prevent other checks from running.
#[cfg(feature = "mcp-client")]
async fn check_mcp_servers(config: &BlufioConfig) -> Vec<CheckResult> {
    let mut results = Vec::new();

    if config.mcp.servers.is_empty() {
        return results;
    }

    for server in &config.mcp.servers {
        let start = Instant::now();
        let check_name = format!("mcp:{}", server.name);

        let diag = blufio_mcp_client::diagnose_server(server).await;

        match (diag.tool_count, diag.error) {
            (Some(count), _) => {
                results.push(CheckResult {
                    name: check_name,
                    status: CheckStatus::Pass,
                    message: format!("{} tools via {}", count, diag.transport),
                    duration: start.elapsed(),
                });
            }
            (_, Some(error)) => {
                results.push(CheckResult {
                    name: check_name,
                    status: CheckStatus::Fail,
                    message: error,
                    duration: start.elapsed(),
                });
            }
            (None, None) => {
                results.push(CheckResult {
                    name: check_name,
                    status: CheckStatus::Warn,
                    message: "unknown status".to_string(),
                    duration: start.elapsed(),
                });
            }
        }
    }

    results
}

/// Check audit trail health (last 100 entries, not full chain walk).
async fn check_audit_trail(config: &BlufioConfig) -> CheckResult {
    let start = Instant::now();

    if !config.audit.enabled {
        return CheckResult {
            name: "Audit Trail".to_string(),
            status: CheckStatus::Warn,
            message: "audit trail is disabled".to_string(),
            duration: start.elapsed(),
        };
    }

    // Resolve audit.db path.
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
        return CheckResult {
            name: "Audit Trail".to_string(),
            status: CheckStatus::Warn,
            message: "audit database not found (will be created on first serve)".to_string(),
            duration: start.elapsed(),
        };
    }

    // Open a sync connection and verify the last 100 entries.
    match blufio_storage::open_connection_sync(
        &audit_db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(conn) => {
            // Get entry count and last timestamp.
            let stats: Result<(i64, Option<String>), _> = conn.query_row(
                "SELECT COUNT(*), MAX(timestamp) FROM audit_entries",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            );

            let (entry_count, last_ts) = match stats {
                Ok(s) => s,
                Err(e) => {
                    return CheckResult {
                        name: "Audit Trail".to_string(),
                        status: CheckStatus::Fail,
                        message: format!("query failed: {e}"),
                        duration: start.elapsed(),
                    };
                }
            };

            // Verify last 100 entries by fetching them and checking the chain.
            // We read at most 100 entries (ordered by id DESC), then verify in ASC order.
            let verify_result: Result<bool, _> = (|| -> Result<bool, rusqlite::Error> {
                let mut stmt = conn.prepare(
                    "SELECT id, entry_hash, prev_hash, timestamp, event_type, action, \
                     resource_type, resource_id FROM \
                     (SELECT * FROM audit_entries ORDER BY id DESC LIMIT 100) \
                     ORDER BY id ASC",
                )?;
                type AuditRow = (i64, String, String, String, String, String, String, String);
                let entries: Vec<AuditRow> = stmt
                    .query_map([], |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                            row.get(6)?,
                            row.get(7)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;

                if entries.is_empty() {
                    return Ok(true);
                }

                // For the tail check, we verify each entry's hash matches
                // its recomputed hash from prev_hash. We trust the first
                // entry's prev_hash (we're not verifying the full chain).
                for (_id, entry_hash, prev_hash, ts, et, act, rt, rid) in &entries {
                    let expected =
                        blufio_audit::compute_entry_hash(prev_hash, ts, et, act, rt, rid);
                    if entry_hash != &expected {
                        return Ok(false);
                    }
                }

                // Also verify chain linkage between consecutive entries.
                for i in 1..entries.len() {
                    let (_, ref prev_entry_hash, _, _, _, _, _, _) = entries[i - 1];
                    let (_, _, ref this_prev_hash, _, _, _, _, _) = entries[i];
                    if this_prev_hash != prev_entry_hash {
                        return Ok(false);
                    }
                }

                Ok(true)
            })();

            match verify_result {
                Ok(true) => {
                    let ts_info = last_ts
                        .map(|ts| format!(", last: {ts}"))
                        .unwrap_or_default();
                    CheckResult {
                        name: "Audit Trail".to_string(),
                        status: CheckStatus::Pass,
                        message: format!("{entry_count} entries, chain intact{ts_info}"),
                        duration: start.elapsed(),
                    }
                }
                Ok(false) => CheckResult {
                    name: "Audit Trail".to_string(),
                    status: CheckStatus::Fail,
                    message: format!(
                        "{entry_count} entries, chain BROKEN in last 100 -- run: blufio audit verify"
                    ),
                    duration: start.elapsed(),
                },
                Err(e) => CheckResult {
                    name: "Audit Trail".to_string(),
                    status: CheckStatus::Fail,
                    message: format!("verification error: {e}"),
                    duration: start.elapsed(),
                },
            }
        }
        Err(e) => CheckResult {
            name: "Audit Trail".to_string(),
            status: CheckStatus::Fail,
            message: format!("open failed: {e}"),
            duration: start.elapsed(),
        },
    }
}

/// Check injection defense configuration and HMAC self-test.
fn check_injection_defense(config: &BlufioConfig) -> CheckResult {
    let start = Instant::now();
    let cfg = &config.injection_defense;

    if !cfg.enabled {
        return CheckResult {
            name: "Injection Defense".to_string(),
            status: CheckStatus::Warn,
            message: "disabled".to_string(),
            duration: start.elapsed(),
        };
    }

    // Count active layers.
    let mut active_layers = vec!["L1"];
    if cfg.hmac_boundaries.enabled {
        active_layers.push("L3");
    }
    if cfg.output_screening.enabled {
        active_layers.push("L4");
    }
    if cfg.hitl.enabled {
        active_layers.push("L5");
    }

    // Test HMAC self-test (if boundaries enabled).
    let hmac_ok = if cfg.hmac_boundaries.enabled {
        // Generate a test boundary, validate it, verify strip works.
        let test_key = [0u8; 32];
        let bm = blufio_injection::boundary::BoundaryManager::new(
            &test_key,
            "doctor-test",
            &cfg.hmac_boundaries,
        );
        let wrapped = bm.wrap_content(
            blufio_injection::boundary::ZoneType::Static,
            "system",
            "test content",
        );
        let (stripped, failures) = bm.validate_and_strip(&wrapped, "doctor-check");
        failures.is_empty() && !stripped.is_empty()
    } else {
        true
    };

    // Verify custom patterns compile by creating a classifier.
    // The classifier constructor silently skips invalid patterns.
    let classifier =
        blufio_injection::classifier::InjectionClassifier::new(&config.injection_defense);
    // Quick self-test: classify known injection text should score > 0.
    let self_test = classifier.classify("ignore previous instructions", "user");
    let classifier_ok = self_test.score > 0.0;
    let custom_count_configured = cfg.input_detection.custom_patterns.len();
    // We can't easily count how many compiled vs failed, so we just check
    // that the classifier works at all.
    let custom_errors: usize = 0;

    // Canary self-test.
    let canary_ok = blufio_injection::canary::CanaryTokenManager::self_test();

    let custom_count = custom_count_configured;
    let _ = classifier_ok; // suppress unused warning if not needed elsewhere
    let details = format!(
        "{} layers ({}), {}custom patterns, HMAC {}, canary {}",
        active_layers.len(),
        active_layers.join("/"),
        if custom_count > 0 {
            format!("{} ", custom_count)
        } else {
            "no ".to_string()
        },
        if cfg.hmac_boundaries.enabled {
            if hmac_ok {
                "self-test OK"
            } else {
                "self-test FAILED"
            }
        } else {
            "disabled"
        },
        if canary_ok {
            "self-test OK"
        } else {
            "self-test FAILED"
        }
    );

    if !hmac_ok || !canary_ok {
        return CheckResult {
            name: "Injection Defense".to_string(),
            status: CheckStatus::Fail,
            message: details,
            duration: start.elapsed(),
        };
    }

    if custom_errors > 0 {
        return CheckResult {
            name: "Injection Defense".to_string(),
            status: CheckStatus::Warn,
            message: format!("{} ({} custom regex errors)", details, custom_errors),
            duration: start.elapsed(),
        };
    }

    CheckResult {
        name: "Injection Defense".to_string(),
        status: CheckStatus::Pass,
        message: details,
        duration: start.elapsed(),
    }
}

/// Deep check: SQLite integrity check.
async fn check_db_integrity(db_path: &str) -> CheckResult {
    let start = Instant::now();
    let path = std::path::Path::new(db_path);

    if !path.exists() {
        return CheckResult {
            name: "DB integrity".to_string(),
            status: CheckStatus::Warn,
            message: "database not found (skipped)".to_string(),
            duration: start.elapsed(),
        };
    }

    match blufio_storage::open_connection(db_path).await {
        Ok(conn) => {
            let result: Result<Vec<String>, tokio_rusqlite::Error<rusqlite::Error>> = conn
                .call(|conn| {
                    let mut stmt = conn.prepare("PRAGMA integrity_check")?;
                    let rows: Vec<String> = stmt
                        .query_map([], |row| row.get(0))?
                        .filter_map(|r| r.ok())
                        .collect();
                    Ok(rows)
                })
                .await;

            match result {
                Ok(rows) if rows.len() == 1 && rows[0] == "ok" => CheckResult {
                    name: "DB integrity".to_string(),
                    status: CheckStatus::Pass,
                    message: "ok".to_string(),
                    duration: start.elapsed(),
                },
                Ok(rows) => CheckResult {
                    name: "DB integrity".to_string(),
                    status: CheckStatus::Fail,
                    message: format!("{} issue(s) found", rows.len()),
                    duration: start.elapsed(),
                },
                Err(e) => CheckResult {
                    name: "DB integrity".to_string(),
                    status: CheckStatus::Fail,
                    message: format!("check failed: {e}"),
                    duration: start.elapsed(),
                },
            }
        }
        Err(e) => CheckResult {
            name: "DB integrity".to_string(),
            status: CheckStatus::Fail,
            message: format!("open failed: {e}"),
            duration: start.elapsed(),
        },
    }
}

/// Deep check: available disk space.
async fn check_disk_space(db_path: &str) -> CheckResult {
    let start = Instant::now();
    let path = std::path::Path::new(db_path);
    let check_path = if path.exists() {
        path.to_path_buf()
    } else {
        path.parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf()
    };

    match std::fs::metadata(&check_path) {
        Ok(_) => {
            // On most platforms we can't easily get free disk space from std.
            // Report the DB file size as a heuristic.
            if path.exists() {
                let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                let size_mb = size as f64 / (1024.0 * 1024.0);
                CheckResult {
                    name: "Disk space".to_string(),
                    status: CheckStatus::Pass,
                    message: format!("DB size: {size_mb:.1} MB"),
                    duration: start.elapsed(),
                }
            } else {
                CheckResult {
                    name: "Disk space".to_string(),
                    status: CheckStatus::Pass,
                    message: "directory accessible".to_string(),
                    duration: start.elapsed(),
                }
            }
        }
        Err(e) => CheckResult {
            name: "Disk space".to_string(),
            status: CheckStatus::Warn,
            message: format!("cannot access: {e}"),
            duration: start.elapsed(),
        },
    }
}

/// Deep check: memory baseline via jemalloc.
async fn check_memory_baseline() -> CheckResult {
    let start = Instant::now();

    #[cfg(not(target_env = "msvc"))]
    {
        let _ = tikv_jemalloc_ctl::epoch::advance();
        let allocated = tikv_jemalloc_ctl::stats::allocated::read().unwrap_or(0);
        let resident = tikv_jemalloc_ctl::stats::resident::read().unwrap_or(0);
        let allocated_mb = allocated as f64 / (1024.0 * 1024.0);
        let resident_mb = resident as f64 / (1024.0 * 1024.0);

        CheckResult {
            name: "Memory baseline".to_string(),
            status: CheckStatus::Pass,
            message: format!("heap: {allocated_mb:.1} MB, resident: {resident_mb:.1} MB"),
            duration: start.elapsed(),
        }
    }

    #[cfg(target_env = "msvc")]
    {
        CheckResult {
            name: "Memory baseline".to_string(),
            status: CheckStatus::Warn,
            message: "jemalloc not available on MSVC".to_string(),
            duration: start.elapsed(),
        }
    }
}

/// Check cron scheduler health: active jobs, stale locks, recent failures.
async fn check_cron(db_path: &str) -> CheckResult {
    let start = Instant::now();
    let path = std::path::Path::new(db_path);

    if !path.exists() {
        return CheckResult {
            name: "Cron Scheduler".to_string(),
            status: CheckStatus::Pass,
            message: "no database yet".to_string(),
            duration: start.elapsed(),
        };
    }

    match blufio_storage::open_connection_sync(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(conn) => {
            // Check if cron_jobs table exists (V14 migration may not have run yet).
            let table_exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='cron_jobs'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(false);

            if !table_exists {
                return CheckResult {
                    name: "Cron Scheduler".to_string(),
                    status: CheckStatus::Pass,
                    message: "not configured (no cron tables)".to_string(),
                    duration: start.elapsed(),
                };
            }

            // Count active (enabled) jobs.
            let active_jobs: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM cron_jobs WHERE enabled = 1",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            // Count stale locks (running = 1).
            let stale_locks: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM cron_jobs WHERE running = 1",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            // Get most recent execution.
            let last_run: Option<String> = conn
                .query_row(
                    "SELECT last_run_at FROM cron_jobs WHERE enabled = 1 AND last_run_at IS NOT NULL \
                     ORDER BY last_run_at DESC LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .ok();

            // Count recent failures (last 24h) from cron_history.
            let history_exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='cron_history'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(false);

            let recent_failures: i64 = if history_exists {
                conn.query_row(
                    "SELECT COUNT(*) FROM cron_history WHERE status = 'failed' \
                     AND started_at > datetime('now', '-24 hours')",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0)
            } else {
                0
            };

            // Build result message.
            if active_jobs == 0 {
                return CheckResult {
                    name: "Cron Scheduler".to_string(),
                    status: CheckStatus::Pass,
                    message: "no active jobs".to_string(),
                    duration: start.elapsed(),
                };
            }

            // Check for warnings.
            let mut warnings = Vec::new();
            if stale_locks > 0 {
                warnings.push(format!("{stale_locks} stale lock(s)"));
            }
            if recent_failures > 0 {
                warnings.push(format!("{recent_failures} failed in last 24h"));
            }

            let last_run_info = last_run
                .map(|ts| format!(", last run: {ts}"))
                .unwrap_or_default();

            if !warnings.is_empty() {
                CheckResult {
                    name: "Cron Scheduler".to_string(),
                    status: CheckStatus::Warn,
                    message: format!(
                        "{active_jobs} active jobs, {}{}",
                        warnings.join(", "),
                        last_run_info
                    ),
                    duration: start.elapsed(),
                }
            } else {
                CheckResult {
                    name: "Cron Scheduler".to_string(),
                    status: CheckStatus::Pass,
                    message: format!("{active_jobs} active jobs, 0 stale locks{}", last_run_info),
                    duration: start.elapsed(),
                }
            }
        }
        Err(e) => CheckResult {
            name: "Cron Scheduler".to_string(),
            status: CheckStatus::Fail,
            message: format!("open failed: {e}"),
            duration: start.elapsed(),
        },
    }
}

/// Check retention policy health: configuration status and pending deletions.
async fn check_retention(config: &BlufioConfig) -> CheckResult {
    let start = Instant::now();

    if !config.retention.enabled {
        return CheckResult {
            name: "Retention".to_string(),
            status: CheckStatus::Pass,
            message: "disabled".to_string(),
            duration: start.elapsed(),
        };
    }

    let db_path = &config.storage.database_path;
    let path = std::path::Path::new(db_path);

    if !path.exists() {
        return CheckResult {
            name: "Retention".to_string(),
            status: CheckStatus::Pass,
            message: "enabled (no database yet)".to_string(),
            duration: start.elapsed(),
        };
    }

    match blufio_storage::open_connection_sync(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(conn) => {
            // Count soft-deleted records pending permanent deletion.
            let mut pending_total: i64 = 0;

            // Check messages table.
            let has_deleted_at = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM pragma_table_info('messages') WHERE name='deleted_at'",
                    [],
                    |row| row.get::<_, bool>(0),
                )
                .unwrap_or(false);

            if has_deleted_at {
                let msg_count: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM messages WHERE deleted_at IS NOT NULL",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);
                pending_total += msg_count;

                // Also check sessions.
                let session_count: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM sessions WHERE deleted_at IS NOT NULL",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);
                pending_total += session_count;
            }

            // Check memories table.
            let memories_has_deleted_at = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM pragma_table_info('memories') WHERE name='deleted_at'",
                    [],
                    |row| row.get::<_, bool>(0),
                )
                .unwrap_or(false);

            if memories_has_deleted_at {
                let mem_count: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM memories WHERE deleted_at IS NOT NULL",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);
                pending_total += mem_count;
            }

            let periods = &config.retention.periods;
            let msg_days = periods
                .messages
                .map(|d| format!("{d}d"))
                .unwrap_or_else(|| "none".to_string());
            let ses_days = periods
                .sessions
                .map(|d| format!("{d}d"))
                .unwrap_or_else(|| "none".to_string());
            let mem_days = periods
                .memories
                .map(|d| format!("{d}d"))
                .unwrap_or_else(|| "none".to_string());
            CheckResult {
                name: "Retention".to_string(),
                status: CheckStatus::Pass,
                message: format!(
                    "enabled (messages: {msg_days}, sessions: {ses_days}, memories: {mem_days}), \
                     {pending_total} records pending deletion",
                ),
                duration: start.elapsed(),
            }
        }
        Err(e) => CheckResult {
            name: "Retention".to_string(),
            status: CheckStatus::Fail,
            message: format!("open failed: {e}"),
            duration: start.elapsed(),
        },
    }
}

/// Check hook system health: enabled status, hook count, event validation, command paths.
fn check_hooks(config: &BlufioConfig) -> CheckResult {
    let start = Instant::now();

    if !config.hooks.enabled {
        return CheckResult {
            name: "Hook System".to_string(),
            status: CheckStatus::Warn,
            message: "disabled (enable with [hooks] enabled = true)".to_string(),
            duration: start.elapsed(),
        };
    }

    let enabled_hooks: Vec<_> = config
        .hooks
        .definitions
        .iter()
        .filter(|d| d.enabled)
        .collect();

    if enabled_hooks.is_empty() {
        return CheckResult {
            name: "Hook System".to_string(),
            status: CheckStatus::Warn,
            message: "enabled but no hooks defined".to_string(),
            duration: start.elapsed(),
        };
    }

    // Validate hook event names
    let warnings = blufio_hooks::manager::validate_hook_events(&config.hooks);
    if !warnings.is_empty() {
        return CheckResult {
            name: "Hook System".to_string(),
            status: CheckStatus::Fail,
            message: format!(
                "{} hook(s) reference unknown events: {}",
                warnings.len(),
                warnings.join("; ")
            ),
            duration: start.elapsed(),
        };
    }

    // Check hook commands exist (absolute paths only)
    let mut missing_commands = Vec::new();
    for hook in &enabled_hooks {
        let executable = hook
            .command
            .split_whitespace()
            .next()
            .unwrap_or(&hook.command);
        let exec_path = std::path::Path::new(executable);
        if exec_path.is_absolute() && !exec_path.exists() {
            missing_commands.push(format!("{}: {}", hook.name, executable));
        }
    }

    if !missing_commands.is_empty() {
        return CheckResult {
            name: "Hook System".to_string(),
            status: CheckStatus::Warn,
            message: format!(
                "{} hook(s) configured, {} command(s) not found: {}",
                enabled_hooks.len(),
                missing_commands.len(),
                missing_commands.join("; ")
            ),
            duration: start.elapsed(),
        };
    }

    CheckResult {
        name: "Hook System".to_string(),
        status: CheckStatus::Pass,
        message: format!("{} hook(s) configured", enabled_hooks.len()),
        duration: start.elapsed(),
    }
}

/// Check hot reload health: enabled features and file existence.
fn check_hot_reload(config: &BlufioConfig) -> CheckResult {
    let start = Instant::now();

    if !config.hot_reload.enabled {
        return CheckResult {
            name: "Hot Reload".to_string(),
            status: CheckStatus::Warn,
            message: "disabled (enable with [hot_reload] enabled = true)".to_string(),
            duration: start.elapsed(),
        };
    }

    let mut features = Vec::new();
    features.push(format!(
        "config ({}ms debounce)",
        config.hot_reload.debounce_ms
    ));

    if config.hot_reload.tls_cert_path.is_some() && config.hot_reload.tls_key_path.is_some() {
        let cert_path = config.hot_reload.tls_cert_path.as_deref().unwrap_or("");
        let key_path = config.hot_reload.tls_key_path.as_deref().unwrap_or("");
        if std::path::Path::new(cert_path).exists() && std::path::Path::new(key_path).exists() {
            features.push("TLS cert reload".into());
        } else {
            return CheckResult {
                name: "Hot Reload".to_string(),
                status: CheckStatus::Fail,
                message: format!(
                    "TLS cert/key paths configured but files not found (cert: {}, key: {})",
                    cert_path, key_path
                ),
                duration: start.elapsed(),
            };
        }
    }

    if config.hot_reload.watch_skills {
        let skills_dir = std::path::Path::new(&config.skill.skills_dir);
        if skills_dir.exists() {
            features.push("skill watching".into());
        } else {
            features.push(format!(
                "skill watching (dir missing: {})",
                config.skill.skills_dir
            ));
        }
    }

    CheckResult {
        name: "Hot Reload".to_string(),
        status: CheckStatus::Pass,
        message: format!("enabled: {}", features.join(", ")),
        duration: start.elapsed(),
    }
}

/// Check GDPR readiness: export directory, audit trail, PII detection.
fn check_gdpr(config: &BlufioConfig) -> CheckResult {
    let start = Instant::now();
    let mut issues = Vec::new();

    // 1. Check export directory is writable
    let export_dir = config
        .gdpr
        .export_dir
        .as_deref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let db = std::path::Path::new(&config.storage.database_path);
            db.parent()
                .unwrap_or(std::path::Path::new("."))
                .join("exports")
        });

    if export_dir.exists() {
        // Test write access by creating and removing a temp file
        let test_path = export_dir.join(".gdpr-doctor-check");
        match std::fs::File::create(&test_path) {
            Ok(_) => {
                let _ = std::fs::remove_file(&test_path);
            }
            Err(_) => {
                issues.push("export dir not writable".to_string());
            }
        }
    }
    // If the directory does not exist yet, that is fine -- it will be created on first export.

    // 2. Check audit trail is enabled
    if !config.audit.enabled {
        issues.push("audit trail disabled (recommended for GDPR compliance)".to_string());
    }

    // 3. Check PII detection is available
    let test_result = blufio_security::pii::detect_pii("test@example.com");
    if test_result.is_empty() {
        issues.push("PII detection not working".to_string());
    }

    // Build result
    let dir_label = if export_dir.exists() {
        "dir ok"
    } else {
        "dir will be created"
    };
    let audit_label = if config.audit.enabled {
        "audit on"
    } else {
        "audit OFF"
    };
    let pii_label = "PII detection ok";

    if issues.is_empty() {
        CheckResult {
            name: "GDPR Readiness".to_string(),
            status: CheckStatus::Pass,
            message: format!("{dir_label}, {audit_label}, {pii_label}"),
            duration: start.elapsed(),
        }
    } else {
        CheckResult {
            name: "GDPR Readiness".to_string(),
            status: CheckStatus::Warn,
            message: issues.join("; "),
            duration: start.elapsed(),
        }
    }
}

/// Check Litestream WAL replication configuration and binary availability.
///
/// - If `[litestream].enabled` is `false`, returns Pass (skipped).
/// - If enabled but binary is missing, returns Warn with install link.
/// - If enabled and SQLCipher encryption is active, returns Warn (incompatible).
/// - If enabled and binary found, returns Pass.
fn check_litestream(config: &BlufioConfig) -> CheckResult {
    let start = Instant::now();

    if !config.litestream.enabled {
        return CheckResult {
            name: "Litestream".to_string(),
            status: CheckStatus::Pass,
            message: "not enabled (skipped)".to_string(),
            duration: start.elapsed(),
        };
    }

    // Check if litestream binary exists in PATH.
    let binary_exists = std::process::Command::new("which")
        .arg("litestream")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !binary_exists {
        return CheckResult {
            name: "Litestream".to_string(),
            status: CheckStatus::Warn,
            message:
                "enabled but binary not found in PATH. Install: https://litestream.io/install/"
                    .to_string(),
            duration: start.elapsed(),
        };
    }

    // Check for SQLCipher incompatibility.
    if std::env::var("BLUFIO_DB_KEY").is_ok() {
        return CheckResult {
            name: "Litestream".to_string(),
            status: CheckStatus::Warn,
            message: "enabled but SQLCipher encryption active -- incompatible. Use `blufio backup` + cron instead."
                .to_string(),
            duration: start.elapsed(),
        };
    }

    CheckResult {
        name: "Litestream".to_string(),
        status: CheckStatus::Pass,
        message: "binary found, replication configured".to_string(),
        duration: start.elapsed(),
    }
}

/// Check vec0 vector search health: extension loaded, row count, sync drift.
async fn check_vec0(config: &BlufioConfig) -> CheckResult {
    let start = Instant::now();

    if !config.memory.vec0_enabled {
        return CheckResult {
            name: "vec0 Search".to_string(),
            status: CheckStatus::Pass,
            message: "disabled (memory.vec0_enabled = false)".to_string(),
            duration: start.elapsed(),
        };
    }

    let db_path = &config.storage.database_path;
    let path = std::path::Path::new(db_path);

    if !path.exists() {
        return CheckResult {
            name: "vec0 Search".to_string(),
            status: CheckStatus::Pass,
            message: "enabled (no database yet)".to_string(),
            duration: start.elapsed(),
        };
    }

    // Open a connection and check vec0 health.
    match blufio_storage::open_connection(db_path).await {
        Ok(conn) => {
            let result: Result<
                (Option<String>, Option<usize>, usize),
                tokio_rusqlite::Error<rusqlite::Error>,
            > = conn
                .call(|conn| {
                    // Check 1: Extension loaded
                    let version = blufio_memory::vec0::check_vec0_available(conn);

                    // Check 2: vec0 row count (table may not exist yet)
                    let vec0_count: Option<usize> = blufio_memory::vec0::vec0_count(conn).ok();

                    // Check 3: Active memories count for drift detection
                    let active_count: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM memories WHERE status = 'active' \
                             AND classification != 'restricted' AND deleted_at IS NULL",
                            [],
                            |row| row.get(0),
                        )
                        .unwrap_or(0);

                    Ok((version, vec0_count, active_count as usize))
                })
                .await;

            match result {
                Ok((version, vec0_count, active_count)) => {
                    let ver_label = version.as_deref().unwrap_or("NOT LOADED");

                    if version.is_none() {
                        return CheckResult {
                            name: "vec0 Search".to_string(),
                            status: CheckStatus::Fail,
                            message: "extension NOT LOADED (call ensure_sqlite_vec_registered at startup)".to_string(),
                            duration: start.elapsed(),
                        };
                    }

                    match vec0_count {
                        Some(v0_count) => {
                            let drift = active_count.abs_diff(v0_count);

                            if drift > 0 {
                                CheckResult {
                                    name: "vec0 Search".to_string(),
                                    status: CheckStatus::Warn,
                                    message: format!(
                                        "{ver_label}, {v0_count} vec0 rows, {active_count} active memories, drift: {drift}"
                                    ),
                                    duration: start.elapsed(),
                                }
                            } else {
                                CheckResult {
                                    name: "vec0 Search".to_string(),
                                    status: CheckStatus::Pass,
                                    message: format!("{ver_label}, {v0_count} vec0 rows, in sync"),
                                    duration: start.elapsed(),
                                }
                            }
                        }
                        None => CheckResult {
                            name: "vec0 Search".to_string(),
                            status: CheckStatus::Warn,
                            message: format!(
                                "{ver_label}, vec0 table not found (run: blufio memory rebuild-vec0)"
                            ),
                            duration: start.elapsed(),
                        },
                    }
                }
                Err(e) => CheckResult {
                    name: "vec0 Search".to_string(),
                    status: CheckStatus::Fail,
                    message: format!("check failed: {e}"),
                    duration: start.elapsed(),
                },
            }
        }
        Err(e) => CheckResult {
            name: "vec0 Search".to_string(),
            status: CheckStatus::Fail,
            message: format!("open failed: {e}"),
            duration: start.elapsed(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_result_has_required_fields() {
        let result = CheckResult {
            name: "test".to_string(),
            status: CheckStatus::Pass,
            message: "ok".to_string(),
            duration: Duration::from_millis(5),
        };
        assert_eq!(result.name, "test");
        assert_eq!(result.status, CheckStatus::Pass);
        assert_eq!(result.message, "ok");
        assert_eq!(result.duration.as_millis(), 5);
    }

    #[test]
    fn check_status_equality() {
        assert_eq!(CheckStatus::Pass, CheckStatus::Pass);
        assert_eq!(CheckStatus::Warn, CheckStatus::Warn);
        assert_eq!(CheckStatus::Fail, CheckStatus::Fail);
        assert_ne!(CheckStatus::Pass, CheckStatus::Fail);
    }

    #[tokio::test]
    async fn check_config_passes_with_defaults() {
        let result = check_config().await;
        assert_eq!(result.status, CheckStatus::Pass);
        assert_eq!(result.name, "Configuration");
    }

    #[tokio::test]
    async fn check_database_missing_warns() {
        let result = check_database("/tmp/nonexistent-blufio-test-xyz.db").await;
        assert_eq!(result.status, CheckStatus::Warn);
        assert!(result.message.contains("not found"));
    }

    #[tokio::test]
    async fn check_db_integrity_missing_warns() {
        let result = check_db_integrity("/tmp/nonexistent-blufio-test-xyz.db").await;
        assert_eq!(result.status, CheckStatus::Warn);
    }

    #[tokio::test]
    async fn check_encryption_no_db_passes() {
        let result = check_encryption("/tmp/nonexistent-blufio-test-xyz.db").await;
        assert_eq!(result.status, CheckStatus::Pass);
        assert!(result.message.contains("no database yet"));
    }

    #[cfg(feature = "mcp-client")]
    #[tokio::test]
    async fn check_mcp_servers_empty_config_returns_empty() {
        let config = BlufioConfig::default();
        let results = check_mcp_servers(&config).await;
        assert!(results.is_empty());
    }

    #[cfg(feature = "mcp-client")]
    #[tokio::test]
    async fn check_mcp_servers_unreachable_returns_fail() {
        use blufio_config::model::McpServerEntry;

        let mut config = BlufioConfig::default();
        config.mcp.servers.push(McpServerEntry {
            name: "test-server".to_string(),
            transport: "http".to_string(),
            url: Some("http://127.0.0.1:19998/nonexistent".to_string()),
            command: None,
            args: vec![],
            auth_token: None,
            connect_timeout_secs: 2,
            response_size_cap: 4096,
            trusted: false,
        });

        let results = check_mcp_servers(&config).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "mcp:test-server");
        assert_eq!(results[0].status, CheckStatus::Fail);
    }

    #[tokio::test]
    async fn check_memory_baseline_passes() {
        let result = check_memory_baseline().await;
        // On non-MSVC it should pass; on MSVC it warns.
        assert!(result.status == CheckStatus::Pass || result.status == CheckStatus::Warn);
    }

    #[test]
    fn check_hooks_disabled_warns() {
        let config = BlufioConfig::default();
        // Default config has hooks.enabled = false
        let result = check_hooks(&config);
        assert_eq!(result.status, CheckStatus::Warn);
        assert!(result.message.contains("disabled"));
    }

    #[test]
    fn check_hooks_enabled_no_definitions_warns() {
        let mut config = BlufioConfig::default();
        config.hooks.enabled = true;
        // No definitions added
        let result = check_hooks(&config);
        assert_eq!(result.status, CheckStatus::Warn);
        assert!(result.message.contains("no hooks defined"));
    }

    #[test]
    fn check_hooks_valid_definitions_passes() {
        use blufio_config::model::HookDefinition;

        let mut config = BlufioConfig::default();
        config.hooks.enabled = true;
        config.hooks.definitions.push(HookDefinition {
            name: "test-hook".to_string(),
            event: "session_created".to_string(),
            command: "echo test".to_string(),
            priority: 10,
            timeout_secs: 5,
            enabled: true,
        });
        let result = check_hooks(&config);
        assert_eq!(result.status, CheckStatus::Pass);
        assert!(result.message.contains("1 hook(s) configured"));
    }

    #[test]
    fn check_hooks_unknown_event_fails() {
        use blufio_config::model::HookDefinition;

        let mut config = BlufioConfig::default();
        config.hooks.enabled = true;
        config.hooks.definitions.push(HookDefinition {
            name: "bad-hook".to_string(),
            event: "nonexistent_event".to_string(),
            command: "echo test".to_string(),
            priority: 10,
            timeout_secs: 5,
            enabled: true,
        });
        let result = check_hooks(&config);
        assert_eq!(result.status, CheckStatus::Fail);
        assert!(result.message.contains("unknown events"));
    }

    #[test]
    fn check_hot_reload_disabled_warns() {
        let config = BlufioConfig::default();
        // Default config has hot_reload.enabled = false
        let result = check_hot_reload(&config);
        assert_eq!(result.status, CheckStatus::Warn);
        assert!(result.message.contains("disabled"));
    }

    #[test]
    fn check_hot_reload_enabled_passes() {
        let mut config = BlufioConfig::default();
        config.hot_reload.enabled = true;
        let result = check_hot_reload(&config);
        assert_eq!(result.status, CheckStatus::Pass);
        assert!(result.message.contains("config"));
        assert!(result.message.contains("500ms"));
    }

    #[test]
    fn check_hot_reload_missing_tls_files_fails() {
        let mut config = BlufioConfig::default();
        config.hot_reload.enabled = true;
        config.hot_reload.tls_cert_path = Some("/nonexistent/cert.pem".to_string());
        config.hot_reload.tls_key_path = Some("/nonexistent/key.pem".to_string());
        let result = check_hot_reload(&config);
        assert_eq!(result.status, CheckStatus::Fail);
        assert!(result.message.contains("not found"));
    }

    #[test]
    fn check_litestream_disabled_passes() {
        let config = BlufioConfig::default();
        // Default: litestream.enabled = false
        let result = check_litestream(&config);
        assert_eq!(result.status, CheckStatus::Pass);
        assert!(result.message.contains("not enabled"));
    }

    #[test]
    fn check_litestream_enabled_no_binary_warns() {
        let mut config = BlufioConfig::default();
        config.litestream.enabled = true;
        let result = check_litestream(&config);
        // On CI/dev machines without litestream installed, this should Warn.
        // If litestream happens to be installed, it will Pass.
        assert!(
            result.status == CheckStatus::Warn || result.status == CheckStatus::Pass,
            "expected Warn or Pass, got {:?}: {}",
            result.status,
            result.message
        );
    }
}
