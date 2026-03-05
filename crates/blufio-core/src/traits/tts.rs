// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! TTS (text-to-speech) adapter trait for audio synthesis providers.

use async_trait::async_trait;

use crate::error::BlufioError;
use crate::traits::adapter::PluginAdapter;
use crate::types::{TtsRequest, TtsResponse};

/// Adapter for text-to-speech providers (OpenAI TTS, ElevenLabs, etc.).
#[async_trait]
pub trait TtsAdapter: PluginAdapter {
    /// Synthesize text to audio.
    async fn synthesize(&self, request: TtsRequest) -> Result<TtsResponse, BlufioError>;

    /// List available voices for this provider.
    async fn list_voices(&self) -> Result<Vec<String>, BlufioError>;
}
