// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! OpenRouter provider adapter for the Blufio agent framework.
//!
//! This crate implements [`ProviderAdapter`] for the OpenRouter API,
//! providing both single-shot completion and streaming SSE responses with
//! tool calling, vision, and provider preference routing.
//!
//! OpenRouter uses an OpenAI-compatible API format with additional features
//! like provider fallback ordering via the `provider` request field,
//! and analytics via X-Title and HTTP-Referer headers.

pub mod client;
pub mod sse;
pub mod types;
