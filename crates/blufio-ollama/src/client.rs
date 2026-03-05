// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTP client for the Ollama native API.
//!
//! Provides [`OllamaClient`] which communicates with a local Ollama instance
//! via its REST API endpoints (`/api/chat`, `/api/tags`).
//!
//! Key differences from cloud provider clients:
//! - No API key or Authorization header (Ollama is a local service)
//! - No TLS enforcement or SSRF protection (localhost)
//! - No retry logic (local service, transient errors unlikely)
//! - NDJSON streaming (not SSE)

use std::pin::Pin;
use std::time::Duration;

use blufio_core::BlufioError;
use futures::Stream;
use tracing::debug;

use crate::stream::parse_ndjson_stream;
use crate::types::{OllamaRequest, OllamaResponse, TagsResponse};

/// HTTP client for Ollama API communication.
///
/// Manages connection to a local Ollama instance. No authentication required.
#[derive(Debug, Clone)]
pub struct OllamaClient {
    client: reqwest::Client,
    base_url: String,
    default_model: String,
}

impl OllamaClient {
    /// Creates a new Ollama API client.
    ///
    /// # Arguments
    /// * `base_url` - Base URL for the Ollama API (e.g., "http://localhost:11434")
    /// * `default_model` - Default model name (e.g., "llama3.2")
    pub fn new(base_url: String, default_model: String) -> Result<Self, BlufioError> {
        // Plain reqwest client -- no API key, no TLS enforcement, no SSRF protection.
        // Ollama is a local service running on localhost.
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|e| BlufioError::Provider {
                message: format!("failed to build HTTP client: {e}"),
                source: Some(Box::new(e)),
            })?;

        Ok(Self {
            client,
            base_url,
            default_model,
        })
    }

    /// Returns the default model name.
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

    /// Sends a non-streaming chat request to `/api/chat`.
    ///
    /// Sets `stream: false` on the request and returns the full response.
    pub async fn chat(&self, request: &OllamaRequest) -> Result<OllamaResponse, BlufioError> {
        let mut req = request.clone();
        req.stream = false;

        let url = format!("{}/api/chat", self.base_url);
        debug!(url = %url, model = %req.model, "sending chat request to Ollama");

        let response = self
            .client
            .post(&url)
            .json(&req)
            .send()
            .await
            .map_err(|e| BlufioError::Provider {
                message: format!("Ollama HTTP request failed: {e}"),
                source: Some(Box::new(e)),
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(BlufioError::Provider {
                message: format!("Ollama API returned {status}: {body}"),
                source: None,
            });
        }

        let body = response.text().await.map_err(|e| BlufioError::Provider {
            message: format!("failed to read Ollama response body: {e}"),
            source: Some(Box::new(e)),
        })?;

        serde_json::from_str(&body).map_err(|e| BlufioError::Provider {
            message: format!("failed to parse Ollama response: {e}"),
            source: Some(Box::new(e)),
        })
    }

    /// Sends a streaming chat request to `/api/chat`.
    ///
    /// Sets `stream: true` and returns an NDJSON stream of response chunks.
    pub async fn chat_stream(
        &self,
        request: &OllamaRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<OllamaResponse, BlufioError>> + Send>>, BlufioError>
    {
        let mut req = request.clone();
        req.stream = true;

        let url = format!("{}/api/chat", self.base_url);
        debug!(url = %url, model = %req.model, "sending streaming chat request to Ollama");

        let response = self
            .client
            .post(&url)
            .json(&req)
            .send()
            .await
            .map_err(|e| BlufioError::Provider {
                message: format!("Ollama HTTP request failed: {e}"),
                source: Some(Box::new(e)),
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(BlufioError::Provider {
                message: format!("Ollama API returned {status}: {body}"),
                source: None,
            });
        }

        Ok(parse_ndjson_stream(response))
    }

    /// Lists locally available models via `/api/tags`.
    pub async fn list_tags(&self) -> Result<TagsResponse, BlufioError> {
        let url = format!("{}/api/tags", self.base_url);
        debug!(url = %url, "listing Ollama models");

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| BlufioError::Provider {
                message: format!("Ollama HTTP request failed: {e}"),
                source: Some(Box::new(e)),
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(BlufioError::Provider {
                message: format!("Ollama API returned {status}: {body}"),
                source: None,
            });
        }

        let body = response.text().await.map_err(|e| BlufioError::Provider {
            message: format!("failed to read Ollama tags response: {e}"),
            source: Some(Box::new(e)),
        })?;

        serde_json::from_str(&body).map_err(|e| BlufioError::Provider {
            message: format!("failed to parse Ollama tags response: {e}"),
            source: Some(Box::new(e)),
        })
    }

    /// Health check: attempts to reach Ollama via `/api/tags`.
    ///
    /// This is a lightweight check that verifies Ollama is running and reachable.
    pub async fn health_check(&self) -> Result<(), BlufioError> {
        self.list_tags().await.map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_client(base_url: &str) -> OllamaClient {
        OllamaClient::new("http://localhost:11434".into(), "llama3.2".into())
            .unwrap()
            .with_base_url(base_url.to_string())
    }

    fn test_request() -> OllamaRequest {
        OllamaRequest {
            model: "llama3.2".into(),
            messages: vec![crate::types::OllamaMessage {
                role: "user".into(),
                content: "Hello".into(),
                tool_calls: None,
            }],
            stream: false,
            tools: None,
            format: None,
        }
    }

    #[tokio::test]
    async fn chat_sends_post_to_api_chat() {
        let server = MockServer::start().await;

        let response_body = serde_json::json!({
            "model": "llama3.2",
            "message": {"role": "assistant", "content": "Hi there!"},
            "done": true,
            "done_reason": "stop",
            "prompt_eval_count": 10,
            "eval_count": 5
        });

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.chat(&test_request()).await.unwrap();

        assert_eq!(result.model, "llama3.2");
        assert_eq!(result.message.content, "Hi there!");
        assert!(result.done);
    }

    #[tokio::test]
    async fn list_tags_sends_get_to_api_tags() {
        let server = MockServer::start().await;

        let response_body = serde_json::json!({
            "models": [
                {"name": "llama3.2:latest", "size": 1234567},
                {"name": "mistral:7b", "size": 7654321}
            ]
        });

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.list_tags().await.unwrap();

        assert_eq!(result.models.len(), 2);
        assert_eq!(result.models[0].name, "llama3.2:latest");
        assert_eq!(result.models[1].name, "mistral:7b");
    }

    #[tokio::test]
    async fn client_uses_correct_base_url() {
        let server = MockServer::start().await;

        let response_body = serde_json::json!({
            "models": [{"name": "test-model"}]
        });

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&server)
            .await;

        // Use a client pointing to our wiremock server.
        let client = test_client(&server.uri());
        let result = client.list_tags().await;
        assert!(result.is_ok(), "should use the correct base URL");
    }

    #[tokio::test]
    async fn no_authorization_header_sent() {
        let server = MockServer::start().await;

        let response_body = serde_json::json!({
            "model": "llama3.2",
            "message": {"role": "assistant", "content": "Hi"},
            "done": true
        });

        // If an Authorization header were sent, this mock wouldn't match
        // because wiremock is strict about headers when using header matchers.
        // We verify by ensuring the request succeeds without any auth header matcher.
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.chat(&test_request()).await;
        assert!(result.is_ok());

        // Verify the request was received (mock was matched).
        let received = server.received_requests().await.unwrap();
        assert_eq!(received.len(), 1);
        // Verify no Authorization header was present.
        assert!(
            !received[0].headers.contains_key("authorization"),
            "Ollama client should NOT send Authorization header"
        );
    }

    #[tokio::test]
    async fn chat_fails_on_4xx() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(
                ResponseTemplate::new(404).set_body_string(r#"{"error":"model not found"}"#),
            )
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.chat(&test_request()).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("404"), "got: {err}");
    }

    #[tokio::test]
    async fn health_check_success() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"models": []})),
            )
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let result = client.health_check().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn health_check_fails_when_unreachable() {
        // Point to a port that nothing is listening on.
        let client =
            OllamaClient::new("http://127.0.0.1:19999".into(), "llama3.2".into()).unwrap();
        let result = client.health_check().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn chat_stream_returns_ndjson_stream() {
        let server = MockServer::start().await;

        // Simulate NDJSON streaming response.
        let ndjson_body = concat!(
            r#"{"model":"llama3.2","message":{"role":"assistant","content":"Hello"},"done":false}"#,
            "\n",
            r#"{"model":"llama3.2","message":{"role":"assistant","content":" world"},"done":false}"#,
            "\n",
            r#"{"model":"llama3.2","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","prompt_eval_count":10,"eval_count":5}"#,
            "\n",
        );

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string(ndjson_body))
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let stream = client.chat_stream(&test_request()).await.unwrap();

        use futures::StreamExt;
        let chunks: Vec<_> = stream.collect().await;
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].as_ref().unwrap().message.content, "Hello");
        assert_eq!(chunks[1].as_ref().unwrap().message.content, " world");
        assert!(chunks[2].as_ref().unwrap().done);
    }
}
