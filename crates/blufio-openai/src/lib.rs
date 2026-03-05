// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! OpenAI provider adapter for the Blufio agent framework.
//!
//! This crate implements [`ProviderAdapter`] for the OpenAI Chat Completions API,
//! providing both single-shot completion and streaming SSE responses with
//! tool calling, vision, and structured outputs.

pub mod client;
pub mod sse;
pub mod types;
