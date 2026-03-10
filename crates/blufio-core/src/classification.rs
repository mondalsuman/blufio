// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Data classification types for the Blufio agent framework.
//!
//! Provides:
//! - [`DataClassification`] -- four-level sensitivity enum (Public < Internal < Confidential < Restricted)
//! - [`Classifiable`] -- trait for getting/setting classification on domain types
//! - [`ClassificationError`] -- error types for classification operations

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// DataClassification
// ---------------------------------------------------------------------------

/// Sensitivity classification for runtime data (memories, messages, sessions).
///
/// Levels are ordered by ascending sensitivity:
/// `Public < Internal < Confidential < Restricted`
///
/// Serialized as lowercase strings: `"public"`, `"internal"`, `"confidential"`, `"restricted"`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum DataClassification {
    /// No restrictions. Safe to share publicly.
    Public,
    /// Default level. Audit-logged only (Phase 54). No export/context restrictions.
    #[default]
    Internal,
    /// Contains sensitive data. PII redacted in logs. Encrypted at rest via SQLCipher.
    Confidential,
    /// Most sensitive. Never exported, never included in LLM context, redacted in logs.
    Restricted,
}

impl fmt::Display for DataClassification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl DataClassification {
    /// Convert to lowercase string for SQLite TEXT storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::Confidential => "confidential",
            Self::Restricted => "restricted",
        }
    }

    /// Parse from SQLite TEXT value. Returns `None` for unrecognized values.
    pub fn from_str_value(s: &str) -> Option<Self> {
        match s {
            "public" => Some(Self::Public),
            "internal" => Some(Self::Internal),
            "confidential" => Some(Self::Confidential),
            "restricted" => Some(Self::Restricted),
            _ => None,
        }
    }

    /// Returns `true` if `self` is a downgrade from `current` (i.e., `self < current`).
    pub fn is_downgrade_from(&self, current: &Self) -> bool {
        *self < *current
    }
}

// ---------------------------------------------------------------------------
// Classifiable trait
// ---------------------------------------------------------------------------

/// Trait for types that carry a data classification level.
///
/// Implemented on domain structs (Memory, Message, Session) that store
/// classified data. Enables generic enforcement logic.
pub trait Classifiable {
    /// Get the current classification level.
    fn classification(&self) -> DataClassification;

    /// Set the classification level.
    fn set_classification(&mut self, level: DataClassification);
}

// ---------------------------------------------------------------------------
// ClassificationError
// ---------------------------------------------------------------------------

/// Errors specific to data classification operations.
#[derive(Debug, Clone, Error)]
#[non_exhaustive]
pub enum ClassificationError {
    /// The provided classification level string is not valid.
    #[error("invalid classification level: {0}")]
    InvalidLevel(String),

    /// A downgrade was rejected (requires explicit confirmation).
    #[error("classification downgrade rejected: cannot change from {current} to {requested}")]
    DowngradeRejected {
        /// The current classification level.
        current: String,
        /// The requested (lower) classification level.
        requested: String,
    },

    /// The entity to classify was not found.
    #[error("{entity_type} not found: {entity_id}")]
    EntityNotFound {
        /// Type of entity (e.g., "memory", "message", "session").
        entity_type: String,
        /// ID of the entity.
        entity_id: String,
    },

    /// A bulk classification operation partially failed.
    #[error("bulk operation failed: {failed} of {total} operations failed")]
    BulkOperationFailed {
        /// Total number of operations attempted.
        total: usize,
        /// Number of operations that failed.
        failed: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- DataClassification ordering ---

    #[test]
    fn classification_ordering_public_lt_internal() {
        assert!(DataClassification::Public < DataClassification::Internal);
    }

    #[test]
    fn classification_ordering_internal_lt_confidential() {
        assert!(DataClassification::Internal < DataClassification::Confidential);
    }

    #[test]
    fn classification_ordering_confidential_lt_restricted() {
        assert!(DataClassification::Confidential < DataClassification::Restricted);
    }

    #[test]
    fn classification_ordering_public_lt_restricted() {
        assert!(DataClassification::Public < DataClassification::Restricted);
    }

    // --- as_str / from_str_value round-trip ---

    #[test]
    fn classification_as_str_values() {
        assert_eq!(DataClassification::Public.as_str(), "public");
        assert_eq!(DataClassification::Internal.as_str(), "internal");
        assert_eq!(DataClassification::Confidential.as_str(), "confidential");
        assert_eq!(DataClassification::Restricted.as_str(), "restricted");
    }

    #[test]
    fn classification_from_str_value_round_trip() {
        for level in [
            DataClassification::Public,
            DataClassification::Internal,
            DataClassification::Confidential,
            DataClassification::Restricted,
        ] {
            let s = level.as_str();
            let parsed = DataClassification::from_str_value(s);
            assert_eq!(parsed, Some(level), "round-trip failed for {s}");
        }
    }

    #[test]
    fn classification_from_str_value_invalid() {
        assert_eq!(DataClassification::from_str_value("invalid"), None);
        assert_eq!(DataClassification::from_str_value(""), None);
        assert_eq!(DataClassification::from_str_value("PUBLIC"), None);
    }

    // --- Default ---

    #[test]
    fn classification_default_is_internal() {
        assert_eq!(DataClassification::default(), DataClassification::Internal);
    }

    // --- is_downgrade_from ---

    #[test]
    fn is_downgrade_from_confidential_to_restricted() {
        assert!(
            DataClassification::Confidential.is_downgrade_from(&DataClassification::Restricted)
        );
    }

    #[test]
    fn is_not_downgrade_restricted_from_public() {
        assert!(!DataClassification::Restricted.is_downgrade_from(&DataClassification::Public));
    }

    #[test]
    fn is_not_downgrade_same_level() {
        assert!(!DataClassification::Internal.is_downgrade_from(&DataClassification::Internal));
    }

    #[test]
    fn is_downgrade_public_from_internal() {
        assert!(DataClassification::Public.is_downgrade_from(&DataClassification::Internal));
    }

    // --- Display ---

    #[test]
    fn classification_display() {
        assert_eq!(format!("{}", DataClassification::Public), "public");
        assert_eq!(format!("{}", DataClassification::Restricted), "restricted");
    }

    // --- Serde round-trip ---

    #[test]
    fn classification_serde_round_trip() {
        for level in [
            DataClassification::Public,
            DataClassification::Internal,
            DataClassification::Confidential,
            DataClassification::Restricted,
        ] {
            let json = serde_json::to_string(&level).unwrap();
            assert_eq!(json, format!("\"{}\"", level.as_str()));
            let parsed: DataClassification = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, level);
        }
    }

    #[test]
    fn classification_serde_serializes_as_lowercase() {
        let json = serde_json::to_string(&DataClassification::Confidential).unwrap();
        assert_eq!(json, "\"confidential\"");
    }

    // --- ClassificationError Display ---

    #[test]
    fn classification_error_invalid_level_display() {
        let err = ClassificationError::InvalidLevel("super_secret".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("invalid classification level"));
        assert!(msg.contains("super_secret"));
    }

    #[test]
    fn classification_error_downgrade_rejected_display() {
        let err = ClassificationError::DowngradeRejected {
            current: "restricted".to_string(),
            requested: "public".to_string(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("downgrade rejected"));
        assert!(msg.contains("restricted"));
        assert!(msg.contains("public"));
    }

    #[test]
    fn classification_error_entity_not_found_display() {
        let err = ClassificationError::EntityNotFound {
            entity_type: "memory".to_string(),
            entity_id: "mem-123".to_string(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("memory"));
        assert!(msg.contains("mem-123"));
    }

    #[test]
    fn classification_error_bulk_operation_failed_display() {
        let err = ClassificationError::BulkOperationFailed {
            total: 10,
            failed: 3,
        };
        let msg = format!("{err}");
        assert!(msg.contains("3"));
        assert!(msg.contains("10"));
    }
}
