// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTP client for the Anthropic Messages API.
//!
//! Provides [`AnthropicClient`] which handles request construction,
//! authentication, streaming SSE responses, and transient error retry.

use std::pin::Pin;
use std::time::Duration;

use blufio_core::BlufioError;
use futures::Stream;
use reqwest::header::{HeaderMap, HeaderValue};
use tracing::{debug, warn};

use crate::sse::{self, StreamEvent};
use crate::types::{ApiErrorResponse, MessageRequest, MessageResponse};

/// Base URL for the Anthropic Messages API.
const API_BASE_URL: &str = "https://api.anthropic.com/v1/messages";

/// HTTP client for Anthropic API communication.
///
/// Manages authentication headers, connection pooling, and retry logic
/// for transient errors (429, 500, 503).
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
    pub fn new(api_key: String, api_version: String, model: String) -> Result<Self, BlufioError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&api_key).map_err(|e| BlufioError::Config(format!(
                "invalid API key header value: {e}"
            )))?,
        );
        headers.insert(
            "anthropic-version",
            HeaderValue::from_str(&api_version).map_err(|e| BlufioError::Config(format!(
                "invalid API version header value: {e}"
            )))?,
        );
        headers.insert(
            "content-type",
            HeaderValue::from_static("application/json"),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|e| BlufioError::Provider {
                message: format!("failed to build HTTP client: {e}"),
                source: Some(Box::new(e)),
            })?;

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

    /// Sends a streaming request and returns a stream of SSE events.
    ///
    /// On transient errors (429, 500, 503), retries once after a 1-second delay.
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
                .map_err(|e| BlufioError::Provider {
                    message: format!("HTTP request failed: {e}"),
                    source: Some(Box::new(e)),
                })?;

            let status = response.status();
            debug!(status = %status, attempt, "streaming response received");

            if status.is_success() {
                return Ok(sse::parse_sse_stream(response));
            }

            if is_transient_error(status) && attempt < self.max_retries {
                let body = response.text().await.unwrap_or_default();
                warn!(status = %status, body = %body, "transient error, will retry");
                last_error = Some(BlufioError::Provider {
                    message: format!("API returned {status}: {body}"),
                    source: None,
                });
                continue;
            }

            // Non-transient error or exhausted retries.
            let body = response.text().await.unwrap_or_default();
            let error_msg = if let Ok(api_err) = serde_json::from_str::<ApiErrorResponse>(&body) {
                format!(
                    "Anthropic API error ({}): {}",
                    api_err.error.type_, api_err.error.message
                )
            } else {
                format!("API returned {status}: {body}")
            };
            return Err(BlufioError::Provider {
                message: error_msg,
                source: None,
            });
        }

        Err(last_error.unwrap_or_else(|| BlufioError::Provider {
            message: "streaming request failed after retries".into(),
            source: None,
        }))
    }

    /// Sends a non-streaming request and returns the full response.
    ///
    /// On transient errors (429, 500, 503), retries once after a 1-second delay.
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
                .map_err(|e| BlufioError::Provider {
                    message: format!("HTTP request failed: {e}"),
                    source: Some(Box::new(e)),
                })?;

            let status = response.status();
            debug!(status = %status, attempt, "completion response received");

            if status.is_success() {
                let body = response.text().await.map_err(|e| BlufioError::Provider {
                    message: format!("failed to read response body: {e}"),
                    source: Some(Box::new(e)),
                })?;
                let msg_response: MessageResponse =
                    serde_json::from_str(&body).map_err(|e| BlufioError::Provider {
                        message: format!("failed to parse API response: {e}"),
                        source: Some(Box::new(e)),
                    })?;
                return Ok(msg_response);
            }

            if is_transient_error(status) && attempt < self.max_retries {
                let body = response.text().await.unwrap_or_default();
                warn!(status = %status, body = %body, "transient error, will retry");
                last_error = Some(BlufioError::Provider {
                    message: format!("API returned {status}: {body}"),
                    source: None,
                });
                continue;
            }

            // Non-transient error or exhausted retries.
            let body = response.text().await.unwrap_or_default();
            let error_msg = if let Ok(api_err) = serde_json::from_str::<ApiErrorResponse>(&body) {
                format!(
                    "Anthropic API error ({}): {}",
                    api_err.error.type_, api_err.error.message
                )
            } else {
                format!("API returned {status}: {body}")
            };
            return Err(BlufioError::Provider {
                message: error_msg,
                source: None,
            });
        }

        Err(last_error.unwrap_or_else(|| BlufioError::Provider {
            message: "completion request failed after retries".into(),
            source: None,
        }))
    }
}

/// Returns true for HTTP status codes that indicate transient errors worth retrying.
fn is_transient_error(status: reqwest::StatusCode) -> bool {
    matches!(
        status.as_u16(),
        429 | 500 | 503 | 529
    )
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
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid_request_error"), "got: {err}");
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
        let err = result.unwrap_err().to_string();
        assert!(err.contains("overloaded_error"), "got: {err}");
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
}
