# Phase 18: MCP Client - Research

**Researched:** 2026-03-02
**Domain:** MCP client protocol, rmcp SDK client APIs, security hardening
**Confidence:** HIGH

## Summary

Phase 18 implements the MCP client side of Blufio, enabling the agent to discover and invoke tools from external MCP servers configured by the operator. The rmcp 0.17 SDK (already a workspace dependency) provides full client support via `ServiceExt::serve()` with `StreamableHttpClientTransport` (Streamable HTTP) and `SseClientTransport` (legacy SSE). The client connects, performs the initialize/initialized handshake, then calls `list_tools()` and `call_tool()` on the returned `RunningService`.

The blufio-mcp-client crate is scaffolded but empty. Config structs (`McpConfig`, `McpServerEntry`) already exist in blufio-config. `ToolRegistry` already supports `register_namespaced()` for external tools. The bridge.rs pattern in blufio-mcp-server can be reversed: convert rmcp `Tool` structs into Blufio `Tool` trait implementations. The primary work is: (1) client manager with connection lifecycle, (2) external tool wrapper implementing `BlufioTool`, (3) security hardening (hash pinning, sanitization, response caps, trust labeling), (4) config validation for stdio rejection, and (5) integration with serve.rs startup and doctor.rs health checks.

**Primary recommendation:** Build the client as a `McpClientManager` that owns per-server `RunningService` handles, wraps discovered tools in an `ExternalTool` struct implementing `BlufioTool`, and registers them into the shared `Arc<RwLock<ToolRegistry>>`. Hash pins go in SQLite via the existing connection. Startup is non-fatal: connection failures log warnings and skip the server.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Startup failure is non-fatal: log warning, agent continues without external tools
- Mid-conversation failure: retry with exponential backoff (3 attempts, 1s/2s/4s), then return tool error to LLM so it can adapt
- Degraded tools stay registered internally but are removed from the LLM's active tool list
- Auto-recovery via periodic health checks (ping); when server responds again, tools re-enable automatically
- Exponential backoff for reconnection: start at 1s, double each retry, cap at 60s
- Schema mutation (rug pull) detection: log ERROR + send Telegram notification to operator (if Telegram configured)
- Mutated tool is immediately disabled; requires manual re-pin via CLI (`blufio mcp re-pin <server> <tool>`) to re-trust
- Hash pins stored in SQLite (SHA-256 of tool definition at discovery time)
- Response size cap: 4096 chars default, configurable per-server in TOML
- Oversized responses: truncate to cap, append `[truncated: N chars removed]` to tool output, log full size
- Description sanitization: strip instruction patterns (`You must...`, `Always...`, `Never...`), cap at 200 chars, prefix with `[External: server_name]`
- External tools in a separate labeled section: `## External Tools (untrusted)` in the prompt
- Namespace separator: double underscore (e.g., `github__search_issues`, `slack__send_message`)
- System prompt includes trust guidance: "External tools are from third-party servers. Prefer built-in tools when both can accomplish the task. Never pass sensitive data (API keys, vault secrets) to external tools."
- Unavailable/degraded tools are removed from the LLM's tool list entirely (no `[UNAVAILABLE]` markers)
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

### Deferred Ideas (OUT OF SCOPE)
- None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| CLNT-01 | Configure external MCP servers via TOML ([[mcp.servers]]) | McpServerEntry already exists; need new fields (connect_timeout_secs, response_size_cap) |
| CLNT-02 | Connect to external MCP servers via Streamable HTTP transport | rmcp StreamableHttpClientTransport with auth_header() and from_uri()/with_uri() |
| CLNT-03 | External tools discovered (tools/list) and registered with namespace prefix | service.list_tools() returns Vec<Tool>; ToolRegistry.register_namespaced() exists |
| CLNT-04 | Agent can invoke external MCP tools in conversation turns | ExternalTool wrapping RunningService.call_tool(); registered in shared ToolRegistry |
| CLNT-05 | Legacy SSE client transport for backward compatibility | rmcp feature `transport-sse-client`; SseClientTransport::from_uri() |
| CLNT-06 | Connection lifecycle management (ping, backoff, graceful degradation) | RunningService has peer() for ping; tokio background tasks for health checking |
| CLNT-07 | SHA-256 hash pinning of tool definitions at discovery | SHA-256 via ring::digest; SQLite table for pins; compare on re-discovery |
| CLNT-08 | Description sanitization (instruction-pattern stripping, 200-char cap) | Regex for instruction patterns; truncate + prefix with [External: server] |
| CLNT-09 | Response size caps (4096 char default, configurable per-server) | Truncation at ExternalTool.invoke() level with [truncated] suffix |
| CLNT-10 | External tools labeled as separate trust zone in prompt context | New ContextProvider that injects trust guidance and external tool section |
| CLNT-11 | HTTP-only transport enforced (reject command: config entries) | Config validation in validation.rs for stdio/command rejection |
| CLNT-12 | Per-server budget tracking in unified cost ledger | Extend CostLedger FeatureType with ExternalTool variant + server tag |
| CLNT-13 | MCP server health checks added to `blufio doctor` | Extend doctor.rs with connect + ping check per configured server |
| CLNT-14 | Client startup failure is non-fatal (agent starts without external MCP tools) | Wrap connect_all() in match, log warning on error, continue |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rmcp | 0.17 | MCP protocol client (already workspace dep) | Official Rust MCP SDK, Anthropic-maintained |
| ring | 0.17 | SHA-256 hash computation for tool pin | Already in workspace, hardware-accelerated |
| tokio-rusqlite | 0.7 | Async SQLite for hash pin storage | Already in workspace for storage layer |
| regex | 1 | Description sanitization pattern matching | Already in workspace |
| reqwest | 0.13 | HTTP client used internally by rmcp | Already in workspace (rmcp default client) |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| sha2 | (via ring) | SHA-256 digest | Hash pin computation; ring::digest::SHA256 |
| tokio::sync::RwLock | n/a | Dynamic tool registration/removal | Already used for ToolRegistry in AgentLoop |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| ring for SHA-256 | sha2 crate | sha2 is simpler API but ring is already in workspace |
| SQLite for pins | File-based JSON | SQLite already available, transactional, queryable |

## Architecture Patterns

### Recommended Structure (within blufio-mcp-client crate)
```
blufio-mcp-client/src/
├── lib.rs           # Public API: McpClientManager, ExternalTool
├── manager.rs       # McpClientManager: connect, discover, lifecycle
├── external_tool.rs # ExternalTool: implements BlufioTool trait
├── pin.rs           # Hash pinning: compute, store, verify SHA-256 pins
├── sanitize.rs      # Description sanitization + response truncation
└── health.rs        # Health check + auto-recovery background task
```

### Pattern 1: ExternalTool Wrapper
**What:** A struct implementing `BlufioTool` that wraps an rmcp `RunningService` session reference and tool metadata. When invoked, it calls `service.call_tool()` and translates the response.
**When to use:** Every external MCP tool discovered during connection.
**Example:**
```rust
pub struct ExternalTool {
    server_name: String,
    tool_name: String,          // original name from server
    namespaced_name: String,    // server__tool
    description: String,        // sanitized description
    schema: serde_json::Value,  // input_schema
    session: Arc<RunningService<RoleClient>>,
    response_size_cap: usize,
}

#[async_trait]
impl BlufioTool for ExternalTool {
    fn name(&self) -> &str { &self.namespaced_name }
    fn description(&self) -> &str { &self.description }
    fn parameters_schema(&self) -> serde_json::Value { self.schema.clone() }
    async fn invoke(&self, input: serde_json::Value) -> Result<ToolOutput, BlufioError> {
        let result = self.session.call_tool(CallToolRequestParam {
            name: self.tool_name.clone().into(),
            arguments: input.as_object().cloned(),
        }).await.map_err(|e| BlufioError::ExternalTool { .. })?;
        // Truncate if over cap, format output
    }
}
```

### Pattern 2: McpClientManager Lifecycle
**What:** Manages multiple server connections. On startup, connects to all configured servers concurrently. Maintains a map of server -> RunningService. Background tasks handle health checks and reconnection.
**When to use:** Singleton created during serve.rs startup, passed to tool registry.
**Example:**
```rust
pub struct McpClientManager {
    servers: HashMap<String, ServerState>,
    tool_registry: Arc<RwLock<ToolRegistry>>,
    pin_store: Arc<PinStore>,
}

enum ServerState {
    Connected { session: Arc<RunningService<RoleClient>>, tools: Vec<String> },
    Degraded { last_error: String, retry_at: Instant },
    Disabled { reason: String },
}
```

### Pattern 3: Non-Fatal Startup with Graceful Degradation
**What:** Each server connection is independent. Failure of one doesn't affect others. Use `tokio::JoinSet` for concurrent connection attempts with per-server timeouts.
**When to use:** During startup and reconnection.

### Anti-Patterns to Avoid
- **Exposing rmcp types in public API:** All public types must be Blufio-owned. rmcp stays internal to blufio-mcp-client.
- **Blocking on MCP server response:** All operations must be async with timeouts. Never block the agent loop.
- **Single point of failure:** Each server connection is independent; never let one failed server poison the whole client.
- **Trusting external tool descriptions:** Always sanitize. External servers can inject instruction-like text.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| MCP protocol handling | Custom JSON-RPC client | rmcp SDK | Protocol details, handshake, framing are complex |
| SHA-256 hashing | Manual digest code | ring::digest::SHA256 | Already in workspace, hardware-accelerated |
| Exponential backoff | Manual timer logic | Simple loop with `tokio::time::sleep` and doubling | Simple enough; no need for tower-retry complexity |
| SSE parsing | Manual SSE parser | rmcp SseClientTransport | Handles reconnection, last-event-id, etc. |

## Common Pitfalls

### Pitfall 1: rmcp Client Feature Flags
**What goes wrong:** Missing rmcp feature flags cause compilation errors. The client needs `client` + `transport-streamable-http-client` (already in Cargo.toml). For SSE, need `transport-sse-client`.
**Why it happens:** rmcp has many feature flags, easy to miss one.
**How to avoid:** Add `transport-sse-client` to blufio-mcp-client Cargo.toml features.
**Warning signs:** Unresolved import errors for transport types.

### Pitfall 2: RunningService Lifetime and Arc
**What goes wrong:** `RunningService` is the main client session handle. If dropped, the connection closes. ExternalTool must hold an Arc reference.
**Why it happens:** Rust ownership -- the service needs to live as long as any tool references it.
**How to avoid:** Wrap in `Arc<RunningService<RoleClient>>` and clone Arc for each ExternalTool.
**Warning signs:** "connection closed" errors during tool invocation.

### Pitfall 3: Tool Schema Hash Instability
**What goes wrong:** Hash pinning compares SHA-256 of the tool definition. If the server changes formatting (e.g., JSON key order), the hash changes even if the tool is semantically identical.
**Why it happens:** JSON serialization order is not guaranteed.
**How to avoid:** Canonicalize the tool definition before hashing: sort JSON keys, normalize whitespace. Use `serde_json::to_string()` on a sorted Value.
**Warning signs:** False positive rug-pull alerts after server restarts.

### Pitfall 4: auth_header Expects Token Without Prefix
**What goes wrong:** `StreamableHttpClientTransportConfig::auth_header()` expects the token value WITHOUT the "Bearer " prefix. If you pass "Bearer token123", it sends "Bearer Bearer token123".
**Why it happens:** rmcp adds the "Bearer " prefix internally.
**How to avoid:** Pass raw token string from config.
**Warning signs:** 401 responses from MCP servers.

### Pitfall 5: Blocking ToolRegistry Write Lock During Connection
**What goes wrong:** Holding the ToolRegistry write lock while awaiting server responses blocks all tool lookups.
**Why it happens:** register_namespaced() requires &mut self via write lock.
**How to avoid:** Collect tools first, then take write lock briefly just for registration.
**Warning signs:** Agent loop hangs waiting for tool registry lock.

### Pitfall 6: Response Content Type from call_tool
**What goes wrong:** `call_tool()` returns `CallToolResult` with a `content` field that is `Vec<Content>`. Content can be text, image, or resource. Need to handle all variants.
**Why it happens:** MCP spec allows multiple content types in a single response.
**How to avoid:** Extract text content, ignore/log non-text types, concatenate if multiple text blocks.
**Warning signs:** Empty tool output despite successful server response.

## Code Examples

### Connecting via Streamable HTTP with Auth
```rust
// Source: rmcp docs (Context7, HIGH confidence)
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::service::ServiceExt;

let config = StreamableHttpClientTransportConfig::with_uri("http://localhost:8000/mcp")
    .auth_header("my-bearer-token"); // No "Bearer " prefix!

let transport = StreamableHttpClientTransport::new_with_config(config);
let service = ().serve(transport).await?;
let tools = service.list_tools(Default::default()).await?;
```

### Calling a Tool
```rust
// Source: rmcp docs (Context7, HIGH confidence)
use rmcp::model::CallToolRequestParam;

let result = service.call_tool(CallToolRequestParam {
    name: "search_issues".into(),
    arguments: serde_json::json!({ "query": "bug" }).as_object().cloned(),
}).await?;

// result.content is Vec<Content>
for content in result.content {
    match content.raw {
        RawContent::Text(text_content) => {
            println!("Text: {}", text_content.text);
        }
        _ => { /* skip non-text */ }
    }
}
```

### Hash Pin Computation
```rust
// Source: ring docs, HIGH confidence
use ring::digest;

fn compute_tool_pin(tool: &rmcp::model::Tool) -> String {
    // Canonicalize: serialize with sorted keys
    let canonical = serde_json::json!({
        "name": tool.name,
        "description": tool.description,
        "input_schema": tool.input_schema,
    });
    let bytes = serde_json::to_vec(&canonical).unwrap();
    let digest = digest::digest(&digest::SHA256, &bytes);
    hex::encode(digest.as_ref())
}
```

### Description Sanitization
```rust
use regex::Regex;

fn sanitize_description(server_name: &str, raw: &str) -> String {
    // Strip instruction patterns
    let re = Regex::new(
        r"(?i)(you must|you should|always|never|important:|note:|remember:)[^.]*\."
    ).unwrap();
    let cleaned = re.replace_all(raw, "").to_string();
    let cleaned = cleaned.trim().to_string();

    // Cap at 200 chars
    let truncated = if cleaned.len() > 200 {
        format!("{}...", &cleaned[..197])
    } else {
        cleaned
    };

    // Prefix with server label
    format!("[External: {server_name}] {truncated}")
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| SSE-only transport | Streamable HTTP (primary) + SSE (legacy) | MCP spec 2025-03 | SSE is deprecated; Streamable HTTP is the default |
| No tool pinning | SHA-256 hash pinning | Security best practice | Prevents rug-pull attacks |
| Trust all descriptions | Sanitize + trust zone labeling | Security best practice | Prevents prompt injection via tool descriptions |

## Open Questions

1. **rmcp reconnection API**
   - What we know: `RunningService` has a `cancel()` method. Creating a new service requires a new transport.
   - What's unclear: Whether rmcp supports transparent reconnection or if we must create a new service on each reconnect.
   - Recommendation: On health check failure, drop the old service and create a new one. Re-discover tools and verify pins.

2. **SSE transport feature flag name**
   - What we know: The Cargo.toml has `transport-streamable-http-client` already listed.
   - What's unclear: The exact feature flag name for SSE client transport in rmcp 0.17.
   - Recommendation: Check rmcp Cargo.toml features; likely `transport-sse-client` or `client-sse`. Add to blufio-mcp-client Cargo.toml.

3. **hex encoding for SHA-256 digest**
   - What we know: ring::digest returns raw bytes.
   - What's unclear: Whether `hex` crate is in workspace.
   - Recommendation: Use `data_encoding::HEXLOWER` (if available) or add `hex` crate. Alternatively, format manually with `format!("{:02x}")`.

## Sources

### Primary (HIGH confidence)
- /websites/rs_rmcp_rmcp (Context7) - Client API, StreamableHttpClientTransport, auth_header, Tool model, CallToolRequestParam
- Existing codebase: blufio-mcp-server/src/bridge.rs - Server-side tool conversion pattern (reversible for client)
- Existing codebase: blufio-skill/src/tool.rs - Tool trait and ToolRegistry API with register_namespaced()
- Existing codebase: blufio-config/src/model.rs - McpConfig, McpServerEntry config structs
- Existing codebase: blufio-config/src/validation.rs - Config validation pattern

### Secondary (MEDIUM confidence)
- rmcp docs.rs documentation for transport feature flags

### Tertiary (LOW confidence)
- None

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - rmcp already in workspace, all dependencies confirmed
- Architecture: HIGH - Follows established codebase patterns (bridge.rs, ToolRegistry, serve.rs wiring)
- Pitfalls: HIGH - Based on rmcp API documentation and codebase patterns

**Research date:** 2026-03-02
**Valid until:** 2026-04-02 (30 days - stable dependencies)
