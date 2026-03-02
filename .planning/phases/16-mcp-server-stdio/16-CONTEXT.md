# Phase 16: MCP Server stdio - Context

**Gathered:** 2026-03-02
**Status:** Ready for planning

<domain>
## Phase Boundary

Implement the MCP server handler so operators can point Claude Desktop at `blufio mcp-server` via stdio and invoke Blufio skills as MCP tools. Covers: ServerHandler trait implementation, tools/list, tools/call, stdio transport, capability negotiation, tool export allowlist enforcement, input validation, and the `mcp-server` CLI subcommand. HTTP transport, resources, prompts, and notifications are Phase 17.

</domain>

<decisions>
## Implementation Decisions

### Tool export policy
- Empty `mcp.export_tools` = export all non-bash tools (built-in + WASM skills). Secure default that's useful out of the box
- WASM skills installed at runtime are automatically visible to MCP clients (subject to allowlist)
- If `bash` appears in `mcp.export_tools`, silently ignore it and log a warning. Don't fail startup
- `mcp.export_tools` supports exact tool names only — no glob patterns. Simple and predictable

### ServerHandler design
- Handler holds `Arc<RwLock<ToolRegistry>>` — same shared reference pattern as serve.rs. Supports dynamic skill installation updating the tool list while MCP server is running
- Advertise `tools` capability only during initialize. Resources, prompts, and notifications deferred to Phase 17
- `blufio mcp-server` runs as a standalone process — no agent loop, no Telegram, no gateway. Lighter weight, faster startup. Claude Desktop launches this directly (SRVR-03)
- Full infrastructure stack: storage + vault initialized for WASM skill lookup and tool invocation. Same init pattern as serve.rs minus channels/mux/agent loop

### CLI subcommand behavior
- Shutdown on SIGTERM/SIGINT OR stdin EOF (parent process closed pipe). Standard for stdio-based MCP servers
- Minimal stderr banner on startup: version + "MCP server ready". All tracing goes to stderr (SRVR-15)
- No CLI flags — all behavior driven by blufio.toml `[mcp]` section. Config file only
- Claude Desktop config entry: `{"command": "blufio", "args": ["mcp-server"]}`. No env vars needed

### Error responses
- Tool execution errors returned as successful JSON-RPC response with `isError: true` in tool result content — MCP convention
- Input validation errors (SRVR-05) return JSON-RPC -32602 with focused message: what's wrong + expected fields. No full schema dump
- Actionable error messages for tool failures: what went wrong + what to try. LLM-readable
- 60s default timeout per tool invocation, configurable in `[mcp]` config. Prevents hung WASM skills from blocking the connection

### Claude's Discretion
- Internal module structure within blufio-mcp-server (handler.rs, transport.rs, etc.)
- Exact rmcp ServerHandler trait method implementations
- How tool_definitions() output maps to MCP tool schema format
- Stderr logging format and verbosity levels
- Exact timeout config field name and location

</decisions>

<specifics>
## Specific Ideas

- Claude Desktop config should be dead simple: `{"command": "blufio", "args": ["mcp-server"]}` — no extra env vars or config paths needed
- The standalone mcp-server process should start fast — skip everything not needed for tool invocation (no Telegram, no gateway, no agent loop, no heartbeat)
- Follow the same infrastructure initialization pattern as serve.rs (storage, vault, tool registry, skill provider) but stripped down to just what tools need

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ToolRegistry` (blufio-skill/src/tool.rs): Already has `tool_definitions()` returning Anthropic-format JSON, `register_builtin()`, `register_namespaced()`, namespace collision detection. Core bridge to MCP tools/list
- `Tool` trait (blufio-skill/src/tool.rs): `name()`, `description()`, `parameters_schema()`, `invoke()` — maps directly to MCP tool definition and tools/call
- `McpSessionId` (blufio-mcp-server/src/types.rs): Ready-to-use newtype for MCP session tracking
- `McpConfig` (blufio-config/src/model.rs): Has `enabled`, `export_tools: Vec<String>`, `servers: Vec<McpServerEntry>` — allowlist already parseable from TOML
- `register_builtins()` (blufio-skill/src/builtin/mod.rs): Registers bash, http, file tools — need to filter bash out for MCP export
- `SkillProvider` (blufio-skill): Manages WASM skill discovery — can be used to populate MCP tool list

### Established Patterns
- Config: `#[serde(deny_unknown_fields)]` on all structs, Figment TOML+env loading
- CLI: clap `Commands` enum in main.rs, each subcommand in its own module (serve.rs, shell.rs, doctor.rs)
- Infrastructure init: serve.rs pattern — open DB, unlock vault, create tool registry, register builtins
- Shared state: `Arc<RwLock<ToolRegistry>>` for dynamic tool registration (serve.rs:199)
- Shutdown: `tokio_util::sync::CancellationToken` via `shutdown::install_signal_handler()`

### Integration Points
- `Commands` enum in main.rs needs `McpServer` variant
- New `mcp_server.rs` module in blufio/src/ for the subcommand handler
- blufio-mcp-server crate needs ServerHandler implementation, tool bridging, and stdio transport
- `Cargo.toml` of main blufio binary needs `blufio-mcp-server` dependency (feature-gated)

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 16-mcp-server-stdio*
*Context gathered: 2026-03-02*
