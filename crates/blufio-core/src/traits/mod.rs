// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Adapter trait definitions for the Blufio plugin architecture.
//!
//! All adapters extend the [`PluginAdapter`] base trait and use
//! `#[async_trait]` for dynamic dispatch compatibility.

pub mod adapter;
pub mod auth;
pub mod channel;
pub mod embedding;
pub mod observability;
pub mod provider;
pub mod skill;
pub mod storage;

// Re-export all traits at the traits module level for convenience.
pub use adapter::PluginAdapter;
pub use auth::AuthAdapter;
pub use channel::ChannelAdapter;
pub use embedding::EmbeddingAdapter;
pub use observability::ObservabilityAdapter;
pub use provider::ProviderAdapter;
pub use skill::SkillRuntimeAdapter;
pub use storage::StorageAdapter;
