// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio healthcheck` command for Docker HEALTHCHECK.
//!
//! Connects to the gateway `/health` endpoint and exits 0 (healthy)
//! or 1 (unhealthy). Designed for use in Dockerfile HEALTHCHECK
//! directives where no shell or curl is available (distroless images).

use std::time::Duration;

use blufio_config::model::BlufioConfig;
use blufio_core::BlufioError;

/// Run the healthcheck: GET /health and exit 0 or 1.
///
/// Uses the gateway host and daemon health_port from configuration.
/// Timeout is 5 seconds to match Docker HEALTHCHECK --timeout.
pub async fn run_healthcheck(config: &BlufioConfig) -> Result<(), BlufioError> {
    let host = &config.gateway.host;
    let port = config.daemon.health_port;
    let url = format!("http://{host}:{port}/health");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| BlufioError::Internal(format!("http client error: {e}")))?;

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| BlufioError::Internal(format!("health check failed: {e}")))?;

    if resp.status().is_success() {
        Ok(())
    } else {
        Err(BlufioError::Internal(format!(
            "unhealthy: status {}",
            resp.status()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthcheck_module_compiles() {
        // Smoke test — actual health endpoint tested via integration tests.
        // run_healthcheck requires a running gateway; unit test confirms compilation.
        let _ = run_healthcheck;
    }
}
