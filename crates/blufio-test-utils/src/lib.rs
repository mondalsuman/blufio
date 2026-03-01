// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Test utilities for Blufio integration tests.
//!
//! Provides mock adapters and test harness infrastructure for fast,
//! deterministic, CI-runnable tests without external services.
//!
//! # Components
//!
//! - [`MockProvider`] - Mock LLM provider with pre-configured responses
//! - [`MockChannel`] - Mock messaging channel with message injection and capture

pub mod harness;
pub mod mock_channel;
pub mod mock_provider;

pub use harness::TestHarness;
pub use mock_channel::MockChannel;
pub use mock_provider::MockProvider;
