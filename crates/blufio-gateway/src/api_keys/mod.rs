// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Scoped API key management for the gateway.
//!
//! Provides API key creation, scope-based authorization, and per-key rate limiting.
//! Keys use the `blf_sk_` prefix for identifiability and are stored as SHA-256 hashes.

pub mod handlers;
pub mod store;

use serde::{Deserialize, Serialize};

/// A stored API key record (never includes the raw key or hash in API responses).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    /// Unique key identifier.
    pub id: String,
    /// Human-readable name for the key.
    pub name: String,
    /// SHA-256 hash of the raw key (internal use only, not serialized to API responses).
    #[serde(skip_serializing)]
    pub key_hash: String,
    /// Allowed scopes (e.g., "chat.completions", "tools.invoke", "admin").
    pub scopes: Vec<String>,
    /// Maximum requests per minute for this key.
    pub rate_limit: i64,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 expiration timestamp (None = never expires).
    pub expires_at: Option<String>,
    /// ISO 8601 revocation timestamp (None = not revoked).
    pub revoked_at: Option<String>,
}

impl ApiKey {
    /// Returns true if the key is currently valid (not revoked and not expired).
    pub fn is_valid(&self) -> bool {
        // Check revocation first (instant rejection).
        if self.revoked_at.is_some() {
            return false;
        }
        // Check expiration.
        if let Some(ref expires) = self.expires_at {
            let now = chrono::Utc::now().to_rfc3339();
            if expires.as_str() <= now.as_str() {
                return false;
            }
        }
        true
    }
}

/// Authentication context attached to requests by the auth middleware.
#[derive(Debug, Clone)]
pub enum AuthContext {
    /// Master bearer token -- full access, no rate limiting.
    Master,
    /// Scoped API key with restricted permissions.
    Scoped {
        /// Key identifier.
        key_id: String,
        /// Allowed scopes.
        scopes: Vec<String>,
        /// Rate limit (requests per minute).
        rate_limit: i64,
    },
}

impl AuthContext {
    /// Create a master auth context (full access).
    pub fn master() -> Self {
        Self::Master
    }

    /// Create a scoped auth context from an API key.
    pub fn scoped(key: &ApiKey) -> Self {
        Self::Scoped {
            key_id: key.id.clone(),
            scopes: key.scopes.clone(),
            rate_limit: key.rate_limit,
        }
    }

    /// Check if this auth context has the required scope.
    ///
    /// Master always returns true. Scoped checks for exact match, "admin" (grants all),
    /// or wildcard "*".
    pub fn has_scope(&self, required: &str) -> bool {
        match self {
            AuthContext::Master => true,
            AuthContext::Scoped { scopes, .. } => scopes
                .iter()
                .any(|s| s == "admin" || s == "*" || s == required),
        }
    }

    /// Returns the key ID if this is a scoped context, None for master.
    pub fn key_id(&self) -> Option<&str> {
        match self {
            AuthContext::Master => None,
            AuthContext::Scoped { key_id, .. } => Some(key_id),
        }
    }

    /// Returns the rate limit for scoped keys, None for master.
    pub fn rate_limit(&self) -> Option<i64> {
        match self {
            AuthContext::Master => None,
            AuthContext::Scoped { rate_limit, .. } => Some(*rate_limit),
        }
    }
}

/// Request body for creating a new API key.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateKeyRequest {
    /// Human-readable name for the key.
    pub name: String,
    /// Scopes to grant (e.g., ["chat.completions", "tools.invoke"]).
    pub scopes: Vec<String>,
    /// Rate limit in requests per minute (default: 60).
    pub rate_limit: Option<i64>,
    /// Hours until key expires (None = never expires).
    pub expires_in_hours: Option<i64>,
}

/// Response body after creating a new API key.
///
/// The `key` field contains the raw API key and is only shown once at creation time.
#[derive(Debug, Clone, Serialize)]
pub struct CreateKeyResponse {
    /// Unique key identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Raw API key (shown only once -- store securely).
    pub key: String,
    /// Granted scopes.
    pub scopes: Vec<String>,
    /// Rate limit in requests per minute.
    pub rate_limit: i64,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 expiration timestamp (None = never expires).
    pub expires_at: Option<String>,
}

/// Require a specific scope from the auth context, returning 403 if not authorized.
pub fn require_scope(ctx: &AuthContext, scope: &str) -> Result<(), axum::http::StatusCode> {
    if ctx.has_scope(scope) {
        Ok(())
    } else {
        Err(axum::http::StatusCode::FORBIDDEN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn master_has_all_scopes() {
        let ctx = AuthContext::Master;
        assert!(ctx.has_scope("chat.completions"));
        assert!(ctx.has_scope("tools.invoke"));
        assert!(ctx.has_scope("admin"));
        assert!(ctx.has_scope("anything"));
    }

    #[test]
    fn scoped_exact_match() {
        let ctx = AuthContext::Scoped {
            key_id: "key-1".into(),
            scopes: vec!["chat.completions".into()],
            rate_limit: 60,
        };
        assert!(ctx.has_scope("chat.completions"));
        assert!(!ctx.has_scope("tools.invoke"));
        assert!(!ctx.has_scope("admin"));
    }

    #[test]
    fn admin_scope_grants_all() {
        let ctx = AuthContext::Scoped {
            key_id: "key-1".into(),
            scopes: vec!["admin".into()],
            rate_limit: 60,
        };
        assert!(ctx.has_scope("chat.completions"));
        assert!(ctx.has_scope("tools.invoke"));
        assert!(ctx.has_scope("anything"));
    }

    #[test]
    fn wildcard_scope_grants_all() {
        let ctx = AuthContext::Scoped {
            key_id: "key-1".into(),
            scopes: vec!["*".into()],
            rate_limit: 60,
        };
        assert!(ctx.has_scope("chat.completions"));
        assert!(ctx.has_scope("tools.invoke"));
    }

    #[test]
    fn api_key_is_valid_basic() {
        let key = ApiKey {
            id: "k1".into(),
            name: "test".into(),
            key_hash: "hash".into(),
            scopes: vec![],
            rate_limit: 60,
            created_at: "2026-01-01T00:00:00Z".into(),
            expires_at: None,
            revoked_at: None,
        };
        assert!(key.is_valid());
    }

    #[test]
    fn api_key_revoked_is_invalid() {
        let key = ApiKey {
            id: "k1".into(),
            name: "test".into(),
            key_hash: "hash".into(),
            scopes: vec![],
            rate_limit: 60,
            created_at: "2026-01-01T00:00:00Z".into(),
            expires_at: None,
            revoked_at: Some("2026-01-02T00:00:00Z".into()),
        };
        assert!(!key.is_valid());
    }

    #[test]
    fn api_key_expired_is_invalid() {
        let key = ApiKey {
            id: "k1".into(),
            name: "test".into(),
            key_hash: "hash".into(),
            scopes: vec![],
            rate_limit: 60,
            created_at: "2026-01-01T00:00:00Z".into(),
            expires_at: Some("2020-01-01T00:00:00Z".into()),
            revoked_at: None,
        };
        assert!(!key.is_valid());
    }

    #[test]
    fn api_key_future_expiry_is_valid() {
        let key = ApiKey {
            id: "k1".into(),
            name: "test".into(),
            key_hash: "hash".into(),
            scopes: vec![],
            rate_limit: 60,
            created_at: "2026-01-01T00:00:00Z".into(),
            expires_at: Some("2099-01-01T00:00:00Z".into()),
            revoked_at: None,
        };
        assert!(key.is_valid());
    }

    #[test]
    fn auth_context_key_id() {
        let master = AuthContext::Master;
        assert!(master.key_id().is_none());

        let scoped = AuthContext::Scoped {
            key_id: "key-123".into(),
            scopes: vec![],
            rate_limit: 60,
        };
        assert_eq!(scoped.key_id(), Some("key-123"));
    }

    #[test]
    fn require_scope_ok_for_master() {
        let ctx = AuthContext::Master;
        assert!(require_scope(&ctx, "admin").is_ok());
    }

    #[test]
    fn require_scope_forbidden_for_missing() {
        let ctx = AuthContext::Scoped {
            key_id: "k1".into(),
            scopes: vec!["chat.completions".into()],
            rate_limit: 60,
        };
        assert!(require_scope(&ctx, "admin").is_err());
    }
}
