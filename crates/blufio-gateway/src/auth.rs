// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Authentication middleware for the gateway.
//!
//! Supports two auth methods (checked in order):
//! 1. Bearer token (`Authorization: Bearer <token>`)
//! 2. Ed25519 keypair signature (`X-Signature` + `X-Timestamp` headers)
//!
//! When no auth method is configured, all requests are rejected (fail-closed).

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use ed25519_dalek::VerifyingKey;

/// Authentication configuration for the gateway.
#[derive(Clone)]
pub struct AuthConfig {
    /// Expected bearer token. If `Some`, bearer auth is enabled.
    pub bearer_token: Option<String>,
    /// Ed25519 public key for keypair signature verification. If `Some`, keypair auth is enabled.
    pub keypair_public_key: Option<VerifyingKey>,
}

impl std::fmt::Debug for AuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthConfig")
            .field(
                "bearer_token",
                &self.bearer_token.as_ref().map(|_| "[redacted]"),
            )
            .field("keypair_public_key", &self.keypair_public_key.is_some())
            .finish()
    }
}

/// Middleware that validates authentication via bearer token or keypair signature.
///
/// Auth methods are checked in priority order:
/// 1. Bearer token (fast path — string comparison)
/// 2. Keypair signature (slow path — Ed25519 verification with replay prevention)
///
/// If neither auth method is configured, all requests are rejected (fail-closed).
pub async fn auth_middleware(
    State(auth): State<AuthConfig>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // If neither auth method is configured, reject all requests (fail-closed).
    if auth.bearer_token.is_none() && auth.keypair_public_key.is_none() {
        tracing::error!("gateway has no auth configured -- rejecting request");
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Priority 1: Check bearer token (fast path — string comparison).
    if let Some(ref expected_token) = auth.bearer_token {
        let auth_header = request
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));

        if let Some(token) = auth_header {
            if token == expected_token {
                return Ok(next.run(request).await);
            }
        }
    }

    // Priority 2: Check keypair signature (slow path — crypto verification).
    if let Some(ref public_key) = auth.keypair_public_key {
        // Check X-Signature and X-Timestamp headers.
        let signature_header = request
            .headers()
            .get("x-signature")
            .and_then(|v| v.to_str().ok());
        let timestamp_header = request
            .headers()
            .get("x-timestamp")
            .and_then(|v| v.to_str().ok());

        if let (Some(sig_hex), Some(timestamp_str)) = (signature_header, timestamp_header) {
            // Replay prevention: reject timestamps older than 60 seconds.
            if let Ok(request_time) = chrono::DateTime::parse_from_rfc3339(timestamp_str) {
                let age = chrono::Utc::now().signed_duration_since(request_time);
                if age.num_seconds().abs() <= 60 {
                    // Verify signature over timestamp bytes.
                    if let Ok(sig_bytes) = hex::decode(sig_hex) {
                        if sig_bytes.len() == 64 {
                            let sig_array: [u8; 64] = sig_bytes
                                .try_into()
                                .expect("length checked above");
                            let signature =
                                ed25519_dalek::Signature::from_bytes(&sig_array);
                            use ed25519_dalek::Verifier;
                            if public_key
                                .verify(timestamp_str.as_bytes(), &signature)
                                .is_ok()
                            {
                                return Ok(next.run(request).await);
                            }
                        }
                    }
                } else {
                    tracing::debug!(
                        age_secs = age.num_seconds(),
                        "keypair auth rejected: timestamp too old"
                    );
                }
            }
        }
    }

    // Neither auth method succeeded.
    Err(StatusCode::UNAUTHORIZED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_config_with_none_token() {
        let config = AuthConfig {
            bearer_token: None,
            keypair_public_key: None,
        };
        assert!(config.bearer_token.is_none());
        assert!(config.keypair_public_key.is_none());
    }

    #[test]
    fn auth_config_with_token() {
        let config = AuthConfig {
            bearer_token: Some("secret-token".to_string()),
            keypair_public_key: None,
        };
        assert_eq!(config.bearer_token.as_deref(), Some("secret-token"));
    }

    #[test]
    fn auth_config_debug_redacts_token() {
        let config = AuthConfig {
            bearer_token: Some("secret-token".to_string()),
            keypair_public_key: None,
        };
        let debug_output = format!("{:?}", config);
        assert!(!debug_output.contains("secret-token"));
        assert!(debug_output.contains("[redacted]"));
    }
}
