// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! GDPR configuration types.
//!
//! Defines [`GdprConfig`] for the `[gdpr]` TOML section. Validated on first use
//! (when running GDPR commands), not at server startup.

use serde::{Deserialize, Serialize};

/// GDPR tooling configuration.
///
/// Controls export directory, auto-export-before-erasure behavior, and default
/// export format. All fields are optional with sensible defaults.
///
/// # Example TOML
///
/// ```toml
/// [gdpr]
/// export_dir = "/var/lib/blufio/gdpr-exports"
/// export_before_erasure = true
/// default_format = "json"
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GdprConfig {
    /// Custom export directory. When `None`, defaults to `{data_dir}/exports/`.
    #[serde(default)]
    pub export_dir: Option<String>,

    /// Whether to automatically export user data before erasure.
    ///
    /// When `true` (the default), `blufio gdpr erase` will export the user's
    /// data before performing deletion, unless `--skip-export` is passed.
    #[serde(default = "default_export_before_erasure")]
    pub export_before_erasure: bool,

    /// Default export format (`"json"` or `"csv"`).
    #[serde(default = "default_gdpr_format")]
    pub default_format: String,
}

impl Default for GdprConfig {
    fn default() -> Self {
        Self {
            export_dir: None,
            export_before_erasure: default_export_before_erasure(),
            default_format: default_gdpr_format(),
        }
    }
}

fn default_export_before_erasure() -> bool {
    true
}

fn default_gdpr_format() -> String {
    "json".to_string()
}
