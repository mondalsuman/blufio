# Technology Stack: MCP Integration (v1.1)

**Project:** Blufio MCP Server + Client
**Researched:** 2026-03-02
**Overall confidence:** HIGH

> This document covers ONLY the new dependencies needed for MCP integration.
> Existing stack (tokio, axum, serde, reqwest, etc.) is validated and unchanged from v1.0.

---

## Recommended Stack

### Core MCP Library

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| `rmcp` | 0.17.0 | Official Rust MCP SDK (server + client) | Official Anthropic-maintained SDK under `modelcontextprotocol/rust-sdk`. Implements MCP spec 2025-11-25. Tokio-native async, serde-based serialization, proc-macro tooling. 3,080 GitHub stars. Released 2026-02-27. Shares tokio/serde/axum ecosystem with existing Blufio stack. |

**Why `rmcp` over alternatives:**

| Crate | Why Not |
|-------|---------|
| `rust-mcp-sdk` | Third-party, not official. Duplicates what rmcp provides. Risk of spec drift. |
| `mcp-protocol-sdk` | Community crate, lower adoption, fewer downloads. |
| `mcpx` | Targets older spec (2025-03-26), not the current 2025-11-25. |
| `pmcp` (Prism) | Enterprise-focused, heavier dependency footprint. |
| Hand-roll JSON-RPC | MCP spec is complex: session management, SSE resumability, protocol version negotiation, `Last-Event-ID` redelivery, `Mcp-Session-Id` lifecycle. Using the official SDK avoids subtle spec violations that would surface as broken Claude Desktop integration. |

### New Supporting Library

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| `schemars` | 1.0 | JSON Schema generation (2020-12 draft) | rmcp 0.17.0 depends on schemars ^1.0. Required by the `#[tool]` proc macro to generate JSON Schema for tool parameter types. MCP requires JSON Schema descriptions for all tool inputs. Not currently in the Blufio workspace. |

### Existing Dependencies Reused (no version changes)

| Library | Version | How MCP Uses It |
|---------|---------|-----------------|
| `tokio` | 1.x | rmcp is tokio-native. stdin/stdout async I/O, child process spawning, HTTP server. |
| `axum` | 0.8.x | `StreamableHttpService` mounts via `Router::nest_service("/mcp", service)`. |
| `serde` | 1.x | JSON-RPC message serialization, tool parameter structs. |
| `serde_json` | 1.x | Tool argument construction, JSON-RPC payloads. |
| `reqwest` | 0.12.x | Used by rmcp's HTTP client transports (streamable-http-client, sse-client). Already in workspace with `rustls-tls`. |
| `tracing` | 0.1.x | rmcp emits tracing events internally. Blufio's subscriber captures them. |
| `uuid` | 1.x | Session ID generation for Streamable HTTP (`Mcp-Session-Id` header). |

---

## rmcp Feature Flags

### Feature Map (what to enable and why)

| Feature Flag | Purpose | Needed For | Side |
|---|---|---|---|
| `server` | Server-side MCP handler trait (`ServerHandler`) | Exposing Blufio tools/skills/memory | Server |
| `client` | Client-side MCP handler trait (`ClientHandler`) | Consuming external MCP servers | Client |
| `macros` | `#[tool]`, `#[tool_router]`, `#[tool_handler]` proc macros | Ergonomic tool definitions with auto-generated JSON Schema | Both |
| `schemars` | JSON Schema generation for tool parameters | MCP requires JSON Schema for all tool inputs | Server |
| `transport-io` | `stdio()` -- stdin/stdout transport | stdio MCP server mode (Claude Desktop) | Server |
| `transport-child-process` | `TokioChildProcess` -- spawn subprocess | MCP client connecting to external stdio servers | Client |
| `transport-streamable-http-server` | `StreamableHttpService` for axum | HTTP-based MCP server endpoint at `/mcp` | Server |
| `transport-streamable-http-client` | `StreamableHttpClientTransport` | MCP client connecting to remote HTTP servers | Client |
| `transport-sse-client-reqwest` | `SseClientTransport` (backward compat) | MCP client connecting to legacy SSE-only servers | Client |
| `reqwest` | HTTP client (rustls backend) | Required by streamable-http-client and sse-client | Client |

**Default features** (`base64`, `macros`, `server`) cover the server basics. Client features and all transports must be explicitly enabled.

### Features NOT needed

| Feature | Why Skip |
|---------|----------|
| `auth` (OAuth2) | Blufio has its own device keypair auth. OAuth2 for MCP is only needed when connecting to third-party MCP servers requiring OAuth. Defer until a concrete use case. |
| `tower` | Blufio already uses axum middleware. rmcp's tower feature adds MCP-specific middleware layers that are not needed for basic integration. |
| `elicitation` | Interactive user input prompting. Not needed for an always-on agent that operates autonomously. |
| `transport-worker` | In-process worker pattern. Not needed -- Blufio uses child process or HTTP for external servers. |

---

## Transport Implementation Details

### Transport 1: stdio (PRIMARY -- Claude Desktop integration)

**MCP Spec behavior:** Client launches server as subprocess. Server reads JSON-RPC from stdin, writes to stdout. Messages are newline-delimited, MUST NOT contain embedded newlines. stderr is for logging only.

**rmcp server API:**
```rust
use rmcp::transport::io::stdio;

// In `blufio mcp serve-stdio` CLI subcommand
let service = BlufioMcpServer::new(agent_handle)
    .serve(stdio())  // binds to process stdin/stdout
    .await?;
service.waiting().await?;  // blocks until client disconnects
```

**rmcp client API (Blufio consuming external stdio servers):**
```rust
use rmcp::transport::child_process::{TokioChildProcess, ConfigureCommandExt};
use tokio::process::Command;

let transport = TokioChildProcess::new(
    Command::new("npx").configure(|cmd| {
        cmd.arg("-y").arg("@modelcontextprotocol/server-filesystem");
    })
)?;
let client = ().serve(transport).await?;
let tools = client.list_tools(Default::default()).await?;
let result = client.call_tool(CallToolRequestParams {
    name: "read_file".into(),
    arguments: serde_json::json!({"path": "/tmp/test.txt"}).as_object().cloned(),
}).await?;
```

**rmcp features needed:** `transport-io` (server), `transport-child-process` (client)

**Integration point:** New `blufio mcp serve-stdio` CLI subcommand. Launches agent in MCP server mode, reading from stdin/stdout instead of starting the HTTP gateway. The existing `blufio serve` continues to run the HTTP/WS gateway.

**Claude Desktop config (`claude_desktop_config.json`):**
```json
{
  "mcpServers": {
    "blufio": {
      "command": "/usr/local/bin/blufio",
      "args": ["mcp", "serve-stdio"]
    }
  }
}
```

### Transport 2: Streamable HTTP (PRIMARY -- remote/production)

**MCP Spec (2025-11-25):** Single endpoint (e.g., `/mcp`) accepting:
- **POST**: Client sends JSON-RPC request/notification/response. Server returns `application/json` (single response) or `text/event-stream` (SSE stream with response + notifications).
- **GET**: Client opens SSE stream for server-initiated messages.
- **DELETE**: Client terminates session.

Session management via `Mcp-Session-Id` header. Supports resumability via `Last-Event-ID`. Protocol version via `MCP-Protocol-Version` header.

**Security requirements (from MCP spec):**
- MUST validate `Origin` header (DNS rebinding protection)
- Bind to 127.0.0.1 for local, not 0.0.0.0
- Session IDs must be cryptographically secure (UUID v4)
- Blufio already has TLS enforcement and SSRF protection -- reuse those

**rmcp server API:**
```rust
use rmcp::transport::streamable_http_server::{
    StreamableHttpService, session::local::LocalSessionManager,
};

// Factory creates a new MCP server instance per session
let mcp_service = StreamableHttpService::new(
    move || Ok(BlufioMcpServer::new(agent_handle.clone())),
    LocalSessionManager::default().into(),
    Default::default(),
);

// Mount on existing axum router alongside /v1/* REST API
let router = existing_blufio_router
    .nest_service("/mcp", mcp_service);

// Serve with existing TCP listener
axum::serve(tcp_listener, router)
    .with_graceful_shutdown(shutdown_signal())
    .await?;
```

**rmcp client API:**
```rust
use rmcp::transport::streamable_http_client::StreamableHttpClientTransport;

let transport = StreamableHttpClientTransport::new(
    "https://remote-server:8443/mcp".parse()?
);
let client = ().serve(transport).await?;
let tools = client.list_tools(Default::default()).await?;
```

**rmcp features needed:** `transport-streamable-http-server` (server), `transport-streamable-http-client` + `reqwest` (client)

**Integration point:** Mount `StreamableHttpService` at `/mcp` on the existing axum `Router` in `blufio-gateway`. Coexists with `/v1/*` REST API and WebSocket endpoints on the same port. No new ports needed.

### Transport 3: SSE Client (BACKWARD COMPAT -- legacy servers only)

**Context:** SSE was the HTTP transport in MCP spec 2024-11-05. Deprecated in 2025-03-26 in favor of Streamable HTTP. Still needed because many existing MCP servers only support the old SSE transport (separate `/sse` and `/messages` endpoints).

**Approach:** Do NOT build a separate SSE server. Streamable HTTP subsumes SSE on the server side (it uses SSE internally for streaming). Only add SSE as a client transport so Blufio can connect to legacy MCP servers that haven't migrated to Streamable HTTP yet.

**rmcp client API:**
```rust
use rmcp::transport::sse_client::SseClientTransport;

let transport = SseClientTransport::new("http://legacy-server:3000/sse")?;
let client = ().serve(transport).await?;
```

**rmcp features needed:** `transport-sse-client-reqwest` + `reqwest` (client only)

**TOML config drives transport selection:**
```toml
[mcp.servers.filesystem]
transport = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem"]

[mcp.servers.github]
transport = "streamable-http"
url = "https://mcp.github.com/mcp"

[mcp.servers.legacy-tool]
transport = "sse"
url = "http://localhost:3000/sse"
```

---

## Cargo.toml Configuration

### New workspace dependency additions

```toml
# Add to [workspace.dependencies] in root Cargo.toml
rmcp = { version = "0.17", default-features = false, features = [
    "server",
    "client",
    "macros",
    "schemars",
    "transport-io",
    "transport-child-process",
    "transport-streamable-http-server",
    "transport-streamable-http-client",
    "transport-sse-client-reqwest",
    "reqwest",
] }
schemars = "1.0"
```

`serde_json` is already a transitive dependency but may need an explicit workspace entry if tool argument construction requires it directly.

### Per-crate dependency allocation

**Option A: All features at workspace level (simpler)**

Every crate that uses rmcp gets all features. Fine for a single binary where tree-shaking happens at link time.

**Option B: Split features per-crate (tighter control)**

```toml
# Root Cargo.toml [workspace.dependencies]
rmcp = { version = "0.17", default-features = false }
schemars = "1.0"

# blufio-mcp-server/Cargo.toml (new crate)
[dependencies]
rmcp = { workspace = true, features = [
    "server", "macros", "schemars",
    "transport-io",
    "transport-streamable-http-server",
] }
schemars = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
blufio-core = { path = "../blufio-core" }

# blufio-mcp-client/Cargo.toml (new crate)
[dependencies]
rmcp = { workspace = true, features = [
    "client",
    "transport-child-process",
    "transport-streamable-http-client",
    "transport-sse-client-reqwest",
    "reqwest",
] }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
blufio-core = { path = "../blufio-core" }

# blufio-gateway/Cargo.toml (existing -- add MCP mount)
[dependencies]
rmcp = { workspace = true, features = [
    "transport-streamable-http-server",
] }
blufio-mcp-server = { path = "../blufio-mcp-server" }
# ... existing deps unchanged
```

**Recommendation:** Option A for v1.1. The binary is a single artifact anyway, and Cargo's feature unification means all features get compiled once regardless. Option B adds maintenance burden with no real binary size savings until/unless Blufio ships separate server-only and client-only binaries.

---

## What NOT to Add

| Temptation | Why Avoid |
|------------|-----------|
| `eventsource-client` or `sse-codec` | rmcp handles SSE internally via its transport features. Adding standalone SSE crates creates duplicate SSE parsing code. |
| `jsonrpc-core` or `jsonrpc-v2` | rmcp implements JSON-RPC 2.0 internally. A separate JSON-RPC crate creates type conflicts and redundant serialization. |
| `tower` as new dependency | rmcp has optional `tower` feature but Blufio already uses axum middleware. Only add if you need rmcp-specific Tower middleware layers. |
| `oauth2` crate directly | rmcp has `auth` feature for OAuth2. Defer unless connecting to OAuth-protected MCP servers becomes a real requirement. |
| `hyper` directly | axum already wraps hyper. `StreamableHttpService` plugs into axum router, not raw hyper. |
| A second MCP crate | Do NOT use rmcp for server and a different crate for client. Use rmcp for both -- consistent types, no conversion layer, shared JSON-RPC implementation. |
| `schemars` 0.8 | rmcp 0.17 requires schemars ^1.0. Using 0.8 will fail to compile. The 1.0 release changed `Schema` from a struct to a `serde_json::Value` wrapper -- cleaner API. |

---

## Dependency Impact Assessment

| Metric | Before (v1.0) | After (v1.1) | Delta |
|--------|----------------|--------------|-------|
| Workspace crates | 14 | 16 | +2 (`blufio-mcp-server`, `blufio-mcp-client`) |
| Direct workspace deps | ~35 | ~37 | +2 (`rmcp`, `schemars`) |
| Transitive deps (est.) | ~300 | ~315 | +~15 (rmcp brings rmcp-macros, pastey, pin-project-lite, etc. -- many already present via tokio/axum/reqwest) |
| Binary size impact (est.) | ~25MB | ~26-27MB | +1-2MB (JSON-RPC impl, SSE codec, session management) |
| Compile time impact (est.) | baseline | +10-15% | rmcp-macros proc macro, schemars derive |

**Low risk:** rmcp's dependency tree heavily overlaps with Blufio's existing deps: tokio, serde, serde_json, futures, reqwest, http, hyper-util, tracing, thiserror, bytes, base64. The genuinely new transitive deps are primarily `rmcp-macros`, `pastey`, and `schemars`.

---

## MCP Protocol Version Support

| Spec Version | Status | rmcp 0.17 Support | Notes |
|---|---|---|---|
| **2025-11-25** | **Current** | Full | Streamable HTTP, session management, resumability, `MCP-Protocol-Version` header |
| 2025-06-18 | Previous | Full | Backward compatible |
| 2025-03-26 | Previous | Full | First version to deprecate SSE |
| 2024-11-05 | Legacy | Via SSE client | SSE client transport connects to servers still on this spec |

---

## Version Compatibility with Existing Stack

| rmcp Dependency | Blufio Existing | Conflict? |
|-----------------|-----------------|-----------|
| tokio ^1 | tokio 1.x | None -- same major |
| serde ^1.0 | serde 1.x | None -- same major |
| serde_json ^1.0 | serde_json (transitive) | None |
| futures ^0.3 | futures 0.3 | None -- same |
| thiserror ^2 | thiserror 2 | None -- same |
| tracing ^0.1 | tracing 0.1 | None -- same |
| reqwest (via features) | reqwest 0.12 | **CHECK:** rmcp's reqwest feature pulls reqwest internally. If rmcp pins a different reqwest minor, Cargo may unify or error. Both use `rustls-tls`. Should resolve cleanly. |
| http (transitive) | http (via axum) | None -- axum 0.8 and rmcp both use http 1.x |
| schemars ^1.0 | **NEW** | No conflict -- new dependency |

**One concern:** rmcp 0.17 may internally depend on a newer reqwest than Blufio's 0.12. Check `cargo tree -p rmcp` after adding the dependency. If version conflict, upgrade Blufio's reqwest to match. Both use rustls-tls so the upgrade path is clean.

---

## Alternatives Considered

| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| MCP SDK | **rmcp 0.17** (official) | rust-mcp-sdk | Not official. Smaller community. Risk of falling behind spec updates. |
| MCP SDK | **rmcp 0.17** (official) | Hand-roll JSON-RPC | MCP spec complexity (session mgmt, resumability, protocol negotiation, SSE framing) makes hand-rolling a 3-4 week effort that would replicate what rmcp provides. |
| JSON Schema | **schemars 1.0** | schemars 0.8 | rmcp 0.17 depends on schemars ^1.0. Must match. 1.0 is also a better API (Schema is serde_json::Value wrapper). |
| HTTP transport | **Streamable HTTP** (2025-11-25 spec) | SSE-only server | SSE deprecated in MCP spec since 2025-03-26. Streamable HTTP is the current standard. |
| HTTP framework | **axum 0.8** (existing) | actix-web | Already using axum. rmcp has first-class axum integration via `nest_service`. No reason to add a framework. |
| SSE backward compat | **SSE client only** | Full SSE server + client | Building an SSE server is unnecessary work. Streamable HTTP covers modern clients. SSE client covers legacy servers. |

---

## Integration Points with Existing Blufio Stack

| Existing Component | How MCP Integrates | Notes |
|---|---|---|
| **blufio-gateway** (axum router) | Mount `StreamableHttpService` at `/mcp` via `Router::nest_service` | Coexists with `/v1/*` REST and WebSocket endpoints on same port |
| **blufio-core** (7 adapter traits) | MCP client likely needs new trait or extension to existing `SkillRuntime` | External MCP tools appear as invocable tools to the agent |
| **blufio-config** (figment + TOML) | New `[mcp]` config section with `[mcp.servers.<name>]` entries | Per-server: transport type, command/URL, args, env vars |
| **blufio-skill** (WASM sandbox) | MCP server exposes each WASM skill as an MCP tool | Skill manifest -> MCP tool schema mapping |
| **blufio-memory** (ONNX embeddings) | MCP server exposes memory search as MCP resource or tool | Semantic search becomes MCP-accessible |
| **blufio-agent** (FSM loop) | Agent discovers external MCP tools and merges into tool list during context assembly | MCP tools treated same as built-in tools |
| **blufio-security** (TLS/SSRF) | MCP HTTP endpoint reuses existing security middleware | Origin validation, TLS enforcement, SSRF protection |
| **blufio** (CLI binary) | New subcommand: `blufio mcp serve-stdio` | Launches MCP server mode on stdin/stdout |

---

## Sources

### Official / Authoritative (HIGH confidence)
- [Official rmcp repository -- modelcontextprotocol/rust-sdk](https://github.com/modelcontextprotocol/rust-sdk) -- 3,080 stars, Anthropic-maintained
- [rmcp 0.17.0 on docs.rs](https://docs.rs/rmcp/0.17.0/rmcp/) -- Released 2026-02-27, feature flags and API reference
- [MCP Specification 2025-11-25: Transports](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports) -- Authoritative spec for stdio and Streamable HTTP
- [schemars 1.0 migration guide](https://graham.cool/schemars/migrating/) -- 0.8 to 1.0 changes

### Tutorials / Guides (MEDIUM confidence, code patterns verified)
- [Shuttle: Build a Streamable HTTP MCP Server in Rust](https://www.shuttle.dev/blog/2025/10/29/stream-http-mcp) -- axum + rmcp integration pattern with `StreamableHttpService`
- [Shuttle: Build a stdio MCP Server in Rust](https://www.shuttle.dev/blog/2025/07/18/how-to-build-a-stdio-mcp-server-in-rust) -- stdio transport pattern
- [DeepWiki: rmcp Client Examples](https://deepwiki.com/modelcontextprotocol/rust-sdk/6.5-client-examples) -- All 8 client examples with feature requirements
- [DeepWiki: rmcp Getting Started](https://deepwiki.com/modelcontextprotocol/rust-sdk/1.1-getting-started) -- Complete feature flag reference
- [HackMD: MCP in Rust Practical Guide](https://hackmd.io/@Hamze/SytKkZP01l) -- End-to-end walkthrough

### Analysis (MEDIUM confidence)
- [Why MCP Deprecated SSE and Went with Streamable HTTP](https://blog.fka.dev/blog/2025-06-06-why-mcp-deprecated-sse-and-go-with-streamable-http/) -- SSE deprecation rationale
- [Auth0: Why MCP's Move Away from SSE Simplifies Security](https://auth0.com/blog/mcp-streamable-http/) -- Security implications of transport change
- [crates.io: rmcp versions](https://crates.io/crates/rmcp/versions) -- Version history

---
*Stack research for: Blufio v1.1 MCP Integration*
*Researched: 2026-03-02*
