// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Helper functions for constructing GDPR bus events.
//!
//! User IDs are always SHA-256 hashed before inclusion in events to prevent
//! PII leakage through the event bus. Events are observable for metrics,
//! hooks, and audit without exposing personal data.

use blufio_bus::events::{BusEvent, GdprEvent, new_event_id, now_timestamp};
use sha2::{Digest, Sha256};

use crate::models::ErasureManifest;

/// Hash a user ID using SHA-256 for safe inclusion in event payloads.
///
/// Events must never contain plaintext user identifiers. This function
/// produces a deterministic hex-encoded hash that can be correlated
/// across events without revealing the original user ID.
pub fn hash_user_id(user_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(user_id.as_bytes());
    hex::encode(hasher.finalize())
}

/// Construct a `BusEvent::Gdpr(GdprEvent::ErasureStarted)` event.
pub fn erasure_started(user_id: &str) -> BusEvent {
    BusEvent::Gdpr(GdprEvent::ErasureStarted {
        event_id: new_event_id(),
        timestamp: now_timestamp(),
        user_id_hash: hash_user_id(user_id),
    })
}

/// Construct a `BusEvent::Gdpr(GdprEvent::ErasureCompleted)` event.
pub fn erasure_completed(user_id: &str, manifest: &ErasureManifest, duration_ms: u64) -> BusEvent {
    BusEvent::Gdpr(GdprEvent::ErasureCompleted {
        event_id: new_event_id(),
        timestamp: now_timestamp(),
        user_id_hash: hash_user_id(user_id),
        messages_deleted: manifest.messages_deleted,
        sessions_deleted: manifest.sessions_deleted,
        memories_deleted: manifest.memories_deleted,
        archives_deleted: manifest.archives_deleted,
        cost_records_anonymized: manifest.cost_records_anonymized,
        duration_ms,
    })
}

/// Construct a `BusEvent::Gdpr(GdprEvent::ExportCompleted)` event.
pub fn export_completed(user_id: &str, format: &str, file_path: &str, size_bytes: u64) -> BusEvent {
    BusEvent::Gdpr(GdprEvent::ExportCompleted {
        event_id: new_event_id(),
        timestamp: now_timestamp(),
        user_id_hash: hash_user_id(user_id),
        format: format.to_string(),
        file_path: file_path.to_string(),
        size_bytes,
    })
}

/// Construct a `BusEvent::Gdpr(GdprEvent::ReportGenerated)` event.
pub fn report_generated(user_id: &str) -> BusEvent {
    BusEvent::Gdpr(GdprEvent::ReportGenerated {
        event_id: new_event_id(),
        timestamp: now_timestamp(),
        user_id_hash: hash_user_id(user_id),
    })
}
