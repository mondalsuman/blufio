// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Network security enforcement for the Blufio agent framework.
//!
//! Provides TLS enforcement, SSRF prevention via DNS resolver filtering,
//! and secret redaction for log output.

pub mod classification_guard;
pub mod pii;
pub mod redact;
pub mod ssrf;
pub mod tls;

pub use classification_guard::{ClassificationGuard, filter_for_export};
pub use pii::{
    PiiMatch, PiiScanResult, PiiType, bulk_classification_changed_event,
    classification_changed_event, classification_enforced_event, detect_pii, luhn_validate,
    pii_detected_event, redact_pii, scan_and_classify,
};
pub use redact::{RedactingWriter, redact, redact_secrets_only, redact_with_pii};
pub use ssrf::SsrfSafeResolver;
pub use tls::{build_secure_client, is_localhost, validate_url};
