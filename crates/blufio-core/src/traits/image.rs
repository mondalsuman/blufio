// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Image generation adapter trait.

use async_trait::async_trait;

use crate::error::BlufioError;
use crate::traits::adapter::PluginAdapter;
use crate::types::{ImageRequest, ImageResponse};

/// Adapter for image generation providers (DALL-E, Stable Diffusion, etc.).
#[async_trait]
pub trait ImageAdapter: PluginAdapter {
    /// Generate images from a text prompt.
    async fn generate(&self, request: ImageRequest) -> Result<ImageResponse, BlufioError>;
}
