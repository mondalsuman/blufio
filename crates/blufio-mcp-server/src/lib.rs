// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! MCP server implementation for Blufio.
//!
//! This crate implements the Model Context Protocol server, allowing
//! MCP clients (like Claude Desktop) to discover and invoke Blufio
//! tools, read resources, and use prompt templates.
//!
//! ## Abstraction Boundary
//!
//! The `rmcp` crate is used freely within this crate for protocol
//! handling. However, **no rmcp types appear in the public API**.
//! All public types are Blufio-owned, defined in [`types`].

pub mod auth;
pub mod bridge;
pub mod handler;
pub mod resources;
pub mod transport;
pub mod types;

// Re-export public types for convenience.
pub use handler::BlufioMcpHandler;
pub use types::McpSessionId;

/// Starts the MCP server on stdio and blocks until shutdown.
///
/// This is the primary entry point for the `blufio mcp-server` subcommand.
/// It connects the handler to stdin/stdout via the rmcp stdio transport,
/// and waits for either a signal (via the cancellation token) or stdin EOF
/// (client disconnected).
///
/// The cancellation token should be obtained from `blufio_agent::shutdown::install_signal_handler()`.
///
/// # Errors
///
/// Returns an error if the MCP server fails to initialize or if the
/// server task encounters a fatal error.
pub async fn serve_stdio(
    handler: BlufioMcpHandler,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), blufio_core::BlufioError> {
    use rmcp::ServiceExt;

    let service = handler
        .serve_with_ct((tokio::io::stdin(), tokio::io::stdout()), cancel)
        .await
        .map_err(|e| {
            blufio_core::BlufioError::Internal(format!("MCP server initialization failed: {e}"))
        })?;

    // Wait for shutdown: either signal (via CancellationToken) or stdin EOF.
    match service.waiting().await {
        Ok(_reason) => {
            tracing::info!("MCP server shutting down (client disconnected or signal received)");
        }
        Err(e) => {
            tracing::error!(error = %e, "MCP server task error");
        }
    }

    Ok(())
}
