// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ollama native provider adapter for the Blufio agent framework.
//!
//! This crate implements [`ProviderAdapter`] for Ollama's native `/api/chat`
//! endpoint with NDJSON streaming, tool calling, and local model discovery.
//!
//! Key differences from cloud providers:
//! - No API key required (Ollama runs locally)
//! - NDJSON streaming (not SSE)
//! - Native `/api/chat` endpoint (not OpenAI compatibility shim)
//! - `/api/tags` for local model discovery

pub mod client;
pub mod stream;
pub mod types;
