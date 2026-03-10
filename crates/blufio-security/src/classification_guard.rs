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

    /// Check if content at this classification level can be exported,
    /// and if so, return it with PII redacted. Returns `None` for Restricted data.
    ///
    /// This is the single entry point for export code paths (Phase 60 GDPR export).
    /// Callers should use this instead of separate `can_export` + `redact_pii` calls.
    pub fn redact_for_export(&self, content: &str, level: DataClassification) -> Option<String> {
        if !self.can_export(level) {
            return None;
        }
        // Apply PII redaction to exportable content.
        Some(crate::pii::redact_pii(content))
    }
}

/// Filter and redact a batch of items for export. Returns only exportable items
/// with PII redacted, plus a count of excluded items.
pub fn filter_for_export(items: &[(String, DataClassification)]) -> (Vec<String>, usize) {
    let guard = ClassificationGuard::instance();
    let mut exported = Vec::new();
    let mut excluded = 0usize;
    for (content, level) in items {
        match guard.redact_for_export(content, *level) {
            Some(redacted) => exported.push(redacted),
            None => excluded += 1,
        }
    }
    if excluded > 0 {
        tracing::warn!(
            excluded_count = excluded,
            "items excluded from export due to classification restrictions"
        );
    }
    (exported, excluded)
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

    // ── Export redaction utility tests ─────────────────────────────

    #[test]
    fn redact_for_export_public_returns_some_with_pii_redacted() {
        let guard = ClassificationGuard::instance();
        let result = guard.redact_for_export(
            "Contact user@example.com for details",
            DataClassification::Public,
        );
        assert!(result.is_some());
        let redacted = result.unwrap();
        assert!(redacted.contains("[EMAIL]"));
        assert!(!redacted.contains("user@example.com"));
    }

    #[test]
    fn redact_for_export_restricted_returns_none() {
        let guard = ClassificationGuard::instance();
        let result = guard.redact_for_export("secret data", DataClassification::Restricted);
        assert!(result.is_none());
    }

    #[test]
    fn redact_for_export_confidential_returns_some_with_pii_redacted() {
        let guard = ClassificationGuard::instance();
        let result = guard.redact_for_export(
            "Call 555-123-4567 for info",
            DataClassification::Confidential,
        );
        assert!(result.is_some());
    }

    #[test]
    fn redact_for_export_with_email_returns_redacted() {
        let guard = ClassificationGuard::instance();
        let result = guard.redact_for_export(
            "Send to admin@corp.com",
            DataClassification::Internal,
        );
        assert!(result.is_some());
        let redacted = result.unwrap();
        assert!(redacted.contains("[EMAIL]"));
        assert!(!redacted.contains("admin@corp.com"));
    }

    #[test]
    fn filter_for_export_mixed_levels() {
        let items = vec![
            ("public data".to_string(), DataClassification::Public),
            ("internal data".to_string(), DataClassification::Internal),
            ("restricted secret".to_string(), DataClassification::Restricted),
            ("confidential info".to_string(), DataClassification::Confidential),
            ("another restricted".to_string(), DataClassification::Restricted),
        ];
        let (exported, excluded) = filter_for_export(&items);
        assert_eq!(exported.len(), 3);
        assert_eq!(excluded, 2);
    }

    #[test]
    fn filter_for_export_excluded_count_matches_restricted() {
        let items = vec![
            ("a".to_string(), DataClassification::Public),
            ("b".to_string(), DataClassification::Restricted),
            ("c".to_string(), DataClassification::Restricted),
            ("d".to_string(), DataClassification::Restricted),
        ];
        let (exported, excluded) = filter_for_export(&items);
        assert_eq!(exported.len(), 1);
        assert_eq!(excluded, 3);
    }
}
