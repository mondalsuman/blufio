// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Transcription (speech-to-text) adapter trait.

use async_trait::async_trait;

use crate::error::BlufioError;
use crate::traits::adapter::PluginAdapter;
use crate::types::{TranscriptionRequest, TranscriptionResponse};

/// Adapter for speech-to-text providers (Whisper, Deepgram, etc.).
#[async_trait]
pub trait TranscriptionAdapter: PluginAdapter {
    /// Transcribe audio to text.
    async fn transcribe(
        &self,
        request: TranscriptionRequest,
    ) -> Result<TranscriptionResponse, BlufioError>;
}
