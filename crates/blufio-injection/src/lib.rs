// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Prompt injection defense system for the Blufio agent framework.
//!
//! Provides a 5-layer defense pipeline:
//! - **L1** (`classifier`): Regex-based pattern detection with confidence scoring
//! - **L3** (`boundary`): HMAC-SHA256 boundary tokens for content zone integrity
//! - **L4** (`output_screen`): Output screening for credential leaks and injection relay
//! - **L5** (`hitl`): Human-in-the-loop confirmation for high-risk operations
//!
//! The pipeline coordinator (`pipeline`) ties all layers together with
//! correlation IDs and cross-layer escalation.

pub mod config;
pub mod patterns;
pub mod classifier;
pub mod events;
pub mod metrics;
pub mod output_screen;
pub mod boundary;
pub mod hitl;
pub mod pipeline;
