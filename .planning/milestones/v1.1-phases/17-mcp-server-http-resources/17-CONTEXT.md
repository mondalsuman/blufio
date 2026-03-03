# Phase 17: MCP Server HTTP + Resources - Context

**Gathered:** 2026-03-02
**Status:** Ready for planning

<domain>
## Phase Boundary

Remote MCP clients access Blufio via Streamable HTTP at /mcp, and MCP clients can browse memory and session history as resources. Prompt templates are available via prompts/list and prompts/get. CORS is restricted to configured origins on MCP HTTP endpoints.

Requirements: SRVR-06, SRVR-07, SRVR-08, SRVR-09, SRVR-10, SRVR-11, SRVR-13, SRVR-14, SRVR-16

</domain>

<decisions>
## Implementation Decisions

### HTTP Transport Mounting
- Mount /mcp as a route on the existing axum gateway (same port, same process)
- No separate port — MCP and REST/WS coexist on the gateway
- Full Streamable HTTP with SSE for server→client notifications (tools/list_changed, progress)
- Use rmcp's transport-streamable-http-server feature for the /mcp endpoint

### MCP Authentication
- Separate `mcp.auth_token` field in McpConfig — distinct from gateway's bearer_token
- MCP clients get a different token than gateway API clients (security isolation)
- All /mcp routes require authentication — no public capability discovery
- Startup validation: refuse to start if `mcp.enabled=true` but `mcp.auth_token` is not set

### Memory Resources
- `blufio://memory/{id}` returns content + metadata: id, content, source, confidence, status, created_at, updated_at
- Raw embedding vectors are excluded (large, not human-useful)
- `blufio://memory/search?q={query}&limit={limit}` — URI template for FTS5 search, default limit 10
- Resource content returned as JSON (application/json MIME type)

### Session Resources
- `blufio://sessions` returns list of session summaries (id, channel, started_at)
- `blufio://sessions/{id}` returns message history for a specific session
- All session resources are read-only
- Content returned as JSON

### Prompt Templates
- Core set of 3 templates: `summarize-conversation`, `search-memory`, `explain-skill`
- Return full messages array: [{role: 'system', content: '...'}, {role: 'user', content: '...'}]
- Parameters are required where needed:
  - `summarize-conversation` requires `session_id`
  - `search-memory` requires `query`
  - `explain-skill` requires `skill_name`
- Clear contract — client knows exactly what to provide

### Tool Annotations (SRVR-11)
- Derive from BlufioTool trait — add annotation methods (is_read_only(), is_destructive(), etc.)
- Each tool declares its own hints (readOnlyHint, destructiveHint, idempotentHint, openWorldHint)
- Type-safe, scales with new skill additions

### Notifications
- tools/list_changed emitted on skill install or discovery changes (SRVR-13)
- Progress notifications include percentage (0-100) + status message for long-running WASM tools (SRVR-14)
- Delivered via SSE on the Streamable HTTP connection

### CORS Policy
- Explicit origin allowlist in TOML: `mcp.cors_origins = ["https://app.example.com"]`
- Empty list = reject all cross-origin requests (secure by default)
- Restricted CORS only applies to /mcp routes — existing gateway routes keep current behavior
- Minimal blast radius for this phase

### Claude's Discretion
- Exact SSE keepalive/heartbeat interval
- Internal structure of the Streamable HTTP handler (how rmcp integrates with axum)
- Error response formatting for MCP protocol errors
- Resource pagination strategy (if memory list is large)
- How to detect skill install/discovery changes for notifications/tools/list_changed

</decisions>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `BlufioMcpHandler` (handler.rs): Existing ServerHandler impl — needs to be extended with resources, prompts, notifications capabilities
- `bridge.rs`: Tool filtering and conversion — extend with tool annotation support
- `McpConfig` (blufio-config/src/model.rs): Needs new fields: auth_token, cors_origins
- `MemoryStore` (blufio-memory/src/store.rs): get_by_id, get_active, FTS5 search — directly usable for memory resources
- `AuthConfig` + `auth_middleware` (blufio-gateway/src/auth.rs): Existing bearer token auth pattern to follow for MCP-specific middleware
- `CorsLayer` (blufio-gateway/src/server.rs): Currently permissive — MCP routes will use a separate restricted CorsLayer

### Established Patterns
- axum Router composition: public_routes + api_routes + ws_routes merged — add mcp_routes the same way
- Auth middleware applied via route_layer with from_fn_with_state
- rmcp ServiceExt pattern used for stdio — similar pattern for HTTP transport
- All public types are Blufio-owned, rmcp types stay internal

### Integration Points
- Gateway server.rs: Mount /mcp routes alongside existing routes
- McpConfig: Add auth_token and cors_origins fields
- BlufioMcpHandler: Extend ServerHandler to advertise resources + prompts capabilities
- serve.rs: Wire MCP HTTP alongside existing gateway startup logic
- BlufioTool trait: Add optional annotation methods with default impls

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 17-mcp-server-http-resources*
*Context gathered: 2026-03-02*
