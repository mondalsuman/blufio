// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! SMS channel adapter (Twilio) for the Blufio agent framework.
//!
//! Uses Twilio REST API for outbound messages and webhook for inbound.
//! Validates X-Twilio-Signature HMAC on incoming webhooks for security.

pub mod api;
pub mod types;
pub mod webhook;

/// SMS channel adapter.
///
/// TODO: Implement `ChannelAdapter` + `PluginAdapter` traits (Plan 03).
pub struct SmsChannel;
