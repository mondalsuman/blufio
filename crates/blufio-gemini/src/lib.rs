// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Google Gemini provider adapter for the Blufio agent framework.
//!
//! This crate implements [`ProviderAdapter`] for Google's native Gemini API,
//! providing both single-shot completion and streaming responses via
//! streamGenerateContent with function calling support.

pub mod client;
pub mod stream;
pub mod types;
