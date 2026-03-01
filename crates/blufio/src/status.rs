// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio status` command implementation.
//!
//! Connects to the gateway health endpoint to display agent state,
//! uptime, memory usage, and cost summary. Falls back gracefully
//! when the agent is not running.

use std::io::IsTerminal;
use std::time::Duration;

use blufio_config::model::BlufioConfig;
use blufio_core::BlufioError;
use serde::{Deserialize, Serialize};

/// Health endpoint response from the gateway.
#[derive(Debug, Deserialize)]
struct HealthResponse {
    status: String,
    uptime_secs: u64,
}

/// Structured status output for `--json` mode.
#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub running: bool,
    pub status: String,
    pub uptime_secs: Option<u64>,
    pub uptime_human: Option<String>,
    pub gateway_host: String,
    pub gateway_port: u16,
}

/// Format seconds into a human-readable duration string.
fn format_uptime(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let minutes = (secs % 3600) / 60;

    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// Run the `blufio status` command.
///
/// Connects to the health endpoint on the gateway and displays agent state.
/// If `--json` is passed, outputs structured JSON for scripting.
/// If `--plain` is passed or stdout is not a TTY, disables colors.
pub async fn run_status(
    config: &BlufioConfig,
    json: bool,
    plain: bool,
) -> Result<(), BlufioError> {
    let host = &config.gateway.host;
    let port = config.daemon.health_port;
    let url = format!("http://{host}:{port}/health");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| BlufioError::Internal(format!("failed to create HTTP client: {e}")))?;

    let result = client.get(&url).send().await;

    match result {
        Ok(resp) if resp.status().is_success() => {
            let health: HealthResponse = resp.json().await.map_err(|e| {
                BlufioError::Internal(format!("failed to parse health response: {e}"))
            })?;

            let uptime_human = format_uptime(health.uptime_secs);

            if json {
                let status_resp = StatusResponse {
                    running: true,
                    status: health.status.clone(),
                    uptime_secs: Some(health.uptime_secs),
                    uptime_human: Some(uptime_human),
                    gateway_host: host.clone(),
                    gateway_port: port,
                };
                println!(
                    "{}",
                    serde_json::to_string_pretty(&status_resp)
                        .unwrap_or_else(|_| "{}".to_string())
                );
            } else {
                let use_color = !plain && std::io::stdout().is_terminal();
                print_status_running(&health.status, &uptime_human, use_color);
            }
        }
        _ => {
            if json {
                let status_resp = StatusResponse {
                    running: false,
                    status: "not running".to_string(),
                    uptime_secs: None,
                    uptime_human: None,
                    gateway_host: host.clone(),
                    gateway_port: port,
                };
                println!(
                    "{}",
                    serde_json::to_string_pretty(&status_resp)
                        .unwrap_or_else(|_| "{}".to_string())
                );
            } else {
                let use_color = !plain && std::io::stdout().is_terminal();
                print_status_offline(host, port, use_color);
            }
        }
    }

    Ok(())
}

/// Print running status with optional colors.
fn print_status_running(status: &str, uptime: &str, use_color: bool) {
    println!();
    println!("  blufio status");
    println!("  {}", "-".repeat(35));

    if use_color {
        use colored::Colorize;
        println!(
            "    State:    {} {} (uptime: {})",
            "✓".green(),
            status.green(),
            uptime
        );
    } else {
        println!("    State:    [OK] {status} (uptime: {uptime})");
    }

    println!();
}

/// Print offline status with optional colors.
fn print_status_offline(host: &str, port: u16, use_color: bool) {
    println!();
    println!("  blufio status");
    println!("  {}", "-".repeat(35));

    if use_color {
        use colored::Colorize;
        println!("    State:    {} {}", "✗".red(), "not running".red());
    } else {
        println!("    State:    [FAIL] not running");
    }

    println!("    Endpoint: http://{host}:{port}/health");
    println!();
    println!("  Start with: blufio serve");
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_uptime_minutes() {
        assert_eq!(format_uptime(120), "2m");
    }

    #[test]
    fn format_uptime_hours() {
        assert_eq!(format_uptime(3720), "1h 2m");
    }

    #[test]
    fn format_uptime_days() {
        assert_eq!(format_uptime(90060), "1d 1h 1m");
    }

    #[test]
    fn status_response_serializes() {
        let resp = StatusResponse {
            running: true,
            status: "healthy".to_string(),
            uptime_secs: Some(3600),
            uptime_human: Some("1h 0m".to_string()),
            gateway_host: "127.0.0.1".to_string(),
            gateway_port: 3000,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"running\":true"));
        assert!(json.contains("\"status\":\"healthy\""));
    }

    #[test]
    fn status_response_offline_serializes() {
        let resp = StatusResponse {
            running: false,
            status: "not running".to_string(),
            uptime_secs: None,
            uptime_human: None,
            gateway_host: "127.0.0.1".to_string(),
            gateway_port: 3000,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"running\":false"));
    }
}
