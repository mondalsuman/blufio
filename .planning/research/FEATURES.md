# Feature Landscape: MCP Integration (Server + Client)

**Domain:** MCP integration for existing Rust AI agent platform (Blufio v1.1)
**Researched:** 2026-03-02
**Confidence:** HIGH (verified against MCP spec 2025-11-25, official Rust SDK rmcp 0.17, existing codebase)

---

## Context

Blufio v1.0 shipped with 28,790 LOC Rust, 14 crates, and a complete agent platform (FSM sessions, Telegram, WASM skills, memory, model routing, multi-agent delegation, Prometheus). This research covers ONLY the MCP server and MCP client features for v1.1.

MCP (Model Context Protocol) spec version targeted: **2025-11-25** (latest stable).

---

## Table Stakes

Features that any MCP integration must have. Missing these = the MCP integration is broken or useless.

### MCP Server: Table Stakes

| Feature | Why Expected | Complexity | Blufio Dependency | Notes |
|---------|--------------|------------|-------------------|-------|
| **JSON-RPC 2.0 message layer** | MCP is built on JSON-RPC 2.0. Every request, response, notification, and error follows this format. Without it, nothing works. | LOW | New code (blufio-mcp crate) | serde + serde_json already in workspace. The rmcp crate handles this, but Blufio could also implement the thin JSON-RPC layer directly for control. |
| **Capability negotiation (initialize/initialized handshake)** | Spec-mandated lifecycle. Client sends `initialize` with its capabilities, server responds with its capabilities and protocol version. Then client sends `initialized` notification. Without this, no MCP client can connect. | LOW | New code | Must declare which server features are supported: tools, resources, prompts. Must include `protocolVersion: "2025-11-25"`. |
| **tools/list and tools/call** | The primary reason to expose Blufio as an MCP server. External clients (Claude Desktop, VS Code, other agents) discover and invoke Blufio's tools. This is the #1 use case in PROJECT.md ("point Claude Desktop at Blufio via stdio and use skills/memory"). | MEDIUM | ToolRegistry, Tool trait, SkillRuntimeAdapter | Map Blufio's existing `Tool` trait (name, description, parameters_schema, invoke) directly to MCP tool definitions (name, description, inputSchema, tools/call). The mapping is nearly 1:1. Built-in tools (bash, HTTP, file) + WASM skills all become MCP tools. |
| **stdio transport** | Spec says clients SHOULD support stdio. Claude Desktop, Claude Code, and most local MCP clients use stdio as the primary transport. PROJECT.md explicitly lists stdio as a required transport. | MEDIUM | New code (stdin/stdout JSON-RPC loop) | Blufio runs as subprocess of the host (Claude Desktop launches `blufio mcp-server`). Communication over stdin/stdout. Must handle line-delimited JSON-RPC. The rmcp crate provides `TokioChildProcess` and stdio transport. |
| **Streamable HTTP transport** | The modern standard for remote MCP connections (replaced legacy SSE). PROJECT.md lists this as required. Needed for programmatic clients and remote access. | MEDIUM | blufio-gateway (axum) | Single HTTP endpoint, POST for requests, GET for SSE stream. Can coexist with existing axum gateway routes. The rmcp crate has `transport-streamable-http-server` feature. |
| **SSE transport (legacy)** | PROJECT.md lists SSE as a required transport. While deprecated in favor of Streamable HTTP, many existing clients still use it. Needed for backward compatibility. | LOW | blufio-gateway (axum) | SSE endpoint + POST endpoint. Axum already has SSE support. Can share transport layer with Streamable HTTP. |
| **Tool input validation** | Spec requires servers MUST validate all tool inputs. Blufio already has JSON Schema for each tool via `parameters_schema()`. | LOW | Existing Tool trait | Validate incoming `tools/call` arguments against the tool's inputSchema before invoking. Return JSON-RPC error (-32602) for invalid input. |
| **Error handling (protocol + tool execution)** | Spec distinguishes protocol errors (JSON-RPC errors for unknown tools, malformed requests) from tool execution errors (returned in result with `isError: true`). Must implement both. | LOW | Existing ToolOutput.is_error | Direct mapping: Blufio's `ToolOutput { content, is_error }` maps to MCP's `{ content: [{ type: "text", text }], isError }`. Protocol errors use standard JSON-RPC error codes. |
| **Ping/keepalive** | Spec defines `ping` method for connection health checking. Clients send pings, server must respond. | LOW | New code | Trivial: respond to `ping` with empty result. |

### MCP Client: Table Stakes

| Feature | Why Expected | Complexity | Blufio Dependency | Notes |
|---------|--------------|------------|-------------------|-------|
| **MCP client connection manager** | Must connect to external MCP servers, perform initialization handshake, discover capabilities. Without this, no external MCP tools are available. | MEDIUM | New code (blufio-mcp crate) | Manages lifecycle: connect -> initialize -> discover tools -> maintain connection. Must handle multiple concurrent MCP server connections. |
| **stdio transport (client side)** | Must launch external MCP servers as subprocesses and communicate via stdin/stdout. This is how most MCP servers are distributed (CLI tools). | MEDIUM | New code (tokio process) | Launch configured command, pipe stdin/stdout, parse JSON-RPC. The rmcp crate's `TokioChildProcess` handles this. |
| **Streamable HTTP transport (client side)** | Must connect to remote MCP servers over HTTP. Needed for cloud-hosted MCP services. | LOW | reqwest (already in workspace) | HTTP POST for requests, optional SSE for notifications. |
| **tools/list discovery** | Must discover available tools from each connected MCP server. Tools become available to the agent during conversations. | LOW | ToolRegistry | Fetch tool list from each MCP server, register them in the agent's ToolRegistry with a namespace prefix (e.g., `mcp.github.create_issue`). |
| **tools/call invocation** | When the LLM requests an MCP tool, the client must route the call to the correct MCP server and return the result. | MEDIUM | Agent loop (session.rs) | Agent loop already handles tool_use content blocks. Add MCP tool routing: if tool name starts with `mcp.{server_name}.`, route to that MCP server's tools/call endpoint. |
| **TOML configuration for MCP servers** | Operators configure which MCP servers to connect to, their transport type, command/URL, and environment variables. PROJECT.md explicitly requires TOML-based config. | LOW | blufio-config | Add `[[mcp.servers]]` array to TOML config. Each entry: name, transport (stdio/http), command or url, optional env vars, optional args. |
| **Connection lifecycle management** | Handle MCP server startup, crashes, reconnection, and shutdown. Always-on agent must recover from MCP server failures without crashing. | MEDIUM | New code | Retry with backoff on connection failure. Log errors, mark server as degraded. Continue operating with remaining MCP servers. Health check integration. |
| **Tool schema forwarding to LLM** | External MCP tools must be included in the LLM's tool definitions so the model can discover and use them. | LOW | ToolRegistry.tool_definitions() | MCP tool schemas (inputSchema) map directly to Anthropic tool definitions (input_schema). Add MCP tools to the definitions array sent to the provider. |

---

## Differentiators

Features that go beyond basic MCP compliance. These create real value for Blufio operators.

### MCP Server: Differentiators

| Feature | Value Proposition | Complexity | Blufio Dependency | Notes |
|---------|-------------------|------------|-------------------|-------|
| **Expose memory as MCP resources** | External clients can read Blufio's long-term memory via `resources/read`. URI scheme: `blufio://memory/{id}` for specific memories, `blufio://memory/search?q={query}` as resource template. Unique: most MCP servers are stateless tools. Blufio has a real memory system with hybrid search. | MEDIUM | MemoryStore, HybridRetriever | Expose `resources/list` (recent/important memories), `resources/templates/list` (search template), `resources/read` (fetch specific memory or search results). This lets Claude Desktop "see" what Blufio remembers. |
| **Expose sessions as MCP resources** | External clients can read conversation history. URI: `blufio://session/{id}` for session messages, `blufio://sessions` for listing. Useful for debugging, auditing, or feeding session context into other tools. | LOW | StorageAdapter (sessions, messages) | Read-only access to session data. Paginated. Operators can configure which sessions are exposed (all vs. specific). |
| **MCP prompts for agent workflows** | Expose predefined prompt templates via `prompts/list` and `prompts/get`. Examples: "summarize-session" (summarize a conversation), "remember-fact" (store a memory), "search-memory" (semantic search). These become slash commands in Claude Desktop. | MEDIUM | Context engine, memory system | Prompts are dynamic: "summarize-session" takes a session_id argument and returns a formatted prompt with the session's messages embedded. More useful than static templates because they pull real data. |
| **Tool annotations** | Add `readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint` to each exposed tool. Helps clients display appropriate UI (confirmation dialogs for destructive tools, auto-approve for read-only). | LOW | Tool trait metadata | bash tool: `destructiveHint: true, openWorldHint: true`. HTTP tool: `openWorldHint: true`. file read: `readOnlyHint: true`. Memory search: `readOnlyHint: true, idempotentHint: true`. Improves trust and usability in Claude Desktop. |
| **listChanged notifications** | Notify connected clients when tools/resources/prompts change (e.g., a new WASM skill is installed, memories are updated). Spec-defined but optional; implementing it makes the server feel alive. | LOW | Skill install events, memory write events | When `blufio skill install` adds a new skill, emit `notifications/tools/list_changed`. When memory is created/updated, emit `notifications/resources/list_changed`. Clients re-fetch the lists. |
| **Resource subscriptions for memory** | Clients can subscribe to specific memory resources and get notified when they change (superseded, forgotten, new related memories). | MEDIUM | MemoryStore write hooks | Subscribe to `blufio://memory/{id}` -- get notified if that memory is superseded or forgotten. Subscribe to `blufio://memory/search?q=project` -- get notified when new memories match the query. Rich integration. |
| **Structured tool output (outputSchema)** | Define JSON schemas for tool outputs, not just inputs. Lets clients parse and validate structured responses. New in 2025-11-25 spec. | LOW | ToolOutput enhancement | Add optional `output_schema()` to Tool trait. Tools like "search-memory" return structured JSON (array of memories with scores). Clients can render this as tables, not raw text. |
| **Progress reporting for long tools** | WASM skills and HTTP tools can take seconds. Report progress via `notifications/progress` so clients show progress bars instead of "thinking...". | LOW | Existing epoch-based timeouts in WASM sandbox | Emit progress notifications during WASM skill execution (fuel consumed / fuel limit) and HTTP requests (connected, waiting, received). Better UX in Claude Desktop. |
| **`blufio mcp-server` CLI subcommand** | Dedicated CLI entry point for running Blufio as an MCP server. Claude Desktop config points to: `"command": "blufio", "args": ["mcp-server"]`. Clean, discoverable, follows MCP conventions. | LOW | CLI (clap) | New subcommand that starts the stdio MCP server loop. Does not start Telegram, gateway, or other services -- just the MCP server. Minimal startup for fast connection. |

### MCP Client: Differentiators

| Feature | Value Proposition | Complexity | Blufio Dependency | Notes |
|---------|-------------------|------------|-------------------|-------|
| **Namespace-prefixed tool names** | External MCP tools are prefixed with their server name: `mcp.github.create_issue`, `mcp.filesystem.read_file`. Prevents name collisions between multiple MCP servers and Blufio's built-in tools. | LOW | ToolRegistry | Convention: `mcp.{server_name}.{tool_name}`. The LLM sees clear provenance. Agent loop routes by prefix. |
| **Per-server budget tracking** | Track token cost and invocation count for each external MCP server. Operators can set per-server limits: "github MCP server max 100 calls/day". | MEDIUM | Cost ledger | Extend cost_events table with mcp_server column. Report in `blufio status`. Kill switch per MCP server when budget exceeded. |
| **MCP server health in `blufio doctor`** | `blufio doctor` checks connectivity to all configured MCP servers: can connect, initialize, list tools. Reports degraded/unhealthy servers. | LOW | Existing doctor command | Add MCP server health checks alongside existing checks (LLM, DB, channel). |
| **Dynamic MCP server discovery** | Hot-reload MCP server configuration without restarting Blufio. Edit TOML, server picks up changes. New MCP servers become available, removed ones disconnect. | MEDIUM | Config watch (already supported in blufio-config) | Watch `mcp.servers` section for changes. Connect to new servers, disconnect removed ones. Emit tool list changes to agent. |
| **Sampling capability (server-initiated LLM calls)** | When an external MCP server requests sampling (`sampling/createMessage`), Blufio routes it through its own LLM provider (with model routing). The MCP server gets LLM access without needing its own API keys. | HIGH | Provider trait, model routing | Declare `sampling` capability during client initialization. Handle `sampling/createMessage` by routing through Blufio's provider. Apply budget caps, model preferences, and rate limits. This turns Blufio into an LLM gateway for MCP servers. |
| **Roots capability** | Declare filesystem roots that MCP servers can operate within. Scopes MCP server access to specific directories. | LOW | Security model | Declare `roots` capability. Return configured roots (e.g., `file:///home/user/projects`). Notify when roots change. Security boundary for file-accessing MCP servers. |

---

## Anti-Features

Features that seem logical for MCP integration but should be explicitly avoided.

| Anti-Feature | Why It Seems Reasonable | Why Problematic | What to Do Instead |
|--------------|------------------------|-----------------|-------------------|
| **Implement MCP from scratch (no SDK)** | Full control, no dependency, matches Blufio's "minimal deps" philosophy. | JSON-RPC 2.0 + MCP lifecycle + transport negotiation + capability negotiation is ~2,000-4,000 lines of protocol code. The spec has edge cases (pagination cursors, cancellation tokens, task states). rmcp 0.17 is the official Rust SDK, actively maintained, tokio-native, and handles all of this. Reinventing it wastes time and introduces spec-compliance bugs. | Use rmcp crate with feature flags. It is the official Rust MCP SDK, uses tokio + serde (already in workspace), and has `ServerHandler`/`ClientHandler` traits. Feature-flag transports to control binary size. |
| **Expose every internal as an MCP tool** | "More tools = more useful." Expose session management, config editing, plugin management, vault operations, etc. | Security disaster. MCP tools are invoked by LLMs. An LLM calling `vault.decrypt_secret` or `config.set_bind_address("0.0.0.0")` is a privilege escalation vector. Tool annotations are hints, not enforcement. | Expose a curated set: built-in tools (bash, HTTP, file), WASM skills, memory search, session read. Never expose vault, config mutation, or security-sensitive operations as MCP tools. |
| **MCP server as the primary agent interface** | "Replace Telegram with MCP. Claude Desktop becomes the interface." | MCP is a tool/resource protocol, not a chat protocol. It has no concept of streaming responses, typing indicators, conversation sessions, or message formatting. The agent loop needs a Channel adapter, not an MCP connection. | MCP server is an auxiliary interface: it exposes tools and data. Telegram (and future channels) remain the primary conversational interfaces. MCP and channels serve different purposes. |
| **Implement Tasks (async tool execution)** | Spec 2025-11-25 adds Tasks for long-running operations. Seems forward-thinking. | Tasks are marked EXPERIMENTAL in the spec. The rmcp crate has partial support. Client adoption is minimal (Claude Desktop does not support tasks yet as of early 2026). Building against an experimental feature risks rework when the spec changes. | Defer Tasks to v1.2+. Synchronous tool execution is sufficient for v1.1. WASM skills have 5-second epoch timeouts which keep tools responsive. If a tool is too slow, it should be redesigned, not made async. |
| **Implement Elicitation** | Spec 2025-11-25 adds Elicitation for servers to request user input. Could be useful for WASM skills that need parameters. | Elicitation requires the client to present UI for user input. Blufio's MCP client connects to external servers -- it has no UI (Telegram is the UI). Implementing elicitation in the client means proxying input requests through Telegram, which is complex and confusing UX. | Defer Elicitation. External MCP servers that need user input should use tool parameters instead. For Blufio-as-server, the connected client (Claude Desktop) handles elicitation natively. |
| **MCP Bundles (.mcpb) distribution** | Package Blufio's MCP server as an .mcpb bundle for easy distribution. | Bundles are a distribution format for MCP servers, not a runtime feature. Blufio already ships as a single binary. An .mcpb bundle would just contain the binary + a manifest.json pointing to `blufio mcp-server`. Adds packaging complexity for minimal value. | Document the Claude Desktop JSON config needed to connect to Blufio. Ship an example config file. The single binary IS the distribution. |
| **OAuth 2.1 authorization for MCP server** | Spec 2025-11-25 adds OAuth with CIMD for remote MCP servers. Enterprise feature. | Blufio's MCP server will primarily run locally (stdio) or on the same machine (localhost HTTP). OAuth adds significant complexity (authorization server, token management, scope negotiation). Not needed when the server is local. | Use Blufio's existing device keypair authentication for HTTP transport. For stdio transport, the process is already authenticated by the OS (parent process launched it). Add OAuth as a v1.2+ feature if remote MCP server access becomes a requirement. |
| **MCP Apps Extension** | Interactive UI within MCP. Could show dashboards, configuration panels. | Experimental. No major client supports it. Adds a web UI dependency which contradicts Blufio's CLI-first philosophy. | Skip entirely. CLI + TOML config is the interface. |

---

## Feature Dependencies

```
[MCP Server - Core]
    |-- requires --> Tool trait + ToolRegistry (existing)
    |-- requires --> JSON-RPC 2.0 layer (rmcp crate)
    |-- requires --> stdio transport (rmcp + tokio stdin/stdout)
    |-- requires --> Streamable HTTP transport (rmcp + axum)
    |-- requires --> SSE transport (rmcp + axum)
    |-- requires --> CLI subcommand `blufio mcp-server` (clap)
    |-- requires --> Capability negotiation (rmcp ServerHandler)

[MCP Server - Resources]
    |-- requires --> MCP Server Core
    |-- requires --> MemoryStore + HybridRetriever (existing)
    |-- requires --> StorageAdapter for sessions (existing)

[MCP Server - Prompts]
    |-- requires --> MCP Server Core
    |-- requires --> Context engine (existing)
    |-- requires --> Memory retriever (existing)

[MCP Server - Notifications]
    |-- requires --> MCP Server Core
    |-- requires --> Skill install events (existing store.rs)
    |-- requires --> Memory write events (existing store.rs)

[MCP Client - Core]
    |-- requires --> JSON-RPC 2.0 layer (rmcp crate)
    |-- requires --> stdio transport client (rmcp + tokio process)
    |-- requires --> Streamable HTTP transport client (rmcp + reqwest)
    |-- requires --> TOML config for MCP servers (blufio-config)
    |-- requires --> Connection lifecycle manager (new code)

[MCP Client - Agent Integration]
    |-- requires --> MCP Client Core
    |-- requires --> ToolRegistry (existing, extend with MCP tools)
    |-- requires --> Agent loop tool routing (existing session.rs)
    |-- requires --> Cost ledger (existing, extend with MCP server tracking)

[MCP Client - Sampling]
    |-- requires --> MCP Client Core
    |-- requires --> Provider trait (existing)
    |-- requires --> Model routing (existing)
    |-- requires --> Cost ledger (existing)
```

### Dependency Notes

- **MCP Server Core has zero dependencies on MCP Client.** They are independent features. Server can ship without client, client can ship without server.
- **MCP Server maps almost 1:1 to existing Blufio types.** `Tool` trait -> MCP tools. `ToolOutput` -> MCP tool result. `SkillManifest` -> MCP tool metadata. `Memory` -> MCP resource. The mapping layer is thin.
- **MCP Client integrates at the ToolRegistry level.** External MCP tools register like any other tool. The agent loop does not need to know about MCP -- it just sees tools.
- **Sampling capability is the most complex client feature.** It requires routing MCP server requests through Blufio's own LLM provider, with all the budget/routing/security implications. Defer to later in the milestone if needed.

---

## MVP Recommendation for v1.1

### Phase 1: MCP Server (ship first)

Priority: **The done condition says "point Claude Desktop at Blufio via stdio and use skills/memory."**

Build in this order:

1. **`blufio mcp-server` CLI subcommand** -- Entry point. Starts stdio MCP server.
2. **Capability negotiation** -- Initialize handshake. Declare tools + resources capabilities.
3. **tools/list mapping** -- Expose all ToolRegistry entries as MCP tools.
4. **tools/call routing** -- Route MCP tool calls to Blufio's Tool::invoke().
5. **Tool annotations** -- Tag each tool with behavior hints.
6. **resources/list + resources/read for memory** -- Expose memory as readable resources.
7. **Streamable HTTP + SSE transports** -- Add HTTP-based transports on axum gateway.
8. **prompts/list + prompts/get** -- Expose workflow prompts (summarize-session, search-memory).
9. **listChanged notifications** -- Notify on skill install, memory changes.

### Phase 2: MCP Client (ship second)

Priority: **The done condition says "configure external MCP servers in TOML, agent uses external MCP tools in conversation."**

Build in this order:

1. **TOML config for MCP servers** -- `[[mcp.servers]]` configuration section.
2. **MCP client connection manager** -- Connect, initialize, discover tools.
3. **stdio transport client** -- Launch MCP servers as subprocesses.
4. **Tool discovery + ToolRegistry integration** -- Register external tools with namespace prefix.
5. **Agent loop MCP tool routing** -- Route `mcp.*` tool calls to external servers.
6. **Streamable HTTP transport client** -- Connect to remote MCP servers.
7. **Connection lifecycle (retry, health)** -- Resilient connections, doctor integration.
8. **Per-server budget tracking** -- Cost attribution per MCP server.

### Defer to v1.2+

- Tasks (experimental spec feature)
- Elicitation (requires UI proxy, complex)
- Sampling capability (complex, requires LLM gateway pattern)
- OAuth 2.1 (only needed for remote servers)
- MCP Apps Extension (experimental)
- MCP Bundles distribution (single binary is sufficient)
- Resource subscriptions for memory (nice-to-have, not needed for done condition)

---

## Feature Sizing Estimates

| Feature | Estimated LOC | Risk | Notes |
|---------|--------------|------|-------|
| MCP Server Core (stdio + negotiate + tools) | 800-1200 | LOW | rmcp handles protocol. Thin mapping layer to existing Tool trait. |
| MCP Server Resources (memory + sessions) | 400-600 | LOW | Read-only wrappers around existing stores. |
| MCP Server Prompts | 200-400 | LOW | Template construction from existing data. |
| MCP Server HTTP transports (Streamable + SSE) | 300-500 | MEDIUM | axum integration. Must coexist with existing gateway routes. |
| MCP Server tool annotations | 100-200 | LOW | Metadata addition to existing tools. |
| MCP Server notifications | 200-300 | MEDIUM | Event plumbing from skill/memory stores to MCP transport. |
| MCP Client Core (connect + discover) | 600-900 | MEDIUM | Connection lifecycle, error handling, reconnection. |
| MCP Client stdio transport | 200-300 | LOW | rmcp handles protocol. Process management. |
| MCP Client HTTP transport | 100-200 | LOW | rmcp + reqwest. |
| MCP Client ToolRegistry integration | 300-500 | LOW | Namespace prefixing, schema forwarding, routing. |
| MCP Client TOML config | 200-300 | LOW | Config struct + validation. |
| MCP Client agent loop integration | 300-500 | MEDIUM | Tool routing in session.rs, handling MCP-specific result formats. |
| **Total estimated** | **3,400-5,900** | | Approximately 15-20% of v1.0 codebase size. |

---

## Sources

### MCP Specification (PRIMARY -- HIGH confidence)
- [MCP Spec 2025-11-25 (latest)](https://modelcontextprotocol.io/specification/2025-11-25) -- Authoritative protocol specification
- [MCP Tools Spec](https://modelcontextprotocol.io/specification/2025-11-25/server/tools) -- Tool listing, calling, annotations, structured output
- [MCP Resources Spec](https://modelcontextprotocol.io/specification/2025-11-25/server/resources) -- Resource URIs, templates, subscriptions, reading
- [MCP Prompts Spec](https://modelcontextprotocol.io/specification/2025-11-25/server/prompts) -- Prompt listing, arguments, messages
- [MCP Sampling Spec](https://modelcontextprotocol.io/specification/2025-11-25/client/sampling) -- Server-initiated LLM calls, tool use in sampling
- [MCP Roots Spec](https://modelcontextprotocol.io/specification/2025-11-25/client/roots) -- Filesystem boundaries
- [MCP Transports Spec](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports) -- stdio, Streamable HTTP, SSE

### Rust SDK (HIGH confidence)
- [Official rmcp crate (0.17)](https://docs.rs/crate/rmcp/latest) -- ServerHandler, ClientHandler, transport support, tool macros
- [modelcontextprotocol/rust-sdk GitHub](https://github.com/modelcontextprotocol/rust-sdk) -- Official Rust SDK, 3.1k stars, 144 contributors

### MCP Ecosystem (MEDIUM confidence)
- [MCP Features Guide (WorkOS)](https://workos.com/blog/mcp-features-guide) -- Comprehensive feature overview: tools, resources, prompts, sampling, roots, elicitation
- [MCP 2025-11-25 Spec Update (WorkOS)](https://workos.com/blog/mcp-2025-11-25-spec-update) -- Tasks, OAuth improvements, sampling with tools, bundles
- [MCP November 2025 Spec Deep Dive (Medium)](https://medium.com/@dave-patten/mcps-next-phase-inside-the-november-2025-specification-49f298502b03) -- Async tasks, enterprise features
- [MCP Tool Annotations Guide](https://blog.marcnuri.com/mcp-tool-annotations-introduction) -- readOnlyHint, destructiveHint, idempotentHint, openWorldHint
- [MCP Memory Service (GitHub)](https://github.com/doobidoo/mcp-memory-service) -- Open-source persistent memory MCP server example
- [MCP Permissions (Cerbos)](https://www.cerbos.dev/blog/mcp-permissions-securing-ai-agent-access-to-tools) -- Security best practices for MCP tool access
- [How to Build MCP Server in Rust (Shuttle)](https://www.shuttle.dev/blog/2025/07/18/how-to-build-a-stdio-mcp-server-in-rust) -- Practical Rust MCP server guide

### Blufio Codebase (verified by reading source)
- `crates/blufio-skill/src/tool.rs` -- Tool trait, ToolRegistry, ToolOutput (maps to MCP tools)
- `crates/blufio-core/src/types.rs` -- SkillManifest, SkillInvocation, SkillResult, ContentBlock
- `crates/blufio-core/src/traits/skill.rs` -- SkillRuntimeAdapter trait
- `crates/blufio-memory/src/types.rs` -- Memory, ScoredMemory, MemorySource, MemoryStatus
- `crates/blufio-memory/src/retriever.rs` -- HybridRetriever (maps to MCP resource read)

---
*Feature research for: MCP Integration (Server + Client) -- Blufio v1.1*
*Researched: 2026-03-02*
