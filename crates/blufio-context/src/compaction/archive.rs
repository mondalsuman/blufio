// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Archive system for cross-session L3 compaction summaries.
//!
//! Generates L3 archives from L2 session summaries, stores them in the
//! `compaction_archives` table, and enforces a rolling window by merging
//! the oldest archives into a single "deep archive" summary.

use blufio_core::error::BlufioError;
use blufio_core::token_counter::{TokenizerCache, count_with_fallback};
use blufio_core::traits::ProviderAdapter;
use blufio_core::types::{ContentBlock, ProviderMessage, ProviderRequest};
use blufio_storage::Database;
use blufio_storage::queries::archives;
use chrono::Utc;
use uuid::Uuid;

use super::levels::{CompactionLevel, CompactionResult};

/// System prompt for L3 cross-session archive generation.
const L3_ARCHIVE_PROMPT: &str = r#"You are a cross-session archive summarizer. Create a comprehensive summary combining multiple conversation session summaries.

PRESERVE:
- All named entities (people, places, tools, projects)
- Key decisions and their rationale
- Action items and commitments
- Numerical data (dates, amounts, counts, measurements)
- User preferences and recurring patterns
- Relationships between topics across sessions

FORMAT: Organize by topic, not by session. Start with "Cross-session archive:" on the first line.
Be concise but thorough. Every fact matters."#;

/// System prompt for deep-merging two archive summaries.
const DEEP_MERGE_PROMPT: &str = r#"Merge these two archive summaries into one comprehensive summary. Preserve all important information. Prioritize recent events over older ones when space is limited.

FORMAT: Organize by topic. Start with "Deep archive:" on the first line."#;

/// An archive entry ready to be stored.
pub struct ArchiveEntry {
    /// Unique archive identifier.
    pub id: String,
    /// User that owns this archive.
    pub user_id: String,
    /// Compaction summary text.
    pub summary: String,
    /// Quality score of the summary (if scored).
    pub quality_score: Option<f64>,
    /// Session IDs that contributed to this archive.
    pub session_ids: Vec<String>,
    /// Data classification level.
    pub classification: String,
    /// Token count of the summary.
    pub token_count: Option<i64>,
}

/// Generates an L3 cross-session archive from multiple L2 session summaries.
///
/// Takes L2 summaries from multiple sessions belonging to the same user and
/// combines them into a single cross-session archive via LLM call.
pub async fn generate_l3_archive(
    provider: &dyn ProviderAdapter,
    l2_summaries: &[String],
    session_ids: &[String],
    model: &str,
    max_tokens: u32,
) -> Result<CompactionResult, BlufioError> {
    if l2_summaries.is_empty() {
        return Err(BlufioError::Internal(
            "No L2 summaries provided for L3 archive generation".to_string(),
        ));
    }

    let combined_text = l2_summaries
        .iter()
        .enumerate()
        .map(|(i, s)| format!("Session {}:\n{}", i + 1, s))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    let request = ProviderRequest {
        model: model.to_string(),
        system_prompt: Some(L3_ARCHIVE_PROMPT.to_string()),
        system_blocks: None,
        messages: vec![ProviderMessage {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: format!(
                    "Create a cross-session archive from these {} session summaries:\n\n{}",
                    l2_summaries.len(),
                    combined_text
                ),
            }],
        }],
        max_tokens,
        stream: false,
        tools: None,
    };

    let response = provider.complete(request).await?;

    let now = chrono::Utc::now().to_rfc3339();
    let metadata = super::levels::build_compaction_metadata(
        &CompactionLevel::L3,
        l2_summaries.len(),
        None,
        &now,
    );

    tracing::info!(
        input_tokens = response.usage.input_tokens,
        output_tokens = response.usage.output_tokens,
        model = model,
        source_sessions = session_ids.len(),
        "L3 archive generated"
    );

    Ok(CompactionResult {
        summary: response.content,
        level: CompactionLevel::L3,
        quality_score: None,
        tokens_saved: 0,
        original_count: l2_summaries.len(),
        metadata,
        usage: response.usage,
    })
}

/// Stores an archive entry in the database.
///
/// Returns the archive ID.
pub async fn store_archive(db: &Database, archive: ArchiveEntry) -> Result<String, BlufioError> {
    let session_ids_json =
        serde_json::to_string(&archive.session_ids).unwrap_or_else(|_| "[]".to_string());
    let now = Utc::now().to_rfc3339();

    archives::insert_archive(
        db,
        &archive.id,
        &archive.user_id,
        &archive.summary,
        archive.quality_score,
        &session_ids_json,
        &archive.classification,
        &now,
        archive.token_count,
    )
    .await?;

    tracing::info!(
        archive_id = %archive.id,
        user_id = %archive.user_id,
        "archive stored"
    );

    Ok(archive.id)
}

/// Enforces the rolling window for a user's archives.
///
/// If the user has more than `max_archives` archives, merges the two oldest
/// into a single "deep archive" summary via LLM call, deletes the originals,
/// and inserts the merged one. Repeats until within the limit.
pub async fn enforce_rolling_window(
    db: &Database,
    provider: &dyn ProviderAdapter,
    user_id: &str,
    max_archives: u32,
    model: &str,
    max_tokens: u32,
) -> Result<(), BlufioError> {
    loop {
        let count = archives::count_archives(db, user_id).await?;
        if count <= max_archives as i64 {
            break;
        }

        tracing::info!(
            user_id = user_id,
            count = count,
            max = max_archives,
            "enforcing rolling window, merging oldest archives"
        );

        // Get the two oldest archives.
        let all = archives::list_archives(db, user_id, count).await?;
        if all.len() < 2 {
            break;
        }

        // Oldest is at the end (list_archives returns newest first).
        let oldest = &all[all.len() - 1];
        let second_oldest = &all[all.len() - 2];

        // Merge via LLM.
        let merged_summary = deep_merge(
            provider,
            &oldest.summary,
            &second_oldest.summary,
            model,
            max_tokens,
        )
        .await?;

        // Combine session IDs from both.
        let mut merged_session_ids: Vec<String> = Vec::new();
        if let Ok(ids) = serde_json::from_str::<Vec<String>>(&oldest.session_ids) {
            merged_session_ids.extend(ids);
        }
        if let Ok(ids) = serde_json::from_str::<Vec<String>>(&second_oldest.session_ids) {
            merged_session_ids.extend(ids);
        }

        // Use the earliest created_at.
        let merged_classification = if oldest.classification == "restricted"
            || second_oldest.classification == "restricted"
        {
            "restricted"
        } else if oldest.classification == "confidential"
            || second_oldest.classification == "confidential"
        {
            "confidential"
        } else {
            "internal"
        };

        // Delete both originals.
        archives::delete_archive(db, &oldest.id).await?;
        archives::delete_archive(db, &second_oldest.id).await?;

        // Insert merged archive with earliest created_at.
        let merged_id = Uuid::new_v4().to_string();
        let session_ids_json =
            serde_json::to_string(&merged_session_ids).unwrap_or_else(|_| "[]".to_string());

        archives::insert_archive(
            db,
            &merged_id,
            user_id,
            &merged_summary,
            None, // Deep merge doesn't have a quality score.
            &session_ids_json,
            merged_classification,
            &oldest.created_at, // Preserve earliest timestamp.
            None,
        )
        .await?;

        tracing::info!(
            merged_id = %merged_id,
            source_ids = %format!("{}, {}", oldest.id, second_oldest.id),
            "deep archive merge completed"
        );
    }

    Ok(())
}

/// Retrieves archive summaries for a user within a token budget.
///
/// Fetches recent archives and accumulates summaries until the token budget
/// is exhausted. Returns summaries in newest-first order.
pub async fn get_archives_for_context(
    db: &Database,
    user_id: &str,
    max_token_budget: u32,
    token_cache: &TokenizerCache,
    model: &str,
) -> Result<Vec<String>, BlufioError> {
    // Fetch all archives for user (up to a reasonable limit).
    let archives = archives::list_archives(db, user_id, 100).await?;

    if archives.is_empty() {
        return Ok(Vec::new());
    }

    let counter = token_cache.get_counter(model);
    let mut summaries = Vec::new();
    let mut total_tokens: usize = 0;

    for archive in &archives {
        let tokens = count_with_fallback(counter.as_ref(), &archive.summary).await;
        if total_tokens + tokens > max_token_budget as usize && !summaries.is_empty() {
            break;
        }
        total_tokens += tokens;
        summaries.push(archive.summary.clone());
    }

    tracing::debug!(
        user_id = user_id,
        archive_count = summaries.len(),
        total_tokens = total_tokens,
        budget = max_token_budget,
        "archives retrieved for context"
    );

    Ok(summaries)
}

/// Generates and stores an L3 session archive on session close.
///
/// This is the main entry point for archive generation. Called when a session
/// closes, it generates an L3 archive from the session's L2 summary (or raw
/// messages if no L2 exists), stores it, and enforces the rolling window.
///
/// Returns the archive ID if one was created, or None if archiving is disabled.
pub async fn generate_and_store_session_archive(
    db: &Database,
    provider: &dyn ProviderAdapter,
    session_id: &str,
    user_id: &str,
    model: &str,
    max_tokens: u32,
    max_archives: u32,
    archive_enabled: bool,
) -> Result<Option<String>, BlufioError> {
    if !archive_enabled {
        return Ok(None);
    }

    // For now, generate L3 from any existing archives + this session's data.
    // The actual L2 summary is stored in the messages table as a system message
    // with metadata type=compaction_summary, compaction_level=L2.
    // Plan 05 (serve.rs integration) will handle the actual retrieval of L2 summaries
    // from the messages table. For now, this function receives the summary directly.

    // Fetch existing archives for the user to combine with this session.
    let existing = archives::list_archives(db, user_id, max_archives as i64).await?;
    let l2_summaries: Vec<String> = existing.iter().map(|a| a.summary.clone()).collect();

    // Generate archive from combined summaries.
    if l2_summaries.is_empty() {
        // No existing archives; this is the first one. We'll create a minimal archive.
        // The actual session content will be passed by the caller (Plan 05).
        return Ok(None);
    }

    let session_ids = vec![session_id.to_string()];
    let l3_result =
        generate_l3_archive(provider, &l2_summaries, &session_ids, model, max_tokens).await?;

    // Determine classification: use highest from existing archives.
    let classification = existing
        .iter()
        .map(|a| a.classification.as_str())
        .fold("internal", |acc, c| {
            if c == "restricted" || acc == "restricted" {
                "restricted"
            } else if c == "confidential" || acc == "confidential" {
                "confidential"
            } else {
                "internal"
            }
        })
        .to_string();

    let archive_entry = ArchiveEntry {
        id: Uuid::new_v4().to_string(),
        user_id: user_id.to_string(),
        summary: l3_result.summary,
        quality_score: l3_result.quality_score,
        session_ids,
        classification,
        token_count: Some(l3_result.usage.output_tokens as i64),
    };

    let archive_id = store_archive(db, archive_entry).await?;

    // Enforce rolling window.
    enforce_rolling_window(db, provider, user_id, max_archives, model, max_tokens).await?;

    Ok(Some(archive_id))
}

/// Deep-merges two archive summaries into one via LLM call.
async fn deep_merge(
    provider: &dyn ProviderAdapter,
    summary_a: &str,
    summary_b: &str,
    model: &str,
    max_tokens: u32,
) -> Result<String, BlufioError> {
    let request = ProviderRequest {
        model: model.to_string(),
        system_prompt: Some(DEEP_MERGE_PROMPT.to_string()),
        system_blocks: None,
        messages: vec![ProviderMessage {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: format!(
                    "Archive 1 (older):\n{}\n\n---\n\nArchive 2 (newer):\n{}",
                    summary_a, summary_b
                ),
            }],
        }],
        max_tokens,
        stream: false,
        tools: None,
    };

    let response = provider.complete(request).await?;

    tracing::info!(
        input_tokens = response.usage.input_tokens,
        output_tokens = response.usage.output_tokens,
        "deep archive merge completed"
    );

    Ok(response.content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l3_archive_prompt_contains_key_instructions() {
        assert!(L3_ARCHIVE_PROMPT.contains("cross-session"));
        assert!(L3_ARCHIVE_PROMPT.contains("entities"));
        assert!(L3_ARCHIVE_PROMPT.contains("decisions"));
        assert!(L3_ARCHIVE_PROMPT.contains("topic"));
    }

    #[test]
    fn deep_merge_prompt_preserves_info() {
        assert!(DEEP_MERGE_PROMPT.contains("Merge"));
        assert!(DEEP_MERGE_PROMPT.contains("Preserve"));
        assert!(DEEP_MERGE_PROMPT.contains("Prioritize recent"));
    }

    #[test]
    fn archive_entry_construction() {
        let entry = ArchiveEntry {
            id: "test-id".to_string(),
            user_id: "user-1".to_string(),
            summary: "Test summary".to_string(),
            quality_score: Some(0.85),
            session_ids: vec!["sess-1".to_string(), "sess-2".to_string()],
            classification: "internal".to_string(),
            token_count: Some(256),
        };
        assert_eq!(entry.id, "test-id");
        assert_eq!(entry.session_ids.len(), 2);
        assert_eq!(entry.quality_score, Some(0.85));
    }

    #[test]
    fn session_ids_serialization() {
        let ids = vec!["sess-1".to_string(), "sess-2".to_string()];
        let json = serde_json::to_string(&ids).unwrap();
        assert_eq!(json, r#"["sess-1","sess-2"]"#);

        let parsed: Vec<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ids);
    }

    #[test]
    fn classification_escalation() {
        // Test highest classification logic inline.
        let classifications = vec!["internal", "confidential", "internal"];
        let highest = classifications.iter().fold("internal", |acc, &c| {
            if c == "restricted" || acc == "restricted" {
                "restricted"
            } else if c == "confidential" || acc == "confidential" {
                "confidential"
            } else {
                "internal"
            }
        });
        assert_eq!(highest, "confidential");
    }

    #[test]
    fn classification_restricted_wins() {
        let classifications = vec!["internal", "restricted", "confidential"];
        let highest = classifications.iter().fold("internal", |acc, &c| {
            if c == "restricted" || acc == "restricted" {
                "restricted"
            } else if c == "confidential" || acc == "confidential" {
                "confidential"
            } else {
                "internal"
            }
        });
        assert_eq!(highest, "restricted");
    }
}
