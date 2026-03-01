// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Skill manifest parsing from TOML.
//!
//! A skill manifest (`skill.toml`) describes a skill's identity, capabilities,
//! and resource limits. The manifest is parsed at install time and used by the
//! WASM sandbox to configure capability gating and resource controls.

use std::path::Path;

use blufio_core::BlufioError;
use blufio_core::types::{
    FilesystemCapability, NetworkCapability, SkillCapabilities, SkillManifest, SkillResources,
};
use serde::Deserialize;

// --- TOML intermediate structs ---

/// Top-level structure of a skill.toml file.
#[derive(Debug, Deserialize)]
struct ManifestFile {
    skill: SkillSection,
    #[serde(default)]
    capabilities: CapabilitiesSection,
    #[serde(default)]
    resources: ResourcesSection,
    #[serde(default)]
    wasm: WasmSection,
}

/// The [skill] section of the manifest.
#[derive(Debug, Deserialize)]
struct SkillSection {
    name: String,
    version: String,
    description: String,
    #[serde(default)]
    author: Option<String>,
}

/// The [capabilities] section of the manifest.
#[derive(Debug, Default, Deserialize)]
struct CapabilitiesSection {
    #[serde(default)]
    network: Option<NetworkSection>,
    #[serde(default)]
    filesystem: Option<FilesystemSection>,
    #[serde(default)]
    env: Vec<String>,
}

/// The [capabilities.network] section.
#[derive(Debug, Deserialize)]
struct NetworkSection {
    #[serde(default)]
    domains: Vec<String>,
}

/// The [capabilities.filesystem] section.
#[derive(Debug, Deserialize)]
struct FilesystemSection {
    #[serde(default)]
    read: Vec<String>,
    #[serde(default)]
    write: Vec<String>,
}

/// The [resources] section.
#[derive(Debug, Default, Deserialize)]
struct ResourcesSection {
    #[serde(default)]
    fuel: Option<u64>,
    #[serde(default)]
    memory_mb: Option<u32>,
    #[serde(default)]
    epoch_timeout_secs: Option<u64>,
}

/// The [wasm] section.
#[derive(Debug, Deserialize)]
struct WasmSection {
    #[serde(default = "default_entry")]
    entry: String,
}

impl Default for WasmSection {
    fn default() -> Self {
        Self {
            entry: default_entry(),
        }
    }
}

fn default_entry() -> String {
    "skill.wasm".to_string()
}

// --- Public API ---

/// Parses a skill manifest from a TOML string.
///
/// Validates that the skill name is non-empty and contains only alphanumeric
/// characters and hyphens.
pub fn parse_manifest(toml_content: &str) -> Result<SkillManifest, BlufioError> {
    let manifest_file: ManifestFile =
        toml::from_str(toml_content).map_err(|e| BlufioError::Skill {
            message: format!("failed to parse skill manifest: {e}"),
            source: Some(Box::new(e)),
        })?;

    // Validate skill name.
    let name = &manifest_file.skill.name;
    if name.is_empty() {
        return Err(BlufioError::Skill {
            message: "skill name must not be empty".to_string(),
            source: None,
        });
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(BlufioError::Skill {
            message: format!(
                "skill name '{name}' contains invalid characters (only alphanumeric, hyphens, underscores allowed)"
            ),
            source: None,
        });
    }

    // Convert capabilities.
    let capabilities = SkillCapabilities {
        network: manifest_file
            .capabilities
            .network
            .map(|n| NetworkCapability { domains: n.domains }),
        filesystem: manifest_file
            .capabilities
            .filesystem
            .map(|f| FilesystemCapability {
                read: f.read,
                write: f.write,
            }),
        env: manifest_file.capabilities.env,
    };

    // Convert resources with defaults.
    let resources = SkillResources {
        fuel: manifest_file.resources.fuel.unwrap_or(1_000_000_000),
        memory_mb: manifest_file.resources.memory_mb.unwrap_or(16),
        epoch_timeout_secs: manifest_file.resources.epoch_timeout_secs.unwrap_or(5),
    };

    Ok(SkillManifest {
        name: manifest_file.skill.name,
        version: manifest_file.skill.version,
        description: manifest_file.skill.description,
        author: manifest_file.skill.author,
        capabilities,
        resources,
        wasm_entry: manifest_file.wasm.entry,
    })
}

/// Loads and parses a skill manifest from a file path.
pub fn load_manifest(path: &Path) -> Result<SkillManifest, BlufioError> {
    let content = std::fs::read_to_string(path).map_err(|e| BlufioError::Skill {
        message: format!("failed to read manifest file '{}': {e}", path.display()),
        source: Some(Box::new(e)),
    })?;
    parse_manifest(&content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_manifest_valid_full() {
        let toml = r#"
[skill]
name = "weather-lookup"
version = "0.1.0"
description = "Looks up current weather for a city"
author = "Test Author"

[capabilities]
env = ["WEATHER_API_KEY"]

[capabilities.network]
domains = ["api.weather.com", "api.openweathermap.org"]

[capabilities.filesystem]
read = ["/tmp/cache"]
write = ["/tmp/cache"]

[resources]
fuel = 500_000_000
memory_mb = 8
epoch_timeout_secs = 3

[wasm]
entry = "weather.wasm"
"#;
        let manifest = parse_manifest(toml).unwrap();
        assert_eq!(manifest.name, "weather-lookup");
        assert_eq!(manifest.version, "0.1.0");
        assert_eq!(manifest.description, "Looks up current weather for a city");
        assert_eq!(manifest.author.as_deref(), Some("Test Author"));
        assert_eq!(manifest.wasm_entry, "weather.wasm");

        // Capabilities
        let network = manifest.capabilities.network.as_ref().unwrap();
        assert_eq!(network.domains.len(), 2);
        assert_eq!(network.domains[0], "api.weather.com");

        let fs = manifest.capabilities.filesystem.as_ref().unwrap();
        assert_eq!(fs.read, vec!["/tmp/cache"]);
        assert_eq!(fs.write, vec!["/tmp/cache"]);

        assert_eq!(manifest.capabilities.env, vec!["WEATHER_API_KEY"]);

        // Resources
        assert_eq!(manifest.resources.fuel, 500_000_000);
        assert_eq!(manifest.resources.memory_mb, 8);
        assert_eq!(manifest.resources.epoch_timeout_secs, 3);
    }

    #[test]
    fn parse_manifest_minimal() {
        let toml = r#"
[skill]
name = "hello"
version = "0.1.0"
description = "A minimal skill"
"#;
        let manifest = parse_manifest(toml).unwrap();
        assert_eq!(manifest.name, "hello");
        assert!(manifest.capabilities.network.is_none());
        assert!(manifest.capabilities.filesystem.is_none());
        assert!(manifest.capabilities.env.is_empty());
        assert_eq!(manifest.wasm_entry, "skill.wasm");
    }

    #[test]
    fn parse_manifest_default_resources() {
        let toml = r#"
[skill]
name = "test"
version = "1.0.0"
description = "Test default resources"
"#;
        let manifest = parse_manifest(toml).unwrap();
        assert_eq!(manifest.resources.fuel, 1_000_000_000);
        assert_eq!(manifest.resources.memory_mb, 16);
        assert_eq!(manifest.resources.epoch_timeout_secs, 5);
    }

    #[test]
    fn parse_manifest_missing_name_fails() {
        let toml = r#"
[skill]
version = "0.1.0"
description = "No name"
"#;
        let result = parse_manifest(toml);
        assert!(result.is_err());
    }

    #[test]
    fn parse_manifest_empty_name_fails() {
        let toml = r#"
[skill]
name = ""
version = "0.1.0"
description = "Empty name"
"#;
        let result = parse_manifest(toml);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must not be empty"));
    }

    #[test]
    fn parse_manifest_invalid_name_chars_fails() {
        let toml = r#"
[skill]
name = "bad skill name!"
version = "0.1.0"
description = "Invalid chars"
"#;
        let result = parse_manifest(toml);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid characters"));
    }

    #[test]
    fn parse_manifest_network_capability() {
        let toml = r#"
[skill]
name = "api-client"
version = "0.1.0"
description = "API client"

[capabilities.network]
domains = ["example.com"]
"#;
        let manifest = parse_manifest(toml).unwrap();
        let network = manifest.capabilities.network.unwrap();
        assert_eq!(network.domains, vec!["example.com"]);
    }

    #[test]
    fn parse_manifest_filesystem_capability() {
        let toml = r#"
[skill]
name = "file-reader"
version = "0.1.0"
description = "File reader"

[capabilities.filesystem]
read = ["/data", "/config"]
write = ["/output"]
"#;
        let manifest = parse_manifest(toml).unwrap();
        let fs = manifest.capabilities.filesystem.unwrap();
        assert_eq!(fs.read, vec!["/data", "/config"]);
        assert_eq!(fs.write, vec!["/output"]);
    }

    #[test]
    fn parse_manifest_empty_capabilities_valid() {
        let toml = r#"
[skill]
name = "no-perms"
version = "0.1.0"
description = "No permissions needed"

[capabilities]
"#;
        let manifest = parse_manifest(toml).unwrap();
        assert!(manifest.capabilities.network.is_none());
        assert!(manifest.capabilities.filesystem.is_none());
        assert!(manifest.capabilities.env.is_empty());
    }
}
