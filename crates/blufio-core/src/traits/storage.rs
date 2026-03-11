// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Storage adapter trait for persistence backends (SQLite, etc.).

use async_trait::async_trait;

use crate::error::BlufioError;
use crate::traits::adapter::PluginAdapter;
use crate::types::{Message, QueueEntry, Session};

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

    // --- Session operations ---

    /// Create a new session.
    async fn create_session(&self, session: &Session) -> Result<(), BlufioError>;

    /// Get a session by ID.
    async fn get_session(&self, id: &str) -> Result<Option<Session>, BlufioError>;

    /// List sessions, optionally filtered by state.
    async fn list_sessions(&self, state: Option<&str>) -> Result<Vec<Session>, BlufioError>;

    /// Update a session's state.
    async fn update_session_state(&self, id: &str, state: &str) -> Result<(), BlufioError>;

    // --- Message operations ---

    /// Insert a new message into a session.
    async fn insert_message(&self, message: &Message) -> Result<(), BlufioError>;

    /// Get messages for a session in chronological order, with optional limit.
    async fn get_messages(
        &self,
        session_id: &str,
        limit: Option<i64>,
    ) -> Result<Vec<Message>, BlufioError>;

    /// Delete specific messages by their IDs within a session.
    ///
    /// Returns the number of messages actually deleted.
    async fn delete_messages_by_ids(
        &self,
        session_id: &str,
        message_ids: &[String],
    ) -> Result<usize, BlufioError>;

    // --- Queue operations ---

    /// Enqueue a new item. Returns the auto-generated queue entry ID.
    async fn enqueue(&self, queue_name: &str, payload: &str) -> Result<i64, BlufioError>;

    /// Dequeue the next pending entry from the named queue.
    async fn dequeue(&self, queue_name: &str) -> Result<Option<QueueEntry>, BlufioError>;

    /// Acknowledge successful processing of a queue entry.
    async fn ack(&self, id: i64) -> Result<(), BlufioError>;

    /// Mark a queue entry as failed (increments attempts, may retry or mark permanently failed).
    async fn fail(&self, id: i64) -> Result<(), BlufioError>;

    // --- Classification operations ---

    /// Get classification level for an entity.
    ///
    /// `entity_type` is one of "memory", "message", or "session".
    /// Returns `None` if the entity does not exist.
    async fn get_entity_classification(
        &self,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<Option<String>, BlufioError>;

    /// Set classification level on an entity.
    ///
    /// Returns `true` if the entity was found and updated, `false` otherwise.
    async fn set_entity_classification(
        &self,
        entity_type: &str,
        entity_id: &str,
        level: &str,
    ) -> Result<bool, BlufioError>;

    /// List entities with their classification levels, optionally filtered by level.
    ///
    /// Returns `Vec<(id, classification_string)>` pairs.
    async fn list_entities_by_classification(
        &self,
        entity_type: &str,
        level: Option<&str>,
    ) -> Result<Vec<(String, String)>, BlufioError>;

    /// Bulk update classification levels with filters.
    ///
    /// Returns `(total, succeeded, failed, errors)`.
    #[allow(clippy::too_many_arguments)]
    async fn bulk_update_classification(
        &self,
        entity_type: &str,
        new_level: &str,
        current_level: Option<&str>,
        session_id: Option<&str>,
        from_date: Option<&str>,
        to_date: Option<&str>,
        pattern: Option<&str>,
        dry_run: bool,
    ) -> Result<(usize, usize, usize, Vec<String>), BlufioError>;
}
