// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Static enforcement singleton for data classification rules.
//!
//! [`ClassificationGuard`] provides pure, stateless functions that enforce
//! per-level controls:
//! - **Restricted**: never exported, never included in LLM context, redacted in logs
//! - **Confidential**: redacted in logs (PII within content), encrypted at rest via SQLCipher
//! - **Internal**: audit-logged only (Phase 54), no restrictions
//! - **Public**: no restrictions
//!
//! The guard is a global singleton (via [`LazyLock`]) with no config dependency,
//! no mutable state, and deterministic behavior.

use std::sync::LazyLock;

use blufio_core::classification::DataClassification;

/// Global singleton instance of [`ClassificationGuard`].
static GUARD: LazyLock<ClassificationGuard> = LazyLock::new(|| ClassificationGuard);

/// Static enforcement for data classification rules.
///
/// All methods are pure functions with no side effects. The guard carries no state
/// and makes deterministic decisions based solely on the classification level.
///
/// # Truth Table
///
/// | Level        | can_export | can_include_in_context | must_redact_in_logs |
/// |--------------|------------|------------------------|---------------------|
/// | Public       | true       | true                   | false               |
/// | Internal     | true       | true                   | false               |
/// | Confidential | true       | true                   | true                |
/// | Restricted   | false      | false                  | true                |
pub struct ClassificationGuard;

impl ClassificationGuard {
    /// Returns the static singleton instance.
    pub fn instance() -> &'static Self {
        &GUARD
    }

    /// Whether data at this classification level can be exported.
    ///
    /// Returns `false` for Restricted data (never exported).
    pub fn can_export(&self, level: DataClassification) -> bool {
        level < DataClassification::Restricted
    }

    /// Whether data at this classification level can be included in LLM context.
    ///
    /// Returns `false` for Restricted data (never sent to LLM).
    pub fn can_include_in_context(&self, level: DataClassification) -> bool {
        level < DataClassification::Restricted
    }

    /// Whether data at this classification level must be redacted in logs.
    ///
    /// Returns `true` for Confidential and Restricted data.
    pub fn must_redact_in_logs(&self, level: DataClassification) -> bool {
        level >= DataClassification::Confidential
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Exhaustive truth table (4 levels x 3 methods = 12 assertions) ──

    #[test]
    fn public_can_export() {
        assert!(ClassificationGuard::instance().can_export(DataClassification::Public));
    }

    #[test]
    fn internal_can_export() {
        assert!(ClassificationGuard::instance().can_export(DataClassification::Internal));
    }

    #[test]
    fn confidential_can_export() {
        assert!(ClassificationGuard::instance().can_export(DataClassification::Confidential));
    }

    #[test]
    fn restricted_cannot_export() {
        assert!(!ClassificationGuard::instance().can_export(DataClassification::Restricted));
    }

    #[test]
    fn public_can_include_in_context() {
        assert!(ClassificationGuard::instance().can_include_in_context(DataClassification::Public));
    }

    #[test]
    fn internal_can_include_in_context() {
        assert!(
            ClassificationGuard::instance().can_include_in_context(DataClassification::Internal)
        );
    }

    #[test]
    fn confidential_can_include_in_context() {
        assert!(
            ClassificationGuard::instance()
                .can_include_in_context(DataClassification::Confidential)
        );
    }

    #[test]
    fn restricted_cannot_include_in_context() {
        assert!(
            !ClassificationGuard::instance()
                .can_include_in_context(DataClassification::Restricted)
        );
    }

    #[test]
    fn public_no_log_redaction() {
        assert!(!ClassificationGuard::instance().must_redact_in_logs(DataClassification::Public));
    }

    #[test]
    fn internal_no_log_redaction() {
        assert!(!ClassificationGuard::instance().must_redact_in_logs(DataClassification::Internal));
    }

    #[test]
    fn confidential_must_redact_in_logs() {
        assert!(
            ClassificationGuard::instance().must_redact_in_logs(DataClassification::Confidential)
        );
    }

    #[test]
    fn restricted_must_redact_in_logs() {
        assert!(
            ClassificationGuard::instance().must_redact_in_logs(DataClassification::Restricted)
        );
    }

    // ── Singleton identity ──────────────────────────────────────────

    #[test]
    fn instance_returns_same_reference() {
        let a = ClassificationGuard::instance() as *const ClassificationGuard;
        let b = ClassificationGuard::instance() as *const ClassificationGuard;
        assert_eq!(a, b, "instance() should return the same static reference");
    }

    // ── Boundary tests ──────────────────────────────────────────────

    #[test]
    fn confidential_is_log_redaction_threshold() {
        // Confidential is the first level that requires log redaction
        assert!(!ClassificationGuard::instance().must_redact_in_logs(DataClassification::Internal));
        assert!(
            ClassificationGuard::instance().must_redact_in_logs(DataClassification::Confidential)
        );
    }

    #[test]
    fn restricted_is_export_exclusion_threshold() {
        // Restricted is the first level excluded from exports
        assert!(ClassificationGuard::instance().can_export(DataClassification::Confidential));
        assert!(!ClassificationGuard::instance().can_export(DataClassification::Restricted));
    }

    #[test]
    fn restricted_is_context_exclusion_threshold() {
        // Restricted is the first level excluded from LLM context
        assert!(
            ClassificationGuard::instance()
                .can_include_in_context(DataClassification::Confidential)
        );
        assert!(
            !ClassificationGuard::instance()
                .can_include_in_context(DataClassification::Restricted)
        );
    }
}
