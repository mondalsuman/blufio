// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Storage, cost ledger, tokenizer, context engine, and memory initialization
//! for the `blufio serve` command.

use std::path::PathBuf;
use std::sync::Arc;

use blufio_config::model::BlufioConfig;
use blufio_context::ContextEngine;
use blufio_core::error::BlufioError;
use blufio_core::token_counter::{TokenizerCache, TokenizerMode};
use blufio_core::StorageAdapter;
use blufio_cost::{BudgetTracker, CostLedger};
use blufio_memory::{HybridRetriever, MemoryExtractor, MemoryProvider, MemoryStore, ModelManager, OnnxEmbedder};
use tracing::{debug, info, warn};

#[cfg(feature = "sqlite")]
use blufio_storage::SqliteStorage;

/// Initialize SQLite storage (migrations included).
pub(crate) async fn init_storage(
    config: &BlufioConfig,
) -> Result<Arc<SqliteStorage>, BlufioError> {
    #[cfg(feature = "sqlite")]
    {
        let storage = SqliteStorage::new(config.storage.clone());
        storage.initialize().await?;
        Ok(Arc::new(storage))
    }

    #[cfg(not(feature = "sqlite"))]
    compile_error!("blufio requires the 'sqlite' feature for storage");
}

/// Apply Litestream WAL pragma and warn about SQLCipher incompatibility.
pub(crate) async fn apply_litestream_pragma(config: &BlufioConfig) -> Result<(), BlufioError> {
    if config.litestream.enabled {
        info!("Litestream mode active: disabling WAL autocheckpoint (PRAGMA wal_autocheckpoint=0)");
        let pragma_conn = blufio_storage::open_connection(&config.storage.database_path).await?;
        pragma_conn
            .call(|conn| conn.execute_batch("PRAGMA wal_autocheckpoint=0;"))
            .await
            .map_err(|e: tokio_rusqlite::Error| {
                BlufioError::Config(format!(
                    "failed to set wal_autocheckpoint=0 for Litestream: {}",
                    e
                ))
            })?;

        // LITE-03: Warn if SQLCipher encryption is active alongside Litestream.
        if std::env::var("BLUFIO_DB_KEY").is_ok() {
            warn!(
                "Litestream is enabled but SQLCipher encryption is active. \
                 Litestream CANNOT replicate encrypted databases. \
                 Use `blufio backup` + cron instead. \
                 See: https://github.com/benbjohnson/litestream/issues/177"
            );
        }
    }
    Ok(())
}

/// Mark stale sessions as interrupted (crash recovery).
pub(crate) async fn mark_stale_sessions(storage: &dyn StorageAdapter) -> Result<(), BlufioError> {
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

/// Initialize cost ledger and budget tracker.
pub(crate) async fn init_cost_tracking(
    config: &BlufioConfig,
) -> Result<(Arc<CostLedger>, Arc<tokio::sync::Mutex<BudgetTracker>>), BlufioError> {
    let cost_ledger = Arc::new(CostLedger::open(&config.storage.database_path).await?);
    let budget_tracker = Arc::new(tokio::sync::Mutex::new(
        BudgetTracker::from_ledger(&config.cost, &cost_ledger).await?,
    ));
    Ok((cost_ledger, budget_tracker))
}

/// Initialize tokenizer cache from config.
pub(crate) fn init_tokenizer(config: &BlufioConfig) -> Arc<TokenizerCache> {
    let tokenizer_mode = if config.performance.tokenizer_mode == "fast" {
        TokenizerMode::Fast
    } else {
        TokenizerMode::Accurate
    };
    Arc::new(TokenizerCache::new(tokenizer_mode))
}

/// Initialize the context engine, including static zone budget check.
pub(crate) async fn init_context_engine(
    config: &BlufioConfig,
    token_cache: &Arc<TokenizerCache>,
) -> Result<ContextEngine, BlufioError> {
    let context_engine =
        ContextEngine::new(&config.agent, &config.context, token_cache.clone()).await?;

    // Static zone budget check at startup (CTXE-01).
    // Advisory only -- logs a warning if system prompt exceeds budget but never truncates.
    {
        let static_tokens = context_engine
            .static_zone()
            .token_count(token_cache, &config.context.compaction_model)
            .await;
        context_engine
            .static_zone()
            .check_budget(static_tokens, config.context.static_zone_budget);
        debug!(
            static_tokens = static_tokens,
            budget = config.context.static_zone_budget,
            "static zone budget check complete"
        );
    }

    Ok(context_engine)
}

/// Initialize the memory system: downloads model, creates embedder, store,
/// retriever, provider, and extractor. Registers the provider with ContextEngine.
///
/// Returns (MemoryProvider, MemoryExtractor, MemoryStore, OnnxEmbedder) on success.
#[allow(dead_code)]
pub(crate) async fn initialize_memory(
    config: &BlufioConfig,
    context_engine: &mut ContextEngine,
) -> Result<
    (
        MemoryProvider,
        Arc<MemoryExtractor>,
        Arc<MemoryStore>,
        Arc<OnnxEmbedder>,
    ),
    BlufioError,
> {
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
    let memory_conn = blufio_storage::open_connection(&config.storage.database_path).await?;
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
        memory_store.clone(),
        embedder.clone(),
        config.memory.extraction_model.clone(),
    ));

    info!("memory system initialized");
    Ok((memory_provider, extractor, memory_store, embedder))
}

/// Initialize the memory system, returning tuple of optional components.
pub(crate) async fn init_memory_system(
    config: &BlufioConfig,
    context_engine: &mut ContextEngine,
) -> (
    Option<MemoryProvider>,
    Option<Arc<MemoryExtractor>>,
    Option<Arc<MemoryStore>>,
    Option<Arc<OnnxEmbedder>>,
) {
    #[cfg(feature = "onnx")]
    let result = if config.memory.enabled {
        match initialize_memory(config, context_engine).await {
            Ok((mp, me, ms, emb)) => (Some(mp), Some(me), Some(ms), Some(emb)),
            Err(e) => {
                warn!(error = %e, "memory system initialization failed, continuing without memory");
                (None, None, None, None)
            }
        }
    } else {
        info!("memory system disabled by configuration");
        (None, None, None, None)
    };

    #[cfg(not(feature = "onnx"))]
    let result: (
        Option<MemoryProvider>,
        Option<Arc<MemoryExtractor>>,
        Option<Arc<MemoryStore>>,
        Option<Arc<OnnxEmbedder>>,
    ) = {
        info!("memory system disabled (onnx feature not enabled)");
        (None, None, None, None)
    };

    result
}
