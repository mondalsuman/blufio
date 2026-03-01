// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio shell` command implementation.
//!
//! Launches an interactive REPL with colored prompt, streaming output,
//! and readline history. Uses the three-zone context engine and records
//! costs for every LLM call. Creates a new session per invocation.

use std::path::PathBuf;
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
use blufio_memory::{MemoryExtractor, MemoryProvider, MemoryStore, HybridRetriever, OnnxEmbedder, ModelManager};
use blufio_router::ModelRouter;
use blufio_storage::SqliteStorage;
use colored::Colorize;
use futures::StreamExt;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use tracing::{debug, info, warn};

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
    let mut context_engine =
        ContextEngine::new(&config.agent, &config.context).await?;

    // Initialize memory system (if enabled).
    #[cfg(feature = "onnx")]
    let memory_provider: Option<MemoryProvider> = if config.memory.enabled {
        match initialize_memory(&config, &mut context_engine).await {
            Ok((mp, _extractor)) => {
                // Shell mode uses the provider for retrieval; extractor is used
                // inline below for explicit commands.
                Some(mp)
            }
            Err(e) => {
                warn!(error = %e, "memory system initialization failed, continuing without memory");
                None
            }
        }
    } else {
        info!("memory system disabled by configuration");
        None
    };

    #[cfg(not(feature = "onnx"))]
    let memory_provider: Option<MemoryProvider> = {
        info!("memory system disabled (onnx feature not enabled)");
        None
    };

    let context_engine = Arc::new(context_engine);

    // Initialize cost ledger.
    let cost_ledger = Arc::new(
        CostLedger::open(&config.storage.database_path).await?
    );

    // Initialize budget tracker from existing ledger data.
    let budget_tracker = Arc::new(tokio::sync::Mutex::new(
        BudgetTracker::from_ledger(&config.cost, &cost_ledger).await?
    ));

    // Initialize model router for per-message routing (even in shell mode).
    let router = Arc::new(ModelRouter::new(config.routing.clone()));

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
                    memory_provider.as_ref(),
                    &router,
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

/// Handles a single shell message: persists, checks budget, routes model,
/// assembles context via context engine, streams output, records costs.
#[allow(clippy::too_many_arguments)]
async fn handle_shell_message(
    config: &BlufioConfig,
    storage: &dyn StorageAdapter,
    provider: &dyn ProviderAdapter,
    context_engine: &ContextEngine,
    cost_ledger: &CostLedger,
    budget_tracker: &tokio::sync::Mutex<BudgetTracker>,
    memory_provider: Option<&MemoryProvider>,
    router: &ModelRouter,
    session_id: &str,
    input: &str,
) -> Result<(), BlufioError> {
    // Budget check before LLM call.
    {
        let mut tracker = budget_tracker.lock().await;
        tracker.check_budget()?;
    }

    // Parse per-message model override and strip prefix.
    let (_, clean_input) = blufio_router::parse_model_override(input);

    // Persist user message (with override prefix stripped).
    let now = chrono::Utc::now().to_rfc3339();
    let user_msg = Message {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        role: "user".to_string(),
        content: clean_input.to_string(),
        token_count: None,
        metadata: None,
        created_at: now,
    };
    storage.insert_message(&user_msg).await?;

    // Route the message to the appropriate model.
    let (model, max_tokens, intended_model) = if config.routing.enabled {
        let recent_msgs = storage.get_messages(session_id, Some(3)).await?;
        let recent_strings: Vec<String> = recent_msgs.iter().map(|m| m.content.clone()).collect();
        let recent_refs: Vec<&str> = recent_strings.iter().map(|s| s.as_str()).collect();

        let budget_util = {
            let tracker = budget_tracker.lock().await;
            tracker.budget_utilization()
        };

        let decision = router.route(input, &recent_refs, budget_util);

        if decision.downgraded {
            let short = ModelRouter::short_model_name(&decision.actual_model);
            eprintln!(
                "{}",
                format!("(Using {short} -- budget downgrade)").dimmed()
            );
        }

        let model = decision.actual_model.clone();
        let max_tokens = decision.max_tokens;
        let intended = Some(decision.intended_model.clone());
        (model, max_tokens, intended)
    } else {
        (config.anthropic.default_model.clone(), config.anthropic.max_tokens, None)
    };

    // Set current query on memory provider for retrieval.
    if let Some(mp) = memory_provider {
        mp.set_current_query(session_id, clean_input).await;
    }

    // Create InboundMessage for context assembly.
    let inbound = InboundMessage {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: Some(session_id.to_string()),
        channel: "cli".to_string(),
        sender_id: "local".to_string(),
        content: MessageContent::Text(clean_input.to_string()),
        timestamp: chrono::Utc::now().to_rfc3339(),
        metadata: None,
    };

    // Assemble context using the three-zone context engine.
    let assembled = context_engine.assemble(
        provider,
        storage,
        session_id,
        &inbound,
        &model,
        max_tokens,
    )
    .await;

    // Clear current query on memory provider (regardless of assembly outcome).
    if let Some(mp) = memory_provider {
        mp.clear_current_query(session_id).await;
    }

    let assembled = assembled?;

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

    // Record message cost with routed model and intended_model tracking.
    if let Some(ref usage) = usage {
        let model_pricing = pricing::get_pricing(&model);
        let cost_usd = pricing::calculate_cost(usage, &model_pricing);

        let mut record = CostRecord::new(
            session_id.to_string(),
            model.clone(),
            FeatureType::Message,
            usage,
            cost_usd,
        );
        if let Some(ref intended) = intended_model {
            record = record.with_intended_model(intended.clone());
        }

        cost_ledger.record(&record).await?;

        {
            let mut tracker = budget_tracker.lock().await;
            tracker.record_cost(cost_usd);
        }

        debug!(
            session_id = %session_id,
            model = %model,
            intended_model = ?intended_model,
            input_tokens = usage.input_tokens,
            output_tokens = usage.output_tokens,
            cost_usd = cost_usd,
            "shell response complete"
        );
    }

    Ok(())
}

/// Initializes the memory system: downloads model, creates embedder, store,
/// retriever, provider, and extractor. Registers the provider with ContextEngine.
///
/// Returns (MemoryProvider, MemoryExtractor) on success.
#[allow(dead_code)]
async fn initialize_memory(
    config: &BlufioConfig,
    context_engine: &mut ContextEngine,
) -> Result<(MemoryProvider, Arc<MemoryExtractor>), BlufioError> {
    // Determine data directory (parent of the database path).
    let db_path = PathBuf::from(&config.storage.database_path);
    let data_dir = db_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    // Download model on first run.
    let model_manager = ModelManager::new(data_dir);
    info!("ensuring embedding model is available...");
    let model_path = model_manager.ensure_model().await?;
    info!(path = %model_path.display(), "embedding model ready");

    // Create ONNX embedder.
    let embedder = Arc::new(OnnxEmbedder::new(&model_path)?);

    // Create memory store (opens its own connection to the same DB).
    let memory_conn = tokio_rusqlite::Connection::open(&config.storage.database_path)
        .await
        .map_err(|e| BlufioError::Storage {
            source: Box::new(e),
        })?;
    let memory_store = Arc::new(MemoryStore::new(memory_conn));

    // Create hybrid retriever.
    let retriever = Arc::new(HybridRetriever::new(
        memory_store.clone(),
        embedder.clone(),
        config.memory.clone(),
    ));

    // Create memory provider and register with context engine.
    let memory_provider = MemoryProvider::new(retriever);
    context_engine.add_conditional_provider(Box::new(memory_provider.clone()));

    // Create memory extractor.
    let extractor = Arc::new(MemoryExtractor::new(
        memory_store,
        embedder,
        config.memory.extraction_model.clone(),
    ));

    info!("memory system initialized");
    Ok((memory_provider, extractor))
}
