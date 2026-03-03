# Phase 17: MCP Server HTTP + Resources - Research

**Researched:** 2026-03-02
**Domain:** MCP Streamable HTTP transport, resources, prompts, notifications, CORS
**Confidence:** HIGH

## Summary

Phase 17 extends the existing MCP server (stdio-only from Phase 16) with HTTP transport, resource browsing, prompt templates, tool annotations, and notifications. The rmcp 0.17 crate provides `StreamableHttpService` as a Tower service that can be composed with axum's Router. The existing `BlufioMcpHandler` needs its `ServerHandler` impl extended with `list_resources`, `read_resource`, `list_resource_templates`, `list_prompts`, and `get_prompt` methods. Memory and session data are already accessible via `MemoryStore` and `StorageAdapter`.

**Primary recommendation:** Mount rmcp's `StreamableHttpService` as a nested axum service at `/mcp`, add MCP-specific auth middleware, and extend `BlufioMcpHandler` to implement resource/prompt/notification capabilities. McpConfig needs `auth_token` and `cors_origins` fields.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Mount /mcp as a route on the existing axum gateway (same port, same process)
- No separate port -- MCP and REST/WS coexist on the gateway
- Full Streamable HTTP with SSE for server->client notifications
- Use rmcp's transport-streamable-http-server feature for the /mcp endpoint
- Separate `mcp.auth_token` field in McpConfig -- distinct from gateway's bearer_token
- MCP clients get a different token than gateway API clients (security isolation)
- All /mcp routes require authentication -- no public capability discovery
- Startup validation: refuse to start if `mcp.enabled=true` but `mcp.auth_token` is not set
- `blufio://memory/{id}` returns content + metadata (id, content, source, confidence, status, created_at, updated_at)
- Raw embedding vectors are excluded
- `blufio://memory/search?q={query}&limit={limit}` -- URI template for FTS5 search, default limit 10
- Resource content returned as JSON (application/json MIME type)
- `blufio://sessions` returns list of session summaries (id, channel, started_at)
- `blufio://sessions/{id}` returns message history for a specific session
- All session resources are read-only
- Core set of 3 templates: `summarize-conversation`, `search-memory`, `explain-skill`
- Return full messages array with role/content pairs
- Parameters are required where needed (session_id, query, skill_name)
- Derive annotations from BlufioTool trait -- add annotation methods
- Each tool declares its own hints (readOnlyHint, destructiveHint, idempotentHint, openWorldHint)
- tools/list_changed emitted on skill install or discovery changes (SRVR-13)
- Progress notifications include percentage (0-100) + status message for long-running WASM tools (SRVR-14)
- Explicit origin allowlist in TOML: `mcp.cors_origins = ["https://app.example.com"]`
- Empty list = reject all cross-origin requests (secure by default)
- Restricted CORS only applies to /mcp routes -- existing gateway routes keep current behavior

### Claude's Discretion
- Exact SSE keepalive/heartbeat interval
- Internal structure of the Streamable HTTP handler (how rmcp integrates with axum)
- Error response formatting for MCP protocol errors
- Resource pagination strategy (if memory list is large)
- How to detect skill install/discovery changes for notifications/tools/list_changed

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| SRVR-06 | Streamable HTTP transport mounted on existing gateway at /mcp | rmcp `StreamableHttpService` is a Tower service; mount as axum nested service with `nest_service` or `route_service` |
| SRVR-07 | MCP-specific auth middleware for HTTP transport (bearer token) | New `mcp_auth_middleware` using same pattern as gateway `auth_middleware`, checking `mcp.auth_token` |
| SRVR-08 | Memory exposed as MCP resources (blufio://memory/{id}, search template) | `MemoryStore::get_by_id` and `search_bm25` already exist; implement `list_resources`/`read_resource` on handler |
| SRVR-09 | Session history exposed as read-only MCP resources | `StorageAdapter::list_sessions` and `get_messages` already exist; expose via `list_resources`/`read_resource` |
| SRVR-10 | Prompt templates via prompts/list and prompts/get | Implement `list_prompts`/`get_prompt` on `BlufioMcpHandler` with 3 hardcoded templates |
| SRVR-11 | Tool annotations (readOnlyHint, destructiveHint, idempotentHint, openWorldHint) | Extend `BlufioTool` trait with default annotation methods; map to rmcp `ToolAnnotations` in bridge |
| SRVR-13 | notifications/tools/list_changed on skill install or discovery changes | Use rmcp `RequestContext::peer` to send notifications; track tool count changes |
| SRVR-14 | Progress notifications for long-running WASM tools | Use rmcp progress notification API via `RequestContext`; requires passing context to tool invocation |
| SRVR-16 | CORS restricted to configured origins on MCP HTTP endpoints | tower-http `CorsLayer` with explicit AllowOrigin from config; applied only to /mcp route group |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rmcp | 0.17 | MCP protocol SDK | Official Rust MCP SDK; already in workspace, used in Phase 16 |
| axum | (workspace) | HTTP framework | Already used by blufio-gateway; composable Router for nested services |
| tower-http | (workspace) | HTTP middleware | Already used for CorsLayer in gateway; reuse for MCP CORS |
| serde_json | 1 | JSON serialization | Standard for resource content serialization |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tokio-util | 0.7 | CancellationToken | Already in workspace; for SSE stream lifecycle |
| uuid | (workspace) | Session ID generation | rmcp uses `uuid::Uuid::new_v4()` internally for session IDs |

### rmcp Feature Flags Required
The `transport-streamable-http-server` feature enables `StreamableHttpService` and `StreamableHttpServerConfig`. This must be added to blufio-mcp-server's Cargo.toml:

```toml
rmcp = { workspace = true, features = ["server", "macros", "transport-io", "transport-streamable-http-server"] }
```

This feature transitively requires: `http`, `http-body`, `http-body-util`, `sse-stream`, `bytes`, `pin-project-lite`.

## Architecture Patterns

### Pattern 1: Mounting StreamableHttpService on axum Router

rmcp's `StreamableHttpService` implements Tower's `Service<Request<Body>>` trait. It can be mounted on axum using `nest_service`:

```rust
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};

let mcp_config = StreamableHttpServerConfig {
    sse_keep_alive: Some(Duration::from_secs(30)),
    sse_retry: Some(Duration::from_secs(5)),
    stateful_mode: true,
    cancellation_token: cancel.child_token(),
};

// Create the handler
let handler = BlufioMcpHandler::new(/* ... */);

// Build the StreamableHttpService
let mcp_service = StreamableHttpService::new(handler, mcp_config);

// Mount on axum router
let mcp_routes = Router::new()
    .nest_service("/mcp", mcp_service)
    .layer(mcp_cors_layer)
    .route_layer(middleware::from_fn_with_state(mcp_auth_state, mcp_auth_middleware));
```

**Confidence:** HIGH -- verified from rmcp source code and Context7 docs.

### Pattern 2: BlufioMcpHandler Resource Implementation

The existing `BlufioMcpHandler` needs access to `MemoryStore` and `StorageAdapter` (in addition to `ToolRegistry`). Extend the struct:

```rust
pub struct BlufioMcpHandler {
    tool_registry: Arc<RwLock<ToolRegistry>>,
    memory_store: Option<Arc<MemoryStore>>,
    storage: Option<Arc<dyn StorageAdapter + Send + Sync>>,
    export_tools: Vec<String>,
    timeout_secs: u64,
}
```

The `Option` wrapping allows the handler to work in both stdio (no storage/memory access) and HTTP (full access) modes.

### Pattern 3: ServerHandler Capability Advertisement

Capabilities must be declared in `get_info()`. Add resources and prompts:

```rust
fn get_info(&self) -> ServerInfo {
    InitializeResult {
        capabilities: ServerCapabilities {
            tools: Some(ToolsCapability::default()),
            resources: if self.memory_store.is_some() || self.storage.is_some() {
                Some(ResourcesCapability::default())
            } else {
                None
            },
            prompts: Some(PromptsCapability::default()),
            ..Default::default()
        },
        // ...
    }
}
```

### Pattern 4: MCP Resource URIs

Resources use URI-based identification:
- Static resources: `list_resources` returns fixed entries with their URIs
- Templates: `list_resource_templates` returns URI templates with `{param}` placeholders
- Reading: `read_resource` receives the full URI and the handler parses/routes it

```rust
// Static resources
Resource {
    uri: "blufio://sessions".into(),
    name: "Session list".into(),
    mime_type: Some("application/json".into()),
    ..
}

// Resource templates
ResourceTemplate {
    uri_template: "blufio://memory/{id}".into(),
    name: "Memory item".into(),
    mime_type: Some("application/json".into()),
    ..
}

ResourceTemplate {
    uri_template: "blufio://memory/search?q={query}&limit={limit}".into(),
    name: "Memory search".into(),
    ..
}
```

### Anti-Patterns to Avoid
- **Coupling HTTP transport to handler logic:** The handler should remain transport-agnostic. Only serve.rs/gateway wiring knows about HTTP vs stdio.
- **Leaking rmcp types into public API:** Keep all rmcp types internal to blufio-mcp-server. Resources return `serde_json::Value`, not rmcp models.
- **Hardcoding CORS origins:** Origins must come from config, not compiled-in.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| SSE stream management | Custom SSE impl | rmcp's `StreamableHttpService` | Handles keep-alive, priming events, reconnection, session management |
| CORS validation | Custom header checking | tower-http `CorsLayer` | Handles preflight, origin matching, credential headers correctly |
| JSON-RPC routing | Custom dispatcher | rmcp's `ServerHandler` dispatch | Handles method dispatch, error codes, pagination |
| MCP session management | Custom session tracking | rmcp's `SessionManager` | Handles session IDs, stream resumption, Last-Event-ID |

## Common Pitfalls

### Pitfall 1: CorsLayer Order Matters
**What goes wrong:** CORS preflight requests (OPTIONS) bypass auth middleware if layers are in wrong order.
**Why it happens:** axum applies layers bottom-to-top. CORS must be applied AFTER auth so OPTIONS requests are handled before auth checks.
**How to avoid:** Apply CORS as the outermost layer on MCP routes, auth as inner layer. Use `CorsLayer::new().allow_methods(...)` not `CorsLayer::permissive()`.
**Warning signs:** OPTIONS requests to /mcp return 401.

### Pitfall 2: Handler State for HTTP vs Stdio
**What goes wrong:** Handler created for stdio mode doesn't have memory/storage access; crashes when resource methods are called.
**Why it happens:** Stdio subcommand initializes minimal infrastructure.
**How to avoid:** Use `Option<Arc<MemoryStore>>` fields; resource methods return empty results or method-not-found when None.

### Pitfall 3: StreamableHttpService Requires CancellationToken
**What goes wrong:** MCP service doesn't shut down when the gateway stops.
**Why it happens:** `StreamableHttpServerConfig` requires a `CancellationToken` to terminate active SSE connections.
**How to avoid:** Pass a child token from the main shutdown handler: `cancel.child_token()`.

### Pitfall 4: Auth Token Validation on Startup
**What goes wrong:** Server starts with `mcp.enabled=true` but no `mcp.auth_token`, exposing unauthenticated MCP endpoints.
**Why it happens:** Missing startup validation.
**How to avoid:** Check `mcp.enabled && mcp.auth_token.is_none()` in serve.rs and return error (same pattern as gateway SEC-02 guard).

### Pitfall 5: Blocking Memory/Storage Calls in Handler
**What goes wrong:** `read_resource` calls `MemoryStore::get_by_id` which does an async SQLite query. If the handler is sync or blocks, the event loop stalls.
**Why it happens:** ServerHandler methods are async but care must be taken with `tokio_rusqlite::Connection::call`.
**How to avoid:** All handler methods are already async. Just ensure `await` is used correctly with `MemoryStore` methods.

## Code Examples

### StreamableHttpService with axum Integration
```rust
// Source: rmcp docs and Context7 research
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService,
};

pub fn build_mcp_router(
    handler: BlufioMcpHandler,
    cancel: CancellationToken,
    cors_origins: &[String],
    auth_token: String,
) -> Router {
    let config = StreamableHttpServerConfig {
        sse_keep_alive: Some(Duration::from_secs(30)),
        sse_retry: Some(Duration::from_secs(5)),
        stateful_mode: true,
        cancellation_token: cancel,
    };

    let mcp_service = StreamableHttpService::new(handler, config);

    // Build restricted CORS layer
    let cors = build_mcp_cors(cors_origins);

    // Build auth state
    let mcp_auth = McpAuthConfig { auth_token };

    Router::new()
        .nest_service("/mcp", mcp_service)
        .route_layer(middleware::from_fn_with_state(mcp_auth, mcp_auth_middleware))
        .layer(cors)
}
```

### Resource Implementation Pattern
```rust
// list_resources returns static resources
async fn list_resources(
    &self,
    _request: Option<PaginatedRequestParam>,
    _context: RequestContext<RoleServer>,
) -> Result<ListResourcesResult, McpError> {
    let mut resources = Vec::new();

    if self.storage.is_some() {
        resources.push(Resource {
            uri: "blufio://sessions".into(),
            name: "Session list".into(),
            description: Some("List of conversation sessions".into()),
            mime_type: Some("application/json".into()),
            ..Default::default()
        });
    }

    Ok(ListResourcesResult {
        resources,
        next_cursor: None,
        meta: None,
    })
}

// list_resource_templates returns URI templates
async fn list_resource_templates(
    &self,
    _request: Option<PaginatedRequestParam>,
    _context: RequestContext<RoleServer>,
) -> Result<ListResourceTemplatesResult, McpError> {
    let mut templates = Vec::new();

    if self.memory_store.is_some() {
        templates.push(ResourceTemplate {
            uri_template: "blufio://memory/{id}".into(),
            name: "Memory item".into(),
            description: Some("Read a specific memory by ID".into()),
            mime_type: Some("application/json".into()),
            ..Default::default()
        });
        templates.push(ResourceTemplate {
            uri_template: "blufio://memory/search?q={query}&limit={limit}".into(),
            name: "Memory search".into(),
            description: Some("Search memories via FTS5".into()),
            mime_type: Some("application/json".into()),
            ..Default::default()
        });
    }

    if self.storage.is_some() {
        templates.push(ResourceTemplate {
            uri_template: "blufio://sessions/{id}".into(),
            name: "Session history".into(),
            description: Some("Message history for a session".into()),
            mime_type: Some("application/json".into()),
            ..Default::default()
        });
    }

    Ok(ListResourceTemplatesResult {
        resource_templates: templates,
        next_cursor: None,
        meta: None,
    })
}
```

### Prompt Template Pattern
```rust
async fn list_prompts(
    &self,
    _request: Option<PaginatedRequestParam>,
    _context: RequestContext<RoleServer>,
) -> Result<ListPromptsResult, McpError> {
    let prompts = vec![
        Prompt {
            name: "summarize-conversation".into(),
            description: Some("Summarize a conversation session".into()),
            arguments: Some(vec![PromptArgument {
                name: "session_id".into(),
                description: Some("Session ID to summarize".into()),
                required: Some(true),
            }]),
        },
        Prompt {
            name: "search-memory".into(),
            description: Some("Search long-term memory".into()),
            arguments: Some(vec![PromptArgument {
                name: "query".into(),
                description: Some("Search query".into()),
                required: Some(true),
            }]),
        },
        Prompt {
            name: "explain-skill".into(),
            description: Some("Explain a Blufio skill".into()),
            arguments: Some(vec![PromptArgument {
                name: "skill_name".into(),
                description: Some("Name of the skill".into()),
                required: Some(true),
            }]),
        },
    ];

    Ok(ListPromptsResult {
        prompts,
        next_cursor: None,
        meta: None,
    })
}
```

### Tool Annotations Pattern
```rust
// Extend BlufioTool trait with default annotation methods
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    async fn invoke(&self, input: serde_json::Value) -> Result<ToolOutput, BlufioError>;

    // New annotation methods with sensible defaults
    fn is_read_only(&self) -> bool { false }
    fn is_destructive(&self) -> bool { false }
    fn is_idempotent(&self) -> bool { false }
    fn is_open_world(&self) -> bool { true }
}

// In bridge.rs, map to rmcp ToolAnnotations
pub fn to_mcp_tool(name: &str, tool: &dyn BlufioTool) -> rmcp::model::Tool {
    let mut mcp_tool = rmcp::model::Tool::new(name, tool.description(), schema);
    mcp_tool.annotations = Some(ToolAnnotations {
        read_only_hint: Some(tool.is_read_only()),
        destructive_hint: Some(tool.is_destructive()),
        idempotent_hint: Some(tool.is_idempotent()),
        open_world_hint: Some(tool.is_open_world()),
        ..Default::default()
    });
    mcp_tool
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| SSE-only transport | Streamable HTTP (POST + SSE) | MCP spec 2025-11-25 | Bidirectional HTTP, server can push via SSE |
| No resource templates | URI templates with parameter expansion | MCP spec 2024 | Enables parameterized resource URIs |
| No tool annotations | readOnlyHint, destructiveHint, etc. | MCP spec 2025 | Clients can make safer tool selection decisions |

## Open Questions

1. **StreamableHttpService integration with axum nest_service**
   - What we know: `StreamableHttpService` implements Tower `Service`. axum's `nest_service` mounts Tower services.
   - What's unclear: Whether body type compatibility requires `BoxBody` conversion.
   - Recommendation: Test during implementation; use `map_request`/`map_response` if needed.

2. **Notification delivery for tools/list_changed (SRVR-13)**
   - What we know: rmcp's `RequestContext::peer` provides access to send notifications.
   - What's unclear: How to detect skill install/discovery changes from outside the handler. The handler doesn't own the skill installation lifecycle.
   - Recommendation: Add a `Notify` channel to the handler. When serve.rs detects a tool change, send a signal. The handler's background task forwards it as an MCP notification. Alternatively, compare tool count on each `list_tools` call and emit if changed.

3. **Progress notifications for WASM tools (SRVR-14)**
   - What we know: rmcp supports progress notifications via the context.
   - What's unclear: BlufioTool::invoke doesn't currently accept a progress callback.
   - Recommendation: This is a deeper change to the Tool trait. Consider adding an optional `progress_sender` parameter or a new `invoke_with_progress` method. For Phase 17, implement the MCP-side plumbing (accepting and forwarding progress), even if WASM tools don't emit progress yet.

## Sources

### Primary (HIGH confidence)
- Context7 /websites/rs_rmcp - StreamableHttpService, ServerHandler, SSE stream implementation
- rmcp 0.17 docs.rs - Transport features, handler trait, capability types
- Existing codebase: handler.rs, bridge.rs, server.rs, auth.rs (direct code inspection)

### Secondary (MEDIUM confidence)
- MCP specification 2025-11-25 (referenced in rmcp code comments)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - rmcp already in workspace, features verified via Context7
- Architecture: HIGH - existing patterns (axum Router, auth middleware) directly reusable
- Pitfalls: HIGH - derived from actual codebase inspection and rmcp source analysis

**Research date:** 2026-03-02
**Valid until:** 2026-04-02 (30 days -- rmcp API is stable at 0.17)
