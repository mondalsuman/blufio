// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTP client for the Anthropic Messages API.
//!
//! Provides [`AnthropicClient`] which handles request construction,
//! authentication, streaming SSE responses, and transient error retry.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use blufio_config::model::SecurityConfig;
use blufio_core::{BlufioError, ErrorContext, ProviderErrorKind};
use blufio_security::SsrfSafeResolver;
use futures::Stream;
use reqwest::header::{HeaderMap, HeaderValue};
use tracing::{debug, warn};

use crate::sse::{self, StreamEvent};
use crate::types::{ApiErrorResponse, MessageRequest, MessageResponse};

/// Provider name used in error context.
const PROVIDER_NAME: &str = "anthropic";

/// Base URL for the Anthropic Messages API.
const API_BASE_URL: &str = "https://api.anthropic.com/v1/messages";

/// HTTP client for Anthropic API communication.
///
/// Manages authentication headers, connection pooling, and retry logic
/// for transient errors (429, 500, 503, 529).
#[derive(Debug, Clone)]
pub struct AnthropicClient {
    client: reqwest::Client,
    default_model: String,
    max_retries: u32,
    base_url: String,
}

impl AnthropicClient {
    /// Creates a new Anthropic API client.
    ///
    /// # Arguments
    /// * `api_key` - Anthropic API key for authentication
    /// * `api_version` - API version string (e.g., "2023-06-01")
    /// * `model` - Default model identifier
    /// * `security_config` - Optional security config for TLS 1.2+ enforcement and SSRF protection.
    ///   When `Some`, enables `min_tls_version(TLS_1_2)` and `SsrfSafeResolver`.
    ///   When `None` (tests), uses a plain reqwest client.
    pub fn new(
        api_key: String,
        api_version: String,
        model: String,
        security_config: Option<&SecurityConfig>,
    ) -> Result<Self, BlufioError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&api_key)
                .map_err(|e| BlufioError::Config(format!("invalid API key header value: {e}")))?,
        );
        headers.insert(
            "anthropic-version",
            HeaderValue::from_str(&api_version).map_err(|e| {
                BlufioError::Config(format!("invalid API version header value: {e}"))
            })?,
        );
        headers.insert("content-type", HeaderValue::from_static("application/json"));

        let mut builder = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(300));

        // Apply security hardening when config is provided:
        // - TLS 1.2+ minimum for all connections (SEC-09)
        // - SSRF-safe DNS resolver that blocks private IP ranges (SEC-09)
        if let Some(sec) = security_config {
            let resolver = SsrfSafeResolver::new(sec.allowed_private_ips.clone());
            builder = builder
                .min_tls_version(reqwest::tls::Version::TLS_1_2)
                .dns_resolver(Arc::new(resolver));
        }

        let client =
            builder
                .build()
                .map_err(|e| BlufioError::provider_server_error(PROVIDER_NAME, e))?;

        Ok(Self {
            client,
            default_model: model,
            max_retries: 1,
            base_url: API_BASE_URL.to_string(),
        })
    }

    /// Returns the default model identifier.
    pub fn default_model(&self) -> &str {
        &self.default_model
    }

    /// Overrides the base URL (for testing with wiremock).
    #[cfg(test)]
    pub fn with_base_url(mut self, url: String) -> Self {
        self.base_url = url;
        self
    }

    /// Extracts the `retry-after` header value as a [`Duration`].
    fn extract_retry_after(response: &reqwest::Response) -> Option<Duration> {
        response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_secs)
    }

    /// Sends a streaming request and returns a stream of SSE events.
    ///
    /// On retryable errors (determined by `error.is_retryable()`), retries once
    /// after a 1-second delay.
    pub async fn stream_message(
        &self,
        request: &MessageRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent, BlufioError>> + Send>>, BlufioError>
    {
        let mut req = request.clone();
        req.stream = true;

        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                warn!(attempt, "retrying streaming request after transient error");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }

            let response = self
                .client
                .post(&self.base_url)
                .json(&req)
                .send()
                .await
                .map_err(|e| {
                    if e.is_timeout() {
                        BlufioError::provider_timeout(PROVIDER_NAME)
                    } else {
                        BlufioError::Provider {
                            kind: ProviderErrorKind::ServerError,
                            context: ErrorContext {
                                provider_name: Some(PROVIDER_NAME.into()),
                                ..Default::default()
                            },
                            source: Some(Box::new(e)),
                        }
                    }
                })?;

            let status = response.status();
            debug!(status = %status, attempt, "streaming response received");

            if status.is_success() {
                return Ok(sse::parse_sse_stream(response));
            }

            let retry_after = Self::extract_retry_after(&response);
            let error = BlufioError::provider_from_http(
                status.as_u16(),
                PROVIDER_NAME,
                None,
            );
            // Attach retry_after to context if present.
            let error = if retry_after.is_some() {
                match error {
                    BlufioError::Provider {
                        kind,
                        mut context,
                        source,
                    } => {
                        context.retry_after = retry_after;
                        BlufioError::Provider {
                            kind,
                            context,
                            source,
                        }
                    }
                    other => other,
                }
            } else {
                error
            };

            if error.is_retryable() && attempt < self.max_retries {
                let body = response.text().await.unwrap_or_default();
                warn!(status = %status, body = %body, "transient error, will retry");
                last_error = Some(error);
                continue;
            }

            // Non-retryable error or exhausted retries -- read body for diagnostics.
            let body = response.text().await.unwrap_or_default();
            let _api_detail = serde_json::from_str::<ApiErrorResponse>(&body).ok();
            return Err(error);
        }

        Err(last_error.unwrap_or_else(|| {
            BlufioError::Provider {
                kind: ProviderErrorKind::ServerError,
                context: ErrorContext {
                    provider_name: Some(PROVIDER_NAME.into()),
                    ..Default::default()
                },
                source: None,
            }
        }))
    }

    /// Sends a non-streaming request and returns the full response.
    ///
    /// On retryable errors (determined by `error.is_retryable()`), retries once
    /// after a 1-second delay.
    pub async fn complete_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageResponse, BlufioError> {
        let mut req = request.clone();
        req.stream = false;

        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                warn!(attempt, "retrying completion request after transient error");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }

            let response = self
                .client
                .post(&self.base_url)
                .json(&req)
                .send()
                .await
                .map_err(|e| {
                    if e.is_timeout() {
                        BlufioError::provider_timeout(PROVIDER_NAME)
                    } else {
                        BlufioError::Provider {
                            kind: ProviderErrorKind::ServerError,
                            context: ErrorContext {
                                provider_name: Some(PROVIDER_NAME.into()),
                                ..Default::default()
                            },
                            source: Some(Box::new(e)),
                        }
                    }
                })?;

            let status = response.status();
            debug!(status = %status, attempt, "completion response received");

            if status.is_success() {
                let body = response.text().await.map_err(|e| {
                    BlufioError::Provider {
                        kind: ProviderErrorKind::ServerError,
                        context: ErrorContext {
                            provider_name: Some(PROVIDER_NAME.into()),
                            ..Default::default()
                        },
                        source: Some(Box::new(e)),
                    }
                })?;
                let msg_response: MessageResponse =
                    serde_json::from_str(&body).map_err(|e| {
                        BlufioError::Provider {
                            kind: ProviderErrorKind::ServerError,
                            context: ErrorContext {
                                provider_name: Some(PROVIDER_NAME.into()),
                                ..Default::default()
                            },
                            source: Some(Box::new(e)),
                        }
                    })?;
                return Ok(msg_response);
            }

            let retry_after = Self::extract_retry_after(&response);
            let error = BlufioError::provider_from_http(
                status.as_u16(),
                PROVIDER_NAME,
                None,
            );
            let error = if retry_after.is_some() {
                match error {
                    BlufioError::Provider {
                        kind,
                        mut context,
                        source,
                    } => {
                        context.retry_after = retry_after;
                        BlufioError::Provider {
                            kind,
                            context,
                            source,
                        }
                    }
                    other => other,
                }
            } else {
                error
            };

            if error.is_retryable() && attempt < self.max_retries {
                let body = response.text().await.unwrap_or_default();
                warn!(status = %status, body = %body, "transient error, will retry");
                last_error = Some(error);
                continue;
            }

            // Non-retryable error or exhausted retries.
            let body = response.text().await.unwrap_or_default();
            let _api_detail = serde_json::from_str::<ApiErrorResponse>(&body).ok();
            return Err(error);
        }

        Err(last_error.unwrap_or_else(|| {
            BlufioError::Provider {
                kind: ProviderErrorKind::ServerError,
                context: ErrorContext {
                    provider_name: Some(PROVIDER_NAME.into()),
                    ..Default::default()
                },
                source: None,
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_client(base_url: &str) -> AnthropicClient {
        AnthropicClient::new(
            "test-api-key".into(),
            "2023-06-01".into(),
            "claude-sonnet-4-20250514".into(),
            None,
        )
        .unwrap()
        .with_base_url(base_url.to_string())
    }

    fn test_request() -> MessageRequest {
        MessageRequest {
            model: "claude-sonnet-4-20250514".into(),
            messages: vec![crate::types::ApiMessage {
                role: "user".into(),
                content: crate::types::ApiContent::Text("Hello".into()),
            }],
            system: None,
            max_tokens: 1024,
            stream: false,
            cache_control: None,
            tools: None,
        }
    }

    #[tokio::test]
    async fn complete_message_success() {
        let server = MockServer::start().await;

        let response_body = serde_json::json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "Hi there!"}],
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        });

        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.complete_message(&test_request()).await.unwrap();

        assert_eq!(result.id, "msg_test");
        assert_eq!(result.usage.input_tokens, 10);
        assert_eq!(result.content.len(), 1);
    }

    #[tokio::test]
    async fn complete_message_retries_on_429() {
        let server = MockServer::start().await;

        let error_body = serde_json::json!({
            "error": {"type": "rate_limit_error", "message": "Rate limited"}
        });
        let success_body = serde_json::json!({
            "id": "msg_retry",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "After retry"}],
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 5, "output_tokens": 3}
        });

        // First request returns 429, second returns 200.
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(429).set_body_json(&error_body))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&success_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.complete_message(&test_request()).await.unwrap();
        assert_eq!(result.id, "msg_retry");
    }

    #[tokio::test]
    async fn complete_message_fails_on_400() {
        let server = MockServer::start().await;

        let error_body = serde_json::json!({
            "error": {"type": "invalid_request_error", "message": "Bad model"}
        });

        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(400).set_body_json(&error_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.complete_message(&test_request()).await;
        assert!(result.is_err());
        // 400 maps to ServerError via provider_from_http
        let err = result.unwrap_err();
        assert!(!err.is_retryable());
    }

    #[tokio::test]
    async fn complete_message_exhausts_retries_on_503() {
        let server = MockServer::start().await;

        let error_body = serde_json::json!({
            "error": {"type": "overloaded_error", "message": "Service overloaded"}
        });

        // Both attempts return 503.
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(503).set_body_json(&error_body))
            .expect(2)
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.complete_message(&test_request()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.is_retryable()); // 503 is retryable, just exhausted attempts
    }

    #[tokio::test]
    async fn client_sends_correct_headers() {
        let server = MockServer::start().await;

        let response_body = serde_json::json!({
            "id": "msg_headers",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "ok"}],
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 1, "output_tokens": 1}
        });

        Mock::given(method("POST"))
            .and(path("/"))
            .and(header("x-api-key", "test-api-key"))
            .and(header("anthropic-version", "2023-06-01"))
            .and(header("content-type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.complete_message(&test_request()).await;
        assert!(result.is_ok(), "headers should match: {result:?}");
    }

    #[tokio::test]
    async fn anthropic_529_maps_to_rate_limited() {
        let server = MockServer::start().await;

        let success_body = serde_json::json!({
            "id": "msg_529",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "After 529 retry"}],
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 5, "output_tokens": 3}
        });

        // First request returns 529 (Anthropic overloaded), which maps to RateLimited.
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(529).set_body_string("overloaded"))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&success_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.complete_message(&test_request()).await.unwrap();
        assert_eq!(result.id, "msg_529");
    }
}
