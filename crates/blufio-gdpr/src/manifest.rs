// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Erasure manifest generation and persistence.
//!
//! A manifest records what was deleted during a GDPR erasure operation.
//! It contains counts and session IDs (no content) for operator audit purposes.
//! The manifest is always written, even with `--skip-export`.

use std::path::{Path, PathBuf};

use crate::models::{ErasureManifest, GdprError};

/// Create an erasure manifest from deletion counts.
pub fn create_manifest(
    user_id: &str,
    session_ids: &[String],
    messages_deleted: u64,
    sessions_deleted: u64,
    memories_deleted: u64,
    archives_deleted: u64,
    cost_records_anonymized: u64,
    audit_entries_redacted: u64,
) -> ErasureManifest {
    ErasureManifest {
        manifest_id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        user_id: user_id.to_string(),
        messages_deleted,
        sessions_deleted,
        memories_deleted,
        archives_deleted,
        cost_records_anonymized,
        audit_entries_redacted,
        session_ids: session_ids.to_vec(),
    }
}

/// Write a manifest to the specified export directory as pretty-printed JSON.
///
/// Creates the export directory if it does not exist. Returns the path to the
/// written manifest file.
///
/// File name format: `gdpr-manifest-{user_id}-{timestamp}.json`
pub fn write_manifest(manifest: &ErasureManifest, export_dir: &Path) -> Result<PathBuf, GdprError> {
    use std::io::Write;

    // Create directory if it does not exist
    std::fs::create_dir_all(export_dir).map_err(|e| {
        GdprError::ExportDirNotWritable(format!("{}: {e}", export_dir.display()))
    })?;

    // Sanitize timestamp for filename (replace colons and plus signs)
    let ts = manifest
        .timestamp
        .replace(':', "-")
        .replace('+', "");
    let filename = format!("gdpr-manifest-{}-{ts}.json", manifest.user_id);
    let path = export_dir.join(filename);

    let json = serde_json::to_string_pretty(manifest).map_err(|e| {
        GdprError::ExportFailed(format!("manifest serialization failed: {e}"))
    })?;

    let mut file = std::fs::File::create(&path).map_err(|e| {
        GdprError::ExportDirNotWritable(format!("{}: {e}", path.display()))
    })?;
    file.write_all(json.as_bytes()).map_err(|e| {
        GdprError::ExportFailed(format!("manifest write failed: {e}"))
    })?;
    file.flush().map_err(|e| {
        GdprError::ExportFailed(format!("manifest flush failed: {e}"))
    })?;

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_manifest_produces_correct_counts() {
        let session_ids = vec!["s1".to_string(), "s2".to_string()];
        let manifest = create_manifest("user-42", &session_ids, 10, 2, 5, 3, 8, 1);

        assert_eq!(manifest.user_id, "user-42");
        assert_eq!(manifest.messages_deleted, 10);
        assert_eq!(manifest.sessions_deleted, 2);
        assert_eq!(manifest.memories_deleted, 5);
        assert_eq!(manifest.archives_deleted, 3);
        assert_eq!(manifest.cost_records_anonymized, 8);
        assert_eq!(manifest.audit_entries_redacted, 1);
        assert_eq!(manifest.session_ids, session_ids);
        assert!(!manifest.manifest_id.is_empty());
        assert!(!manifest.timestamp.is_empty());
    }

    #[test]
    fn write_manifest_creates_json_file() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = create_manifest("test-user", &["s1".into()], 5, 1, 2, 1, 3, 0);

        let path = write_manifest(&manifest, dir.path()).unwrap();

        assert!(path.exists(), "manifest file should exist");
        assert!(
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("gdpr-manifest-test-user-"),
            "filename should follow convention"
        );

        // Verify it is valid JSON
        let contents = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed["user_id"], "test-user");
        assert_eq!(parsed["messages_deleted"], 5);
    }

    #[test]
    fn write_manifest_creates_directory_if_missing() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("sub").join("dir");
        let manifest = create_manifest("u1", &[], 0, 0, 0, 0, 0, 0);

        let path = write_manifest(&manifest, &nested).unwrap();
        assert!(path.exists());
    }
}
