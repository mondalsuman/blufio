// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio mcp-server` subcommand implementation.
//!
//! Starts an MCP server on stdio for Claude Desktop integration.
//! Initializes minimal infrastructure (storage, vault, tool registry)
//! without starting the agent loop, Telegram, gateway, heartbeat, or
//! memory system.
//!
//! **SRVR-15**: All tracing output goes to stderr, never stdout.
//! stdout is reserved for the MCP JSON-RPC protocol stream.

use std::sync::Arc;

use blufio_config::model::BlufioConfig;
use blufio_core::BlufioError;
use blufio_skill::ToolRegistry;
use tracing::info;

/// Runs the MCP server on stdio.
///
/// This is the entry point that Claude Desktop invokes via:
/// ```json
/// { "command": "blufio", "args": ["mcp-server"] }
/// ```
///
/// Initializes minimal infrastructure, creates the MCP handler, and
/// runs the server over stdio with graceful shutdown handling.
pub async fn run_mcp_server(config: BlufioConfig) -> Result<(), BlufioError> {
    // Initialize tracing to stderr (SRVR-15).
    init_tracing_stderr(&config.agent.log_level);

    // Log bash exclusion warning if needed.
    if config.mcp.export_tools.iter().any(|t| t == "bash") {
        tracing::warn!("'bash' in mcp.export_tools is ignored (security: never exported via MCP)");
    }

    // Open database.
    let db = crate::open_db(&config).await?;

    // Vault startup check (for WASM skills that may need secrets).
    {
        let vault_conn = tokio_rusqlite::Connection::open(&config.storage.database_path)
            .await
            .map_err(|e| BlufioError::Storage {
                source: Box::new(e),
            })?;
        match blufio_vault::vault_startup_check(vault_conn, &config.vault).await {
            Ok(Some(_vault)) => info!("vault unlocked"),
            Ok(None) => tracing::debug!("no vault found"),
            Err(e) => {
                tracing::error!(error = %e, "vault startup check failed");
                return Err(e);
            }
        }
    }

    // Initialize tool registry with built-in tools.
    let mut tool_registry = ToolRegistry::new();
    blufio_skill::builtin::register_builtins(&mut tool_registry);
    info!(count = tool_registry.len(), "tool registry initialized");
    let tool_registry = Arc::new(tokio::sync::RwLock::new(tool_registry));

    // Print startup banner to stderr.
    eprintln!("blufio {} MCP server ready", env!("CARGO_PKG_VERSION"));

    // Create handler and start stdio server.
    let handler = blufio_mcp_server::BlufioMcpHandler::new(tool_registry, &config.mcp);
    let cancel = blufio_agent::shutdown::install_signal_handler();

    // serve_stdio connects handler to stdin/stdout and blocks until shutdown.
    blufio_mcp_server::serve_stdio(handler, cancel).await?;

    // Clean close.
    db.close().await?;

    info!("MCP server shutdown complete");
    Ok(())
}

/// Initializes tracing subscriber targeting stderr only.
///
/// Uses the same `RedactingMakeWriter` pattern as serve.rs, ensuring
/// all log output goes to stderr and passes through secret redaction.
/// stdout is reserved exclusively for the MCP JSON-RPC protocol stream.
fn init_tracing_stderr(log_level: &str) {
    use tracing_subscriber::EnvFilter;

    let vault_values = std::sync::Arc::new(std::sync::RwLock::new(Vec::<String>::new()));

    let redacting_writer = RedactingMakeWriter {
        vault_values: vault_values.clone(),
    };

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("blufio={log_level},warn")));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_names(false)
        .with_writer(redacting_writer)
        .init();
}

/// A `MakeWriter` implementation that creates `RedactingWriter` instances
/// targeting stderr. Identical to the one in serve.rs.
struct RedactingMakeWriter {
    vault_values: std::sync::Arc<std::sync::RwLock<Vec<String>>>,
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for RedactingMakeWriter {
    type Writer = blufio_security::RedactingWriter<std::io::Stderr>;

    fn make_writer(&'a self) -> Self::Writer {
        blufio_security::RedactingWriter::new(std::io::stderr(), self.vault_values.clone())
    }
}
