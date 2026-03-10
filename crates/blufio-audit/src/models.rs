// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Core data types for the audit trail subsystem.
//!
//! - [`AuditEntry`] -- a persisted audit record with hash chain fields.
//! - [`PendingEntry`] -- an entry awaiting hash computation and persistence.
//! - [`AuditErasureReport`] -- result of a GDPR erasure operation.
//! - [`AuditError`] -- crate-local error type for audit operations.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A persisted audit trail entry with all fields populated.
///
/// The `entry_hash` is computed over immutable fields only:
/// `prev_hash`, `timestamp`, `event_type`, `action`, `resource_type`, `resource_id`.
///
/// PII fields (`actor`, `session_id`, `details_json`) are excluded from the hash
/// to support GDPR erasure without breaking the chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Auto-incremented row ID.
    pub id: i64,
    /// SHA-256 hash of immutable fields (hex-encoded, 64 chars).
    pub entry_hash: String,
    /// Hash of the previous entry (or GENESIS_HASH for the first entry).
    pub prev_hash: String,
    /// ISO 8601 timestamp of when the event occurred.
    pub timestamp: String,
    /// Dot-separated event type (e.g., "session.created").
    pub event_type: String,
    /// Action performed (e.g., "create", "delete", "update").
    pub action: String,
    /// Type of resource affected (e.g., "session", "memory").
    pub resource_type: String,
    /// Identifier of the resource affected.
    pub resource_id: String,
    /// Actor who performed the action (e.g., "user:123", "system").
    pub actor: String,
    /// Session in which the action occurred.
    pub session_id: String,
    /// JSON metadata associated with the event.
    pub details_json: String,
    /// 0 = normal, 1 = GDPR-erased (PII fields replaced with "[ERASED]").
    pub pii_marker: i32,
}

/// An audit entry pending hash computation and persistence.
///
/// The `id`, `entry_hash`, and `prev_hash` fields are assigned by the
/// [`AuditWriter`] background task when the entry is flushed to the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingEntry {
    /// ISO 8601 timestamp.
    pub timestamp: String,
    /// Dot-separated event type.
    pub event_type: String,
    /// Action performed.
    pub action: String,
    /// Type of resource affected.
    pub resource_type: String,
    /// Identifier of the resource.
    pub resource_id: String,
    /// Actor who performed the action.
    pub actor: String,
    /// Session identifier.
    pub session_id: String,
    /// JSON metadata.
    pub details_json: String,
}

/// Report returned by a GDPR erasure operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditErasureReport {
    /// Number of entries matching the erasure criteria.
    pub entries_found: usize,
    /// Number of entries actually erased (should equal entries_found).
    pub entries_erased: usize,
    /// Row IDs of erased entries.
    pub erased_ids: Vec<i64>,
}

/// Crate-local error type for audit operations.
#[derive(Debug, Error)]
pub enum AuditError {
    /// The audit database is unavailable (cannot open, connection lost).
    #[error("audit database unavailable: {0}")]
    DbUnavailable(String),

    /// The hash chain is broken at the specified entry.
    #[error("audit chain broken at entry {entry_id}: expected {expected}, got {actual}")]
    ChainBroken {
        expected: String,
        actual: String,
        entry_id: i64,
    },

    /// A batch flush to the database failed.
    #[error("audit flush failed: {0}")]
    FlushFailed(String),

    /// Chain verification failed.
    #[error("audit verify failed: {0}")]
    VerifyFailed(String),
}
