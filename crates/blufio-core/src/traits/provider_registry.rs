// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Provider registry trait for the gateway API layer.
//!
//! Provides model resolution and provider lookup used by the OpenAI-compatible
//! gateway endpoints (`/v1/chat/completions`, `/v1/models`).

use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;

use crate::error::BlufioError;
use crate::traits::ProviderAdapter;

/// Info about a model available through a provider.
#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    /// Model identifier in provider/model format (e.g., "openai/gpt-4o").
    pub id: String,
    /// Object type (always "model").
    pub object: String,
    /// Unix timestamp of creation (0 if unknown).
    pub created: i64,
    /// Provider that owns the model.
    pub owned_by: String,
}

/// Registry of initialized provider adapters.
///
/// Provides model resolution and provider lookup for the gateway API.
/// Implementations wrap the set of initialized provider adapters and
/// expose them for direct access by the chat completions handler.
#[async_trait]
pub trait ProviderRegistry: Send + Sync {
    /// Get a provider adapter by name (e.g., "openai", "ollama").
    fn get_provider(&self, name: &str) -> Option<Arc<dyn ProviderAdapter + Send + Sync>>;

    /// Get the default provider name from config.
    fn default_provider(&self) -> &str;

    /// List all available models across all providers.
    ///
    /// If `provider_filter` is `Some`, only list models from that provider.
    async fn list_models(
        &self,
        provider_filter: Option<&str>,
    ) -> Result<Vec<ModelInfo>, BlufioError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_info_serializes() {
        let info = ModelInfo {
            id: "openai/gpt-4o".into(),
            object: "model".into(),
            created: 0,
            owned_by: "openai".into(),
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["id"], "openai/gpt-4o");
        assert_eq!(json["object"], "model");
        assert_eq!(json["created"], 0);
        assert_eq!(json["owned_by"], "openai");
    }

    // Verify the trait is object-safe (can be used as dyn).
    fn _assert_object_safe(_: &dyn ProviderRegistry) {}
}
