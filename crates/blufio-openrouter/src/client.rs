// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTP client for the OpenRouter Chat Completions API.
//!
//! Provides [`OpenRouterClient`] which handles request construction,
//! authentication with Bearer token, X-Title and HTTP-Referer headers,
//! streaming SSE responses, and transient error retry.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use blufio_config::model::SecurityConfig;
use blufio_core::BlufioError;
use blufio_security::SsrfSafeResolver;
use futures::Stream;
use reqwest::header::{HeaderMap, HeaderValue};
use tracing::{debug, warn};

use crate::sse;
use crate::types::{ApiErrorResponse, RouterRequest, RouterResponse, SseChunk};

/// Default base URL for the OpenRouter API.
const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";

/// HTTP client for OpenRouter API communication.
///
/// Manages authentication headers (Authorization, X-Title, HTTP-Referer),
/// connection pooling, and retry logic for transient errors (429, 500, 503).
#[derive(Debug, Clone)]
pub struct OpenRouterClient {
    client: reqwest::Client,
    default_model: String,
    max_retries: u32,
    base_url: String,
}

impl OpenRouterClient {
    /// Creates a new OpenRouter API client.
    ///
    /// # Arguments
    /// * `api_key` - OpenRouter API key for Bearer authentication
    /// * `model` - Default model identifier (e.g., "anthropic/claude-sonnet-4")
    /// * `x_title` - Application title sent via X-Title header
    /// * `http_referer` - Optional HTTP Referer header for analytics
    /// * `security_config` - Optional security config for TLS 1.2+ and SSRF protection.
    ///   When `Some`, enables `min_tls_version(TLS_1_2)` and `SsrfSafeResolver`.
    ///   When `None` (tests), uses a plain reqwest client.
    pub fn new(
        api_key: String,
        model: String,
        x_title: String,
        http_referer: Option<String>,
        security_config: Option<&SecurityConfig>,
    ) -> Result<Self, BlufioError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {api_key}"))
                .map_err(|e| BlufioError::Config(format!("invalid API key header value: {e}")))?,
        );
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        headers.insert(
            "x-title",
            HeaderValue::from_str(&x_title)
                .map_err(|e| BlufioError::Config(format!("invalid X-Title header value: {e}")))?,
        );
        if let Some(ref referer) = http_referer {
            headers.insert(
                "http-referer",
                HeaderValue::from_str(referer).map_err(|e| {
                    BlufioError::Config(format!("invalid HTTP-Referer header value: {e}"))
                })?,
            );
        }

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

        let client = builder.build().map_err(|e| BlufioError::Provider {
            message: format!("failed to build HTTP client: {e}"),
            source: Some(Box::new(e)),
        })?;

        Ok(Self {
            client,
            default_model: model,
            max_retries: 1,
            base_url: DEFAULT_BASE_URL.to_string(),
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

    /// Sends a non-streaming request and returns the full response.
    ///
    /// On transient errors (429, 500, 503), retries once after a 1-second delay.
    pub async fn complete_chat(
        &self,
        request: &RouterRequest,
    ) -> Result<RouterResponse, BlufioError> {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                warn!(attempt, "retrying OpenRouter completion request after transient error");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }

            let response = self
                .client
                .post(self.completions_url())
                .json(request)
                .send()
                .await
                .map_err(|e| BlufioError::Provider {
                    message: format!("HTTP request failed: {e}"),
                    source: Some(Box::new(e)),
                })?;

            let status = response.status();
            debug!(status = %status, attempt, "OpenRouter completion response received");

            if status.is_success() {
                let body = response.text().await.map_err(|e| BlufioError::Provider {
                    message: format!("failed to read response body: {e}"),
                    source: Some(Box::new(e)),
                })?;
                let chat_response: RouterResponse =
                    serde_json::from_str(&body).map_err(|e| BlufioError::Provider {
                        message: format!("failed to parse OpenRouter API response: {e}"),
                        source: Some(Box::new(e)),
                    })?;
                return Ok(chat_response);
            }

            if is_transient_error(status) && attempt < self.max_retries {
                let body = response.text().await.unwrap_or_default();
                warn!(status = %status, body = %body, "transient error, will retry");
                last_error = Some(BlufioError::Provider {
                    message: format!("OpenRouter API returned {status}: {body}"),
                    source: None,
                });
                continue;
            }

            // Non-transient error or exhausted retries.
            let body = response.text().await.unwrap_or_default();
            let error_msg = if let Ok(api_err) = serde_json::from_str::<ApiErrorResponse>(&body) {
                format!(
                    "OpenRouter API error ({}): {}",
                    api_err.error.type_.unwrap_or_default(),
                    api_err.error.message
                )
            } else {
                format!("OpenRouter API returned {status}: {body}")
            };
            return Err(BlufioError::Provider {
                message: error_msg,
                source: None,
            });
        }

        Err(last_error.unwrap_or_else(|| BlufioError::Provider {
            message: "OpenRouter completion request failed after retries".into(),
            source: None,
        }))
    }

    /// Sends a streaming request and returns a stream of SSE chunks.
    ///
    /// On transient errors (429, 500, 503), retries once after a 1-second delay.
    pub async fn stream_chat(
        &self,
        request: &RouterRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<SseChunk, BlufioError>> + Send>>, BlufioError>
    {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                warn!(attempt, "retrying OpenRouter streaming request after transient error");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }

            let response = self
                .client
                .post(self.completions_url())
                .json(request)
                .send()
                .await
                .map_err(|e| BlufioError::Provider {
                    message: format!("HTTP request failed: {e}"),
                    source: Some(Box::new(e)),
                })?;

            let status = response.status();
            debug!(status = %status, attempt, "OpenRouter streaming response received");

            if status.is_success() {
                return Ok(sse::parse_openrouter_sse_stream(response));
            }

            if is_transient_error(status) && attempt < self.max_retries {
                let body = response.text().await.unwrap_or_default();
                warn!(status = %status, body = %body, "transient error, will retry");
                last_error = Some(BlufioError::Provider {
                    message: format!("OpenRouter API returned {status}: {body}"),
                    source: None,
                });
                continue;
            }

            // Non-transient error or exhausted retries.
            let body = response.text().await.unwrap_or_default();
            let error_msg = if let Ok(api_err) = serde_json::from_str::<ApiErrorResponse>(&body) {
                format!(
                    "OpenRouter API error ({}): {}",
                    api_err.error.type_.unwrap_or_default(),
                    api_err.error.message
                )
            } else {
                format!("OpenRouter API returned {status}: {body}")
            };
            return Err(BlufioError::Provider {
                message: error_msg,
                source: None,
            });
        }

        Err(last_error.unwrap_or_else(|| BlufioError::Provider {
            message: "OpenRouter streaming request failed after retries".into(),
            source: None,
        }))
    }
}

/// Returns true for HTTP status codes that indicate transient errors worth retrying.
fn is_transient_error(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 429 | 500 | 503)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_client(base_url: &str) -> OpenRouterClient {
        OpenRouterClient::new(
            "test-api-key".into(),
            "anthropic/claude-sonnet-4".into(),
            "Blufio".into(),
            Some("https://blufio.dev".into()),
            None,
        )
        .unwrap()
        .with_base_url(base_url.to_string())
    }

    fn test_request() -> RouterRequest {
        RouterRequest {
            model: "anthropic/claude-sonnet-4".into(),
            messages: vec![crate::types::ChatMessage {
                role: "user".into(),
                content: Some(crate::types::ChatContent::Text("Hello".into())),
                tool_calls: None,
                tool_call_id: None,
            }],
            max_completion_tokens: Some(1024),
            stream: false,
            tools: None,
            stream_options: None,
            provider: None,
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
            "model": "anthropic/claude-sonnet-4",
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
    async fn client_sends_authorization_header() {
        let server = MockServer::start().await;

        let response_body = serde_json::json!({
            "id": "chatcmpl-auth",
            "choices": [{
                "message": {"role": "assistant", "content": "ok"},
                "finish_reason": "stop",
                "index": 0
            }],
            "model": "anthropic/claude-sonnet-4",
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        });

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("authorization", "Bearer test-api-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.complete_chat(&test_request()).await;
        assert!(result.is_ok(), "authorization header should match: {result:?}");
    }

    #[tokio::test]
    async fn client_sends_x_title_header() {
        let server = MockServer::start().await;

        let response_body = serde_json::json!({
            "id": "chatcmpl-title",
            "choices": [{
                "message": {"role": "assistant", "content": "ok"},
                "finish_reason": "stop",
                "index": 0
            }],
            "model": "anthropic/claude-sonnet-4",
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        });

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("x-title", "Blufio"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.complete_chat(&test_request()).await;
        assert!(result.is_ok(), "X-Title header should match: {result:?}");
    }

    #[tokio::test]
    async fn client_sends_http_referer_header() {
        let server = MockServer::start().await;

        let response_body = serde_json::json!({
            "id": "chatcmpl-referer",
            "choices": [{
                "message": {"role": "assistant", "content": "ok"},
                "finish_reason": "stop",
                "index": 0
            }],
            "model": "anthropic/claude-sonnet-4",
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        });

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("http-referer", "https://blufio.dev"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.complete_chat(&test_request()).await;
        assert!(result.is_ok(), "HTTP-Referer header should match: {result:?}");
    }

    #[tokio::test]
    async fn client_uses_correct_endpoint() {
        let server = MockServer::start().await;

        let response_body = serde_json::json!({
            "id": "chatcmpl-url",
            "choices": [{
                "message": {"role": "assistant", "content": "ok"},
                "finish_reason": "stop",
                "index": 0
            }],
            "model": "anthropic/claude-sonnet-4",
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
    async fn default_base_url_is_openrouter() {
        let client = OpenRouterClient::new(
            "key".into(),
            "model".into(),
            "Title".into(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(client.base_url(), "https://openrouter.ai/api/v1");
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
            "model": "anthropic/claude-sonnet-4",
            "usage": {"prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8}
        });

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
    async fn complete_chat_retries_on_500() {
        let server = MockServer::start().await;

        let error_body = serde_json::json!({
            "error": {"type": "server_error", "message": "Internal error"}
        });
        let success_body = serde_json::json!({
            "id": "chatcmpl-500retry",
            "choices": [{
                "message": {"role": "assistant", "content": "recovered"},
                "finish_reason": "stop",
                "index": 0
            }],
            "model": "anthropic/claude-sonnet-4",
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

    #[tokio::test]
    async fn complete_chat_exhausts_retries_on_503() {
        let server = MockServer::start().await;

        let error_body = serde_json::json!({
            "error": {"type": "server_error", "message": "Service overloaded"}
        });

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(503).set_body_json(&error_body))
            .expect(2)
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.complete_chat(&test_request()).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("server_error"), "got: {err}");
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
        let err = result.unwrap_err().to_string();
        assert!(err.contains("authentication_error"), "got: {err}");
    }

    #[tokio::test]
    async fn client_without_http_referer() {
        let server = MockServer::start().await;

        let response_body = serde_json::json!({
            "id": "chatcmpl-no-referer",
            "choices": [{
                "message": {"role": "assistant", "content": "ok"},
                "finish_reason": "stop",
                "index": 0
            }],
            "model": "anthropic/claude-sonnet-4",
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        });

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&server)
            .await;

        // Client without http_referer.
        let client = OpenRouterClient::new(
            "test-key".into(),
            "anthropic/claude-sonnet-4".into(),
            "Blufio".into(),
            None, // no referer
            None,
        )
        .unwrap()
        .with_base_url(server.uri());

        let result = client.complete_chat(&test_request()).await;
        assert!(result.is_ok());
    }
}
