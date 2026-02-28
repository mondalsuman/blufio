// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! TLS enforcement for outbound HTTP connections.
//!
//! Provides a secure reqwest client builder with TLS 1.2+ minimum and
//! URL validation that blocks non-TLS connections to remote hosts.

use std::sync::Arc;

use blufio_config::model::SecurityConfig;
use blufio_core::BlufioError;
use tracing::error;

use crate::ssrf::SsrfSafeResolver;

/// Build a reqwest::Client with security defaults.
///
/// - Minimum TLS 1.2 for all connections.
/// - SSRF-safe DNS resolver that blocks private IP ranges.
/// - Localhost connections are exempt from TLS requirement (validated separately).
pub fn build_secure_client(config: &SecurityConfig) -> Result<reqwest::Client, BlufioError> {
    let resolver = SsrfSafeResolver::new(config.allowed_private_ips.clone());

    reqwest::Client::builder()
        .min_tls_version(reqwest::tls::Version::TLS_1_2)
        .dns_resolver(Arc::new(resolver))
        .build()
        .map_err(|e| {
            error!("failed to build secure HTTP client: {e}");
            BlufioError::Security(format!("failed to build secure HTTP client: {e}"))
        })
}

/// Validate a URL for security policy compliance.
///
/// - Localhost URLs (127.0.0.1, ::1, localhost) are allowed with any scheme.
/// - Remote URLs MUST use HTTPS.
///
/// Call this before making requests to enforce TLS policy.
pub fn validate_url(url: &str) -> Result<(), BlufioError> {
    let parsed = url::Url::parse(url).map_err(|e| {
        BlufioError::Security(format!("invalid URL: {e}"))
    })?;

    let host = parsed.host_str().unwrap_or("");

    // Localhost is exempt from TLS.
    if is_localhost(host) {
        return Ok(());
    }

    // Remote connections must use HTTPS.
    if parsed.scheme() != "https" {
        error!(url = %url, "TLS required for remote connections");
        return Err(BlufioError::Security(
            "TLS required for remote connections -- use HTTPS".to_string(),
        ));
    }

    Ok(())
}

/// Check if an address refers to localhost.
pub fn is_localhost(addr: &str) -> bool {
    matches!(
        addr,
        "127.0.0.1" | "::1" | "localhost" | "[::1]"
    ) || addr.starts_with("127.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_secure_client_succeeds() {
        let config = SecurityConfig {
            bind_address: "127.0.0.1".to_string(),
            require_tls: true,
            allowed_private_ips: vec![],
        };
        let client = build_secure_client(&config);
        assert!(client.is_ok());
    }

    #[test]
    fn validate_url_allows_https_remote() {
        assert!(validate_url("https://api.anthropic.com/v1/messages").is_ok());
    }

    #[test]
    fn validate_url_blocks_http_remote() {
        let result = validate_url("http://api.anthropic.com/v1/messages");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("TLS required"));
    }

    #[test]
    fn validate_url_allows_http_localhost() {
        assert!(validate_url("http://127.0.0.1:8080/health").is_ok());
        assert!(validate_url("http://localhost:3000/api").is_ok());
        assert!(validate_url("http://[::1]:8080/test").is_ok());
    }

    #[test]
    fn is_localhost_identifies_loopback() {
        assert!(is_localhost("127.0.0.1"));
        assert!(is_localhost("127.0.0.2"));
        assert!(is_localhost("::1"));
        assert!(is_localhost("[::1]"));
        assert!(is_localhost("localhost"));
    }

    #[test]
    fn is_localhost_rejects_non_loopback() {
        assert!(!is_localhost("10.0.0.1"));
        assert!(!is_localhost("192.168.1.1"));
        assert!(!is_localhost("api.anthropic.com"));
        assert!(!is_localhost("8.8.8.8"));
    }
}
