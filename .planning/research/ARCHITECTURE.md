# Architecture Patterns: MCP Integration into Blufio

**Domain:** MCP server + client integration into existing Rust AI agent platform
**Researched:** 2026-03-02
**Overall confidence:** HIGH (MCP spec is well-documented; rmcp is the official Rust SDK; existing codebase thoroughly analyzed)

## Executive Summary

Blufio's MCP integration requires two new crates (`blufio-mcp-server`, `blufio-mcp-client`) and targeted modifications to three existing crates (`blufio-config`, `blufio-gateway`, and the `blufio` binary). The MCP server exposes Blufio's tools, memory, and skills as MCP primitives to external clients (Claude Desktop, AI IDEs). The MCP client consumes external MCP servers as a new tool source, registered into the existing `ToolRegistry`. Both components use the `rmcp` crate (official Rust MCP SDK, v0.17+) for protocol handling.

The key architectural insight: **MCP server is NOT a channel adapter** -- it is a parallel exposure surface that runs alongside the gateway. MCP client IS a tool provider that feeds into the existing ToolRegistry, fitting naturally alongside built-in tools and WASM skills.

## Recommended Architecture

### High-Level Component Map

```
                    +------------------+
                    |  Claude Desktop  |
                    |  / AI IDE / CLI  |
                    +--------+---------+
                             |
                    stdio / Streamable HTTP
                             |
              +--------------v--------------+
              |     blufio-mcp-server       |
              |  (new crate)                |
              |                             |
              |  ServerHandler impl:        |
              |  - tools/* --> ToolRegistry |
              |  - resources/* --> Memory   |
              |  - prompts/* --> System     |
              +---------+--+--+------------+
                        |  |  |
          +-------------+  |  +-------------+
          v                v                v
   ToolRegistry      MemoryStore      ContextEngine
   (blufio-skill)   (blufio-memory)  (blufio-context)
          ^
          |  registers MCP tools
          |
   +------+----------+
   | blufio-mcp-client|       +-------------------+
   | (new crate)      +------>| External MCP      |
   |                  |       | Servers            |
   | McpRemoteTool    |       | (filesystem, DB,   |
   | per server conn  |       |  GitHub, etc.)     |
   +------------------+       +-------------------+

   Existing flow (unchanged):
   Telegram/Gateway --> AgentLoop --> Provider --> LLM
                           |
                      ToolRegistry (now includes MCP client tools)
```

### Why Two New Crates (Not Modifications to Existing Crates)

| Option | Verdict | Rationale |
|--------|---------|-----------|
| Modify blufio-gateway to serve MCP | REJECTED | Gateway is a ChannelAdapter (receives user messages, routes to agent loop). MCP server is a capability exposure surface (receives tool/resource requests, executes directly). Different responsibilities, different lifecycle, different auth model. Merging them creates a confusing hybrid. |
| Add MCP to blufio-skill | REJECTED | blufio-skill owns the Tool trait and ToolRegistry. MCP client tools should register INTO the registry, but the MCP protocol handling (connection management, JSON-RPC, transport) does not belong in the skill crate. |
| Single blufio-mcp crate | REJECTED | Server and client have different dependency profiles. Server needs access to ToolRegistry + MemoryStore + ContextEngine (reads FROM Blufio). Client needs transport + connection management (writes INTO ToolRegistry). Separate crates allow independent feature-flagging and cleaner dependency graphs. |
| Two new crates: blufio-mcp-server + blufio-mcp-client | ACCEPTED | Clean separation of concerns. Each crate has a focused dependency set. Feature-flagged in the binary crate like all other adapters. Follows established pattern (blufio-telegram, blufio-gateway are separate crates). |

### Component Boundaries

| Component | Responsibility | Communicates With | New/Modified |
|-----------|---------------|-------------------|--------------|
| `blufio-mcp-server` | Expose Blufio capabilities via MCP protocol (tools, resources, prompts) | blufio-skill (ToolRegistry), blufio-memory (MemoryStore), blufio-context (ContextEngine) | NEW |
| `blufio-mcp-client` | Connect to external MCP servers, discover tools, register as Tool impls | blufio-skill (ToolRegistry, Tool trait), blufio-core (BlufioError) | NEW |
| `blufio-config` | Add `[mcp]` and `[[mcp.servers]]` config sections | N/A (data model only) | MODIFIED |
| `blufio-skill` | No changes to Tool/ToolRegistry interfaces | N/A | UNCHANGED |
| `blufio` (binary) | Wire MCP server startup + MCP client initialization in serve.rs | blufio-mcp-server, blufio-mcp-client | MODIFIED |
| `blufio-gateway` | Share axum Router with MCP Streamable HTTP service | blufio-mcp-server (service mounted on same router) | MINOR MODIFICATION |
| `blufio-core` | No trait changes needed. MCP client tools implement existing Tool trait. | N/A | UNCHANGED |

## MCP Server Architecture (blufio-mcp-server)

### What It Exposes

The MCP server exposes three primitive types per the MCP 2025-11-25 specification:

**1. Tools (model-controlled)**
Maps directly to Blufio's `ToolRegistry`. Every tool registered in the registry (built-in bash/http/file tools + WASM skills + delegation tool + MCP client tools) is exposed as an MCP tool.

```
MCP tools/list  -->  ToolRegistry::tool_definitions()  -->  Convert to MCP format
MCP tools/call  -->  ToolRegistry::get(name)::invoke(input)  -->  Convert ToolOutput to MCP CallToolResult
```

Schema translation (Blufio to MCP):
- `Tool::name()` maps to MCP `tool.name`
- `Tool::description()` maps to MCP `tool.description`
- `Tool::parameters_schema()` maps to MCP `tool.inputSchema`
- `ToolOutput { content, is_error }` maps to MCP `CallToolResult { content: [TextContent], isError }`

**2. Resources (application-controlled)**
Exposes Blufio's memory system as MCP resources. Each stored memory fact becomes a readable resource.

```
MCP resources/list  -->  MemoryStore::list()  -->  Convert to MCP Resource list
MCP resources/read  -->  MemoryStore::get(uri)  -->  Convert to MCP ResourceContents
```

Resource URI scheme: `blufio://memory/{memory_id}` for individual facts, `blufio://memory/search?q={query}` as a resource template for semantic search.

Additionally, expose session history as resources:
- `blufio://session/{session_id}` -- conversation history for a session
- `blufio://config` -- current (redacted) configuration

**3. Prompts (user-controlled)**
Expose the system prompt as an MCP prompt template:
- `blufio://prompt/system` -- the agent's system prompt
- `blufio://prompt/skill/{name}` -- skill SKILL.md documentation

### Server Capabilities Declaration

```json
{
  "capabilities": {
    "tools": { "listChanged": true },
    "resources": { "subscribe": false, "listChanged": false },
    "prompts": { "listChanged": false }
  }
}
```

`tools.listChanged = true` because WASM skills can be installed/removed at runtime, and MCP client tools are discovered dynamically. When the ToolRegistry changes, the server emits `notifications/tools/list_changed`.

### Transport Strategy

The MCP server must support three transports to satisfy the milestone requirements:

| Transport | Use Case | Implementation |
|-----------|----------|----------------|
| **stdio** | Claude Desktop, local AI apps | `rmcp` transport-io feature. Binary runs as child process spawned by host app. Requires a CLI subcommand (`blufio mcp-server`). |
| **Streamable HTTP** | Remote clients, programmatic access, multi-client | `rmcp` transport-streamable-http-server feature. Mounted on the existing axum Router at `/mcp` endpoint, sharing the same TCP listener as the gateway. |
| **SSE** | Backward compatibility with older MCP clients | Subsumed by Streamable HTTP in the 2025-03-26+ spec. The Streamable HTTP transport already uses SSE for server-to-client streaming. No separate SSE implementation needed. |

### stdio Transport: New CLI Subcommand

Claude Desktop spawns MCP servers as child processes over stdio. Blufio needs a `blufio mcp-server` subcommand that:
1. Initializes storage, memory, and tool registry (read-only access)
2. Creates the MCP ServerHandler
3. Serves on stdio (stdin/stdout)
4. Runs until the parent process closes the pipe

This is a lightweight mode -- no Telegram, no gateway, no agent loop. Just the MCP server with read access to Blufio's data.

```json
// Claude Desktop configuration (claude_desktop_config.json):
{
  "mcpServers": {
    "blufio": {
      "command": "/usr/local/bin/blufio",
      "args": ["mcp-server"],
      "env": {
        "BLUFIO_VAULT_KEY": "..."
      }
    }
  }
}
```

### Streamable HTTP Transport: Shared axum Router

The Streamable HTTP transport mounts on the existing gateway's axum Router. This avoids opening a second TCP port.

```rust
// In gateway server.rs, when building the router:
let mcp_service = StreamableHttpService::new(
    || Ok(BlufioMcpServer::new(tool_registry.clone(), memory_store.clone())),
    LocalSessionManager::default().into(),
    Default::default(),
);

// Merge MCP route into existing gateway router
let app = Router::new()
    .merge(public_routes)
    .merge(api_routes)
    .merge(ws_routes)
    .nest_service("/mcp", mcp_service)  // NEW
    .layer(CorsLayer::permissive());
```

Authentication for the `/mcp` endpoint reuses the gateway's existing auth middleware (bearer token or Ed25519 keypair signature). The MCP Streamable HTTP spec supports session IDs via `Mcp-Session-Id` header, which rmcp handles automatically.

### ServerHandler Implementation

```rust
// blufio-mcp-server/src/handler.rs (conceptual)
use rmcp::ServerHandler;
use rmcp::model::*;

pub struct BlufioMcpServer {
    tool_registry: Arc<RwLock<ToolRegistry>>,
    memory_store: Option<Arc<MemoryStore>>,
    // context_engine for prompts, etc.
}

impl ServerHandler for BlufioMcpServer {
    fn name(&self) -> String { "blufio".into() }

    async fn list_tools(
        &self,
        _params: ListToolsRequestParams,
    ) -> Result<ListToolsResult, ErrorData> {
        let registry = self.tool_registry.read().await;
        let tools = registry.tool_definitions()
            .into_iter()
            .map(|def| Tool {
                name: def["name"].as_str().unwrap().to_string(),
                description: Some(def["description"].as_str().unwrap().to_string()),
                input_schema: def["input_schema"].clone(),
                ..Default::default()
            })
            .collect();
        Ok(ListToolsResult { tools, next_cursor: None })
    }

    async fn call_tool(
        &self,
        params: CallToolRequestParams,
    ) -> Result<CallToolResult, ErrorData> {
        let registry = self.tool_registry.read().await;
        let tool = registry.get(&params.name)
            .ok_or_else(|| ErrorData::invalid_params("unknown tool"))?;
        let output = tool.invoke(params.arguments.unwrap_or_default()).await
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;
        Ok(CallToolResult {
            content: vec![Content::text(output.content)],
            is_error: Some(output.is_error),
            ..Default::default()
        })
    }

    // resources/list, resources/read, prompts/list, prompts/get ...
}
```

## MCP Client Architecture (blufio-mcp-client)

### How It Fits the Existing Pattern

The MCP client does NOT need a new adapter trait. External MCP tools register directly into the existing `ToolRegistry` as `Arc<dyn Tool>` implementations. This is the same mechanism used by built-in tools and WASM skills.

Each external MCP server connection produces a set of `McpRemoteTool` structs that implement `Tool`:

```rust
// blufio-mcp-client/src/tool.rs (conceptual)
pub struct McpRemoteTool {
    prefixed_name: String,      // "server_name.tool_name"
    original_name: String,      // "tool_name"
    description: String,
    input_schema: serde_json::Value,
    client: Arc<McpClientSession>,  // rmcp client connection
}

#[async_trait]
impl Tool for McpRemoteTool {
    fn name(&self) -> &str { &self.prefixed_name }
    fn description(&self) -> &str { &self.description }
    fn parameters_schema(&self) -> serde_json::Value { self.input_schema.clone() }

    async fn invoke(&self, input: serde_json::Value) -> Result<ToolOutput, BlufioError> {
        let result = self.client
            .call_tool(&self.original_name, input)
            .await
            .map_err(|e| BlufioError::Skill {
                message: format!("MCP tool {} failed: {e}", self.prefixed_name),
                source: None,
            })?;
        Ok(ToolOutput {
            content: extract_text_content(&result.content),
            is_error: result.is_error.unwrap_or(false),
        })
    }
}
```

### Connection Management

The MCP client manager handles multiple server connections:

```rust
// blufio-mcp-client/src/manager.rs (conceptual)
pub struct McpClientManager {
    connections: HashMap<String, McpConnection>,
}

pub struct McpConnection {
    name: String,
    client: Arc<McpClientSession>,
    tools: Vec<Arc<McpRemoteTool>>,
    status: ConnectionStatus,
}

impl McpClientManager {
    /// Initialize all configured MCP server connections.
    /// Discovers tools from each and registers them in the ToolRegistry.
    pub async fn initialize(
        configs: &[McpServerConfig],
        tool_registry: &Arc<RwLock<ToolRegistry>>,
    ) -> Result<Self, BlufioError> {
        let mut connections = HashMap::new();

        for server_config in configs {
            match Self::connect_server(server_config).await {
                Ok(conn) => {
                    // Register discovered tools
                    let mut registry = tool_registry.write().await;
                    for tool in &conn.tools {
                        registry.register(tool.clone());
                    }
                    info!(
                        server = server_config.name.as_str(),
                        tool_count = conn.tools.len(),
                        "MCP server connected, tools registered"
                    );
                    connections.insert(server_config.name.clone(), conn);
                }
                Err(e) => {
                    warn!(
                        server = server_config.name.as_str(),
                        error = %e,
                        "failed to connect MCP server, skipping"
                    );
                }
            }
        }

        Ok(Self { connections })
    }

    /// Graceful shutdown: close all MCP client connections.
    pub async fn shutdown(&self) -> Result<(), BlufioError> {
        for (name, conn) in &self.connections {
            if let Err(e) = conn.client.cancel().await {
                warn!(server = name.as_str(), error = %e, "MCP client shutdown error");
            }
        }
        Ok(())
    }
}
```

### Client Transport Strategy

| Transport | Config | When Used |
|-----------|--------|-----------|
| **stdio** | `command = "/path/to/server"`, `args = [...]` | Local MCP servers (filesystem, git, etc.) spawned as child processes |
| **Streamable HTTP** | `url = "https://host/mcp"` | Remote MCP servers |

The transport is determined by config: if `command` is set, use stdio (spawn child process via `TokioChildProcess`). If `url` is set, use Streamable HTTP via `StreamableHttpClientTransport`.

### Tool Name Namespacing

External MCP tools must be namespaced to avoid collisions with built-in tools. Strategy: prefix with the MCP server name.

```
Built-in:            bash, http, file
WASM skill:          my-skill
MCP server "github": github.create_issue, github.list_repos
MCP server "fs":     fs.read_file, fs.write_file
```

The prefix is `{server_name}.{tool_name}`. The MCP client strips the prefix before forwarding to the actual MCP server's `tools/call`.

## Configuration Model (blufio-config modifications)

### New TOML Sections

```toml
# MCP integration settings
[mcp]
# Enable MCP server (expose Blufio capabilities)
server_enabled = false

# Enable MCP client (consume external MCP servers)
client_enabled = false

# Streamable HTTP endpoint path (mounted on gateway)
server_endpoint = "/mcp"

# MCP server name (reported in initialize handshake)
server_name = "blufio"

# MCP server version
server_version = "0.1.0"

# External MCP server connections (client mode)
[[mcp.servers]]
name = "filesystem"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/home/user/documents"]

[[mcp.servers]]
name = "github"
url = "https://api.github.com/mcp"
headers = { Authorization = "Bearer ${GITHUB_TOKEN}" }

[[mcp.servers]]
name = "database"
command = "/usr/local/bin/mcp-server-sqlite"
args = ["--db", "/path/to/data.db"]
env = { DB_READONLY = "true" }
```

### Config Struct Additions

```rust
// In blufio-config/src/model.rs

/// MCP integration configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct McpConfig {
    /// Enable the MCP server (expose Blufio capabilities to MCP clients).
    #[serde(default)]
    pub server_enabled: bool,

    /// Enable the MCP client (consume external MCP servers as tool sources).
    #[serde(default)]
    pub client_enabled: bool,

    /// HTTP endpoint path for Streamable HTTP transport.
    #[serde(default = "default_mcp_endpoint")]
    pub server_endpoint: String,

    /// Server name reported in MCP initialize handshake.
    #[serde(default = "default_mcp_server_name")]
    pub server_name: String,

    /// Server version reported in MCP initialize handshake.
    #[serde(default = "default_mcp_server_version")]
    pub server_version: String,

    /// External MCP server configurations (client mode).
    #[serde(default)]
    pub servers: Vec<McpServerConfig>,
}

/// Configuration for a single external MCP server connection.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct McpServerConfig {
    /// Unique name for this MCP server (used as tool name prefix).
    pub name: String,

    /// Command to spawn for stdio transport (mutually exclusive with url).
    #[serde(default)]
    pub command: Option<String>,

    /// Arguments for the stdio command.
    #[serde(default)]
    pub args: Vec<String>,

    /// URL for Streamable HTTP transport (mutually exclusive with command).
    #[serde(default)]
    pub url: Option<String>,

    /// Extra HTTP headers for Streamable HTTP transport.
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,

    /// Environment variables to set for stdio child process.
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,

    /// Connection timeout in seconds.
    #[serde(default = "default_mcp_timeout")]
    pub timeout_secs: u64,
}
```

## Data Flow Changes

### Current Flow (v1.0, unchanged)

```
User Message
  --> Channel (Telegram/Gateway)
    --> AgentLoop::handle_inbound()
      --> ContextEngine::assemble() [system prompt + memory + history]
      --> ProviderAdapter::stream() [LLM call with tool definitions from ToolRegistry]
        --> LLM responds with tool_use?
          YES --> ToolRegistry::get(name)::invoke() --> tool_result --> re-call LLM
          NO  --> send response via Channel
```

### New Flow (v1.1, additions in brackets)

```
User Message
  --> Channel (Telegram/Gateway)
    --> AgentLoop::handle_inbound()
      --> ContextEngine::assemble() [system prompt + memory + history]
      --> ProviderAdapter::stream() [tool definitions now include [MCP client tools]]
        --> LLM responds with tool_use?
          YES --> ToolRegistry::get(name)::invoke()
                  |
                  +-- Built-in tool? --> direct execution
                  +-- WASM skill?   --> wasmtime sandbox
                  +-- [MCP tool?]   --> [McpRemoteTool::invoke() --> JSON-RPC to external server]
                  |
                  --> tool_result --> re-call LLM
          NO  --> send response via Channel

[MCP Server (parallel, independent):]
  External MCP Client (Claude Desktop)
    --> [blufio mcp-server (stdio)] or [gateway /mcp (Streamable HTTP)]
      --> [BlufioMcpServer::call_tool()] --> ToolRegistry::get()::invoke()
      --> [BlufioMcpServer::list_resources()] --> MemoryStore::list()
      --> [BlufioMcpServer::read_resource()] --> MemoryStore::get()
```

### What Changes in the Agent Loop

**Nothing.** The agent loop is completely unaware of MCP. It continues to:
1. Get tool definitions from ToolRegistry (which now includes MCP client tools)
2. Invoke tools by name via ToolRegistry (McpRemoteTool handles the JSON-RPC call)

This is the beauty of the existing architecture: the `Tool` trait abstraction means new tool sources (MCP servers) plug in without modifying the agent loop.

### What Changes in serve.rs

The `run_serve()` function gains two new initialization blocks:

```rust
// After tool_registry initialization, before agent loop creation:

// Initialize MCP client (if enabled).
#[cfg(feature = "mcp-client")]
let _mcp_client_manager = if config.mcp.client_enabled && !config.mcp.servers.is_empty() {
    match McpClientManager::initialize(&config.mcp.servers, &tool_registry).await {
        Ok(manager) => {
            info!(
                server_count = manager.connection_count(),
                tool_count = manager.tool_count(),
                "MCP client initialized"
            );
            Some(manager)
        }
        Err(e) => {
            warn!(error = %e, "MCP client initialization failed, continuing without external tools");
            None
        }
    }
} else {
    debug!("MCP client disabled or no servers configured");
    None
};
```

And in the gateway Router construction, when `mcp.server_enabled`:

```rust
#[cfg(feature = "mcp-server")]
if config.mcp.server_enabled {
    let mcp_service = BlufioMcpServer::streamable_http_service(
        tool_registry.clone(),
        memory_store.clone(),
    );
    // Mount on the gateway's axum Router at /mcp
    app = app.nest_service(&config.mcp.server_endpoint, mcp_service);
    info!(endpoint = config.mcp.server_endpoint.as_str(), "MCP server mounted on gateway");
}
```

## Dependency Graph

### New Crate Dependencies

```
blufio-mcp-server:
  depends on: blufio-core, blufio-skill (ToolRegistry), blufio-memory (MemoryStore),
              blufio-context (ContextEngine), rmcp
  depended on by: blufio (binary)

blufio-mcp-client:
  depends on: blufio-core (BlufioError), blufio-skill (Tool, ToolOutput, ToolRegistry),
              rmcp
  depended on by: blufio (binary)
```

### New External Dependencies

| Dependency | Version | Purpose | Size Impact |
|------------|---------|---------|-------------|
| `rmcp` | 0.17+ | Official MCP SDK (JSON-RPC, protocol messages, transports) | Moderate -- uses tokio, serde, axum (already in workspace) |
| `schemars` | 1.0 | JSON Schema generation for tool definitions (required by rmcp macros) | Small |

Both `tokio`, `serde`, `serde_json`, and `axum` are already workspace dependencies, so `rmcp`'s transitive deps mostly overlap with the existing workspace.

### Feature Flags (binary crate)

```toml
# In crates/blufio/Cargo.toml [features]
default = [
    "telegram", "anthropic", "sqlite", "onnx", "prometheus",
    "keypair", "gateway", "mcp-server", "mcp-client"
]
mcp-server = ["dep:blufio-mcp-server"]
mcp-client = ["dep:blufio-mcp-client"]
```

Both are enabled by default since this is a v1.1 feature milestone. They can be disabled for minimal builds.

## Build Order (Considering Existing Dependencies)

Given the dependency graph, the build order for the MCP milestone phases:

```
Phase 1: Config model changes (blufio-config)
         No new crate deps, just add McpConfig + McpServerConfig structs.
         Validates: TOML parsing, deny_unknown_fields, defaults.

Phase 2: MCP server crate (blufio-mcp-server)
         Depends on: blufio-core, blufio-skill, blufio-memory
         New external dep: rmcp
         Deliverables: ServerHandler impl, tool/resource/prompt mapping,
                       stdio transport, Streamable HTTP service factory.

Phase 3: MCP server wiring (blufio binary + blufio-gateway)
         Wire MCP server into serve.rs and gateway Router.
         Add `blufio mcp-server` CLI subcommand for stdio mode.
         Test: Claude Desktop can connect and list/call tools.

Phase 4: MCP client crate (blufio-mcp-client)
         Depends on: blufio-core, blufio-skill
         Uses: rmcp client features
         Deliverables: McpClientManager, McpRemoteTool, connection lifecycle.

Phase 5: MCP client wiring (blufio binary)
         Wire McpClientManager into serve.rs.
         Register discovered tools into ToolRegistry.
         Test: Agent can discover and invoke external MCP tools in conversation.

Phase 6: Integration testing + tech debt
         E2E: Claude Desktop -> blufio mcp-server -> tools work
         E2E: Agent uses external MCP tools in conversation
         Fix v1.0 tech debt items (GET /v1/sessions, systemd file).
```

**Phase ordering rationale:**
- Config first because both server and client need it.
- Server before client because the milestone "done" criterion lists Claude Desktop connectivity first, and server is the more visible deliverable.
- Server wiring immediately after server crate so we can test the stdio transport with Claude Desktop early.
- Client after server because client is additive (tools appear in registry) and benefits from server testing revealing any ToolRegistry integration issues.

## Patterns to Follow

### Pattern 1: Feature-Gated Crate Import

Follow the exact pattern used by blufio-gateway, blufio-telegram, etc.

```rust
// In serve.rs
#[cfg(feature = "mcp-server")]
use blufio_mcp_server::BlufioMcpServer;

#[cfg(feature = "mcp-client")]
use blufio_mcp_client::McpClientManager;
```

### Pattern 2: Graceful Degradation on Connection Failure

MCP client connections to external servers MUST fail gracefully. A broken MCP server connection should not prevent Blufio from starting. This matches the pattern used for memory system initialization:

```rust
match McpClientManager::initialize(&config.mcp.servers, &tool_registry).await {
    Ok(manager) => Some(manager),
    Err(e) => {
        warn!(error = %e, "MCP client failed, continuing without");
        None
    }
}
```

### Pattern 3: Tool Trait Implementation for MCP Bridge

MCP remote tools implement the same `Tool` trait as built-ins:

```rust
#[async_trait]
impl Tool for McpRemoteTool {
    fn name(&self) -> &str { &self.prefixed_name }  // "server_name.tool_name"
    fn description(&self) -> &str { &self.description }
    fn parameters_schema(&self) -> serde_json::Value { self.input_schema.clone() }
    async fn invoke(&self, input: serde_json::Value) -> Result<ToolOutput, BlufioError> {
        // Forward to external MCP server via rmcp client
    }
}
```

### Pattern 4: MCP Server as Axum Nested Service

Mount on existing Router, not a separate listener:

```rust
let mcp_service = StreamableHttpService::new(
    move || Ok(BlufioMcpServer::new(/* shared state */)),
    LocalSessionManager::default().into(),
    Default::default(),
);
// Nest at configured endpoint on the existing gateway Router
router = router.nest_service("/mcp", mcp_service);
```

### Pattern 5: Shared State via Arc

The MCP server needs read access to ToolRegistry, MemoryStore, and ContextEngine. These are already `Arc`-wrapped in serve.rs. Pass cloned `Arc` references to the MCP server, same as they are passed to AgentLoop:

```rust
// Both agent loop and MCP server share:
let tool_registry: Arc<RwLock<ToolRegistry>>;  // existing
let memory_store: Arc<MemoryStore>;             // existing (if memory enabled)
```

## Anti-Patterns to Avoid

### Anti-Pattern 1: MCP Server as Channel Adapter
**What:** Making the MCP server implement ChannelAdapter and feeding into the agent loop.
**Why bad:** MCP server handles tool calls and resource reads, not conversations. An MCP client asking for `tools/list` is not sending a user message. The lifecycle, auth model, and data flow are fundamentally different from a channel.
**Instead:** MCP server reads from shared state (ToolRegistry, MemoryStore) directly, bypassing the agent loop entirely.

### Anti-Pattern 2: Separate TCP Port for MCP
**What:** Opening a second TCP port for the MCP Streamable HTTP endpoint.
**Why bad:** Doubles operational complexity (firewall rules, TLS certs, health checks). The axum Router pattern supports mounting multiple services on one listener.
**Instead:** Mount MCP at `/mcp` on the existing gateway listener.

### Anti-Pattern 3: Blocking MCP Client Initialization
**What:** Failing to start Blufio if any external MCP server is unreachable.
**Why bad:** External servers may be temporarily down. Blufio's core value is reliability ("months without restart").
**Instead:** Warn and continue. MCP tools from unreachable servers simply are not registered.

### Anti-Pattern 4: Duplicating Tool Schema Conversion
**What:** Writing separate schema conversion for MCP server (Blufio to MCP) and MCP client (MCP to Blufio) that share no code.
**Why bad:** Both directions deal with the same JSON Schema for tool parameters. Divergent implementations lead to subtle incompatibilities.
**Instead:** Use shared conversion functions for tool schema marshaling between Blufio's tool_definitions format and MCP's tool format.

### Anti-Pattern 5: Write Lock on ToolRegistry for MCP Server Reads
**What:** Taking a write lock on ToolRegistry when MCP server only needs to read.
**Why bad:** MCP server tool listing is a hot path (called frequently by MCP clients). Write locks block the agent loop's tool execution.
**Instead:** The ToolRegistry is already behind `Arc<RwLock<ToolRegistry>>`. MCP server takes read locks only.

## Scalability Considerations

| Concern | Single user (v1.1) | 10 MCP clients | 100 MCP clients |
|---------|---------------------|-----------------|-----------------|
| MCP server connections | stdio (1 Claude Desktop) + Streamable HTTP | Streamable HTTP with session management | Need connection limits, backpressure |
| MCP client connections | 1-3 external servers | Same (configured, not dynamic) | Same (configured, not dynamic) |
| Tool registry size | ~10 built-in + ~5 MCP | Same | Same |
| Memory impact | ~2-5MB for rmcp + connections | ~5-10MB | Needs investigation |

For v1.1 (single operator), scalability is not a concern. The architecture supports growth because:
- rmcp's `LocalSessionManager` handles multiple Streamable HTTP sessions
- ToolRegistry reads are O(1) hashmap lookups
- MCP client connections are static (configured, not on-demand)

## Security Considerations

### MCP Server Auth

The MCP Streamable HTTP endpoint (`/mcp`) MUST be protected by the same auth middleware as the gateway's `/v1/*` routes (bearer token or Ed25519 keypair). The stdio transport inherits process-level access control (whoever can run `blufio mcp-server` has access).

### MCP Client Security

External MCP servers are untrusted. The MCP client MUST:
1. Validate all tool names from external servers (no injection via tool names)
2. Sanitize tool outputs before passing to the LLM (prevent prompt injection)
3. Respect SSRF protections when connecting to external servers via Streamable HTTP
4. Never expose Blufio's vault secrets to external MCP servers
5. Apply timeouts on all MCP client RPC calls (prevent hung connections)

### Tool Annotations

Per the MCP spec: "tool annotations should be considered untrusted unless obtained from a trusted server." Blufio should NOT propagate tool annotations from external MCP servers to the LLM without operator review.

## Sources

- [MCP Specification 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25) -- HIGH confidence
- [MCP Architecture](https://modelcontextprotocol.io/specification/2025-11-25/architecture) -- HIGH confidence
- [MCP Server Tools Spec](https://modelcontextprotocol.io/specification/2025-11-25/server/tools) -- HIGH confidence
- [MCP Server Resources Spec](https://modelcontextprotocol.io/specification/2025-11-25/server/resources) -- HIGH confidence
- [MCP Transports: Streamable HTTP](https://modelcontextprotocol.io/specification/2025-03-26/basic/transports) -- HIGH confidence
- [rmcp crate - Official Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk) -- HIGH confidence, v0.17.0, 3.1k stars
- [rmcp docs.rs API reference](https://docs.rs/rmcp/latest/rmcp/) -- HIGH confidence
- [Building Streamable HTTP MCP Server in Rust (Shuttle)](https://www.shuttle.dev/blog/2025/10/29/stream-http-mcp) -- MEDIUM confidence
- [Claude Desktop MCP Configuration](https://support.claude.com/en/articles/10949351-getting-started-with-local-mcp-servers-on-claude-desktop) -- HIGH confidence
- [Why MCP deprecated SSE for Streamable HTTP](https://blog.fka.dev/blog/2025-06-06-why-mcp-deprecated-sse-and-go-with-streamable-http/) -- MEDIUM confidence
- Blufio codebase analysis (19 crates, serve.rs, gateway, skill, agent loop examined directly) -- HIGH confidence
