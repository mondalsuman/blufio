// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio shell` command implementation.
//!
//! Launches an interactive REPL with colored prompt, streaming output,
//! and readline history. Uses the three-zone context engine and records
//! costs for every LLM call. Creates a new session per invocation.

use std::sync::Arc;

use blufio_anthropic::AnthropicProvider;
use blufio_config::model::BlufioConfig;
use blufio_context::ContextEngine;
use blufio_cost::ledger::{CostRecord, FeatureType};
use blufio_cost::{pricing, BudgetTracker, CostLedger};
use blufio_core::error::BlufioError;
use blufio_core::types::{
    InboundMessage, Message, MessageContent, Session, StreamEventType, TokenUsage,
};
use blufio_core::{ProviderAdapter, StorageAdapter};
use blufio_storage::SqliteStorage;
use colored::Colorize;
use futures::StreamExt;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use tracing::{debug, info};

/// Runs the `blufio shell` interactive REPL.
///
/// Creates a CLI session, prompts for user input, and streams LLM responses
/// directly to stdout. Uses context engine for prompt assembly and records
/// costs for every call.
pub async fn run_shell(config: BlufioConfig) -> Result<(), BlufioError> {
    // Initialize storage.
    let storage = SqliteStorage::new(config.storage.clone());
    storage.initialize().await?;
    let storage: Arc<dyn StorageAdapter + Send + Sync> = Arc::new(storage);

    // Initialize Anthropic provider.
    let provider: Arc<dyn ProviderAdapter + Send + Sync> =
        Arc::new(AnthropicProvider::new(&config).await.inspect_err(|_| {
            eprintln!(
                "error: Anthropic API key required. Set via: config, ANTHROPIC_API_KEY env var, or `blufio config set-secret anthropic.api_key`"
            );
        })?);

    // Initialize context engine.
    let context_engine = Arc::new(
        ContextEngine::new(&config.agent, &config.context).await?
    );

    // Initialize cost ledger.
    let cost_ledger = Arc::new(
        CostLedger::open(&config.storage.database_path).await?
    );

    // Initialize budget tracker from existing ledger data.
    let budget_tracker = Arc::new(tokio::sync::Mutex::new(
        BudgetTracker::from_ledger(&config.cost, &cost_ledger).await?
    ));

    // Create a new CLI session.
    let session_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let session = Session {
        id: session_id.clone(),
        channel: "cli".to_string(),
        user_id: Some("local".to_string()),
        state: "active".to_string(),
        metadata: None,
        created_at: now.clone(),
        updated_at: now,
    };
    storage.create_session(&session).await?;

    // Set up readline editor.
    let mut rl = DefaultEditor::new().map_err(|e| {
        BlufioError::Internal(format!("failed to initialize readline: {e}"))
    })?;

    // Print welcome message.
    println!("{}", "blufio shell".bold().green());
    println!("Type {} to exit.\n", "/quit".yellow());

    // REPL loop.
    let prompt = format!("{}> ", "blufio".green());
    loop {
        match rl.readline(&prompt) {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed == "/quit" || trimmed == "/exit" {
                    break;
                }
                if trimmed.is_empty() {
                    continue;
                }

                let _ = rl.add_history_entry(&line);

                // Process the message.
                if let Err(e) = handle_shell_message(
                    &config,
                    storage.as_ref(),
                    provider.as_ref(),
                    &context_engine,
                    &cost_ledger,
                    &budget_tracker,
                    &session_id,
                    trimmed,
                )
                .await
                {
                    match &e {
                        BlufioError::BudgetExhausted { message } => {
                            eprintln!("{}", message.yellow());
                        }
                        _ => {
                            eprintln!("{}: {e}", "error".red());
                        }
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl+C
                break;
            }
            Err(ReadlineError::Eof) => {
                // Ctrl+D
                break;
            }
            Err(e) => {
                eprintln!("{}: {e}", "error".red());
                break;
            }
        }
    }

    // Log session cost summary on exit.
    let session_cost = cost_ledger.session_total(&session_id).await.unwrap_or(0.0);
    if session_cost > 0.0 {
        println!(
            "{}",
            format!("session cost: ${session_cost:.4}").dimmed()
        );
    }

    // Clean up: close session.
    storage
        .update_session_state(&session_id, "closed")
        .await?;
    storage.close().await?;

    println!("{}", "goodbye".dimmed());
    Ok(())
}

/// Handles a single shell message: persists, checks budget, assembles context via
/// context engine, streams output, records costs.
#[allow(clippy::too_many_arguments)]
async fn handle_shell_message(
    config: &BlufioConfig,
    storage: &dyn StorageAdapter,
    provider: &dyn ProviderAdapter,
    context_engine: &ContextEngine,
    cost_ledger: &CostLedger,
    budget_tracker: &tokio::sync::Mutex<BudgetTracker>,
    session_id: &str,
    input: &str,
) -> Result<(), BlufioError> {
    // Budget check before LLM call.
    {
        let mut tracker = budget_tracker.lock().await;
        tracker.check_budget()?;
    }

    // Persist user message.
    let now = chrono::Utc::now().to_rfc3339();
    let user_msg = Message {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        role: "user".to_string(),
        content: input.to_string(),
        token_count: None,
        metadata: None,
        created_at: now,
    };
    storage.insert_message(&user_msg).await?;

    // Create InboundMessage for context assembly.
    let inbound = InboundMessage {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: Some(session_id.to_string()),
        channel: "cli".to_string(),
        sender_id: "local".to_string(),
        content: MessageContent::Text(input.to_string()),
        timestamp: chrono::Utc::now().to_rfc3339(),
        metadata: None,
    };

    // Assemble context using the three-zone context engine.
    let assembled = context_engine.assemble(
        provider,
        storage,
        session_id,
        &inbound,
        &config.anthropic.default_model,
        config.anthropic.max_tokens,
    )
    .await?;

    // Record compaction costs if compaction was triggered.
    if let Some(ref compaction_usage) = assembled.compaction_usage {
        let compaction_model = assembled.compaction_model.as_deref()
            .unwrap_or("claude-haiku-4-5-20250901");
        let model_pricing = pricing::get_pricing(compaction_model);
        let cost_usd = pricing::calculate_cost(compaction_usage, &model_pricing);

        let record = CostRecord::new(
            session_id.to_string(),
            compaction_model.to_string(),
            FeatureType::Compaction,
            compaction_usage,
            cost_usd,
        );

        cost_ledger.record(&record).await?;

        {
            let mut tracker = budget_tracker.lock().await;
            tracker.record_cost(cost_usd);
        }

        info!(
            session_id = %session_id,
            model = %compaction_model,
            cost_usd = cost_usd,
            "compaction cost recorded"
        );
    }

    // Stream the response.
    let mut stream = provider.stream(assembled.request).await?;
    let mut full_response = String::new();
    let mut usage: Option<TokenUsage> = None;

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => match chunk.event_type {
                StreamEventType::ContentBlockDelta => {
                    if let Some(text) = &chunk.text {
                        print!("{text}");
                        std::io::Write::flush(&mut std::io::stdout()).ok();
                        full_response.push_str(text);
                    }
                }
                StreamEventType::MessageStart | StreamEventType::MessageDelta => {
                    if let Some(u) = chunk.usage {
                        usage = Some(u);
                    }
                }
                StreamEventType::MessageStop => {
                    break;
                }
                StreamEventType::Error => {
                    if let Some(err) = &chunk.error {
                        eprintln!("\n{}: {err}", "error".red());
                    }
                    break;
                }
                _ => {}
            },
            Err(e) => {
                eprintln!("\n{}: {e}", "error".red());
                break;
            }
        }
    }

    // Print newline after response.
    println!();

    // Persist assistant response.
    let now = chrono::Utc::now().to_rfc3339();
    let assistant_msg = Message {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        role: "assistant".to_string(),
        content: full_response,
        token_count: usage.as_ref().map(|u| i64::from(u.output_tokens)),
        metadata: None,
        created_at: now,
    };
    storage.insert_message(&assistant_msg).await?;

    // Record message cost.
    if let Some(ref usage) = usage {
        let model_pricing = pricing::get_pricing(&config.anthropic.default_model);
        let cost_usd = pricing::calculate_cost(usage, &model_pricing);

        let record = CostRecord::new(
            session_id.to_string(),
            config.anthropic.default_model.clone(),
            FeatureType::Message,
            usage,
            cost_usd,
        );

        cost_ledger.record(&record).await?;

        {
            let mut tracker = budget_tracker.lock().await;
            tracker.record_cost(cost_usd);
        }

        debug!(
            session_id = %session_id,
            input_tokens = usage.input_tokens,
            output_tokens = usage.output_tokens,
            cost_usd = cost_usd,
            "shell response complete"
        );
    }

    Ok(())
}
