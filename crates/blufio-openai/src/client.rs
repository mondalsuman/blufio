// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTP client for the OpenAI Chat Completions API.
//!
//! Provides [`OpenAIClient`] which handles request construction,
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

use crate::sse;
use crate::types::{ApiErrorResponse, ChatRequest, ChatResponse, SseChunk};

/// Provider name used in error context.
const PROVIDER_NAME: &str = "openai";

/// HTTP client for OpenAI API communication.
///
/// Manages authentication headers, connection pooling, and retry logic
/// for transient errors (429, 500, 503).
#[derive(Debug, Clone)]
pub struct OpenAIClient {
    client: reqwest::Client,
    default_model: String,
    max_retries: u32,
    base_url: String,
}

impl OpenAIClient {
    /// Creates a new OpenAI API client.
    ///
    /// # Arguments
    /// * `api_key` - OpenAI API key for Bearer authentication
    /// * `model` - Default model identifier
    /// * `base_url` - Base URL for the API (e.g., "https://api.openai.com/v1")
    /// * `security_config` - Optional security config for TLS 1.2+ enforcement and SSRF protection.
    ///   When `Some`, enables `min_tls_version(TLS_1_2)` and `SsrfSafeResolver`.
    ///   When `None` (tests), uses a plain reqwest client.
    pub fn new(
        api_key: String,
        model: String,
        base_url: String,
        security_config: Option<&SecurityConfig>,
    ) -> Result<Self, BlufioError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {api_key}"))
                .map_err(|e| BlufioError::Config(format!("invalid API key header value: {e}")))?,
        );
        headers.insert("content-type", HeaderValue::from_static("application/json"));

        let mut builder = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(300));

        // Apply security hardening when config is provided:
        // - TLS 1.2+ minimum for all connections
        // - SSRF-safe DNS resolver that blocks private IP ranges
        if let Some(sec) = security_config {
            let resolver = SsrfSafeResolver::new(sec.allowed_private_ips.clone());
            builder = builder
                .min_tls_version(reqwest::tls::Version::TLS_1_2)
                .dns_resolver(Arc::new(resolver));
        }

        let client = builder
            .build()
            .map_err(|e| BlufioError::provider_server_error(PROVIDER_NAME, e))?;

        Ok(Self {
            client,
            default_model: model,
            max_retries: 1,
            base_url,
        })
    }

    /// Returns the default model identifier.
    pub fn default_model(&self) -> &str {
        &self.default_model
    }

    /// Returns the base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Overrides the base URL (for testing with wiremock).
    #[cfg(test)]
    pub fn with_base_url(mut self, url: String) -> Self {
        self.base_url = url;
        self
    }

    /// Returns the chat completions endpoint URL.
    fn completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url)
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

    /// Sends a non-streaming request and returns the full response.
    ///
    /// On retryable errors (determined by `error.is_retryable()`), retries once
    /// after a 1-second delay.
    pub async fn complete_chat(&self, request: &ChatRequest) -> Result<ChatResponse, BlufioError> {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                warn!(attempt, "retrying completion request after transient error");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }

            let response = self
                .client
                .post(self.completions_url())
                .json(request)
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
                let body = response.text().await.map_err(|e| BlufioError::Provider {
                    kind: ProviderErrorKind::ServerError,
                    context: ErrorContext {
                        provider_name: Some(PROVIDER_NAME.into()),
                        ..Default::default()
                    },
                    source: Some(Box::new(e)),
                })?;
                let chat_response: ChatResponse =
                    serde_json::from_str(&body).map_err(|e| BlufioError::Provider {
                        kind: ProviderErrorKind::ServerError,
                        context: ErrorContext {
                            provider_name: Some(PROVIDER_NAME.into()),
                            ..Default::default()
                        },
                        source: Some(Box::new(e)),
                    })?;
                return Ok(chat_response);
            }

            let retry_after = Self::extract_retry_after(&response);
            let error = BlufioError::provider_from_http(status.as_u16(), PROVIDER_NAME, None);
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

        Err(last_error.unwrap_or_else(|| BlufioError::Provider {
            kind: ProviderErrorKind::ServerError,
            context: ErrorContext {
                provider_name: Some(PROVIDER_NAME.into()),
                ..Default::default()
            },
            source: None,
        }))
    }

    /// Sends a streaming request and returns a stream of SSE chunks.
    ///
    /// On retryable errors (determined by `error.is_retryable()`), retries once
    /// after a 1-second delay.
    pub async fn stream_chat(
        &self,
        request: &ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<SseChunk, BlufioError>> + Send>>, BlufioError>
    {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                warn!(attempt, "retrying streaming request after transient error");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }

            let response = self
                .client
                .post(self.completions_url())
                .json(request)
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
                return Ok(sse::parse_openai_sse_stream(response));
            }

            let retry_after = Self::extract_retry_after(&response);
            let error = BlufioError::provider_from_http(status.as_u16(), PROVIDER_NAME, None);
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

        Err(last_error.unwrap_or_else(|| BlufioError::Provider {
            kind: ProviderErrorKind::ServerError,
            context: ErrorContext {
                provider_name: Some(PROVIDER_NAME.into()),
                ..Default::default()
            },
            source: None,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_client(base_url: &str) -> OpenAIClient {
        // Strip trailing /chat/completions if present (wiremock uses root path).
        OpenAIClient::new(
            "test-api-key".into(),
            "gpt-4o".into(),
            "https://api.openai.com/v1".into(),
            None,
        )
        .unwrap()
        .with_base_url(base_url.to_string())
    }

    fn test_request() -> ChatRequest {
        ChatRequest {
            model: "gpt-4o".into(),
            messages: vec![crate::types::ChatMessage {
                role: "user".into(),
                content: Some(crate::types::ChatContent::Text("Hello".into())),
                tool_calls: None,
                tool_call_id: None,
            }],
            max_completion_tokens: Some(1024),
            stream: false,
            tools: None,
            response_format: None,
            stream_options: None,
        }
    }

    #[tokio::test]
    async fn complete_chat_success() {
        let server = MockServer::start().await;

        let response_body = serde_json::json!({
            "id": "chatcmpl-test",
            "choices": [{
                "message": {"role": "assistant", "content": "Hi there!"},
                "finish_reason": "stop",
                "index": 0
            }],
            "model": "gpt-4o",
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        });

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.complete_chat(&test_request()).await.unwrap();

        assert_eq!(result.id, "chatcmpl-test");
        assert_eq!(result.usage.as_ref().unwrap().prompt_tokens, 10);
        assert_eq!(result.choices.len(), 1);
    }

    #[tokio::test]
    async fn complete_chat_retries_on_429() {
        let server = MockServer::start().await;

        let error_body = serde_json::json!({
            "error": {"type": "rate_limit_error", "message": "Rate limited"}
        });
        let success_body = serde_json::json!({
            "id": "chatcmpl-retry",
            "choices": [{
                "message": {"role": "assistant", "content": "After retry"},
                "finish_reason": "stop",
                "index": 0
            }],
            "model": "gpt-4o",
            "usage": {"prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8}
        });

        // First request returns 429, second returns 200.
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429).set_body_json(&error_body))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&success_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.complete_chat(&test_request()).await.unwrap();
        assert_eq!(result.id, "chatcmpl-retry");
    }

    #[tokio::test]
    async fn complete_chat_fails_on_400() {
        let server = MockServer::start().await;

        let error_body = serde_json::json!({
            "error": {"type": "invalid_request_error", "message": "Bad model"}
        });

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(400).set_body_json(&error_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.complete_chat(&test_request()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(!err.is_retryable());
    }

    #[tokio::test]
    async fn complete_chat_fails_on_401() {
        let server = MockServer::start().await;

        let error_body = serde_json::json!({
            "error": {"type": "authentication_error", "message": "Invalid API key"}
        });

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_json(&error_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.complete_chat(&test_request()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        // 401 maps to AuthFailed, which is not retryable
        assert!(!err.is_retryable());
    }

    #[tokio::test]
    async fn complete_chat_exhausts_retries_on_503() {
        let server = MockServer::start().await;

        let error_body = serde_json::json!({
            "error": {"type": "server_error", "message": "Service overloaded"}
        });

        // Both attempts return 503.
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(503).set_body_json(&error_body))
            .expect(2)
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.complete_chat(&test_request()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.is_retryable()); // 503 is retryable, just exhausted
    }

    #[tokio::test]
    async fn client_sends_correct_headers() {
        let server = MockServer::start().await;

        let response_body = serde_json::json!({
            "id": "chatcmpl-headers",
            "choices": [{
                "message": {"role": "assistant", "content": "ok"},
                "finish_reason": "stop",
                "index": 0
            }],
            "model": "gpt-4o",
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        });

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("authorization", "Bearer test-api-key"))
            .and(header("content-type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.complete_chat(&test_request()).await;
        assert!(result.is_ok(), "headers should match: {result:?}");
    }

    #[tokio::test]
    async fn client_uses_base_url() {
        let server = MockServer::start().await;

        let response_body = serde_json::json!({
            "id": "chatcmpl-url",
            "choices": [{
                "message": {"role": "assistant", "content": "ok"},
                "finish_reason": "stop",
                "index": 0
            }],
            "model": "gpt-4o",
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        });

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        assert_eq!(
            client.completions_url(),
            format!("{}/chat/completions", server.uri())
        );
        let result = client.complete_chat(&test_request()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn retries_500_then_succeeds() {
        let server = MockServer::start().await;

        let error_body = serde_json::json!({
            "error": {"type": "server_error", "message": "Internal server error"}
        });
        let success_body = serde_json::json!({
            "id": "chatcmpl-500retry",
            "choices": [{
                "message": {"role": "assistant", "content": "recovered"},
                "finish_reason": "stop",
                "index": 0
            }],
            "model": "gpt-4o",
            "usage": {"prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8}
        });

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(500).set_body_json(&error_body))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&success_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.complete_chat(&test_request()).await.unwrap();
        assert_eq!(result.id, "chatcmpl-500retry");
    }
}
