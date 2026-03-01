// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio serve` command implementation.
//!
//! Starts the full Blufio agent with Telegram channel, Anthropic provider,
//! SQLite storage, three-zone context engine, and cost tracking.
//! Installs signal handlers for graceful shutdown.

use std::path::PathBuf;
use std::sync::Arc;

use blufio_agent::{AgentLoop, HeartbeatRunner};
use blufio_agent::shutdown;
use blufio_anthropic::AnthropicProvider;
use blufio_config::model::BlufioConfig;
use blufio_context::ContextEngine;
use blufio_cost::{BudgetTracker, CostLedger};
use blufio_core::error::BlufioError;
use blufio_core::{ChannelAdapter, StorageAdapter};
use blufio_memory::{MemoryExtractor, MemoryProvider, MemoryStore, HybridRetriever, OnnxEmbedder, ModelManager};
use blufio_router::ModelRouter;
use blufio_skill::{SkillProvider, ToolRegistry};
use blufio_storage::SqliteStorage;
use blufio_telegram::TelegramChannel;
use tracing::{debug, error, info, warn};

/// Runs the `blufio serve` command.
///
/// Initializes all adapters (storage, provider, channel), context engine,
/// cost ledger and budget tracker. Marks stale sessions as interrupted
/// (crash recovery), installs signal handlers, and enters the main agent loop.
pub async fn run_serve(config: BlufioConfig) -> Result<(), BlufioError> {
    // Initialize tracing subscriber.
    init_tracing(&config.agent.log_level);

    info!("starting blufio serve");

    // Initialize storage.
    let storage = SqliteStorage::new(config.storage.clone());
    storage.initialize().await?;
    let storage = Arc::new(storage);

    // Mark stale sessions as interrupted (crash recovery).
    mark_stale_sessions(storage.as_ref()).await?;

    // Initialize cost ledger (opens its own connection to the same DB).
    let cost_ledger = Arc::new(
        CostLedger::open(&config.storage.database_path).await?
    );

    // Initialize budget tracker from existing ledger data (restart recovery).
    let budget_tracker = Arc::new(tokio::sync::Mutex::new(
        BudgetTracker::from_ledger(&config.cost, &cost_ledger).await?
    ));

    // Initialize context engine.
    let mut context_engine =
        ContextEngine::new(&config.agent, &config.context).await?;

    // Initialize memory system (if enabled).
    let (memory_provider, memory_extractor) = if config.memory.enabled {
        match initialize_memory(&config, &mut context_engine).await {
            Ok((mp, me)) => (Some(mp), Some(me)),
            Err(e) => {
                warn!(error = %e, "memory system initialization failed, continuing without memory");
                (None, None)
            }
        }
    } else {
        info!("memory system disabled by configuration");
        (None, None)
    };

    // Initialize tool registry with built-in tools.
    let mut tool_registry = ToolRegistry::new();
    blufio_skill::builtin::register_builtins(&mut tool_registry);
    info!("tool registry initialized with {} built-in tools", tool_registry.len());
    let tool_registry = Arc::new(tokio::sync::RwLock::new(tool_registry));

    // Register SkillProvider with context engine for progressive tool discovery.
    let skill_provider = SkillProvider::new(
        tool_registry.clone(),
        config.skill.max_skills_in_prompt,
    );
    context_engine.add_conditional_provider(Box::new(skill_provider));

    let context_engine = Arc::new(context_engine);

    // Initialize Anthropic provider.
    let provider = AnthropicProvider::new(&config).await.map_err(|e| {
        error!(error = %e, "failed to initialize Anthropic provider");
        eprintln!(
            "error: Anthropic API key required. Set via: config, ANTHROPIC_API_KEY env var, or `blufio config set-secret anthropic.api_key`"
        );
        e
    })?;
    let provider = Arc::new(provider);

    // Initialize Telegram channel.
    let mut channel = TelegramChannel::new(config.telegram.clone()).map_err(|e| {
        error!(error = %e, "failed to initialize Telegram channel");
        eprintln!(
            "error: Telegram bot token required. Set via: config or `blufio config set-secret telegram.bot_token`"
        );
        e
    })?;

    // Connect to Telegram.
    channel.connect().await?;
    info!("Telegram channel connected");

    // Initialize model router.
    let router = Arc::new(ModelRouter::new(config.routing.clone()));
    if config.routing.enabled {
        if let Some(ref forced) = config.routing.force_model {
            info!(model = forced.as_str(), "model routing enabled with forced model");
        } else {
            info!(
                simple = config.routing.simple_model.as_str(),
                standard = config.routing.standard_model.as_str(),
                complex = config.routing.complex_model.as_str(),
                "model routing enabled"
            );
        }
    } else {
        info!(
            model = config.anthropic.default_model.as_str(),
            "model routing disabled, using default model"
        );
    }

    // Initialize heartbeat runner (if enabled).
    let heartbeat_runner = if config.heartbeat.enabled {
        let runner = Arc::new(HeartbeatRunner::new(
            config.heartbeat.clone(),
            provider.clone(),
            storage.clone(),
            cost_ledger.clone(),
        ));
        info!(
            interval_secs = config.heartbeat.interval_secs,
            delivery = config.heartbeat.delivery.as_str(),
            monthly_budget = config.heartbeat.monthly_budget_usd,
            "heartbeat system enabled"
        );
        Some(runner)
    } else {
        info!("heartbeat system disabled");
        None
    };

    // Install signal handler.
    let cancel = shutdown::install_signal_handler();

    // Spawn heartbeat background task if enabled.
    if let Some(ref runner) = heartbeat_runner {
        let hb_runner = runner.clone();
        let hb_cancel = cancel.clone();
        let interval_secs = config.heartbeat.interval_secs;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                std::time::Duration::from_secs(interval_secs),
            );
            // Skip the first immediate tick.
            interval.tick().await;

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        match hb_runner.execute().await {
                            Ok(Some(result)) if result.has_content => {
                                info!(
                                    content_len = result.content.len(),
                                    "heartbeat generated actionable content"
                                );
                            }
                            Ok(Some(_)) => {
                                debug!("heartbeat executed but no actionable content");
                            }
                            Ok(None) => {
                                debug!("heartbeat skipped (unchanged state)");
                            }
                            Err(e) => {
                                warn!(error = %e, "heartbeat execution failed (non-fatal)");
                            }
                        }
                    }
                    _ = hb_cancel.cancelled() => {
                        info!("heartbeat task shutting down");
                        break;
                    }
                }
            }
        });
    }

    // Create and run agent loop with context engine, cost tracking, routing, and tools.
    let mut agent_loop = AgentLoop::new(
        Box::new(channel),
        provider,
        storage,
        context_engine,
        cost_ledger,
        budget_tracker,
        memory_provider,
        memory_extractor,
        router,
        heartbeat_runner,
        tool_registry,
        config,
    )
    .await?;

    agent_loop.run(cancel).await?;

    info!("blufio serve shutdown complete");
    Ok(())
}

/// Marks any sessions that were left in "active" state as "interrupted".
///
/// This handles the case where the process was previously killed without
/// graceful shutdown, leaving sessions in an active state.
async fn mark_stale_sessions(
    storage: &dyn StorageAdapter,
) -> Result<(), BlufioError> {
    let active_sessions = storage.list_sessions(Some("active")).await?;
    if !active_sessions.is_empty() {
        info!(
            count = active_sessions.len(),
            "marking stale sessions as interrupted"
        );
        for session in &active_sessions {
            storage
                .update_session_state(&session.id, "interrupted")
                .await?;
        }
    }
    Ok(())
}

/// Initializes the memory system: downloads model, creates embedder, store,
/// retriever, provider, and extractor. Registers the provider with ContextEngine.
///
/// Returns (MemoryProvider, MemoryExtractor) on success.
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

/// Initializes the tracing subscriber with the given log level.
fn init_tracing(log_level: &str) {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("blufio={log_level},warn")));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_names(false)
        .init();
}
