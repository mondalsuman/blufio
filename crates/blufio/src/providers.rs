// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Concrete [`ProviderRegistry`] implementation for the Blufio binary.
//!
//! Wraps all initialized provider adapters and provides model resolution,
//! provider lookup, and model listing for the gateway API layer.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use blufio_config::model::BlufioConfig;
use blufio_core::error::BlufioError;
use blufio_core::traits::{ModelInfo, ProviderAdapter, ProviderRegistry};
use tracing::warn;

/// Concrete provider registry that wraps initialized provider adapters.
///
/// Created during server startup, holds all successfully initialized providers
/// and routes model requests to the appropriate provider.
pub struct ConcreteProviderRegistry {
    providers: HashMap<String, Arc<dyn ProviderAdapter + Send + Sync>>,
    default_provider: String,
    /// Ollama provider stored separately for `list_local_models()` access.
    #[cfg(feature = "ollama")]
    ollama: Option<Arc<blufio_ollama::OllamaProvider>>,
}

impl ConcreteProviderRegistry {
    /// Creates a registry from config, initializing each provider behind its feature flag.
    ///
    /// - Providers are only initialized if their config section has meaningful values
    ///   (e.g., `api_key` set for cloud providers, `default_model` set for Ollama).
    /// - Non-default providers that fail to init log a warning and are skipped.
    /// - Default provider failure is a hard error.
    pub async fn from_config(config: &BlufioConfig) -> Result<Self, BlufioError> {
        let default_provider = config.providers.default.clone();
        let mut providers: HashMap<String, Arc<dyn ProviderAdapter + Send + Sync>> = HashMap::new();

        #[cfg(feature = "ollama")]
        let mut ollama_arc: Option<Arc<blufio_ollama::OllamaProvider>> = None;

        // --- Anthropic ---
        #[cfg(feature = "anthropic")]
        {
            let is_default = default_provider == "anthropic";
            // Anthropic config-required: api_key in config or ANTHROPIC_API_KEY env var
            let has_config = config
                .anthropic
                .api_key
                .as_ref()
                .is_some_and(|k| !k.is_empty())
                || std::env::var("ANTHROPIC_API_KEY").is_ok();

            if has_config || is_default {
                match blufio_anthropic::AnthropicProvider::new(config).await {
                    Ok(provider) => {
                        providers.insert("anthropic".into(), Arc::new(provider));
                    }
                    Err(e) => {
                        if is_default {
                            return Err(BlufioError::Config(format!(
                                "Default provider 'anthropic' failed to initialize: {e}"
                            )));
                        }
                        warn!("Skipping anthropic provider: {e}");
                    }
                }
            }
        }

        // --- OpenAI ---
        #[cfg(feature = "openai")]
        {
            let is_default = default_provider == "openai";
            let has_config = config
                .providers
                .openai
                .api_key
                .as_ref()
                .is_some_and(|k| !k.is_empty())
                || std::env::var("OPENAI_API_KEY").is_ok();

            if has_config || is_default {
                match blufio_openai::OpenAIProvider::new(config).await {
                    Ok(provider) => {
                        providers.insert("openai".into(), Arc::new(provider));
                    }
                    Err(e) => {
                        if is_default {
                            return Err(BlufioError::Config(format!(
                                "Default provider 'openai' failed to initialize: {e}"
                            )));
                        }
                        warn!("Skipping openai provider: {e}");
                    }
                }
            }
        }

        // --- Ollama ---
        #[cfg(feature = "ollama")]
        {
            let is_default = default_provider == "ollama";
            // Ollama config-required: default_model must be set (no API key needed)
            let has_config = config
                .providers
                .ollama
                .default_model
                .as_ref()
                .is_some_and(|m| !m.is_empty());

            if has_config || is_default {
                match blufio_ollama::OllamaProvider::new(config).await {
                    Ok(provider) => {
                        let arc = Arc::new(provider);
                        providers.insert("ollama".into(), arc.clone());
                        ollama_arc = Some(arc);
                    }
                    Err(e) => {
                        if is_default {
                            return Err(BlufioError::Config(format!(
                                "Default provider 'ollama' failed to initialize: {e}"
                            )));
                        }
                        warn!("Skipping ollama provider: {e}");
                    }
                }
            }
        }

        // --- OpenRouter ---
        #[cfg(feature = "openrouter")]
        {
            let is_default = default_provider == "openrouter";
            let has_config = config
                .providers
                .openrouter
                .api_key
                .as_ref()
                .is_some_and(|k| !k.is_empty())
                || std::env::var("OPENROUTER_API_KEY").is_ok();

            if has_config || is_default {
                match blufio_openrouter::OpenRouterProvider::new(config).await {
                    Ok(provider) => {
                        providers.insert("openrouter".into(), Arc::new(provider));
                    }
                    Err(e) => {
                        if is_default {
                            return Err(BlufioError::Config(format!(
                                "Default provider 'openrouter' failed to initialize: {e}"
                            )));
                        }
                        warn!("Skipping openrouter provider: {e}");
                    }
                }
            }
        }

        // --- Gemini ---
        #[cfg(feature = "gemini")]
        {
            let is_default = default_provider == "gemini";
            let has_config = config
                .providers
                .gemini
                .api_key
                .as_ref()
                .is_some_and(|k| !k.is_empty())
                || std::env::var("GEMINI_API_KEY").is_ok();

            if has_config || is_default {
                match blufio_gemini::GeminiProvider::new(config).await {
                    Ok(provider) => {
                        providers.insert("gemini".into(), Arc::new(provider));
                    }
                    Err(e) => {
                        if is_default {
                            return Err(BlufioError::Config(format!(
                                "Default provider 'gemini' failed to initialize: {e}"
                            )));
                        }
                        warn!("Skipping gemini provider: {e}");
                    }
                }
            }
        }

        // Ensure default provider was actually initialized
        if !providers.contains_key(&default_provider) {
            return Err(BlufioError::Config(format!(
                "Default provider '{default_provider}' not available. \
                 Ensure its feature flag is enabled and config is valid."
            )));
        }

        Ok(Self {
            providers,
            default_provider,
            #[cfg(feature = "ollama")]
            ollama: ollama_arc,
        })
    }

    /// Creates a registry from pre-built providers (for testing without API keys).
    #[allow(dead_code)]
    pub fn from_providers(
        providers: HashMap<String, Arc<dyn ProviderAdapter + Send + Sync>>,
        default: String,
    ) -> Self {
        Self {
            providers,
            default_provider: default,
            #[cfg(feature = "ollama")]
            ollama: None,
        }
    }

    /// Parses a model identifier into `(provider_name, model_id)`.
    ///
    /// - `"openai/gpt-4o"` -> `("openai", "gpt-4o")`
    /// - `"gpt-4o"` -> `(default_provider, "gpt-4o")`
    #[allow(dead_code)]
    pub fn resolve_model<'a>(&'a self, model: &'a str) -> (&'a str, &'a str) {
        if let Some(idx) = model.find('/') {
            let (provider, rest) = model.split_at(idx);
            (provider, &rest[1..])
        } else {
            (&self.default_provider, model)
        }
    }
}

/// Static model lists for cloud providers.
fn static_models_for(provider: &str) -> Vec<ModelInfo> {
    let models: &[&str] = match provider {
        "openai" => &[
            "gpt-4o",
            "gpt-4o-mini",
            "gpt-4-turbo",
            "o1",
            "o1-mini",
            "o3-mini",
        ],
        "anthropic" => &[
            "claude-sonnet-4-20250514",
            "claude-haiku-35-20241022",
            "claude-opus-4-20250514",
        ],
        "gemini" => &["gemini-2.0-flash", "gemini-1.5-pro", "gemini-1.5-flash"],
        // OpenRouter is a pass-through; models are cloud-side.
        "openrouter" => &[],
        _ => &[],
    };

    models
        .iter()
        .map(|name| ModelInfo {
            id: format!("{provider}/{name}"),
            object: "model".into(),
            created: 0,
            owned_by: provider.into(),
        })
        .collect()
}

#[async_trait]
impl ProviderRegistry for ConcreteProviderRegistry {
    fn get_provider(&self, name: &str) -> Option<Arc<dyn ProviderAdapter + Send + Sync>> {
        self.providers.get(name).cloned()
    }

    fn default_provider(&self) -> &str {
        &self.default_provider
    }

    async fn list_models(
        &self,
        provider_filter: Option<&str>,
    ) -> Result<Vec<ModelInfo>, BlufioError> {
        let mut all_models = Vec::new();

        let provider_names: Vec<&String> = if let Some(filter) = provider_filter {
            self.providers
                .keys()
                .filter(|name| name.as_str() == filter)
                .collect()
        } else {
            self.providers.keys().collect()
        };

        for name in provider_names {
            // For Ollama, attempt to discover local models dynamically.
            #[cfg(feature = "ollama")]
            if name == "ollama" {
                if let Some(ref ollama) = self.ollama {
                    match ollama.list_local_models().await {
                        Ok(model_names) => {
                            for model_name in model_names {
                                all_models.push(ModelInfo {
                                    id: format!("ollama/{model_name}"),
                                    object: "model".into(),
                                    created: 0,
                                    owned_by: "ollama".into(),
                                });
                            }
                        }
                        Err(e) => {
                            warn!("Failed to list Ollama models: {e}");
                        }
                    }
                }
                continue;
            }

            // Cloud providers: use static model lists.
            all_models.extend(static_models_for(name));
        }

        Ok(all_models)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;

    use async_trait::async_trait;
    use blufio_core::types::{
        AdapterType, HealthStatus, ProviderRequest, ProviderResponse, ProviderStreamChunk,
    };
    use futures::stream::Stream;

    /// A mock provider for testing without API keys.
    struct MockProvider {
        provider_name: String,
    }

    impl MockProvider {
        fn new(name: &str) -> Self {
            Self {
                provider_name: name.into(),
            }
        }
    }

    #[async_trait]
    impl blufio_core::traits::PluginAdapter for MockProvider {
        fn name(&self) -> &str {
            &self.provider_name
        }

        fn version(&self) -> semver::Version {
            semver::Version::new(0, 1, 0)
        }

        fn adapter_type(&self) -> AdapterType {
            AdapterType::Provider
        }

        async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
            Ok(HealthStatus::Healthy)
        }

        async fn shutdown(&self) -> Result<(), BlufioError> {
            Ok(())
        }
    }

    #[async_trait]
    impl ProviderAdapter for MockProvider {
        async fn complete(
            &self,
            _request: ProviderRequest,
        ) -> Result<ProviderResponse, BlufioError> {
            Err(BlufioError::Internal("mock provider: complete not implemented".to_string()))
        }

        async fn stream(
            &self,
            _request: ProviderRequest,
        ) -> Result<
            Pin<Box<dyn Stream<Item = Result<ProviderStreamChunk, BlufioError>> + Send>>,
            BlufioError,
        > {
            Err(BlufioError::Internal("mock provider: stream not implemented".to_string()))
        }
    }

    fn make_registry(names: &[&str], default: &str) -> ConcreteProviderRegistry {
        let mut providers: HashMap<String, Arc<dyn ProviderAdapter + Send + Sync>> = HashMap::new();
        for name in names {
            providers.insert((*name).into(), Arc::new(MockProvider::new(name)));
        }
        ConcreteProviderRegistry::from_providers(providers, default.into())
    }

    #[test]
    fn from_providers_constructor() {
        let reg = make_registry(&["openai", "anthropic"], "anthropic");
        assert!(reg.get_provider("openai").is_some());
        assert!(reg.get_provider("anthropic").is_some());
        assert!(reg.get_provider("ollama").is_none());
    }

    #[test]
    fn default_provider_returns_configured() {
        let reg = make_registry(&["openai", "anthropic"], "openai");
        assert_eq!(reg.default_provider(), "openai");
    }

    #[test]
    fn get_provider_by_name() {
        let reg = make_registry(&["gemini", "openai"], "openai");
        assert!(reg.get_provider("gemini").is_some());
        assert!(reg.get_provider("unknown").is_none());
    }

    #[test]
    fn resolve_model_with_prefix() {
        let reg = make_registry(&["openai"], "anthropic");
        let (provider, model) = reg.resolve_model("openai/gpt-4o");
        assert_eq!(provider, "openai");
        assert_eq!(model, "gpt-4o");
    }

    #[test]
    fn resolve_model_without_prefix_routes_to_default() {
        let reg = make_registry(&["anthropic"], "anthropic");
        let (provider, model) = reg.resolve_model("claude-sonnet-4-20250514");
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-sonnet-4-20250514");
    }

    #[test]
    fn resolve_model_with_nested_slashes() {
        let reg = make_registry(&["openrouter"], "openrouter");
        // OpenRouter models can have slashes: "anthropic/claude-3.5-sonnet"
        let (provider, model) = reg.resolve_model("openrouter/anthropic/claude-3.5-sonnet");
        assert_eq!(provider, "openrouter");
        assert_eq!(model, "anthropic/claude-3.5-sonnet");
    }

    #[tokio::test]
    async fn list_models_returns_static_lists() {
        let reg = make_registry(&["openai", "anthropic"], "anthropic");
        let models = reg.list_models(None).await.unwrap();

        let openai_models: Vec<_> = models.iter().filter(|m| m.owned_by == "openai").collect();
        let anthropic_models: Vec<_> = models
            .iter()
            .filter(|m| m.owned_by == "anthropic")
            .collect();

        assert_eq!(openai_models.len(), 6);
        assert_eq!(anthropic_models.len(), 3);

        // Check format: "provider/model"
        assert!(openai_models.iter().any(|m| m.id == "openai/gpt-4o"));
        assert!(
            anthropic_models
                .iter()
                .any(|m| m.id == "anthropic/claude-sonnet-4-20250514")
        );
    }

    #[tokio::test]
    async fn list_models_with_filter() {
        let reg = make_registry(&["openai", "anthropic", "gemini"], "anthropic");
        let models = reg.list_models(Some("gemini")).await.unwrap();

        assert!(models.iter().all(|m| m.owned_by == "gemini"));
        assert_eq!(models.len(), 3);
        assert!(models.iter().any(|m| m.id == "gemini/gemini-2.0-flash"));
    }

    #[tokio::test]
    async fn list_models_filter_unknown_provider_returns_empty() {
        let reg = make_registry(&["openai"], "openai");
        let models = reg.list_models(Some("nonexistent")).await.unwrap();
        assert!(models.is_empty());
    }

    #[test]
    fn static_models_openrouter_returns_empty() {
        let models = static_models_for("openrouter");
        assert!(models.is_empty());
    }

    #[test]
    fn static_models_unknown_returns_empty() {
        let models = static_models_for("unknown");
        assert!(models.is_empty());
    }
}
