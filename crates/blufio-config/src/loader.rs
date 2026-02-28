// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Configuration loader using Figment for layered config merging.
//!
//! Supports XDG hierarchy: `./blufio.toml` > `~/.config/blufio/blufio.toml` > `/etc/blufio/blufio.toml`
//! with environment variable overrides via `BLUFIO_` prefix.

#![allow(clippy::result_large_err)] // figment::Error is external and cannot be boxed without wrapper

use std::path::Path;

use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};

use crate::model::BlufioConfig;

/// Load configuration from the standard XDG hierarchy with env var overrides.
///
/// Merge order (later overrides earlier):
/// 1. Compiled defaults
/// 2. `/etc/blufio/blufio.toml` (system-wide)
/// 3. `~/.config/blufio/blufio.toml` (user XDG config)
/// 4. `./blufio.toml` (local directory)
/// 5. `BLUFIO_*` environment variables
pub fn load_config() -> Result<BlufioConfig, figment::Error> {
    Figment::new()
        .merge(Serialized::defaults(BlufioConfig::default()))
        .merge(Toml::file("/etc/blufio/blufio.toml"))
        .merge(Toml::file(
            dirs::config_dir()
                .map(|d| d.join("blufio/blufio.toml"))
                .unwrap_or_default(),
        ))
        .merge(Toml::file("blufio.toml"))
        .merge(env_provider())
        .extract()
}

/// Load configuration from a specific TOML file path only (no XDG lookup).
///
/// Used for testing and explicit config file specification.
pub fn load_config_from_str(toml_content: &str) -> Result<BlufioConfig, figment::Error> {
    Figment::new()
        .merge(Serialized::defaults(BlufioConfig::default()))
        .merge(Toml::string(toml_content))
        .extract()
}

/// Load configuration from a specific file path with env var overrides.
pub fn load_config_from_path(path: &Path) -> Result<BlufioConfig, figment::Error> {
    Figment::new()
        .merge(Serialized::defaults(BlufioConfig::default()))
        .merge(Toml::file(path))
        .merge(env_provider())
        .extract()
}

/// Build the Figment used internally for config loading (exposed for diagnostic use).
///
/// Returns the Figment before extraction so callers can inspect metadata.
pub fn build_figment() -> Figment {
    Figment::new()
        .merge(Serialized::defaults(BlufioConfig::default()))
        .merge(Toml::file("/etc/blufio/blufio.toml"))
        .merge(Toml::file(
            dirs::config_dir()
                .map(|d| d.join("blufio/blufio.toml"))
                .unwrap_or_default(),
        ))
        .merge(Toml::file("blufio.toml"))
        .merge(env_provider())
}

/// Create the environment variable provider using explicit `map()` for section-to-dot mapping.
///
/// CRITICAL: Uses `Env::map()` NOT `Env::split("_")` to avoid ambiguity with
/// underscore-containing key names. For example, `BLUFIO_TELEGRAM_BOT_TOKEN` must
/// map to `telegram.bot_token`, not `telegram.bot.token`.
fn env_provider() -> Env {
    Env::prefixed("BLUFIO_").map(|key| {
        // `key` is the lowercased env var name with prefix stripped.
        // Example: BLUFIO_TELEGRAM_BOT_TOKEN -> "telegram_bot_token"
        let key_str = key.as_str();
        let mapped = key_str
            .replacen("agent_", "agent.", 1)
            .replacen("telegram_", "telegram.", 1)
            .replacen("anthropic_", "anthropic.", 1)
            .replacen("storage_", "storage.", 1)
            .replacen("security_", "security.", 1)
            .replacen("cost_", "cost.", 1);
        mapped.into()
    })
}
