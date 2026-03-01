// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Static zone: loads and caches the system prompt, formatted as
//! cache-aligned blocks for Anthropic prompt caching.

use blufio_config::model::AgentConfig;
use blufio_core::error::BlufioError;
use tracing::info;

/// The static zone holds the system prompt text and provides it
/// as structured JSON blocks with cache_control markers.
#[derive(Debug, Clone)]
pub struct StaticZone {
    /// The loaded system prompt text.
    system_prompt: String,
}

impl StaticZone {
    /// Creates a new static zone by loading the system prompt from config.
    ///
    /// # Priority
    /// 1. `config.system_prompt_file` -- reads from disk
    /// 2. `config.system_prompt` -- inline string
    /// 3. Default: "You are {name}, a concise personal assistant."
    pub async fn new(config: &AgentConfig) -> Result<Self, BlufioError> {
        let system_prompt = load_system_prompt(config).await?;
        Ok(Self { system_prompt })
    }

    /// Returns the system prompt as a JSON array of structured blocks
    /// with `cache_control: {"type": "ephemeral"}` on the last block.
    ///
    /// Format:
    /// ```json
    /// [{"type": "text", "text": "<system prompt>", "cache_control": {"type": "ephemeral"}}]
    /// ```
    pub fn system_blocks(&self) -> serde_json::Value {
        serde_json::json!([{
            "type": "text",
            "text": self.system_prompt,
            "cache_control": {"type": "ephemeral"}
        }])
    }

    /// Returns the raw system prompt text.
    pub fn system_prompt(&self) -> &str {
        &self.system_prompt
    }
}

/// Loads the system prompt following config priority: file > inline > default.
async fn load_system_prompt(config: &AgentConfig) -> Result<String, BlufioError> {
    // Priority 1: file path
    if let Some(ref file_path) = config.system_prompt_file {
        match tokio::fs::read_to_string(file_path).await {
            Ok(content) => {
                let trimmed = content.trim().to_string();
                if !trimmed.is_empty() {
                    info!(path = file_path.as_str(), "loaded system prompt from file");
                    return Ok(trimmed);
                }
            }
            Err(e) => {
                tracing::warn!(
                    path = file_path.as_str(),
                    error = %e,
                    "failed to read system prompt file, falling back"
                );
            }
        }
    }

    // Priority 2: inline string
    if let Some(ref prompt) = config.system_prompt
        && !prompt.is_empty()
    {
        return Ok(prompt.clone());
    }

    // Priority 3: default
    Ok(format!(
        "You are {}, a concise personal assistant.",
        config.name
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn static_zone_default_prompt() {
        let config = AgentConfig::default();
        let zone = StaticZone::new(&config).await.unwrap();
        assert!(zone.system_prompt().contains("blufio"));
        assert!(zone.system_prompt().contains("concise personal assistant"));
    }

    #[tokio::test]
    async fn static_zone_inline_prompt() {
        let config = AgentConfig {
            system_prompt: Some("Custom prompt.".into()),
            ..Default::default()
        };
        let zone = StaticZone::new(&config).await.unwrap();
        assert_eq!(zone.system_prompt(), "Custom prompt.");
    }

    #[tokio::test]
    async fn static_zone_file_prompt() {
        let dir = std::env::temp_dir().join("blufio-context-test");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("sys-prompt.md");
        std::fs::write(&file_path, "File-based prompt.").unwrap();

        let config = AgentConfig {
            system_prompt: Some("Inline.".into()),
            system_prompt_file: Some(file_path.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let zone = StaticZone::new(&config).await.unwrap();
        assert_eq!(zone.system_prompt(), "File-based prompt.");

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[tokio::test]
    async fn system_blocks_format() {
        let config = AgentConfig {
            system_prompt: Some("Test prompt.".into()),
            ..Default::default()
        };
        let zone = StaticZone::new(&config).await.unwrap();
        let blocks = zone.system_blocks();

        assert!(blocks.is_array());
        let arr = blocks.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["type"], "text");
        assert_eq!(arr[0]["text"], "Test prompt.");
        assert_eq!(arr[0]["cache_control"]["type"], "ephemeral");
    }
}
