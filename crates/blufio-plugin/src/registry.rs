// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Plugin registry for managing compiled-in adapter plugins.
//!
//! The `PluginRegistry` stores `PluginEntry` records keyed by plugin name.
//! Each entry contains a manifest, status, and optional factory for creating
//! adapter instances at runtime.

use blufio_core::traits::adapter::PluginAdapter;
use blufio_core::types::AdapterType;
use blufio_core::BlufioError;
use std::collections::HashMap;

use crate::manifest::PluginManifest;

/// Status of a plugin in the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginStatus {
    /// Plugin is active and will be initialized.
    Enabled,
    /// Plugin is explicitly disabled by user.
    Disabled,
    /// Plugin is compiled in but missing required configuration.
    NotConfigured,
}

impl std::fmt::Display for PluginStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginStatus::Enabled => write!(f, "enabled"),
            PluginStatus::Disabled => write!(f, "disabled"),
            PluginStatus::NotConfigured => write!(f, "not-configured"),
        }
    }
}

/// Factory trait for creating adapter instances from configuration.
///
/// Factories are optional -- the registry can hold manifests without factories
/// for catalog display purposes (plugin list/search).
pub trait PluginFactory: Send + Sync {
    /// The adapter type this factory produces.
    fn adapter_type(&self) -> AdapterType;

    /// Create a new adapter instance from the given configuration.
    fn create(
        &self,
        config: &serde_json::Value,
    ) -> Result<Box<dyn PluginAdapter>, BlufioError>;
}

/// A single entry in the plugin registry.
pub struct PluginEntry {
    /// Plugin manifest with metadata.
    pub manifest: PluginManifest,
    /// Current status of the plugin.
    pub status: PluginStatus,
    /// Optional factory for creating adapter instances.
    pub factory: Option<Box<dyn PluginFactory>>,
}

impl std::fmt::Debug for PluginEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginEntry")
            .field("manifest", &self.manifest)
            .field("status", &self.status)
            .field("factory", &self.factory.is_some())
            .finish()
    }
}

/// Registry of compiled-in adapter plugins.
///
/// Stores plugin entries keyed by name, supporting registration, lookup,
/// filtering by adapter type, and status toggling.
pub struct PluginRegistry {
    entries: HashMap<String, PluginEntry>,
}

impl PluginRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Register a plugin with default status `Enabled`.
    pub fn register(
        &mut self,
        manifest: PluginManifest,
        factory: Option<Box<dyn PluginFactory>>,
    ) {
        self.register_with_status(manifest, factory, PluginStatus::Enabled);
    }

    /// Register a plugin with an explicit status.
    pub fn register_with_status(
        &mut self,
        manifest: PluginManifest,
        factory: Option<Box<dyn PluginFactory>>,
        status: PluginStatus,
    ) {
        let name = manifest.name.clone();
        self.entries.insert(
            name,
            PluginEntry {
                manifest,
                status,
                factory,
            },
        );
    }

    /// Get a plugin entry by name.
    pub fn get(&self, name: &str) -> Option<&PluginEntry> {
        self.entries.get(name)
    }

    /// Get all enabled plugins matching the given adapter type.
    pub fn get_enabled(&self, adapter_type: AdapterType) -> Vec<&PluginEntry> {
        self.entries
            .values()
            .filter(|e| e.status == PluginStatus::Enabled && e.manifest.adapter_type == adapter_type)
            .collect()
    }

    /// List all plugin entries, sorted by name.
    pub fn list_all(&self) -> Vec<&PluginEntry> {
        let mut entries: Vec<&PluginEntry> = self.entries.values().collect();
        entries.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
        entries
    }

    /// Toggle a plugin's enabled status.
    ///
    /// If `enabled` is true, sets status to `Enabled`.
    /// If `enabled` is false, sets status to `Disabled`.
    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> Result<(), BlufioError> {
        let entry = self.entries.get_mut(name).ok_or_else(|| {
            BlufioError::AdapterNotFound {
                adapter_type: "unknown".to_string(),
                name: name.to_string(),
            }
        })?;
        entry.status = if enabled {
            PluginStatus::Enabled
        } else {
            PluginStatus::Disabled
        };
        Ok(())
    }

    /// Returns the number of registered plugins.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if no plugins are registered.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_manifest(name: &str, adapter_type: AdapterType) -> PluginManifest {
        PluginManifest {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            description: format!("Test plugin {name}"),
            adapter_type,
            author: None,
            capabilities: vec![],
            min_blufio_version: None,
            config_keys: vec![],
        }
    }

    #[test]
    fn register_and_get_roundtrip() {
        let mut registry = PluginRegistry::new();
        registry.register(test_manifest("telegram", AdapterType::Channel), None);

        let entry = registry.get("telegram").unwrap();
        assert_eq!(entry.manifest.name, "telegram");
        assert_eq!(entry.status, PluginStatus::Enabled);
    }

    #[test]
    fn get_enabled_filters_by_type_and_status() {
        let mut registry = PluginRegistry::new();
        registry.register(test_manifest("telegram", AdapterType::Channel), None);
        registry.register(test_manifest("anthropic", AdapterType::Provider), None);
        registry.register_with_status(
            test_manifest("disabled-chan", AdapterType::Channel),
            None,
            PluginStatus::Disabled,
        );

        let channels = registry.get_enabled(AdapterType::Channel);
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].manifest.name, "telegram");

        let providers = registry.get_enabled(AdapterType::Provider);
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].manifest.name, "anthropic");
    }

    #[test]
    fn set_enabled_toggles_status() {
        let mut registry = PluginRegistry::new();
        registry.register(test_manifest("telegram", AdapterType::Channel), None);

        assert_eq!(registry.get("telegram").unwrap().status, PluginStatus::Enabled);

        registry.set_enabled("telegram", false).unwrap();
        assert_eq!(registry.get("telegram").unwrap().status, PluginStatus::Disabled);

        registry.set_enabled("telegram", true).unwrap();
        assert_eq!(registry.get("telegram").unwrap().status, PluginStatus::Enabled);
    }

    #[test]
    fn set_enabled_returns_error_for_unknown_plugin() {
        let mut registry = PluginRegistry::new();
        let result = registry.set_enabled("nonexistent", true);
        assert!(result.is_err());
    }

    #[test]
    fn list_all_returns_sorted() {
        let mut registry = PluginRegistry::new();
        registry.register(test_manifest("zebra", AdapterType::Channel), None);
        registry.register(test_manifest("alpha", AdapterType::Provider), None);
        registry.register(test_manifest("middle", AdapterType::Storage), None);

        let all = registry.list_all();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].manifest.name, "alpha");
        assert_eq!(all[1].manifest.name, "middle");
        assert_eq!(all[2].manifest.name, "zebra");
    }

    #[test]
    fn len_and_is_empty() {
        let mut registry = PluginRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);

        registry.register(test_manifest("test", AdapterType::Channel), None);
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
    }
}
