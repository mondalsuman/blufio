// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! iMessage channel adapter for the Blufio agent framework.
//!
//! Integrates with BlueBubbles server running on macOS for sending and
//! receiving iMessage conversations. Experimental -- requires macOS host
//! with BlueBubbles installed.

pub mod api;
pub mod types;
pub mod webhook;

/// iMessage channel adapter.
///
/// TODO: Implement `ChannelAdapter` + `PluginAdapter` traits (Plan 02).
pub struct IMessageChannel;
