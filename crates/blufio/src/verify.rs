// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio verify` command implementation.
//!
//! Verifies a file's Minisign signature against the embedded public key.
//! Auto-detects `.minisig` sidecar files or accepts explicit `--signature` path.
//!
//! Output convention (matches existing blufio commands):
//! - Status messages to stderr (`eprintln!`)
//! - Final result to stdout (`println!`)
//! - Exit code 0 on success, 1 on any failure

use std::path::Path;

use blufio_core::BlufioError;

/// Run the verify command.
///
/// Verifies `file_path` against its Minisign signature. If `signature_path`
/// is `None`, looks for `<file_path>.minisig` alongside the file.
///
/// On success, prints `Verified: <filename> (signed by <trusted comment>)` to stdout.
/// On failure, returns `BlufioError::Signature` with an actionable error message.
pub fn run_verify(file_path: &str, signature_path: Option<&str>) -> Result<(), BlufioError> {
    let file = Path::new(file_path);
    let sig = signature_path.map(Path::new);

    eprintln!("blufio: verifying {file_path}");

    match blufio_verify::verify_signature(file, sig) {
        Ok(result) => {
            println!(
                "Verified: {} (signed by {})",
                result.file_name, result.trusted_comment
            );
            Ok(())
        }
        Err(e) => Err(BlufioError::Signature(e.to_string())),
    }
}
