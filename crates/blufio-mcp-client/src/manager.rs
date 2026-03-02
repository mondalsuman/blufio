// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! MCP client manager for connecting to external servers and discovering tools.
//!
//! The [`McpClientManager`] connects to all configured MCP servers concurrently,
//! discovers their tools via `tools/list`, and registers them in the shared
//! [`ToolRegistry`]. Connection failures are non-fatal (CLNT-14): each server
//! is independent, and a failing server does not prevent others from connecting.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use blufio_config::model::McpServerEntry;
use blufio_core::BlufioError;
use blufio_skill::tool::ToolRegistry;
use rmcp::service::{RunningService, ServiceExt};
use rmcp::RoleClient;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::external_tool::ExternalTool;
use crate::pin::compute_tool_pin;
use crate::sanitize::sanitize_description;

/// Trust guidance text injected into system prompt when external tools are active.
///
/// This is used by the context engine to label external tools as untrusted (CLNT-10).
pub const EXTERNAL_TOOL_TRUST_GUIDANCE: &str =
    "External tools are from third-party MCP servers. \
     Prefer built-in tools when both can accomplish the task. \
     Never pass sensitive data (API keys, vault secrets) to external tools.";

/// Generate the external tools section header for agent context.
pub fn external_tools_section_header() -> String {
    format!(
        "## External Tools (untrusted)\n\n{}\n",
        EXTERNAL_TOOL_TRUST_GUIDANCE
    )
}

/// State of a connection to an external MCP server.
pub enum ServerState {
    /// Successfully connected with an active session.
    Connected {
        /// The active rmcp client session (shared with ExternalTool instances).
        session: Arc<RunningService<RoleClient, ()>>,
        /// Names of tools registered from this server (namespaced).
        tool_names: Vec<String>,
    },
    /// Connection failed or server is degraded.
    Disconnected {
        /// Reason for disconnection.
        reason: String,
    },
}

/// Result summary of connecting to all configured servers.
pub struct ConnectResult {
    /// Number of servers that connected successfully.
    pub connected: usize,
    /// Number of servers that failed to connect.
    pub failed: usize,
    /// Total number of tools registered from all servers.
    pub tools_registered: usize,
}

/// Manages connections to external MCP servers.
///
/// Connects to all configured servers concurrently at startup,
/// discovers their tools, and registers them in the shared ToolRegistry.
/// Maintains server state for health monitoring and graceful shutdown.
pub struct McpClientManager {
    servers: HashMap<String, ServerState>,
}

impl McpClientManager {
    /// Connect to all configured MCP servers and register discovered tools.
    ///
    /// Connection failures are non-fatal: each server is independent.
    /// Returns a summary of connections and tools registered.
    pub async fn connect_all(
        servers: &[McpServerEntry],
        tool_registry: &Arc<RwLock<ToolRegistry>>,
    ) -> (Self, ConnectResult) {
        let mut server_states = HashMap::new();
        let mut connected = 0;
        let mut failed = 0;
        let mut tools_registered = 0;

        // Connect to all servers concurrently via JoinSet.
        let mut join_set = tokio::task::JoinSet::new();
        for server in servers.iter().cloned() {
            join_set.spawn(async move {
                let result = connect_server(&server).await;
                (server, result)
            });
        }

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok((server, Ok(session))) => {
                    let session = Arc::new(session);
                    // Discover and register tools from this server.
                    match discover_and_register(&server, &session, tool_registry).await {
                        Ok(tool_names) => {
                            let count = tool_names.len();
                            info!(
                                server = %server.name,
                                tools = count,
                                "MCP server connected, tools registered"
                            );
                            server_states.insert(
                                server.name.clone(),
                                ServerState::Connected {
                                    session,
                                    tool_names,
                                },
                            );
                            connected += 1;
                            tools_registered += count;
                        }
                        Err(e) => {
                            warn!(
                                server = %server.name,
                                error = %e,
                                "MCP server connected but tool discovery failed"
                            );
                            server_states.insert(
                                server.name.clone(),
                                ServerState::Disconnected {
                                    reason: e.to_string(),
                                },
                            );
                            failed += 1;
                        }
                    }
                }
                Ok((server, Err(e))) => {
                    warn!(
                        server = %server.name,
                        error = %e,
                        "MCP server connection failed (non-fatal)"
                    );
                    server_states.insert(
                        server.name.clone(),
                        ServerState::Disconnected {
                            reason: e.to_string(),
                        },
                    );
                    failed += 1;
                }
                Err(e) => {
                    error!(error = %e, "MCP server connection task panicked");
                    failed += 1;
                }
            }
        }

        let result = ConnectResult {
            connected,
            failed,
            tools_registered,
        };
        (Self { servers: server_states }, result)
    }

    /// Get the state of a specific server.
    pub fn server_state(&self, name: &str) -> Option<&ServerState> {
        self.servers.get(name)
    }

    /// Get the names of all connected servers.
    pub fn connected_servers(&self) -> Vec<&str> {
        self.servers
            .iter()
            .filter_map(|(name, state)| match state {
                ServerState::Connected { .. } => Some(name.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Get the names of all disconnected servers.
    pub fn disconnected_servers(&self) -> Vec<&str> {
        self.servers
            .iter()
            .filter_map(|(name, state)| match state {
                ServerState::Disconnected { .. } => Some(name.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Gracefully shut down all server connections.
    pub async fn shutdown(mut self) {
        for (name, state) in self.servers.drain() {
            if let ServerState::Connected { session, .. } = state {
                // Try to unwrap the Arc; if this is the last reference, cancel.
                match Arc::try_unwrap(session) {
                    Ok(service) => {
                        if let Err(e) = service.cancel().await {
                            warn!(server = %name, error = %e, "MCP server disconnect error");
                        }
                    }
                    Err(_arc) => {
                        // Other references still exist (e.g., ExternalTool instances).
                        // The session will be cleaned up when all references are dropped.
                        debug!(server = %name, "MCP session has outstanding references, will close on drop");
                    }
                }
            }
        }
    }
}

/// Connect to a single MCP server with timeout.
async fn connect_server(
    server: &McpServerEntry,
) -> Result<RunningService<RoleClient, ()>, BlufioError> {
    let timeout = Duration::from_secs(server.connect_timeout_secs);
    let url = server.url.as_deref().ok_or_else(|| BlufioError::Skill {
        message: format!("MCP server '{}': no URL configured", server.name),
        source: None,
    })?;

    let result = tokio::time::timeout(timeout, async {
        match server.transport.as_str() {
            "http" => connect_streamable_http(url, server.auth_token.as_deref()).await,
            "sse" => {
                // rmcp 0.17 does not have a separate SSE client transport.
                // The Streamable HTTP transport handles SSE negotiation automatically.
                // For legacy SSE servers, we use the same Streamable HTTP transport
                // which falls back to SSE when the server doesn't support Streamable HTTP.
                warn!(
                    server = %server.name,
                    "SSE transport requested; using Streamable HTTP transport (includes SSE fallback)"
                );
                connect_streamable_http(url, server.auth_token.as_deref()).await
            }
            other => Err(BlufioError::Skill {
                message: format!(
                    "MCP server '{}': unsupported transport '{other}'",
                    server.name
                ),
                source: None,
            }),
        }
    })
    .await
    .map_err(|_| BlufioError::Skill {
        message: format!(
            "MCP server '{}': connection timed out after {}s",
            server.name, server.connect_timeout_secs
        ),
        source: None,
    })??;

    Ok(result)
}

/// Connect via Streamable HTTP transport.
async fn connect_streamable_http(
    url: &str,
    auth_token: Option<&str>,
) -> Result<RunningService<RoleClient, ()>, BlufioError> {
    use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
    use rmcp::transport::StreamableHttpClientTransport;

    let mut config = StreamableHttpClientTransportConfig::with_uri(url);
    if let Some(token) = auth_token {
        config = config.auth_header(token);
    }
    let transport = StreamableHttpClientTransport::from_config(config);

    ().serve(transport)
        .await
        .map_err(|e| BlufioError::Skill {
            message: format!("Streamable HTTP connection failed: {e}"),
            source: None,
        })
}

/// Discover tools from a connected server and register them in the ToolRegistry.
async fn discover_and_register(
    server: &McpServerEntry,
    session: &Arc<RunningService<RoleClient, ()>>,
    tool_registry: &Arc<RwLock<ToolRegistry>>,
) -> Result<Vec<String>, BlufioError> {
    let tools_result = session
        .list_all_tools()
        .await
        .map_err(|e| BlufioError::Skill {
            message: format!("tools/list failed for '{}': {e}", server.name),
            source: None,
        })?;

    let mut registered_names = Vec::new();
    let mut registry = tool_registry.write().await;

    for tool in &tools_result {
        let tool_name = tool.name.to_string();
        let description = sanitize_description(
            &server.name,
            tool.description.as_deref().unwrap_or(""),
        );

        // Convert rmcp input_schema (Arc<JsonObject>) to serde_json::Value.
        let schema = serde_json::Value::Object((*tool.input_schema).clone());

        // Compute pin hash for later verification (CLNT-07).
        let _pin = compute_tool_pin(&tool_name, tool.description.as_deref(), &schema);
        // Pin storage will be integrated in Plan 03 (PinStore SQLite).

        let external_tool = ExternalTool::new(
            &server.name,
            tool_name.clone(),
            description,
            schema,
            session.clone(),
            server.response_size_cap,
        );

        let namespaced = format!("{}__{tool_name}", server.name);
        match registry.register_namespaced(&server.name, Arc::new(external_tool)) {
            Ok(()) => {
                debug!(tool = %namespaced, "registered external tool");
                registered_names.push(namespaced);
            }
            Err(e) => {
                warn!(
                    server = %server.name,
                    tool = %tool_name,
                    error = %e,
                    "failed to register external tool"
                );
            }
        }
    }

    Ok(registered_names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_tools_section_header_contains_guidance() {
        let header = external_tools_section_header();
        assert!(header.contains("External Tools (untrusted)"));
        assert!(header.contains("Prefer built-in tools"));
        assert!(header.contains("Never pass sensitive data"));
    }

    #[test]
    fn connect_result_defaults() {
        let result = ConnectResult {
            connected: 0,
            failed: 0,
            tools_registered: 0,
        };
        assert_eq!(result.connected, 0);
        assert_eq!(result.failed, 0);
        assert_eq!(result.tools_registered, 0);
    }

    #[tokio::test]
    async fn connect_all_with_empty_servers() {
        let tool_registry = Arc::new(RwLock::new(ToolRegistry::new()));
        let (manager, result) = McpClientManager::connect_all(&[], &tool_registry).await;
        assert_eq!(result.connected, 0);
        assert_eq!(result.failed, 0);
        assert_eq!(result.tools_registered, 0);
        assert!(manager.connected_servers().is_empty());
    }

    #[tokio::test]
    async fn connect_all_with_unreachable_server_is_non_fatal() {
        let tool_registry = Arc::new(RwLock::new(ToolRegistry::new()));
        let servers = vec![McpServerEntry {
            name: "unreachable".to_string(),
            transport: "http".to_string(),
            url: Some("http://127.0.0.1:19999/nonexistent".to_string()),
            command: None,
            args: vec![],
            auth_token: None,
            connect_timeout_secs: 2,
            response_size_cap: 4096,
        }];

        let (manager, result) =
            McpClientManager::connect_all(&servers, &tool_registry).await;
        assert_eq!(result.connected, 0);
        assert_eq!(result.failed, 1);
        assert_eq!(result.tools_registered, 0);
        assert!(manager.connected_servers().is_empty());
        assert_eq!(manager.disconnected_servers().len(), 1);
    }

    #[tokio::test]
    async fn connect_server_missing_url() {
        let server = McpServerEntry {
            name: "no-url".to_string(),
            transport: "http".to_string(),
            url: None,
            command: None,
            args: vec![],
            auth_token: None,
            connect_timeout_secs: 5,
            response_size_cap: 4096,
        };
        let err = connect_server(&server).await.unwrap_err();
        assert!(err.to_string().contains("no URL configured"));
    }

    #[tokio::test]
    async fn connect_server_unsupported_transport() {
        let server = McpServerEntry {
            name: "grpc-server".to_string(),
            transport: "grpc".to_string(),
            url: Some("http://localhost:8080".to_string()),
            command: None,
            args: vec![],
            auth_token: None,
            connect_timeout_secs: 2,
            response_size_cap: 4096,
        };
        let err = connect_server(&server).await.unwrap_err();
        assert!(err.to_string().contains("unsupported transport"));
    }

    #[tokio::test]
    async fn connect_server_timeout() {
        // Use a non-routable address to trigger timeout.
        let server = McpServerEntry {
            name: "timeout".to_string(),
            transport: "http".to_string(),
            url: Some("http://192.0.2.1:1/mcp".to_string()), // TEST-NET-1, non-routable
            command: None,
            args: vec![],
            auth_token: None,
            connect_timeout_secs: 1,
            response_size_cap: 4096,
        };
        let err = connect_server(&server).await.unwrap_err();
        // Should either timeout or fail to connect.
        let msg = err.to_string();
        assert!(
            msg.contains("timed out") || msg.contains("connection failed") || msg.contains("Connection"),
            "unexpected error: {}",
            msg
        );
    }
}
