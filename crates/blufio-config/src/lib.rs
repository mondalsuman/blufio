// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Configuration system for the Blufio agent framework.
//!
//! Provides TOML configuration parsing with strict validation (`deny_unknown_fields`),
//! XDG file hierarchy lookup, environment variable overrides, and Elm-style diagnostic
//! error rendering with typo suggestions.

pub mod loader;
pub mod model;

pub use loader::{load_config, load_config_from_path, load_config_from_str};
pub use model::BlufioConfig;
