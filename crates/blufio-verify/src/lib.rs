// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

//! Minisign signature verification for Blufio releases.
//!
//! Provides [`verify_signature`] for verifying files against their `.minisig`
//! Minisign signatures using an embedded public key. The public key is compiled
//! into the binary — no external key file needed.
//!
//! # Usage
//!
//! ```no_run
//! use std::path::Path;
//!
//! // Auto-detect .minisig sidecar
//! let result = blufio_verify::verify_signature(
//!     Path::new("blufio-v1.2.0"),
//!     None,
//! ).unwrap();
//!
//! println!("Verified: {} (signed by {})", result.file_name, result.trusted_comment);
//! ```

use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

/// Minisign public key for verifying Blufio releases.
///
/// Generated with: `minisign -G -p blufio.pub -s blufio.key -W`
///
/// Verify independently: `minisign -Vm <file> -P RWTmPtqu+v8klbkI0Z14bv0xLeninEpdYbIsJkTjPzcs0K2oGtx6Lsd5`
const MINISIGN_PUBLIC_KEY: &str = "RWTmPtqu+v8klbkI0Z14bv0xLeninEpdYbIsJkTjPzcs0K2oGtx6Lsd5";

/// Result of a successful signature verification.
#[derive(Debug)]
pub struct VerifyResult {
    /// Name of the verified file.
    pub file_name: String,
    /// Trusted comment from the signature (cryptographically bound).
    pub trusted_comment: String,
}

/// Errors from signature verification.
///
/// Each variant produces a distinct, actionable error message that names the
/// file and explains what failed.
#[derive(Debug)]
pub enum VerifyError {
    /// The file to verify was not found.
    FileNotFound {
        /// Path that was not found.
        path: String,
    },
    /// The signature file was not found.
    SignatureNotFound {
        /// Path that was not found.
        path: String,
        /// Optional hint for the user (e.g., "Use --signature <path>").
        hint: Option<String>,
    },
    /// The signature file has invalid format.
    InvalidSignature {
        /// Description of what is invalid.
        message: String,
    },
    /// The signature does not match the file content.
    VerificationFailed {
        /// Name of the file that failed verification.
        file_name: String,
        /// Description of the failure.
        message: String,
    },
    /// The embedded public key is invalid (should never happen in production).
    InvalidKey(String),
    /// I/O error reading file or signature.
    Io {
        /// What was being read.
        context: String,
        /// The underlying error message.
        message: String,
    },
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VerifyError::FileNotFound { path } => {
                write!(f, "File not found: {path}")
            }
            VerifyError::SignatureNotFound { path, hint } => {
                write!(f, "Signature file not found: {path}")?;
                if let Some(hint) = hint {
                    write!(f, "\n  {hint}")?;
                }
                Ok(())
            }
            VerifyError::InvalidSignature { message } => {
                write!(f, "Invalid signature format: {message}")
            }
            VerifyError::VerificationFailed { file_name, message } => {
                write!(
                    f,
                    "Signature verification failed for '{file_name}': {message}"
                )
            }
            VerifyError::InvalidKey(message) => {
                write!(f, "Invalid embedded public key: {message}")
            }
            VerifyError::Io { context, message } => {
                write!(f, "Failed to read {context}: {message}")
            }
        }
    }
}

impl std::error::Error for VerifyError {}

/// Get the embedded Minisign public key.
///
/// Parses the compile-time constant into a [`minisign_verify::PublicKey`].
/// This should never fail in production since the key is validated at development time.
pub fn embedded_public_key() -> Result<minisign_verify::PublicKey, VerifyError> {
    minisign_verify::PublicKey::from_base64(MINISIGN_PUBLIC_KEY)
        .map_err(|e| VerifyError::InvalidKey(e.to_string()))
}

/// Verify a file's Minisign signature.
///
/// If `signature_path` is `None`, looks for `<file_path>.minisig` alongside the file
/// (standard Minisign convention). If `signature_path` is `Some`, uses the explicit path.
///
/// Returns the verified file name and trusted comment on success.
///
/// # Errors
///
/// Returns [`VerifyError`] with a distinct variant for each failure mode:
/// - [`VerifyError::FileNotFound`] — the file to verify does not exist
/// - [`VerifyError::SignatureNotFound`] — the `.minisig` file does not exist
/// - [`VerifyError::InvalidSignature`] — the signature file has invalid format
/// - [`VerifyError::VerificationFailed`] — the signature does not match the file content
/// - [`VerifyError::InvalidKey`] — the embedded public key is invalid
/// - [`VerifyError::Io`] — I/O error reading file or signature
pub fn verify_signature(
    file_path: &Path,
    signature_path: Option<&Path>,
) -> Result<VerifyResult, VerifyError> {
    // 1. Check file exists
    if !file_path.exists() {
        return Err(VerifyError::FileNotFound {
            path: file_path.display().to_string(),
        });
    }

    // 2. Resolve signature path (explicit or auto-detect .minisig)
    let sig_path = resolve_signature_path(file_path, signature_path)?;

    // 3. Load embedded public key
    let public_key = embedded_public_key()?;

    // 4. Read signature from file
    let sig_content = std::fs::read_to_string(&sig_path).map_err(|e| VerifyError::Io {
        context: format!("signature file '{}'", sig_path.display()),
        message: e.to_string(),
    })?;

    let signature = minisign_verify::Signature::decode(&sig_content).map_err(|e| {
        VerifyError::InvalidSignature {
            message: e.to_string(),
        }
    })?;

    // 5. Read file content
    let content = std::fs::read(file_path).map_err(|e| VerifyError::Io {
        context: format!("file '{}'", file_path.display()),
        message: e.to_string(),
    })?;

    // 6. Verify signature against content
    let file_name = file_path
        .file_name()
        .unwrap_or(file_path.as_os_str())
        .to_string_lossy()
        .to_string();

    public_key
        .verify(&content, &signature, false)
        .map_err(|e| VerifyError::VerificationFailed {
            file_name: file_name.clone(),
            message: e.to_string(),
        })?;

    // 7. Extract trusted comment
    let trusted_comment = signature.trusted_comment().to_string();

    // 8. Return result
    Ok(VerifyResult {
        file_name,
        trusted_comment,
    })
}

/// Resolve the signature file path.
///
/// If an explicit path is provided, validates it exists.
/// Otherwise, auto-detects by appending `.minisig` to the full filename.
///
/// Note: Uses `OsString::push()` to append `.minisig`, NOT `Path::with_extension()`
/// which would replace the last extension (e.g., `.tar.gz` becomes `.tar.minisig`).
fn resolve_signature_path(
    file_path: &Path,
    explicit_sig: Option<&Path>,
) -> Result<PathBuf, VerifyError> {
    if let Some(sig) = explicit_sig {
        if sig.exists() {
            return Ok(sig.to_path_buf());
        }
        return Err(VerifyError::SignatureNotFound {
            path: sig.display().to_string(),
            hint: None,
        });
    }

    // Auto-detect: append .minisig to the full filename
    let mut sig_name: OsString = file_path.as_os_str().to_owned();
    sig_name.push(".minisig");
    let auto_path = PathBuf::from(sig_name);

    if auto_path.exists() {
        Ok(auto_path)
    } else {
        Err(VerifyError::SignatureNotFound {
            path: auto_path.display().to_string(),
            hint: Some("Use --signature <path> to specify manually".to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test public key (same as production key — validates the embedded constant).
    const TEST_PUBLIC_KEY: &str = "RWTmPtqu+v8klbkI0Z14bv0xLeninEpdYbIsJkTjPzcs0K2oGtx6Lsd5";

    /// Test file content that has been pre-signed.
    const TEST_FILE_CONTENT: &[u8] = b"test file content for verification\n";

    /// Pre-signed signature for TEST_FILE_CONTENT using the embedded key.
    /// Generated with: `minisign -S -m test-file.txt -s blufio.key -t "signed by blufio maintainer" -W`
    const TEST_SIGNATURE: &str = "\
untrusted comment: signature from minisign secret key\n\
RUTmPtqu+v8klV1TCnHjkn4Q5AvIUalZmv+3Go0/VqkX2KKj4QoEeNK/eacr9M1PScsKSOVQSvIWkYmPY9NViNbURtYu8sh9uws=\n\
trusted comment: signed by blufio maintainer\n\
5kW267eqSprLNBsW0v3UKj0dzCN7U+HcodgJgte598/VxULxYz7d9ba0uYUWIBKkRXpLJA2G5DvGYp/RG0LGAA==";

    #[test]
    fn test_embedded_key_parses() {
        let key = embedded_public_key();
        assert!(key.is_ok(), "Embedded public key should parse: {key:?}");
    }

    #[test]
    fn test_verify_valid_signature() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test-file.txt");
        let sig_path = dir.path().join("test-file.txt.minisig");

        std::fs::write(&file_path, TEST_FILE_CONTENT).unwrap();
        std::fs::write(&sig_path, TEST_SIGNATURE).unwrap();

        let result = verify_signature(&file_path, None);
        assert!(result.is_ok(), "Valid signature should verify: {result:?}");

        let result = result.unwrap();
        assert_eq!(result.file_name, "test-file.txt");
        assert_eq!(result.trusted_comment, "signed by blufio maintainer");
    }

    #[test]
    fn test_verify_valid_signature_explicit_path() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test-file.txt");
        let sig_path = dir.path().join("custom-sig.minisig");

        std::fs::write(&file_path, TEST_FILE_CONTENT).unwrap();
        std::fs::write(&sig_path, TEST_SIGNATURE).unwrap();

        let result = verify_signature(&file_path, Some(&sig_path));
        assert!(
            result.is_ok(),
            "Valid signature with explicit path should verify: {result:?}"
        );
    }

    #[test]
    fn test_verify_file_not_found() {
        let result = verify_signature(Path::new("/nonexistent/file.bin"), None);
        assert!(result.is_err());

        let err = result.unwrap_err();
        match &err {
            VerifyError::FileNotFound { path } => {
                assert!(path.contains("nonexistent"));
            }
            other => panic!("Expected FileNotFound, got: {other}"),
        }
        assert!(err.to_string().contains("File not found"));
    }

    #[test]
    fn test_verify_signature_not_found_auto() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("file-without-sig.txt");
        std::fs::write(&file_path, b"some content").unwrap();

        let result = verify_signature(&file_path, None);
        assert!(result.is_err());

        let err = result.unwrap_err();
        match &err {
            VerifyError::SignatureNotFound { path, hint } => {
                assert!(path.contains("file-without-sig.txt.minisig"));
                assert!(hint.is_some());
                assert!(
                    hint.as_ref().unwrap().contains("--signature"),
                    "Hint should mention --signature flag"
                );
            }
            other => panic!("Expected SignatureNotFound, got: {other}"),
        }
    }

    #[test]
    fn test_verify_signature_not_found_explicit() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("file.txt");
        std::fs::write(&file_path, b"content").unwrap();

        let explicit_sig = dir.path().join("nonexistent.minisig");
        let result = verify_signature(&file_path, Some(&explicit_sig));
        assert!(result.is_err());

        let err = result.unwrap_err();
        match &err {
            VerifyError::SignatureNotFound { hint, .. } => {
                assert!(
                    hint.is_none(),
                    "Explicit path should NOT have --signature hint"
                );
            }
            other => panic!("Expected SignatureNotFound, got: {other}"),
        }
    }

    #[test]
    fn test_verify_tampered_content() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("tampered-file.txt");
        let sig_path = dir.path().join("tampered-file.txt.minisig");

        // Write different content than what was signed
        std::fs::write(&file_path, b"this content was tampered with\n").unwrap();
        std::fs::write(&sig_path, TEST_SIGNATURE).unwrap();

        let result = verify_signature(&file_path, None);
        assert!(result.is_err());

        let err = result.unwrap_err();
        match &err {
            VerifyError::VerificationFailed { file_name, .. } => {
                assert_eq!(file_name, "tampered-file.txt");
            }
            other => panic!("Expected VerificationFailed, got: {other}"),
        }
        assert!(err.to_string().contains("Signature verification failed"));
    }

    #[test]
    fn test_verify_invalid_signature_format() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("file.txt");
        let sig_path = dir.path().join("file.txt.minisig");

        std::fs::write(&file_path, b"content").unwrap();
        std::fs::write(&sig_path, "this is not a valid signature format").unwrap();

        let result = verify_signature(&file_path, None);
        assert!(result.is_err());

        let err = result.unwrap_err();
        match &err {
            VerifyError::InvalidSignature { .. } => {}
            other => panic!("Expected InvalidSignature, got: {other}"),
        }
        assert!(err.to_string().contains("Invalid signature format"));
    }

    #[test]
    fn test_auto_detect_appends_minisig() {
        // Verify path construction: file.tar.gz -> file.tar.gz.minisig (not file.tar.minisig)
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("release.tar.gz");
        std::fs::write(&file_path, b"archive content").unwrap();

        // Create signature at the correctly-appended path
        let expected_sig = dir.path().join("release.tar.gz.minisig");
        std::fs::write(&expected_sig, TEST_SIGNATURE).unwrap();

        // This will fail verification (wrong content) but should find the right sig file
        let result = verify_signature(&file_path, None);
        // If it found the signature, it would try to verify (and fail due to content mismatch)
        // If it didn't find it, it would return SignatureNotFound
        match result {
            Err(VerifyError::VerificationFailed { .. }) => {
                // Good — it found the sig file and attempted verification
            }
            Err(VerifyError::SignatureNotFound { path, .. }) => {
                panic!("Auto-detect should find release.tar.gz.minisig, but looked for: {path}");
            }
            Ok(_) => {
                panic!("Should not verify with wrong content");
            }
            Err(other) => {
                panic!("Unexpected error: {other}");
            }
        }

        // Also verify the wrong path was NOT used
        let wrong_path = dir.path().join("release.tar.minisig");
        assert!(
            !wrong_path.exists(),
            "Should not create release.tar.minisig"
        );
    }

    #[test]
    fn test_error_display_formats() {
        // FileNotFound
        let err = VerifyError::FileNotFound {
            path: "/path/to/file".to_string(),
        };
        assert_eq!(err.to_string(), "File not found: /path/to/file");

        // SignatureNotFound with hint
        let err = VerifyError::SignatureNotFound {
            path: "/path/to/file.minisig".to_string(),
            hint: Some("Use --signature <path> to specify manually".to_string()),
        };
        let display = err.to_string();
        assert!(display.contains("Signature file not found"));
        assert!(display.contains("--signature"));

        // SignatureNotFound without hint
        let err = VerifyError::SignatureNotFound {
            path: "/path/to/sig.minisig".to_string(),
            hint: None,
        };
        let display = err.to_string();
        assert!(display.contains("Signature file not found"));
        assert!(!display.contains("--signature"));

        // VerificationFailed
        let err = VerifyError::VerificationFailed {
            file_name: "blufio".to_string(),
            message: "content mismatch".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Signature verification failed for 'blufio': content mismatch"
        );

        // InvalidKey
        let err = VerifyError::InvalidKey("bad key".to_string());
        assert_eq!(err.to_string(), "Invalid embedded public key: bad key");
    }

    #[test]
    fn test_public_key_matches_constant() {
        assert_eq!(
            MINISIGN_PUBLIC_KEY, TEST_PUBLIC_KEY,
            "Embedded key and test key should match"
        );
    }
}
