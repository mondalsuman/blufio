// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Domain model types for storage entities.
//!
//! These types represent the rows stored in the SQLite database.
//! They are defined here for the storage crate's internal use.
//! The canonical shared types live in `blufio-core::types` for
//! use across adapter trait boundaries.

use serde::{Deserialize, Serialize};

/// A conversation session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier.
    pub id: String,
    /// Channel the session originates from (e.g., "telegram", "cli").
    pub channel: String,
    /// Optional user identifier from the channel.
    pub user_id: Option<String>,
    /// Session state: "active", "paused", "closed".
    pub state: String,
    /// Optional JSON metadata blob.
    pub metadata: Option<String>,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last-update timestamp.
    pub updated_at: String,
}

/// A single message within a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique message identifier.
    pub id: String,
    /// Session this message belongs to.
    pub session_id: String,
    /// Role: "user", "assistant", "system", or "tool".
    pub role: String,
    /// Message content (text or JSON for tool results).
    pub content: String,
    /// Token count for cost tracking (populated after LLM response).
    pub token_count: Option<i64>,
    /// Optional JSON metadata blob.
    pub metadata: Option<String>,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
}

/// A crash-safe message queue entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueEntry {
    /// Auto-increment queue entry ID.
    pub id: i64,
    /// Queue name for routing (e.g., "inbound", "outbound").
    pub queue_name: String,
    /// JSON payload.
    pub payload: String,
    /// Status: "pending", "processing", "completed", "failed".
    pub status: String,
    /// Number of processing attempts so far.
    pub attempts: i32,
    /// Maximum allowed attempts before permanent failure.
    pub max_attempts: i32,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last-update timestamp.
    pub updated_at: String,
    /// ISO 8601 timestamp until which this entry is locked for processing.
    pub locked_until: Option<String>,
}
