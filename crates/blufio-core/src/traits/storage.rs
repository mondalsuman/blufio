// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Storage adapter trait for persistence backends (SQLite, etc.).

use async_trait::async_trait;

use crate::error::BlufioError;
use crate::traits::adapter::PluginAdapter;

/// Adapter for storage and persistence backends.
///
/// Storage adapters manage the lifecycle of database connections
/// and provide the foundation for conversation history, configuration
/// persistence, and other stateful operations.
#[async_trait]
pub trait StorageAdapter: PluginAdapter {
    /// Initializes the storage backend (migrations, connection pool, etc.).
    async fn initialize(&self) -> Result<(), BlufioError>;

    /// Closes the storage backend, flushing pending writes and releasing connections.
    async fn close(&self) -> Result<(), BlufioError>;
}
