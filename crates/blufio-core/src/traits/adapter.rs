// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Base adapter trait that all plugin adapters must implement.

use async_trait::async_trait;

use crate::error::BlufioError;
use crate::types::{AdapterType, HealthStatus};

/// The base trait for all Blufio plugin adapters.
///
/// Every adapter (channel, provider, storage, etc.) must implement this trait,
/// which provides identity, lifecycle, and health check capabilities.
#[async_trait]
pub trait PluginAdapter: Send + Sync + 'static {
    /// Returns the human-readable name of this adapter instance.
    fn name(&self) -> &str;

    /// Returns the semantic version of this adapter.
    fn version(&self) -> semver::Version;

    /// Returns the type of adapter (channel, provider, storage, etc.).
    fn adapter_type(&self) -> AdapterType;

    /// Performs a health check and returns the adapter's current status.
    async fn health_check(&self) -> Result<HealthStatus, BlufioError>;

    /// Gracefully shuts down the adapter, releasing any held resources.
    async fn shutdown(&self) -> Result<(), BlufioError>;
}
