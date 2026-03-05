// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTP client for the Gemini generateContent API.
//!
//! Provides [`GeminiClient`] which handles request construction,
//! query-parameter authentication, streaming, and transient error retry.
//! Unlike OpenAI/Anthropic, Gemini uses query parameter `?key={api_key}`
//! for authentication, not headers.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use blufio_config::model::SecurityConfig;
use blufio_core::BlufioError;
use blufio_security::SsrfSafeResolver;
use futures::Stream;
use reqwest::header::{HeaderMap, HeaderValue};
use tracing::{debug, warn};

use crate::stream;
use crate::types::{ApiErrorResponse, GenerateContentRequest, GenerateContentResponse};

/// Default base URL for the Gemini API.
const API_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

/// HTTP client for Gemini API communication.
///
/// API key is sent as a query parameter `?key={api_key}`, not in headers.
/// Manages connection pooling and retry logic for transient errors (429, 500, 503).
#[derive(Debug, Clone)]
pub struct GeminiClient {
    client: reqwest::Client,
    api_key: String,
    default_model: String,
    max_retries: u32,
    base_url: String,
}

impl GeminiClient {
    /// Creates a new Gemini API client.
    ///
    /// # Arguments
    /// * `api_key` - Gemini API key (sent as query parameter, not header)
    /// * `model` - Default model identifier (e.g., "gemini-2.0-flash")
    /// * `security_config` - Optional security config for TLS 1.2+ and SSRF protection
    pub fn new(
        api_key: String,
        model: String,
        security_config: Option<&SecurityConfig>,
    ) -> Result<Self, BlufioError> {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("application/json"));

        let mut builder = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(300));

        // Apply security hardening when config is provided.
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
            api_key,
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

    /// Returns the generateContent endpoint URL for the given model.
    fn generate_content_url(&self, model: &str) -> String {
        format!(
            "{}/models/{}:generateContent?key={}",
            self.base_url, model, self.api_key
        )
    }

    /// Returns the streamGenerateContent endpoint URL for the given model.
    fn stream_generate_content_url(&self, model: &str) -> String {
        format!(
            "{}/models/{}:streamGenerateContent?key={}",
            self.base_url, model, self.api_key
        )
    }

    /// Sends a non-streaming generateContent request.
    ///
    /// On transient errors (429, 500, 503), retries once after a 1-second delay.
    pub async fn generate_content(
        &self,
        request: &GenerateContentRequest,
        model: Option<&str>,
    ) -> Result<GenerateContentResponse, BlufioError> {
        let model = model.unwrap_or(&self.default_model);
        let url = self.generate_content_url(model);
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                warn!(attempt, "retrying Gemini request after transient error");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }

            let response = self
                .client
                .post(&url)
                .json(request)
                .send()
                .await
                .map_err(|e| BlufioError::Provider {
                    message: format!("HTTP request failed: {e}"),
                    source: Some(Box::new(e)),
                })?;

            let status = response.status();
            debug!(status = %status, attempt, "Gemini response received");

            if status.is_success() {
                let body = response.text().await.map_err(|e| BlufioError::Provider {
                    message: format!("failed to read response body: {e}"),
                    source: Some(Box::new(e)),
                })?;
                let resp: GenerateContentResponse =
                    serde_json::from_str(&body).map_err(|e| BlufioError::Provider {
                        message: format!("failed to parse Gemini response: {e}"),
                        source: Some(Box::new(e)),
                    })?;
                return Ok(resp);
            }

            if is_transient_error(status) && attempt < self.max_retries {
                let body = response.text().await.unwrap_or_default();
                warn!(status = %status, body = %body, "transient error, will retry");
                last_error = Some(BlufioError::Provider {
                    message: format!("Gemini API returned {status}: {body}"),
                    source: None,
                });
                continue;
            }

            // Non-transient error or exhausted retries.
            let body = response.text().await.unwrap_or_default();
            let error_msg = if let Ok(api_err) = serde_json::from_str::<ApiErrorResponse>(&body) {
                format!(
                    "Gemini API error ({}): {}",
                    api_err.error.status.unwrap_or_default(),
                    api_err.error.message
                )
            } else {
                format!("Gemini API returned {status}: {body}")
            };
            return Err(BlufioError::Provider {
                message: error_msg,
                source: None,
            });
        }

        Err(last_error.unwrap_or_else(|| BlufioError::Provider {
            message: "Gemini request failed after retries".into(),
            source: None,
        }))
    }

    /// Sends a streaming streamGenerateContent request.
    ///
    /// On transient errors (429, 500, 503), retries once after a 1-second delay.
    pub async fn stream_generate_content(
        &self,
        request: &GenerateContentRequest,
        model: Option<&str>,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<GenerateContentResponse, BlufioError>> + Send>>,
        BlufioError,
    > {
        let model = model.unwrap_or(&self.default_model);
        let url = self.stream_generate_content_url(model);
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                warn!(
                    attempt,
                    "retrying Gemini streaming request after transient error"
                );
                tokio::time::sleep(Duration::from_secs(1)).await;
            }

            let response = self
                .client
                .post(&url)
                .json(request)
                .send()
                .await
                .map_err(|e| BlufioError::Provider {
                    message: format!("HTTP request failed: {e}"),
                    source: Some(Box::new(e)),
                })?;

            let status = response.status();
            debug!(status = %status, attempt, "Gemini streaming response received");

            if status.is_success() {
                return Ok(stream::parse_gemini_stream(response));
            }

            if is_transient_error(status) && attempt < self.max_retries {
                let body = response.text().await.unwrap_or_default();
                warn!(status = %status, body = %body, "transient error, will retry");
                last_error = Some(BlufioError::Provider {
                    message: format!("Gemini API returned {status}: {body}"),
                    source: None,
                });
                continue;
            }

            // Non-transient error or exhausted retries.
            let body = response.text().await.unwrap_or_default();
            let error_msg = if let Ok(api_err) = serde_json::from_str::<ApiErrorResponse>(&body) {
                format!(
                    "Gemini API error ({}): {}",
                    api_err.error.status.unwrap_or_default(),
                    api_err.error.message
                )
            } else {
                format!("Gemini API returned {status}: {body}")
            };
            return Err(BlufioError::Provider {
                message: error_msg,
                source: None,
            });
        }

        Err(last_error.unwrap_or_else(|| BlufioError::Provider {
            message: "Gemini streaming request failed after retries".into(),
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
    use crate::types::{GeminiContent, GeminiPart, TextPart};
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_client(base_url: &str) -> GeminiClient {
        GeminiClient::new("test-api-key".into(), "gemini-2.0-flash".into(), None)
            .unwrap()
            .with_base_url(base_url.to_string())
    }

    fn test_request() -> GenerateContentRequest {
        GenerateContentRequest {
            contents: vec![GeminiContent {
                role: "user".into(),
                parts: vec![GeminiPart::Text(TextPart {
                    text: "Hello".into(),
                })],
            }],
            system_instruction: None,
            tools: None,
            generation_config: None,
        }
    }

    #[tokio::test]
    async fn generate_content_success() {
        let server = MockServer::start().await;

        let response_body = serde_json::json!({
            "candidates": [{
                "content": {"role": "model", "parts": [{"text": "Hi there!"}]},
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5,
                "totalTokenCount": 15
            }
        });

        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:generateContent"))
            .and(query_param("key", "test-api-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client
            .generate_content(&test_request(), None)
            .await
            .unwrap();

        assert_eq!(result.candidates.len(), 1);
        let usage = result.usage_metadata.as_ref().unwrap();
        assert_eq!(usage.prompt_token_count, 10);
        assert_eq!(usage.candidates_token_count, 5);
    }

    #[tokio::test]
    async fn api_key_sent_as_query_parameter() {
        let server = MockServer::start().await;

        let response_body = serde_json::json!({
            "candidates": [{
                "content": {"role": "model", "parts": [{"text": "ok"}]},
                "finishReason": "STOP"
            }]
        });

        // This mock explicitly requires key=test-api-key as a query parameter.
        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:generateContent"))
            .and(query_param("key", "test-api-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.generate_content(&test_request(), None).await;
        assert!(
            result.is_ok(),
            "API key should be in query param: {result:?}"
        );
    }

    #[tokio::test]
    async fn correct_generate_content_url() {
        let client = GeminiClient::new("my-key".into(), "gemini-2.0-flash".into(), None).unwrap();

        let url = client.generate_content_url("gemini-2.0-flash");
        assert!(url.contains("/models/gemini-2.0-flash:generateContent"));
        assert!(url.contains("key=my-key"));
    }

    #[tokio::test]
    async fn correct_stream_url() {
        let client = GeminiClient::new("my-key".into(), "gemini-2.0-flash".into(), None).unwrap();

        let url = client.stream_generate_content_url("gemini-2.0-flash");
        assert!(url.contains("/models/gemini-2.0-flash:streamGenerateContent"));
        assert!(url.contains("key=my-key"));
    }

    #[tokio::test]
    async fn retries_on_429() {
        let server = MockServer::start().await;

        let error_body = serde_json::json!({
            "error": {
                "code": 429,
                "message": "Resource exhausted",
                "status": "RESOURCE_EXHAUSTED"
            }
        });
        let success_body = serde_json::json!({
            "candidates": [{
                "content": {"role": "model", "parts": [{"text": "After retry"}]},
                "finishReason": "STOP"
            }]
        });

        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(429).set_body_json(&error_body))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&success_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client
            .generate_content(&test_request(), None)
            .await
            .unwrap();
        match &result.candidates[0].content.parts[0] {
            GeminiPart::Text(tp) => assert_eq!(tp.text, "After retry"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn retries_on_500() {
        let server = MockServer::start().await;

        let error_body = serde_json::json!({
            "error": {"code": 500, "message": "Internal error", "status": "INTERNAL"}
        });
        let success_body = serde_json::json!({
            "candidates": [{
                "content": {"role": "model", "parts": [{"text": "recovered"}]},
                "finishReason": "STOP"
            }]
        });

        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(500).set_body_json(&error_body))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&success_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client
            .generate_content(&test_request(), None)
            .await
            .unwrap();
        match &result.candidates[0].content.parts[0] {
            GeminiPart::Text(tp) => assert_eq!(tp.text, "recovered"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn retries_on_503() {
        let server = MockServer::start().await;

        let error_body = serde_json::json!({
            "error": {"code": 503, "message": "Service unavailable", "status": "UNAVAILABLE"}
        });
        let success_body = serde_json::json!({
            "candidates": [{
                "content": {"role": "model", "parts": [{"text": "back up"}]},
                "finishReason": "STOP"
            }]
        });

        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(503).set_body_json(&error_body))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&success_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client
            .generate_content(&test_request(), None)
            .await
            .unwrap();
        match &result.candidates[0].content.parts[0] {
            GeminiPart::Text(tp) => assert_eq!(tp.text, "back up"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn fails_on_400() {
        let server = MockServer::start().await;

        let error_body = serde_json::json!({
            "error": {"code": 400, "message": "Invalid model", "status": "INVALID_ARGUMENT"}
        });

        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(400).set_body_json(&error_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.generate_content(&test_request(), None).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("INVALID_ARGUMENT"), "got: {err}");
    }

    #[tokio::test]
    async fn exhausts_retries_on_503() {
        let server = MockServer::start().await;

        let error_body = serde_json::json!({
            "error": {"code": 503, "message": "Service unavailable", "status": "UNAVAILABLE"}
        });

        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(503).set_body_json(&error_body))
            .expect(2)
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.generate_content(&test_request(), None).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("UNAVAILABLE"), "got: {err}");
    }

    #[tokio::test]
    async fn stream_sends_to_correct_url() {
        let server = MockServer::start().await;

        let response_body = r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"Hello"}]},"finishReason":"STOP"}]}"#;

        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:streamGenerateContent"))
            .and(query_param("key", "test-api-key"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_string(response_body.to_string()),
            )
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let mut stream = client
            .stream_generate_content(&test_request(), None)
            .await
            .unwrap();

        let chunk = futures::StreamExt::next(&mut stream)
            .await
            .unwrap()
            .unwrap();
        match &chunk.candidates[0].content.parts[0] {
            GeminiPart::Text(tp) => assert_eq!(tp.text, "Hello"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn content_type_header_sent() {
        let server = MockServer::start().await;

        let response_body = serde_json::json!({
            "candidates": [{
                "content": {"role": "model", "parts": [{"text": "ok"}]},
                "finishReason": "STOP"
            }]
        });

        Mock::given(method("POST"))
            .and(path("/models/gemini-2.0-flash:generateContent"))
            .and(wiremock::matchers::header(
                "content-type",
                "application/json",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.generate_content(&test_request(), None).await;
        assert!(
            result.is_ok(),
            "content-type header should be set: {result:?}"
        );
    }
}
