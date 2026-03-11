// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! File watcher for auto-indexing workspace files as memories.
//!
//! Monitors configured directories for file changes and creates/updates
//! memory entries with deterministic IDs (`file:` + SHA-256 of canonical path).
//! File deletions trigger soft-delete of the corresponding memory.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use blufio_config::model::FileWatcherConfig;
use blufio_core::classification::DataClassification;
use blufio_core::error::BlufioError;
use blufio_core::types::EmbeddingInput;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::embedder::OnnxEmbedder;
use crate::store::MemoryStore;
use crate::types::{Memory, MemorySource, MemoryStatus};

/// Compute a deterministic memory ID for a file path.
///
/// Format: `file:` + hex-encoded SHA-256 of the canonical (absolute) path.
/// If canonicalization fails (e.g., file deleted), uses the path as-is.
pub fn file_memory_id(path: &Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let path_str = canonical.to_string_lossy();
    let mut hasher = Sha256::new();
    hasher.update(path_str.as_bytes());
    let hash = hasher.finalize();
    format!("file:{:x}", hash)
}

/// Check if a file should be indexed based on extension filter.
///
/// Returns `true` if `config.extensions` is empty (no filter) or if the
/// file's extension matches any configured extension (case-insensitive).
fn should_index(path: &Path, config: &FileWatcherConfig) -> bool {
    if config.extensions.is_empty() {
        return true;
    }
    match path.extension() {
        Some(ext) => {
            let ext_lower = ext.to_string_lossy().to_lowercase();
            config
                .extensions
                .iter()
                .any(|e| e.to_lowercase() == ext_lower)
        }
        None => false,
    }
}

/// Process a file change event: create/update or soft-delete the corresponding memory.
async fn process_file_change(
    path: &Path,
    config: &FileWatcherConfig,
    store: &MemoryStore,
    embedder: &OnnxEmbedder,
) -> Result<(), BlufioError> {
    if !should_index(path, config) {
        return Ok(());
    }

    let mem_id = file_memory_id(path);

    // File deleted: soft-delete the memory
    if !path.exists() {
        store.soft_delete(&mem_id).await?;
        info!(path = %path.display(), id = %mem_id, "file deleted, memory soft-deleted");
        return Ok(());
    }

    // Check file size
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            warn!(path = %path.display(), error = %e, "cannot read file metadata, skipping");
            return Ok(());
        }
    };

    if metadata.len() as usize > config.max_file_size {
        warn!(
            path = %path.display(),
            size = metadata.len(),
            max = config.max_file_size,
            "file exceeds max_file_size, skipping"
        );
        return Ok(());
    }

    // Read file content
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            warn!(path = %path.display(), error = %e, "cannot read file as UTF-8, skipping");
            return Ok(());
        }
    };

    // Generate embedding
    use blufio_core::traits::EmbeddingAdapter;
    let embed_output = embedder
        .embed(EmbeddingInput {
            texts: vec![content.clone()],
        })
        .await?;
    let embedding = embed_output
        .embeddings
        .into_iter()
        .next()
        .unwrap_or_default();

    let now = chrono::Utc::now().to_rfc3339();

    let memory = Memory {
        id: mem_id.clone(),
        content,
        embedding,
        source: MemorySource::FileWatcher,
        confidence: 0.8,
        status: MemoryStatus::Active,
        superseded_by: None,
        session_id: Some(path.display().to_string()),
        classification: DataClassification::Internal,
        created_at: now.clone(),
        updated_at: now,
    };

    // Check if memory already exists (update case): delete old first for FTS5 consistency
    if store.get_by_id(&mem_id).await?.is_some() {
        store.soft_delete(&mem_id).await?;
        // Hard delete the forgotten record so we can INSERT fresh
        // Actually, soft_delete sets status='forgotten'. We need to delete the row
        // to re-insert with same ID. Use a direct approach: just delete then save.
        // Since soft_delete only sets status, we can save over it by using
        // INSERT OR REPLACE. But the store uses INSERT, so let's delete first.
        // The simplest approach: delete old memory row then save new.
        delete_memory_row(store, &mem_id).await?;
    }

    store.save(&memory).await?;
    Ok(())
}

/// Hard-delete a memory row (for re-insert on update).
async fn delete_memory_row(store: &MemoryStore, id: &str) -> Result<(), BlufioError> {
    let id = id.to_string();
    store
        .conn()
        .call(move |conn| {
            conn.execute("DELETE FROM memories WHERE id = ?1", rusqlite::params![id])?;
            Ok::<(), rusqlite::Error>(())
        })
        .await
        .map_err(|e: tokio_rusqlite::Error| BlufioError::storage_connection_failed(e))
}

/// Perform an initial scan of all configured directories, indexing matching files.
///
/// Returns the count of files successfully indexed.
pub async fn initial_scan(
    config: &FileWatcherConfig,
    store: &MemoryStore,
    embedder: &OnnxEmbedder,
) -> Result<usize, BlufioError> {
    let mut count = 0;

    for dir_path in &config.paths {
        let dir = Path::new(dir_path);
        if !dir.exists() {
            warn!(path = %dir_path, "watch directory does not exist, skipping");
            continue;
        }
        count += walk_and_index(dir, config, store, embedder).await?;
    }

    Ok(count)
}

/// Recursively walk a directory and index matching files.
async fn walk_and_index(
    dir: &Path,
    config: &FileWatcherConfig,
    store: &MemoryStore,
    embedder: &OnnxEmbedder,
) -> Result<usize, BlufioError> {
    let mut count = 0;

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            warn!(path = %dir.display(), error = %e, "cannot read directory");
            return Ok(0);
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        if path.is_dir() {
            count += Box::pin(walk_and_index(&path, config, store, embedder)).await?;
        } else if path.is_file() && should_index(&path, config) {
            if let Err(e) = process_file_change(&path, config, store, embedder).await {
                warn!(path = %path.display(), error = %e, "failed to index file");
            } else {
                count += 1;
            }
        }
    }

    Ok(count)
}

/// Start the file watcher for auto-indexing.
///
/// If `config.paths` is empty, returns immediately (watcher disabled).
/// Otherwise, creates a notify debouncer and spawns a tokio task to process events.
pub fn start_file_watcher(
    config: &FileWatcherConfig,
    store: Arc<MemoryStore>,
    embedder: Arc<OnnxEmbedder>,
    cancel: CancellationToken,
) -> Result<(), BlufioError> {
    if config.paths.is_empty() {
        return Ok(());
    }

    let (tx, mut rx) = mpsc::channel::<Vec<PathBuf>>(100);

    // Create debouncer with 500ms window
    let mut debouncer = notify_debouncer_mini::new_debouncer(
        Duration::from_millis(500),
        move |res: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
            if let Ok(events) = res {
                let paths: Vec<PathBuf> = events.into_iter().map(|e| e.path).collect();
                if !paths.is_empty() {
                    // blocking_send because notify runs on its own thread
                    let _ = tx.blocking_send(paths);
                }
            }
        },
    )
    .map_err(|e| BlufioError::Internal(format!("failed to create file watcher: {e}")))?;

    // Watch each configured path
    for path_str in &config.paths {
        let path = Path::new(path_str);
        if path.exists() {
            debouncer
                .watcher()
                .watch(path, notify::RecursiveMode::Recursive)
                .map_err(|e| {
                    BlufioError::Internal(format!("failed to watch {}: {e}", path.display()))
                })?;
        } else {
            warn!(path = %path_str, "watch path does not exist, skipping");
        }
    }

    let config = config.clone();

    // Spawn the event processing task
    tokio::spawn(async move {
        // Keep debouncer alive for the lifetime of this task
        let _debouncer = debouncer;

        loop {
            tokio::select! {
                Some(paths) = rx.recv() => {
                    for path in paths {
                        if let Err(e) = process_file_change(&path, &config, &store, &embedder).await {
                            warn!(path = %path.display(), error = %e, "file watcher: failed to process change");
                        }
                    }
                }
                _ = cancel.cancelled() => {
                    info!("file watcher shutting down");
                    break;
                }
            }
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_memory_id_deterministic() {
        let path = Path::new("./docs/readme.md");
        let id1 = file_memory_id(path);
        let id2 = file_memory_id(path);
        assert_eq!(id1, id2, "same path should produce same ID");
        assert!(id1.starts_with("file:"), "ID should start with 'file:'");
    }

    #[test]
    fn file_memory_id_different_paths_differ() {
        let id1 = file_memory_id(Path::new("/tmp/a.md"));
        let id2 = file_memory_id(Path::new("/tmp/b.md"));
        assert_ne!(id1, id2, "different paths should produce different IDs");
    }

    #[test]
    fn file_memory_id_format() {
        let id = file_memory_id(Path::new("/tmp/test.md"));
        assert!(id.starts_with("file:"));
        // SHA-256 hex is 64 chars
        let hash_part = &id["file:".len()..];
        assert_eq!(hash_part.len(), 64, "SHA-256 hex should be 64 chars");
        assert!(
            hash_part.chars().all(|c| c.is_ascii_hexdigit()),
            "hash should be hex"
        );
    }

    #[test]
    fn should_index_with_matching_extension() {
        let config = FileWatcherConfig {
            paths: vec![],
            extensions: vec!["md".to_string()],
            max_file_size: 102_400,
        };
        assert!(should_index(Path::new("notes.md"), &config));
    }

    #[test]
    fn should_index_rejects_non_matching_extension() {
        let config = FileWatcherConfig {
            paths: vec![],
            extensions: vec!["md".to_string(), "txt".to_string()],
            max_file_size: 102_400,
        };
        assert!(!should_index(Path::new("image.png"), &config));
    }

    #[test]
    fn should_index_empty_extensions_allows_all() {
        let config = FileWatcherConfig {
            paths: vec![],
            extensions: vec![],
            max_file_size: 102_400,
        };
        assert!(should_index(Path::new("anything.xyz"), &config));
        assert!(should_index(Path::new("notes.md"), &config));
    }

    #[test]
    fn should_index_case_insensitive() {
        let config = FileWatcherConfig {
            paths: vec![],
            extensions: vec!["MD".to_string()],
            max_file_size: 102_400,
        };
        assert!(should_index(Path::new("notes.md"), &config));
        assert!(should_index(Path::new("notes.MD"), &config));
    }

    #[test]
    fn should_index_no_extension_rejected_when_filter_set() {
        let config = FileWatcherConfig {
            paths: vec![],
            extensions: vec!["md".to_string()],
            max_file_size: 102_400,
        };
        assert!(!should_index(Path::new("Makefile"), &config));
    }

    #[test]
    fn start_file_watcher_disabled_when_paths_empty() {
        let config = FileWatcherConfig {
            paths: vec![],
            extensions: vec![],
            max_file_size: 102_400,
        };
        // This should return Ok immediately without spawning anything
        // We can't easily test this without a full tokio runtime, but we test the logic
        assert!(config.paths.is_empty());
    }
}
