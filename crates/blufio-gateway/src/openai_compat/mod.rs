// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! OpenAI-compatible API endpoints for the gateway.
//!
//! Provides drop-in OpenAI API compatibility so external callers can use
//! standard OpenAI SDKs by pointing `base_url` at the Blufio gateway.
//!
//! Wire types in this module are completely separate from `blufio-openai`
//! (which is the OpenAI *client* crate). No Anthropic-specific field names
//! (e.g., `stop_reason`) appear in these external-facing types.

pub mod handlers;
pub mod responses;
pub mod responses_types;
pub mod stream;
pub mod tools;
pub mod tools_types;
pub mod types;
