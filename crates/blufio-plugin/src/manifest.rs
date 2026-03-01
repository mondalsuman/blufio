// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Plugin manifest parsing from `plugin.toml` files.
//!
//! Plugin manifests describe adapter plugins (Channel, Provider, Storage, etc.)
//! and are distinct from skill manifests which describe WASM sandbox skills.

use blufio_core::types::AdapterType;
use blufio_core::BlufioError;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Parsed plugin manifest describing an adapter plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Unique name of the plugin (e.g., "telegram", "anthropic").
    pub name: String,
    /// Semantic version string.
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// Type of adapter this plugin provides.
    pub adapter_type: AdapterType,
    /// Optional author identifier.
    pub author: Option<String>,
    /// Capabilities the plugin provides (e.g., ["streaming", "editing", "typing"]).
    pub capabilities: Vec<String>,
    /// Minimum Blufio version required (e.g., "0.1.0").
    pub min_blufio_version: Option<String>,
    /// Required config keys (e.g., ["telegram.bot_token"]).
    pub config_keys: Vec<String>,
}

/// Intermediate TOML deserialization struct for `plugin.toml`.
#[derive(Debug, Deserialize)]
struct PluginManifestFile {
    plugin: PluginSection,
}

/// The `[plugin]` section of a `plugin.toml` file.
#[derive(Debug, Deserialize)]
struct PluginSection {
    name: String,
    version: String,
    description: String,
    adapter_type: String,
    author: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    min_blufio_version: Option<String>,
    #[serde(default)]
    config_keys: Vec<String>,
}

/// Parse a plugin manifest from TOML content.
///
/// Validates that the adapter_type is a valid `AdapterType` variant,
/// name is non-empty, and version is non-empty.
pub fn parse_plugin_manifest(toml_content: &str) -> Result<PluginManifest, BlufioError> {
    let file: PluginManifestFile =
        toml::from_str(toml_content).map_err(|e| BlufioError::Config(format!("invalid plugin manifest: {e}")))?;

    let section = file.plugin;

    if section.name.is_empty() {
        return Err(BlufioError::Config(
            "plugin manifest: name must not be empty".to_string(),
        ));
    }

    if section.version.is_empty() {
        return Err(BlufioError::Config(
            "plugin manifest: version must not be empty".to_string(),
        ));
    }

    let adapter_type = AdapterType::from_str(&section.adapter_type).map_err(|_| {
        BlufioError::Config(format!(
            "plugin manifest: invalid adapter_type '{}'. Expected one of: Channel, Provider, Storage, Embedding, Observability, Auth, SkillRuntime",
            section.adapter_type
        ))
    })?;

    Ok(PluginManifest {
        name: section.name,
        version: section.version,
        description: section.description,
        adapter_type,
        author: section.author,
        capabilities: section.capabilities,
        min_blufio_version: section.min_blufio_version,
        config_keys: section.config_keys,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_manifest() {
        let toml = r#"
[plugin]
name = "telegram"
version = "0.1.0"
description = "Telegram Bot API channel adapter"
adapter_type = "Channel"
author = "Blufio Contributors"
capabilities = ["text", "images", "editing"]
min_blufio_version = "0.1.0"
config_keys = ["telegram.bot_token"]
"#;
        let manifest = parse_plugin_manifest(toml).unwrap();
        assert_eq!(manifest.name, "telegram");
        assert_eq!(manifest.version, "0.1.0");
        assert_eq!(manifest.adapter_type, AdapterType::Channel);
        assert_eq!(manifest.capabilities, vec!["text", "images", "editing"]);
        assert_eq!(manifest.config_keys, vec!["telegram.bot_token"]);
        assert_eq!(manifest.author.as_deref(), Some("Blufio Contributors"));
        assert_eq!(manifest.min_blufio_version.as_deref(), Some("0.1.0"));
    }

    #[test]
    fn parse_invalid_adapter_type() {
        let toml = r#"
[plugin]
name = "bad"
version = "0.1.0"
description = "invalid type"
adapter_type = "FooBar"
"#;
        let result = parse_plugin_manifest(toml);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid adapter_type"));
    }

    #[test]
    fn parse_missing_name() {
        let toml = r#"
[plugin]
name = ""
version = "0.1.0"
description = "empty name"
adapter_type = "Channel"
"#;
        let result = parse_plugin_manifest(toml);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("name must not be empty"));
    }

    #[test]
    fn parse_missing_version() {
        let toml = r#"
[plugin]
name = "test"
version = ""
description = "empty version"
adapter_type = "Channel"
"#;
        let result = parse_plugin_manifest(toml);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("version must not be empty"));
    }

    #[test]
    fn parse_minimal_manifest() {
        let toml = r#"
[plugin]
name = "minimal"
version = "1.0.0"
description = "a minimal plugin"
adapter_type = "Storage"
"#;
        let manifest = parse_plugin_manifest(toml).unwrap();
        assert_eq!(manifest.name, "minimal");
        assert_eq!(manifest.adapter_type, AdapterType::Storage);
        assert!(manifest.capabilities.is_empty());
        assert!(manifest.config_keys.is_empty());
        assert!(manifest.author.is_none());
        assert!(manifest.min_blufio_version.is_none());
    }
}
