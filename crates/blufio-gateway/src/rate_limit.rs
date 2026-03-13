// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sliding window rate limiter middleware for scoped API keys.
//!
//! Enforces per-key request limits using atomic SQLite counters.
//! Master bearer tokens bypass rate limiting entirely.

use axum::{
    extract::{Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};

use crate::api_keys::AuthContext;
use crate::server::GatewayState;

/// Returns the start of the current minute as an ISO 8601 string.
///
/// Used as the rate limit window key. Truncates to minute boundary.
fn current_minute_start() -> String {
    let now = chrono::Utc::now();
    now.format("%Y-%m-%dT%H:%M:00Z").to_string()
}

/// Returns seconds remaining until the next minute boundary.
fn seconds_until_next_minute() -> u64 {
    let now = chrono::Utc::now();
    let secs = now.second();
    (60 - secs) as u64
}

/// Rate limiting middleware that enforces per-key request counts.
///
/// Must run AFTER `auth_middleware` (which inserts `AuthContext` into extensions).
///
/// Behavior:
/// - `AuthContext::Master`: No rate limiting, no rate limit headers.
/// - `AuthContext::Scoped`: Enforces sliding window counter per key.
/// - Missing `AuthContext`: Passes through (auth middleware handles rejection).
pub async fn rate_limit_middleware(
    State(state): State<GatewayState>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Extract auth context (set by auth middleware).
    let auth_ctx = request.extensions().get::<AuthContext>().cloned();

    let auth_ctx = match auth_ctx {
        Some(ctx) => ctx,
        None => {
            // No auth context = either unauthenticated or auth middleware didn't run.
            // Pass through -- auth middleware handles rejection.
            return Ok(next.run(request).await);
        }
    };

    match auth_ctx {
        AuthContext::Master => {
            // Master token: no rate limiting.
            Ok(next.run(request).await)
        }
        AuthContext::Scoped {
            ref key_id,
            rate_limit,
            ..
        } => {
            let key_store = match state.auth.key_store.as_ref() {
                Some(store) => store,
                None => return Ok(next.run(request).await),
            };

            let window_start = current_minute_start();
            let count = key_store
                .increment_rate_count(key_id, &window_start)
                .await
                .map_err(|e| {
                    tracing::error!(error = %e, "rate limit counter error");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;

            if count > rate_limit {
                let retry_after = seconds_until_next_minute();
                tracing::debug!(
                    key_id = %key_id,
                    count = count,
                    limit = rate_limit,
                    "rate limit exceeded"
                );

                // Re-insert AuthContext since we consumed it.
                request.extensions_mut().insert(auth_ctx);

                let mut response = Response::builder()
                    .status(StatusCode::TOO_MANY_REQUESTS)
                    .body(axum::body::Body::from(
                        serde_json::json!({
                            "error": {
                                "message": "Rate limit exceeded",
                                "type": "rate_limit_error",
                                "code": "rate_limit_exceeded"
                            }
                        })
                        .to_string(),
                    ))
                    .expect("valid response builder");

                let headers = response.headers_mut();
                headers.insert(
                    "Retry-After",
                    HeaderValue::from_str(&retry_after.to_string())
                        .expect("valid header: numeric retry_after"),
                );
                headers.insert(
                    "X-RateLimit-Limit",
                    HeaderValue::from_str(&rate_limit.to_string())
                        .expect("valid header: numeric rate_limit"),
                );
                headers.insert("X-RateLimit-Remaining", HeaderValue::from_static("0"));
                headers.insert(
                    "X-RateLimit-Reset",
                    HeaderValue::from_str(&retry_after.to_string())
                        .expect("valid header: numeric retry_after"),
                );
                headers.insert("Content-Type", HeaderValue::from_static("application/json"));

                return Ok(response);
            }

            // Re-insert AuthContext for downstream handlers.
            request.extensions_mut().insert(auth_ctx);

            let mut response = next.run(request).await;

            // Add rate limit headers to successful responses.
            let remaining = rate_limit - count;
            let retry_after = seconds_until_next_minute();
            let headers = response.headers_mut();
            headers.insert(
                "X-RateLimit-Limit",
                HeaderValue::from_str(&rate_limit.to_string())
                    .expect("valid header: numeric rate_limit"),
            );
            headers.insert(
                "X-RateLimit-Remaining",
                HeaderValue::from_str(&remaining.to_string())
                    .expect("valid header: numeric remaining"),
            );
            headers.insert(
                "X-RateLimit-Reset",
                HeaderValue::from_str(&retry_after.to_string())
                    .expect("valid header: numeric retry_after"),
            );

            Ok(response)
        }
    }
}

use chrono::Timelike;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_minute_start_format() {
        let start = current_minute_start();
        assert!(start.ends_with(":00Z"));
        assert!(start.contains('T'));
    }

    #[test]
    fn seconds_until_next_minute_range() {
        let secs = seconds_until_next_minute();
        assert!(secs > 0 && secs <= 60);
    }
}
