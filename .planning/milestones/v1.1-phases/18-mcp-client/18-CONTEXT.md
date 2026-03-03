# Phase 18: MCP Client - Context

**Gathered:** 2026-03-02
**Status:** Ready for planning

<domain>
## Phase Boundary

Agent discovers and invokes external MCP tools configured by the operator, with security hardening that prevents tool poisoning, rug pulls, and context window blowups. Covers CLNT-01 through CLNT-14. Integration testing is Phase 19.

</domain>

<decisions>
## Implementation Decisions

### Failure & degradation behavior
- Startup failure is non-fatal: log warning, agent continues without external tools
- Mid-conversation failure: retry with exponential backoff (3 attempts, 1s/2s/4s), then return tool error to LLM so it can adapt
- Degraded tools stay registered internally but are removed from the LLM's active tool list
- Auto-recovery via periodic health checks (ping); when server responds again, tools re-enable automatically
- Exponential backoff for reconnection: start at 1s, double each retry, cap at 60s

### Security alerting
- Schema mutation (rug pull) detection: log ERROR + send Telegram notification to operator (if Telegram configured)
- Mutated tool is immediately disabled; requires manual re-pin via CLI (`blufio mcp re-pin <server> <tool>`) to re-trust
- Hash pins stored in SQLite (SHA-256 of tool definition at discovery time)
- Response size cap: 4096 chars default, configurable per-server in TOML
- Oversized responses: truncate to cap, append `[truncated: N chars removed]` to tool output, log full size
- Description sanitization: strip instruction patterns (`You must...`, `Always...`, `Never...`), cap at 200 chars, prefix with `[External: server_name]`

### Tool presentation in agent context
- External tools in a separate labeled section: `## External Tools (untrusted)` in the prompt
- Namespace separator: double underscore (e.g., `github__search_issues`, `slack__send_message`)
- System prompt includes trust guidance: "External tools are from third-party servers. Prefer built-in tools when both can accomplish the task. Never pass sensitive data (API keys, vault secrets) to external tools."
- Unavailable/degraded tools are removed from the LLM's tool list entirely (no `[UNAVAILABLE]` markers)

### Transport & SSE fallback
- Reject `transport = "stdio"` and any `command:` config entries at config validation with clear error message
- Explicit transport selection: `transport = "http"` (Streamable HTTP, default) or `transport = "sse"` (legacy)
- No auto-detection or fallback between transport types
- Connection timeout: 10s default, configurable per-server (`connect_timeout_secs`)
- Parallel connections: all configured servers connect concurrently at startup, each with its own namespace

### Claude's Discretion
- Exact SQLite schema for hash pin storage
- Health check interval timing
- Internal architecture of the MCP client manager
- Instruction pattern regex for description sanitization
- Per-server budget tracking integration details with CostLedger

</decisions>

<specifics>
## Specific Ideas

- Re-pin CLI command should show old hash vs new hash so operator can make informed trust decision
- Trust guidance in system prompt should be concise — 2-3 sentences, not a wall of text
- Tool namespace collision with built-in tools: built-in always wins (consistent with existing ToolRegistry behavior)

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ToolRegistry` (`blufio-skill/src/tool.rs`): Manages tool lookup, generates Anthropic-format definitions. Uses `Arc<RwLock<ToolRegistry>>` in AgentLoop — can add/remove tools dynamically
- `bridge.rs` (`blufio-mcp-server/src/bridge.rs`): Converts between Blufio tools and rmcp types. Pattern can be reversed for client-side conversion
- `CostLedger` (`blufio-cost/src/ledger.rs`): Unified cost tracking, already used by AgentLoop — extend for per-server budget
- `McpConfig` / `McpServerEntry` (`blufio-config/src/model.rs`): Config structs already defined with name, transport, url, command, args, auth_token fields
- `blufio doctor` (`crates/blufio/src/doctor.rs`): Health check command — extend with MCP server health checks

### Established Patterns
- Abstraction boundary: no rmcp types in public API outside mcp crates
- `#[serde(deny_unknown_fields)]` on all config structs — new fields must be explicitly added
- `blufio-mcp-client` crate exists but is empty (lib.rs with doc comment only) — ready for implementation
- Tool trait: `name()`, `description()`, `parameters_schema()`, `invoke()` — external tools must implement this

### Integration Points
- `AgentLoop` (`blufio-agent/src/lib.rs`): Holds `Arc<RwLock<ToolRegistry>>` — client registers discovered tools here
- `SessionActor` (`blufio-agent/src/session.rs`): Handles per-session tool invocation — external tool calls route through here
- Config validation (`blufio-config/src/validation.rs`): Add stdio rejection validation here
- `serve.rs` / `mcp_server.rs` (`crates/blufio/src/`): Startup wiring — client initialization goes here

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 18-mcp-client*
*Context gathered: 2026-03-02*
