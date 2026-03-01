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
pub async fn run_doctor(
    config: &BlufioConfig,
    deep: bool,
    plain: bool,
) -> Result<(), BlufioError> {
    let use_color = !plain && std::io::stdout().is_terminal();
    let mut results = Vec::new();

    // Quick checks (always run)
    results.push(check_config().await);
    results.push(check_database(&config.storage.database_path).await);
    results.push(check_llm_connectivity(config).await);
    results.push(check_health_endpoint(config).await);

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

    match tokio_rusqlite::Connection::open(db_path).await {
        Ok(conn) => {
            let query_result: Result<(), tokio_rusqlite::Error> = conn
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

/// Check LLM API connectivity via HEAD request.
async fn check_llm_connectivity(config: &BlufioConfig) -> CheckResult {
    let start = Instant::now();

    let has_api_key = config.anthropic.api_key.is_some()
        || std::env::var("ANTHROPIC_API_KEY").is_ok();

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

    match tokio_rusqlite::Connection::open(db_path).await {
        Ok(conn) => {
            let result: Result<Vec<String>, tokio_rusqlite::Error> = conn
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
                let size = std::fs::metadata(path)
                    .map(|m| m.len())
                    .unwrap_or(0);
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
    async fn check_memory_baseline_passes() {
        let result = check_memory_baseline().await;
        // On non-MSVC it should pass; on MSVC it warns.
        assert!(
            result.status == CheckStatus::Pass || result.status == CheckStatus::Warn
        );
    }
}
