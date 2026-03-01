// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Provider adapter trait for LLM provider integrations (Anthropic, OpenAI, etc.).

use std::pin::Pin;

use async_trait::async_trait;
use futures_core::Stream;

use crate::error::BlufioError;
use crate::traits::adapter::PluginAdapter;
use crate::types::{ProviderRequest, ProviderResponse, ProviderStreamChunk};

/// Adapter for LLM provider integrations.
///
/// Provider adapters handle communication with language model APIs,
/// supporting both single-shot completion and streaming responses.
#[async_trait]
pub trait ProviderAdapter: PluginAdapter {
    /// Sends a completion request and returns the full response.
    async fn complete(
        &self,
        request: ProviderRequest,
    ) -> Result<ProviderResponse, BlufioError>;

    /// Sends a completion request and returns a stream of response chunks.
    async fn stream(
        &self,
        request: ProviderRequest,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<ProviderStreamChunk, BlufioError>> + Send>>,
        BlufioError,
    >;
}
