// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Authentication middleware for the gateway.
//!
//! Supports three auth methods (checked in order):
//! 1. Master bearer token (`Authorization: Bearer <token>`)
//! 2. Scoped API key (`Authorization: Bearer blf_sk_...`)
//! 3. Ed25519 keypair signature (`X-Signature` + `X-Timestamp` headers)
//!
//! When no auth method is configured, all requests are rejected (fail-closed).

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use ed25519_dalek::VerifyingKey;

use crate::api_keys::{AuthContext, store::ApiKeyStore};

/// Authentication configuration for the gateway.
#[derive(Clone)]
pub struct AuthConfig {
    /// Expected bearer token. If `Some`, bearer auth is enabled.
    pub bearer_token: Option<String>,
    /// Ed25519 public key for keypair signature verification. If `Some`, keypair auth is enabled.
    pub keypair_public_key: Option<VerifyingKey>,
    /// API key store for scoped key lookup. If `Some`, scoped API key auth is enabled.
    pub key_store: Option<Arc<ApiKeyStore>>,
}

impl std::fmt::Debug for AuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthConfig")
            .field(
                "bearer_token",
                &self.bearer_token.as_ref().map(|_| "[redacted]"),
            )
            .field("keypair_public_key", &self.keypair_public_key.is_some())
            .field("key_store", &self.key_store.is_some())
            .finish()
    }
}

/// Middleware that validates authentication via bearer token, scoped API key,
/// or keypair signature.
///
/// Auth methods are checked in priority order:
/// 1. Master bearer token (fast path -- string comparison)
/// 2. Scoped API key (`blf_sk_` prefix -- SHA-256 hash lookup)
/// 3. Keypair signature (slow path -- Ed25519 verification with replay prevention)
///
/// On success, inserts [`AuthContext`] into request extensions for downstream
/// handlers and middleware (e.g., rate limiter, scope enforcement).
///
/// If neither auth method is configured, all requests are rejected (fail-closed).
pub async fn auth_middleware(
    State(auth): State<AuthConfig>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // If no auth method is configured, reject all requests (fail-closed).
    let has_any_auth = auth.bearer_token.is_some()
        || auth.keypair_public_key.is_some()
        || auth.key_store.is_some();
    if !has_any_auth {
        tracing::error!("gateway has no auth configured -- rejecting request");
        return Err(StatusCode::UNAUTHORIZED);
    }

    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    // Priority 1: Check master bearer token (fast path -- string comparison).
    if let Some(ref expected_token) = auth.bearer_token
        && let Some(ref token) = auth_header
        && token == expected_token
    {
        request.extensions_mut().insert(AuthContext::master());
        return Ok(next.run(request).await);
    }

    // Priority 2: Check scoped API key (blf_sk_ prefix -- SHA-256 hash lookup).
    if let Some(ref key_store) = auth.key_store
        && let Some(ref token) = auth_header
        && token.starts_with("blf_sk_")
    {
        let key_hash = crate::api_keys::store::hash_key(token);
        match key_store.lookup(&key_hash).await {
            Ok(Some(key)) => {
                if key.is_valid() {
                    request.extensions_mut().insert(AuthContext::scoped(&key));
                    return Ok(next.run(request).await);
                } else {
                    tracing::debug!(
                        key_id = %key.id,
                        "scoped key rejected: expired or revoked"
                    );
                    return Err(StatusCode::UNAUTHORIZED);
                }
            }
            Ok(None) => {
                tracing::debug!("scoped key rejected: unknown key");
                return Err(StatusCode::UNAUTHORIZED);
            }
            Err(e) => {
                tracing::error!(error = %e, "API key lookup failed");
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }

    // Priority 3: Check keypair signature (slow path -- crypto verification).
    if let Some(ref public_key) = auth.keypair_public_key {
        let signature_header = request
            .headers()
            .get("x-signature")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let timestamp_header = request
            .headers()
            .get("x-timestamp")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        if let (Some(sig_hex), Some(timestamp_str)) = (signature_header, timestamp_header) {
            // Replay prevention: reject timestamps older than 60 seconds.
            if let Ok(request_time) = chrono::DateTime::parse_from_rfc3339(&timestamp_str) {
                let age = chrono::Utc::now().signed_duration_since(request_time);
                if age.num_seconds().abs() <= 60 {
                    // Verify signature over timestamp bytes.
                    if let Ok(sig_bytes) = hex::decode(&sig_hex)
                        && sig_bytes.len() == 64
                    {
                        let sig_array: [u8; 64] =
                            sig_bytes.try_into().expect("length checked above");
                        let signature = ed25519_dalek::Signature::from_bytes(&sig_array);
                        use ed25519_dalek::Verifier;
                        if public_key
                            .verify(timestamp_str.as_bytes(), &signature)
                            .is_ok()
                        {
                            request.extensions_mut().insert(AuthContext::master());
                            return Ok(next.run(request).await);
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
            key_store: None,
        };
        assert!(config.bearer_token.is_none());
        assert!(config.keypair_public_key.is_none());
        assert!(config.key_store.is_none());
    }

    #[test]
    fn auth_config_with_token() {
        let config = AuthConfig {
            bearer_token: Some("secret-token".to_string()),
            keypair_public_key: None,
            key_store: None,
        };
        assert_eq!(config.bearer_token.as_deref(), Some("secret-token"));
    }

    #[test]
    fn auth_config_debug_redacts_token() {
        let config = AuthConfig {
            bearer_token: Some("secret-token".to_string()),
            keypair_public_key: None,
            key_store: None,
        };
        let debug_output = format!("{:?}", config);
        assert!(!debug_output.contains("secret-token"));
        assert!(debug_output.contains("[redacted]"));
    }
}
