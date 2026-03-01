// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio serve` command implementation.
//!
//! Starts the full Blufio agent with Telegram channel, Anthropic provider,
//! and SQLite storage. Installs signal handlers for graceful shutdown.

use std::sync::Arc;

use blufio_agent::AgentLoop;
use blufio_agent::shutdown;
use blufio_anthropic::AnthropicProvider;
use blufio_config::model::BlufioConfig;
use blufio_core::error::BlufioError;
use blufio_core::{ChannelAdapter, StorageAdapter};
use blufio_storage::SqliteStorage;
use blufio_telegram::TelegramChannel;
use tracing::{error, info};

/// Runs the `blufio serve` command.
///
/// Initializes all adapters (storage, provider, channel), marks stale sessions
/// as interrupted (crash recovery), installs signal handlers, and enters the
/// main agent loop.
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

    // Install signal handler.
    let cancel = shutdown::install_signal_handler();

    // Create and run agent loop.
    let mut agent_loop = AgentLoop::new(
        Box::new(channel),
        provider,
        storage,
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
