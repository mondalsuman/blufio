// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Email channel adapter for the Blufio agent framework.
//!
//! Uses IMAP for incoming messages and SMTP (lettre) for outgoing.
//! Supports thread-to-session mapping via In-Reply-To/References headers,
//! quoted-text stripping, and HTML-to-plaintext conversion.

pub mod imap;
pub mod parsing;
pub mod smtp;

/// Email channel adapter.
///
/// TODO: Implement `ChannelAdapter` + `PluginAdapter` traits (Plan 02).
pub struct EmailChannel;
