// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Plugin registry, manifest parser, and built-in adapter catalog.
//!
//! The plugin system manages compiled-in adapter modules (Channel, Provider,
//! Storage, etc.) through a registry pattern. Each plugin has a manifest
//! describing its metadata, capabilities, and required configuration keys.

pub mod catalog;
pub mod manifest;
pub mod registry;

pub use catalog::{builtin_catalog, search_catalog};
pub use manifest::{parse_plugin_manifest, PluginManifest};
pub use registry::{PluginEntry, PluginFactory, PluginRegistry, PluginStatus};
