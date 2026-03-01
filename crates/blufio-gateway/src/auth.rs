// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bearer token authentication middleware for the gateway.
//!
//! When a bearer token is configured, all requests must include a valid
//! `Authorization: Bearer <token>` header. When no token is configured,
//! auth is disabled and all requests are allowed.

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};

/// Authentication configuration for the gateway.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Expected bearer token. If `None`, auth is disabled.
    pub bearer_token: Option<String>,
}

/// Middleware that validates bearer token authentication.
///
/// If no bearer_token is configured (None), all requests pass through.
/// Otherwise, the Authorization header must contain `Bearer <expected_token>`.
pub async fn auth_middleware(
    State(auth): State<AuthConfig>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // If no bearer_token configured, allow all requests (auth disabled).
    let Some(ref expected_token) = auth.bearer_token else {
        return Ok(next.run(request).await);
    };

    // Extract Authorization header.
    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match auth_header {
        Some(token) if token == expected_token => Ok(next.run(request).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_config_with_none_token() {
        let config = AuthConfig {
            bearer_token: None,
        };
        assert!(config.bearer_token.is_none());
    }

    #[test]
    fn auth_config_with_token() {
        let config = AuthConfig {
            bearer_token: Some("secret-token".to_string()),
        };
        assert_eq!(config.bearer_token.as_deref(), Some("secret-token"));
    }
}
