// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio context` CLI subcommands for compaction, archive management, and
//! context status reporting.

use std::sync::Arc;

use blufio_config::model::BlufioConfig;
use blufio_context::compaction::levels::{compact_to_l1, CompactionLevel};
use blufio_context::compaction::quality::{apply_gate, evaluate_quality, QualityWeights};
use blufio_core::error::BlufioError;
use blufio_core::token_counter::{count_with_fallback, TokenizerCache, TokenizerMode};
use blufio_core::types::Message;
use blufio_core::{ProviderAdapter, StorageAdapter};
use blufio_storage::queries::archives;
use blufio_storage::{Database, SqliteStorage};
use clap::Subcommand;

/// Context management subcommands.
#[derive(Subcommand, Debug)]
pub enum ContextCommand {
    /// Run compaction on a session's messages.
    Compact {
        /// Session ID to compact.
        #[arg(long)]
        session: String,
        /// Preview only -- do not persist or delete messages.
        #[arg(long)]
        dry_run: bool,
    },
    /// Manage cross-session archives.
    Archive {
        #[command(subcommand)]
        action: ArchiveCommand,
    },
    /// Show zone usage breakdown for a session.
    Status {
        /// Session ID to inspect.
        #[arg(long)]
        session: String,
    },
}

/// Archive management subcommands.
#[derive(Subcommand, Debug)]
pub enum ArchiveCommand {
    /// List archives, optionally filtered by user.
    List {
        /// Filter by user ID.
        #[arg(long)]
        user: Option<String>,
    },
    /// View full details of an archive.
    View {
        /// Archive ID to view.
        archive_id: String,
    },
    /// Prune old archives for a user.
    Prune {
        /// User ID whose archives to prune.
        #[arg(long)]
        user: String,
        /// Number of archives to keep (default: from config max_archives).
        #[arg(long)]
        keep: Option<u32>,
    },
}

/// Run the `blufio context` subcommand.
pub async fn run_context(
    config: &BlufioConfig,
    command: ContextCommand,
) -> Result<(), BlufioError> {
    match command {
        ContextCommand::Compact { session, dry_run } => {
            run_compact(config, &session, dry_run).await
        }
        ContextCommand::Archive { action } => run_archive(config, action).await,
        ContextCommand::Status { session } => run_status(config, &session).await,
    }
}

/// Run `blufio context compact --session <id> [--dry-run]`.
async fn run_compact(
    config: &BlufioConfig,
    session_id: &str,
    dry_run: bool,
) -> Result<(), BlufioError> {
    // Use SqliteStorage for message access via StorageAdapter trait.
    let storage = SqliteStorage::new(config.storage.clone());
    storage.initialize().await?;

    // Load messages for the session.
    let messages = storage.get_messages(session_id, None).await?;
    if messages.is_empty() {
        println!("No messages found for session {session_id}");
        return Ok(());
    }

    println!("Session {session_id}: {} messages loaded", messages.len());

    // Initialize provider for compaction.
    let provider: Arc<dyn ProviderAdapter + Send + Sync> =
        Arc::new(blufio_anthropic::AnthropicProvider::new(config).await.map_err(|e| {
            eprintln!(
                "error: Anthropic API key required for compaction. \
                 Set via: config, ANTHROPIC_API_KEY env var"
            );
            e
        })?);

    let compaction_model = &config.context.compaction_model;

    // Run L0->L1 compaction.
    println!("Running L1 compaction with model {compaction_model}...");
    let result = compact_to_l1(
        provider.as_ref(),
        &messages,
        compaction_model,
        config.context.max_tokens_l1,
    )
    .await?;

    println!("\n--- Compaction Result ---");
    println!("Level: {}", result.level.as_str());
    println!("Original messages: {}", result.original_count);
    println!("Tokens saved: {}", result.tokens_saved);
    println!("\nSummary:\n{}\n", result.summary);

    // Run quality scoring if enabled.
    if config.context.quality_scoring {
        println!("--- Quality Scoring ---");
        match evaluate_quality(
            provider.as_ref(),
            &messages,
            &result.summary,
            compaction_model,
        )
        .await
        {
            Ok(scores) => {
                let weights = QualityWeights::from_config(&config.context);
                let weighted = scores.weighted_score(&weights);

                println!("Entity retention:    {:.2}", scores.entity);
                println!("Decision retention:  {:.2}", scores.decision);
                println!("Action retention:    {:.2}", scores.action);
                println!("Numerical retention: {:.2}", scores.numerical);
                println!("Weighted score:      {:.2}", weighted);

                let weakest = scores.weakest_dimension();
                let gate = apply_gate(
                    weighted,
                    config.context.quality_gate_proceed,
                    config.context.quality_gate_retry,
                    &weakest,
                );
                println!("Gate result:         {gate:?}");
            }
            Err(e) => {
                println!("Quality scoring failed: {e}");
                println!("(Would treat as 0.5 in production)");
            }
        }
    } else {
        println!("Quality scoring disabled in config.");
    }

    if dry_run {
        println!("\n[DRY RUN] No changes persisted.");
    } else {
        // Persist the compaction summary.
        blufio_context::compaction::persist_compaction_summary_with_level(
            &storage,
            session_id,
            &result.summary,
            result.original_count,
            &CompactionLevel::L1,
            result.quality_score,
        )
        .await?;

        // Delete original messages via Database for the parameterized IN clause.
        let db = Database::open(&config.storage.database_path).await?;
        let msg_ids: Vec<String> = messages.iter().map(|m| m.id.clone()).collect();
        blufio_storage::queries::messages::delete_messages_by_ids(&db, session_id, &msg_ids)
            .await?;

        println!(
            "\nCompaction persisted. {} messages deleted, summary stored.",
            messages.len()
        );
    }

    Ok(())
}

/// Run `blufio context archive <subcommand>`.
async fn run_archive(config: &BlufioConfig, command: ArchiveCommand) -> Result<(), BlufioError> {
    let db = Database::open(&config.storage.database_path).await?;

    match command {
        ArchiveCommand::List { user } => {
            let archive_list = if let Some(ref user_id) = user {
                archives::list_archives(&db, user_id, 100).await?
            } else {
                archives::list_all_archives(&db, 100).await?
            };

            if archive_list.is_empty() {
                println!("No archives found.");
                return Ok(());
            }

            // Print header.
            println!(
                "{:<12} {:<12} {:>7} {:>8} {:<14} {:<20} {:>6}",
                "ID", "USER", "SCORE", "SESSIONS", "CLASSIFICATION", "CREATED", "TOKENS"
            );
            println!("{}", "-".repeat(85));

            for arc in &archive_list {
                let id_short = if arc.id.len() > 10 {
                    &arc.id[..10]
                } else {
                    &arc.id
                };
                let user_short = if arc.user_id.len() > 10 {
                    &arc.user_id[..10]
                } else {
                    &arc.user_id
                };
                let score = arc
                    .quality_score
                    .map(|s| format!("{:.2}", s))
                    .unwrap_or_else(|| "-".to_string());
                let session_count: usize =
                    serde_json::from_str::<Vec<String>>(&arc.session_ids)
                        .map(|v| v.len())
                        .unwrap_or(0);
                let token_str = arc
                    .token_count
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "-".to_string());
                let created_short = if arc.created_at.len() > 19 {
                    &arc.created_at[..19]
                } else {
                    &arc.created_at
                };

                println!(
                    "{:<12} {:<12} {:>7} {:>8} {:<14} {:<20} {:>6}",
                    id_short,
                    user_short,
                    score,
                    session_count,
                    arc.classification,
                    created_short,
                    token_str
                );
            }

            println!("\nTotal: {} archives", archive_list.len());
        }

        ArchiveCommand::View { archive_id } => {
            let arc = archives::get_archive(&db, &archive_id).await?;
            match arc {
                Some(a) => {
                    println!("Archive: {}", a.id);
                    println!("User: {}", a.user_id);
                    println!(
                        "Quality score: {}",
                        a.quality_score
                            .map(|s| format!("{:.2}", s))
                            .unwrap_or_else(|| "-".to_string())
                    );
                    println!("Session IDs: {}", a.session_ids);
                    println!("Classification: {}", a.classification);
                    println!("Created at: {}", a.created_at);
                    println!(
                        "Token count: {}",
                        a.token_count
                            .map(|t| t.to_string())
                            .unwrap_or_else(|| "-".to_string())
                    );
                    println!("\n--- Summary ---\n{}", a.summary);
                }
                None => {
                    println!("Archive not found: {archive_id}");
                }
            }
        }

        ArchiveCommand::Prune { user, keep } => {
            let keep_count = keep.unwrap_or(config.context.max_archives);
            let all_archives = archives::list_archives(&db, &user, 1000).await?;
            let total = all_archives.len();

            if total as u32 <= keep_count {
                println!(
                    "User {user} has {} archives (keep limit: {}). Nothing to prune.",
                    total, keep_count
                );
                return Ok(());
            }

            // Archives are ordered newest-first; delete from the end (oldest).
            let to_delete = &all_archives[keep_count as usize..];
            let mut deleted = 0;
            for arc in to_delete {
                if archives::delete_archive(&db, &arc.id).await? {
                    deleted += 1;
                }
            }

            println!(
                "Pruned {} archives for user {user} (kept {} of {total}).",
                deleted, keep_count
            );
        }
    }

    Ok(())
}

/// Run `blufio context status --session <id>`.
async fn run_status(config: &BlufioConfig, session_id: &str) -> Result<(), BlufioError> {
    // Use SqliteStorage for message access via StorageAdapter trait.
    let storage = SqliteStorage::new(config.storage.clone());
    storage.initialize().await?;

    // Load messages for token counting.
    let messages = storage.get_messages(session_id, None).await?;
    if messages.is_empty() {
        println!("No messages found for session {session_id}");
        return Ok(());
    }

    // Determine compaction level from metadata.
    let compaction_level = detect_compaction_level(&messages);

    // Count tokens.
    let tokenizer_mode = if config.performance.tokenizer_mode == "fast" {
        TokenizerMode::Fast
    } else {
        TokenizerMode::Accurate
    };
    let token_cache = Arc::new(TokenizerCache::new(tokenizer_mode));
    let model = &config.context.compaction_model;
    let counter = token_cache.get_counter(model);

    let mut total_tokens: usize = 0;
    for msg in &messages {
        let count = count_with_fallback(counter.as_ref(), &msg.content).await;
        total_tokens += count;
    }

    // Zone budgets from config.
    let static_budget = config.context.static_zone_budget;
    let conditional_budget = config.context.conditional_zone_budget;
    let total_budget = config.context.context_budget;
    let dynamic_budget = total_budget
        .saturating_sub(static_budget)
        .saturating_sub(conditional_budget);

    // For status, we estimate dynamic token usage = total tokens from messages.
    let dynamic_pct = if dynamic_budget > 0 {
        (total_tokens as f64 / dynamic_budget as f64 * 100.0).min(999.9)
    } else {
        0.0
    };

    println!("Context status for session {session_id}:");
    println!();
    println!(
        "  Static zone:      - / {} budget (system prompt -- computed at assembly time)",
        static_budget
    );
    println!(
        "  Conditional zone:  - / {} budget (memories, skills, archives -- computed at assembly time)",
        conditional_budget
    );
    println!(
        "  Dynamic zone:      {} tokens / {} adaptive budget ({:.1}%)",
        total_tokens, dynamic_budget, dynamic_pct
    );
    println!(
        "  Compaction level:  {}",
        compaction_level.unwrap_or_else(|| "none".to_string())
    );
    println!("  Message count:     {}", messages.len());

    Ok(())
}

/// Detect the highest compaction level present in session messages.
fn detect_compaction_level(messages: &[Message]) -> Option<String> {
    let mut highest_level: Option<String> = None;
    for msg in messages {
        if msg.role == "system" {
            if let Some(ref meta_str) = msg.metadata {
                if let Ok(meta) = serde_json::from_str::<serde_json::Value>(meta_str) {
                    if let Some(level) = meta.get("compaction_level").and_then(|v| v.as_str()) {
                        let priority = match level {
                            "L1" => 1,
                            "L2" => 2,
                            "L3" => 3,
                            _ => 0,
                        };
                        let current_priority = highest_level
                            .as_deref()
                            .map(|l| match l {
                                "L1" => 1,
                                "L2" => 2,
                                "L3" => 3,
                                _ => 0,
                            })
                            .unwrap_or(0);
                        if priority > current_priority {
                            highest_level = Some(level.to_string());
                        }
                    }
                }
            }
        }
    }
    highest_level
}
