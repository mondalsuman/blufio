// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Configuration system for the Blufio agent framework.
//!
//! Provides TOML configuration parsing with strict validation (`deny_unknown_fields`),
//! XDG file hierarchy lookup, environment variable overrides, and Elm-style diagnostic
//! error rendering with typo suggestions.
//!
//! # Usage
//!
//! ```no_run
//! use blufio_config::load_and_validate;
//!
//! let config = load_and_validate().expect("config errors");
//! println!("Agent name: {}", config.agent.name);
//! ```

pub mod diagnostic;
pub mod loader;
pub mod model;
pub mod validation;

pub use diagnostic::{render_errors, ConfigError};
pub use loader::{load_config, load_config_from_path, load_config_from_str};
pub use model::BlufioConfig;

/// Load configuration from the XDG hierarchy and validate it.
///
/// This is the high-level entry point that:
/// 1. Loads config from TOML files + env vars via Figment
/// 2. On success: runs post-deserialization validation
/// 3. On Figment error: converts to rich miette diagnostics with typo suggestions
///
/// Returns either a valid `BlufioConfig` or a list of diagnostic errors.
pub fn load_and_validate() -> Result<BlufioConfig, Vec<ConfigError>> {
    match loader::load_config() {
        Ok(config) => {
            validation::validate_config(&config)?;
            Ok(config)
        }
        Err(err) => {
            // Read TOML source files for error source span information
            let toml_sources = collect_toml_sources();
            Err(diagnostic::figment_to_config_errors(err, &toml_sources))
        }
    }
}

/// Load configuration from a specific TOML string and validate it.
///
/// Useful for testing and explicit configuration.
pub fn load_and_validate_str(toml_content: &str) -> Result<BlufioConfig, Vec<ConfigError>> {
    match loader::load_config_from_str(toml_content) {
        Ok(config) => {
            validation::validate_config(&config)?;
            Ok(config)
        }
        Err(err) => {
            let sources = vec![("<inline>".to_string(), toml_content.to_string())];
            Err(diagnostic::figment_to_config_errors(err, &sources))
        }
    }
}

/// Collect TOML source file contents for error span resolution.
fn collect_toml_sources() -> Vec<(String, String)> {
    let mut sources = Vec::new();

    // Local config
    if let Ok(content) = std::fs::read_to_string("blufio.toml") {
        let path = std::env::current_dir()
            .map(|d| d.join("blufio.toml").display().to_string())
            .unwrap_or_else(|_| "blufio.toml".to_string());
        sources.push((path, content));
    }

    // XDG user config
    if let Some(config_dir) = dirs::config_dir() {
        let path = config_dir.join("blufio/blufio.toml");
        if let Ok(content) = std::fs::read_to_string(&path) {
            sources.push((path.display().to_string(), content));
        }
    }

    // System config
    let system_path = std::path::Path::new("/etc/blufio/blufio.toml");
    if let Ok(content) = std::fs::read_to_string(system_path) {
        sources.push((system_path.display().to_string(), content));
    }

    sources
}
