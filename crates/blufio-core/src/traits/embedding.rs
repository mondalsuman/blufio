// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Embedding adapter trait for vector embedding generation.

use async_trait::async_trait;

use crate::error::BlufioError;
use crate::traits::adapter::PluginAdapter;
use crate::types::{EmbeddingInput, EmbeddingOutput};

/// Adapter for generating vector embeddings from text or other inputs.
///
/// Embedding adapters power semantic search and memory retrieval
/// by converting content into vector representations.
#[async_trait]
pub trait EmbeddingAdapter: PluginAdapter {
    /// Generates embeddings for the given input.
    async fn embed(&self, input: EmbeddingInput) -> Result<EmbeddingOutput, BlufioError>;
}
