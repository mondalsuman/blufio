// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Built-in adapter catalog.
//!
//! Returns hardcoded `PluginManifest` entries for the 6 default adapters
//! compiled into the Blufio binary. No network calls are made.

use blufio_core::types::AdapterType;

use crate::manifest::PluginManifest;

/// Returns manifests for all built-in adapters.
///
/// The catalog contains 6 default adapters:
/// - telegram (Channel)
/// - anthropic (Provider)
/// - sqlite (Storage)
/// - onnx-embedder (Embedding)
/// - prometheus (Observability)
/// - keypair-auth (Auth)
pub fn builtin_catalog() -> Vec<PluginManifest> {
    vec![
        PluginManifest {
            name: "telegram".to_string(),
            version: "0.1.0".to_string(),
            description: "Telegram Bot API channel adapter".to_string(),
            adapter_type: AdapterType::Channel,
            author: Some("Blufio Contributors".to_string()),
            capabilities: vec![
                "text".to_string(),
                "images".to_string(),
                "documents".to_string(),
                "voice".to_string(),
                "editing".to_string(),
                "typing".to_string(),
            ],
            min_blufio_version: Some("0.1.0".to_string()),
            config_keys: vec!["telegram.bot_token".to_string()],
        },
        PluginManifest {
            name: "anthropic".to_string(),
            version: "0.1.0".to_string(),
            description: "Anthropic Claude LLM provider".to_string(),
            adapter_type: AdapterType::Provider,
            author: Some("Blufio Contributors".to_string()),
            capabilities: vec![
                "streaming".to_string(),
                "tool_use".to_string(),
                "prompt_caching".to_string(),
            ],
            min_blufio_version: Some("0.1.0".to_string()),
            config_keys: vec!["anthropic.api_key".to_string()],
        },
        PluginManifest {
            name: "sqlite".to_string(),
            version: "0.1.0".to_string(),
            description: "SQLite WAL-mode persistent storage".to_string(),
            adapter_type: AdapterType::Storage,
            author: Some("Blufio Contributors".to_string()),
            capabilities: vec![
                "sessions".to_string(),
                "messages".to_string(),
                "queue".to_string(),
            ],
            min_blufio_version: Some("0.1.0".to_string()),
            config_keys: vec![],
        },
        PluginManifest {
            name: "onnx-embedder".to_string(),
            version: "0.1.0".to_string(),
            description: "Local ONNX embedding model".to_string(),
            adapter_type: AdapterType::Embedding,
            author: Some("Blufio Contributors".to_string()),
            capabilities: vec![
                "offline".to_string(),
                "semantic_search".to_string(),
            ],
            min_blufio_version: Some("0.1.0".to_string()),
            config_keys: vec![],
        },
        PluginManifest {
            name: "prometheus".to_string(),
            version: "0.1.0".to_string(),
            description: "Prometheus metrics exporter".to_string(),
            adapter_type: AdapterType::Observability,
            author: Some("Blufio Contributors".to_string()),
            capabilities: vec![
                "counters".to_string(),
                "gauges".to_string(),
                "histograms".to_string(),
            ],
            min_blufio_version: Some("0.1.0".to_string()),
            config_keys: vec![],
        },
        PluginManifest {
            name: "keypair-auth".to_string(),
            version: "0.1.0".to_string(),
            description: "Ed25519 device keypair authentication".to_string(),
            adapter_type: AdapterType::Auth,
            author: Some("Blufio Contributors".to_string()),
            capabilities: vec![
                "bearer_token".to_string(),
                "signing".to_string(),
            ],
            min_blufio_version: Some("0.1.0".to_string()),
            config_keys: vec![],
        },
    ]
}

/// Search the built-in catalog by query string.
///
/// Filters entries whose name or description contains the query (case-insensitive).
/// If query is empty, returns all entries.
pub fn search_catalog(query: &str) -> Vec<PluginManifest> {
    if query.is_empty() {
        return builtin_catalog();
    }
    let query_lower = query.to_lowercase();
    builtin_catalog()
        .into_iter()
        .filter(|m| {
            m.name.to_lowercase().contains(&query_lower)
                || m.description.to_lowercase().contains(&query_lower)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_returns_six_entries() {
        let catalog = builtin_catalog();
        assert_eq!(catalog.len(), 6);
    }

    #[test]
    fn builtin_catalog_covers_all_adapter_types() {
        let catalog = builtin_catalog();
        let types: std::collections::HashSet<AdapterType> =
            catalog.iter().map(|m| m.adapter_type).collect();

        assert!(types.contains(&AdapterType::Channel));
        assert!(types.contains(&AdapterType::Provider));
        assert!(types.contains(&AdapterType::Storage));
        assert!(types.contains(&AdapterType::Embedding));
        assert!(types.contains(&AdapterType::Observability));
        assert!(types.contains(&AdapterType::Auth));
    }

    #[test]
    fn search_catalog_finds_telegram() {
        let results = search_catalog("telegram");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "telegram");
    }

    #[test]
    fn search_catalog_case_insensitive() {
        let results = search_catalog("ANTHROPIC");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "anthropic");
    }

    #[test]
    fn search_catalog_by_description() {
        let results = search_catalog("WAL-mode");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "sqlite");
    }

    #[test]
    fn search_catalog_empty_returns_all() {
        let results = search_catalog("");
        assert_eq!(results.len(), 6);
    }

    #[test]
    fn search_catalog_no_match() {
        let results = search_catalog("xyz_nonexistent");
        assert!(results.is_empty());
    }
}
