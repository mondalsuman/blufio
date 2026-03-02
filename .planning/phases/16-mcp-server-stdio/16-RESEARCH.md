# Phase 16: MCP Server stdio - Research

**Researched:** 2026-03-02
**Domain:** MCP protocol server implementation (rmcp SDK, stdio transport, tool bridging)
**Confidence:** HIGH

## Summary

Phase 16 implements the MCP server handler so Claude Desktop can connect to Blufio via stdio and invoke skills as MCP tools. The rmcp 0.17 SDK provides a `ServerHandler` trait with default method implementations for `initialize`, `list_tools`, and `call_tool`. The stdio transport is built into rmcp via the `transport-io` feature (already enabled in Cargo.toml). The primary engineering challenge is bridging Blufio's `ToolRegistry` (Anthropic-format tool definitions) to MCP's tool schema format, and enforcing the export allowlist with bash permanently excluded.

**Primary recommendation:** Implement a `BlufioMcpHandler` struct holding `Arc<RwLock<ToolRegistry>>` and `McpConfig`, implementing `ServerHandler` with custom `initialize`, `list_tools`, and `call_tool` methods. Use `serve_server_with_ct` for stdio transport with `CancellationToken` for clean shutdown. Add `mcp_server.rs` subcommand module in the main binary following the same pattern as `serve.rs` but with stripped-down infrastructure (storage + vault + tool registry only, no agent loop/channels/mux).

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Empty `mcp.export_tools` = export all non-bash tools (built-in + WASM skills). Secure default that's useful out of the box
- WASM skills installed at runtime are automatically visible to MCP clients (subject to allowlist)
- If `bash` appears in `mcp.export_tools`, silently ignore it and log a warning. Don't fail startup
- `mcp.export_tools` supports exact tool names only -- no glob patterns. Simple and predictable
- Handler holds `Arc<RwLock<ToolRegistry>>` -- same shared reference pattern as serve.rs
- Advertise `tools` capability only during initialize. Resources, prompts, and notifications deferred to Phase 17
- `blufio mcp-server` runs as a standalone process -- no agent loop, no Telegram, no gateway. Lighter weight, faster startup
- Full infrastructure stack: storage + vault initialized for WASM skill lookup and tool invocation. Same init pattern as serve.rs minus channels/mux/agent loop
- Shutdown on SIGTERM/SIGINT OR stdin EOF (parent process closed pipe). Standard for stdio-based MCP servers
- Minimal stderr banner on startup: version + "MCP server ready". All tracing goes to stderr (SRVR-15)
- No CLI flags -- all behavior driven by blufio.toml `[mcp]` section. Config file only
- Claude Desktop config entry: `{"command": "blufio", "args": ["mcp-server"]}`. No env vars needed
- Tool execution errors returned as successful JSON-RPC response with `isError: true` in tool result content -- MCP convention
- Input validation errors (SRVR-05) return JSON-RPC -32602 with focused message: what's wrong + expected fields. No full schema dump
- Actionable error messages for tool failures: what went wrong + what to try. LLM-readable
- 60s default timeout per tool invocation, configurable in `[mcp]` config. Prevents hung WASM skills from blocking the connection

### Claude's Discretion
- Internal module structure within blufio-mcp-server (handler.rs, transport.rs, etc.)
- Exact rmcp ServerHandler trait method implementations
- How tool_definitions() output maps to MCP tool schema format
- Stderr logging format and verbosity levels
- Exact timeout config field name and location

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| SRVR-01 | User can connect Claude Desktop to Blufio via stdio and list available tools | rmcp `ServerHandler::list_tools` + `serve_server_with_ct` with stdio transport |
| SRVR-02 | User can invoke Blufio skills from Claude Desktop via MCP tools/call | rmcp `ServerHandler::call_tool` bridged to `ToolRegistry::get().invoke()` |
| SRVR-03 | `blufio mcp-server` CLI subcommand for stdio-only mode (no agent loop) | New `McpServer` variant in Commands enum, `mcp_server.rs` module |
| SRVR-04 | Capability negotiation (initialize/initialized handshake) with MCP spec | rmcp `ServerHandler::initialize` returning `InitializeResult` with `ServerCapabilities` |
| SRVR-05 | Tool input validation against inputSchema with JSON-RPC -32602 errors | jsonschema crate for validation, return `McpError` with code -32602 |
| SRVR-12 | Explicit MCP tool export allowlist (bash permanently excluded) | Filter in `list_tools` and `call_tool` against `McpConfig::export_tools` |
| SRVR-15 | All logging redirected to stderr in stdio mode | tracing-subscriber writing to stderr, `#[deny(clippy::print_stdout)]` lint |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rmcp | 0.17 | MCP protocol SDK (ServerHandler trait, stdio transport) | Official Anthropic-maintained Rust SDK; already in workspace |
| tokio | workspace | Async runtime for server loop | Standard Rust async; already used throughout project |
| tokio-util | workspace | CancellationToken for graceful shutdown | Already used via `shutdown::install_signal_handler()` |
| serde_json | 1 | JSON-RPC message handling and tool parameter bridging | Already in workspace |
| tracing | workspace | Structured logging to stderr | Already used throughout project |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| jsonschema | 0.28+ | Tool input validation against JSON Schema (SRVR-05) | Validate tool inputs before invocation |
| schemars | 1 | JSON Schema generation (already in workspace) | Already used for schema generation |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| jsonschema crate | Manual validation | jsonschema handles all JSON Schema keywords; manual is brittle and incomplete |
| Custom stdio reader | rmcp transport-io | rmcp handles framing, buffering, EOF detection; custom adds bugs |

## Architecture Patterns

### Recommended Module Structure
```
crates/blufio-mcp-server/src/
├── lib.rs           # Public API, re-exports
├── types.rs         # McpSessionId (existing)
├── handler.rs       # BlufioMcpHandler implementing ServerHandler
└── bridge.rs        # Tool registry -> MCP tool schema bridging + allowlist filtering
```

### Pattern 1: ServerHandler Implementation
**What:** Implement `ServerHandler` directly (not via `#[tool_handler]` macro) because Blufio has its own ToolRegistry that needs dynamic bridging, not rmcp's `ToolRouter`.
**When to use:** Always for this phase -- the macro expects rmcp's own tool routing, but we bridge from Blufio's existing `ToolRegistry`.
**Example:**
```rust
// Source: rmcp 0.17 docs
pub struct BlufioMcpHandler {
    tool_registry: Arc<RwLock<ToolRegistry>>,
    config: McpConfig,
}

impl ServerHandler for BlufioMcpHandler {
    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability::default()),
                // resources, prompts: None (Phase 17)
                ..Default::default()
            },
            server_info: Implementation {
                name: "blufio".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            ..Default::default()
        })
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let registry = self.tool_registry.read().await;
        let tools = self.filtered_tool_list(&registry);
        Ok(ListToolsResult { tools, next_cursor: None })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        // Validate tool is in allowlist, invoke, return result
    }
}
```

### Pattern 2: Tool Schema Bridging
**What:** Convert Blufio's Anthropic-format tool definitions to MCP tool schema format.
**When to use:** In `list_tools` and when building tool definitions.
**Mapping:**
```
Anthropic format:               MCP format:
{                               rmcp::model::Tool {
  "name": "http",                  name: "http",
  "description": "...",            description: Some("..."),
  "input_schema": {...}            input_schema: ToolInputSchema {
}                                    type_: "object",
                                     properties: {...},
                                     required: [...],
                                  }
                                }
```

### Pattern 3: Stdio Transport Setup
**What:** Use rmcp's built-in stdio transport via `serve_server_with_ct`.
**When to use:** In the `mcp_server.rs` CLI subcommand entry point.
**Example:**
```rust
use rmcp::ServiceExt;
use tokio::io::{stdin, stdout};

let handler = BlufioMcpHandler::new(tool_registry, config.mcp.clone());
let ct = CancellationToken::new();

// rmcp provides IntoTransport for (stdin, stdout) pairs
let service = handler.serve_with_ct((stdin(), stdout()), ct.clone()).await?;

// Wait for either shutdown signal or service completion
tokio::select! {
    _ = cancel.cancelled() => { /* signal received */ }
    _ = service.waiting() => { /* client disconnected / EOF */ }
}
```

### Pattern 4: Stdin EOF Detection for Shutdown
**What:** When Claude Desktop closes the pipe (parent exits), stdin reaches EOF. rmcp's stdio transport handles this -- the service future completes.
**When to use:** The `tokio::select!` above covers this case via `service.waiting()`.

### Anti-Patterns to Avoid
- **Using `#[tool_handler]` macro:** This expects rmcp's `ToolRouter` with static tool definitions. Blufio has a dynamic `ToolRegistry` that must be bridged at runtime.
- **Printing to stdout:** Any non-JSON-RPC output to stdout corrupts the protocol stream. ALL logging must go to stderr.
- **Blocking the async runtime:** Tool invocations (especially WASM) can take time. Always use `tokio::time::timeout()` to enforce the 60s default.
- **Exposing bash tool via MCP:** Security critical -- bash must NEVER appear in MCP tool list regardless of config.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| JSON-RPC framing | Custom stdin/stdout reader | rmcp transport-io | Handles message framing, buffering, EOF, error recovery |
| MCP protocol handshake | Manual initialize/initialized | rmcp ServerHandler | Correct protocol state machine with version negotiation |
| JSON Schema validation | Manual field checking | jsonschema crate | Handles all JSON Schema keywords including nested objects, arrays, enums |
| Signal handling | Custom signal code | `shutdown::install_signal_handler()` | Already proven in serve.rs, handles SIGTERM + SIGINT |

**Key insight:** rmcp handles all protocol-level concerns (framing, handshake, method dispatch). The only custom work is bridging Blufio's `ToolRegistry` to rmcp's tool types and enforcing the export allowlist.

## Common Pitfalls

### Pitfall 1: Stdout Contamination
**What goes wrong:** Any `println!()`, `dbg!()`, or logging to stdout corrupts the JSON-RPC stream, causing Claude Desktop to disconnect.
**Why it happens:** Rust's default print macros write to stdout. Tracing defaults to stdout in some configurations.
**How to avoid:** Use `#[deny(clippy::print_stdout)]` lint on the mcp-server crate. Configure tracing-subscriber to write to stderr only. Test by running the server and verifying no non-JSON-RPC output on stdout.
**Warning signs:** Claude Desktop connects but immediately errors or shows "invalid JSON" errors.

### Pitfall 2: Tool Schema Mismatch
**What goes wrong:** Blufio's `parameters_schema()` returns Anthropic-format JSON Schema, but MCP expects the schema in `ToolInputSchema` struct format with explicit `type_`, `properties`, and `required` fields.
**Why it happens:** Different JSON Schema representations between Anthropic API and MCP spec.
**How to avoid:** Write a bridge function that destructures the `serde_json::Value` from `parameters_schema()` into rmcp's `ToolInputSchema` fields. Test with actual tool schemas.
**Warning signs:** Claude Desktop shows tools but reports "invalid schema" or fails to generate valid tool calls.

### Pitfall 3: Missing Bash Exclusion
**What goes wrong:** bash tool appears in MCP tools/list, giving Claude Desktop arbitrary command execution on the host.
**Why it happens:** Filtering is forgotten or has an edge case (e.g., allowlist empty means "all" but should still exclude bash).
**How to avoid:** The bash exclusion filter runs AFTER allowlist resolution. If allowlist is empty (meaning "all non-bash"), bash is excluded. If allowlist is non-empty and includes "bash", bash is still excluded with a warning log.
**Warning signs:** Security audit fails, or `blufio mcp-server` start logs show "bash" in tool list.

### Pitfall 4: Blocking Tool Invocations Without Timeout
**What goes wrong:** A WASM skill hangs indefinitely, blocking the entire MCP connection for all other tool calls.
**Why it happens:** No timeout on `tool.invoke()`.
**How to avoid:** Wrap every `tool.invoke()` call in `tokio::time::timeout(duration, ...)`. Return `isError: true` with timeout message on `Elapsed`.
**Warning signs:** Claude Desktop shows "connection lost" or "tool timeout" after long waits.

### Pitfall 5: Not Handling Invalid Tool Names in call_tool
**What goes wrong:** Client sends `tools/call` with a tool name not in the export list, or a tool that doesn't exist in the registry.
**Why it happens:** Missing validation before tool lookup.
**How to avoid:** Check export allowlist first, then check registry. Return JSON-RPC -32602 (Invalid params) for unknown/non-exported tools.
**Warning signs:** Panics or unhelpful error messages.

## Code Examples

### Tool Allowlist Filtering
```rust
/// Returns the filtered list of tools based on export_tools config.
fn filtered_tools(&self, registry: &ToolRegistry) -> Vec<(String, Arc<dyn Tool>)> {
    let export_list = &self.config.export_tools;

    registry.list().into_iter()
        .filter(|(name, _)| {
            // ALWAYS exclude bash
            if *name == "bash" { return false; }

            // Empty export_tools = export all non-bash tools
            if export_list.is_empty() { return true; }

            // Explicit allowlist
            export_list.iter().any(|e| e == name)
        })
        .map(|(name, _)| {
            let tool = registry.get(name).unwrap();
            (name.to_string(), tool)
        })
        .collect()
}
```

### Tool Schema Bridge (Anthropic -> MCP)
```rust
/// Converts a Blufio tool's parameters_schema() to rmcp ToolInputSchema.
fn to_mcp_input_schema(schema: &serde_json::Value) -> ToolInputSchema {
    ToolInputSchema {
        type_: "object".to_string(),
        properties: schema.get("properties")
            .cloned()
            .unwrap_or_default(),
        required: schema.get("required")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default(),
    }
}
```

### MCP Server Startup (mcp_server.rs)
```rust
pub async fn run_mcp_server(config: BlufioConfig) -> Result<(), BlufioError> {
    // SRVR-15: All tracing to stderr
    init_tracing_stderr(&config.agent.log_level);

    // Warn if bash in export_tools
    if config.mcp.export_tools.iter().any(|t| t == "bash") {
        tracing::warn!("'bash' in mcp.export_tools is ignored (security: never exported)");
    }

    // Minimal infrastructure (no agent loop, no channels)
    let db = open_db(&config).await?;
    // ... vault, tool registry, skill provider init (same as serve.rs) ...

    eprintln!("blufio {} MCP server ready", env!("CARGO_PKG_VERSION"));

    let handler = BlufioMcpHandler::new(tool_registry, config.mcp.clone());
    let cancel = shutdown::install_signal_handler();

    let service = handler
        .serve_with_ct((tokio::io::stdin(), tokio::io::stdout()), cancel.clone())
        .await
        .map_err(|e| BlufioError::Internal(format!("MCP server init failed: {e}")))?;

    // Wait for shutdown signal OR stdin EOF
    tokio::select! {
        _ = cancel.cancelled() => {
            tracing::info!("MCP server shutting down (signal)");
        }
        _ = service.waiting() => {
            tracing::info!("MCP server shutting down (client disconnected)");
        }
    }

    Ok(())
}
```

### Input Validation (SRVR-05)
```rust
async fn call_tool(
    &self,
    request: CallToolRequestParam,
    _context: RequestContext<RoleServer>,
) -> Result<CallToolResult, McpError> {
    let tool_name = &request.name;

    // Check export allowlist
    if !self.is_tool_exported(tool_name) {
        return Err(McpError::invalid_params(
            format!("tool '{}' is not available", tool_name), None
        ));
    }

    // Look up tool
    let registry = self.tool_registry.read().await;
    let tool = registry.get(tool_name).ok_or_else(|| {
        McpError::invalid_params(
            format!("tool '{}' not found", tool_name), None
        )
    })?;

    // Validate input against schema
    let input = request.arguments.unwrap_or_default();
    let schema = tool.parameters_schema();
    if let Err(errors) = validate_input(&input, &schema) {
        return Err(McpError::invalid_params(
            format!("invalid input for '{}': {}", tool_name, errors), None
        ));
    }

    // Invoke with timeout
    let timeout_duration = Duration::from_secs(self.timeout_secs);
    match tokio::time::timeout(timeout_duration, tool.invoke(input)).await {
        Ok(Ok(output)) => Ok(CallToolResult {
            content: vec![Content::text(output.content)],
            is_error: Some(output.is_error),
            ..Default::default()
        }),
        Ok(Err(e)) => Ok(CallToolResult {
            content: vec![Content::text(format!("Tool error: {e}. Try checking the input parameters."))],
            is_error: Some(true),
            ..Default::default()
        }),
        Err(_) => Ok(CallToolResult {
            content: vec![Content::text(format!(
                "Tool '{}' timed out after {}s. The operation may still be running.",
                tool_name, self.timeout_secs
            ))],
            is_error: Some(true),
            ..Default::default()
        }),
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| SSE transport (deprecated in MCP) | Streamable HTTP + stdio | MCP spec 2025-11-25 | Phase 16 uses stdio only; HTTP in Phase 17 |
| Custom JSON-RPC framing | rmcp SDK handles framing | rmcp 0.17 | No need to implement low-level protocol |
| Manual capability negotiation | rmcp ServerHandler default impl | rmcp 0.17 | initialize/initialized handled by trait defaults |

## Open Questions

1. **McpConfig timeout field**
   - What we know: CONTEXT.md specifies 60s default, configurable in `[mcp]` config
   - What's unclear: Exact field name (e.g., `tool_timeout_secs` vs `timeout_secs`)
   - Recommendation: Add `tool_timeout_secs: u64` with `#[serde(default = "default_tool_timeout")]` returning 60. Consistent with existing config style.

2. **jsonschema crate addition**
   - What we know: Need JSON Schema validation for SRVR-05
   - What's unclear: Whether jsonschema is already in workspace Cargo.toml
   - Recommendation: Add `jsonschema = "0.28"` to workspace dependencies. Lightweight, well-maintained, supports JSON Schema draft 2020-12.

3. **rmcp Content type for tool results**
   - What we know: rmcp has `Content::text()` and related types
   - What's unclear: Exact import path and whether `CallToolResult` has `is_error` field or uses wrapper
   - Recommendation: Verify exact types during implementation. The rmcp docs show `CallToolResult` with `content: Vec<Content>` and `is_error: Option<bool>`.

## Sources

### Primary (HIGH confidence)
- Context7 /websites/rs_rmcp - ServerHandler trait, serve_with_ct, ServerCapabilities, tool methods
- rmcp 0.17 Cargo.toml in workspace - version and features (transport-io, server, macros)
- Blufio codebase - ToolRegistry, McpConfig, serve.rs infrastructure patterns, CLI structure

### Secondary (MEDIUM confidence)
- MCP spec 2025-11-25 - capability negotiation, tools/list, tools/call, error codes

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - rmcp already in workspace, all patterns verified from codebase
- Architecture: HIGH - direct mapping from existing serve.rs patterns and rmcp API
- Pitfalls: HIGH - based on MCP protocol specifics and known stdio transport gotchas

**Research date:** 2026-03-02
**Valid until:** 2026-04-02 (stable -- rmcp 0.17 is established)
