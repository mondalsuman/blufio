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

pub use classification_guard::ClassificationGuard;
pub use pii::{PiiMatch, PiiType, detect_pii, luhn_validate, redact_pii};
pub use redact::{RedactingWriter, redact};
pub use ssrf::SsrfSafeResolver;
pub use tls::{build_secure_client, is_localhost, validate_url};
