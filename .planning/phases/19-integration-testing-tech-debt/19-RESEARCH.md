# Phase 19: Integration Testing + Tech Debt - Research

**Researched:** 2026-03-03
**Domain:** E2E testing, Prometheus metrics, connection management, tech debt resolution
**Confidence:** HIGH

## Summary

Phase 19 covers two distinct categories: (1) integration testing and observability for the MCP system built in Phases 15-18, and (2) resolving accumulated v1.0 tech debt. The codebase already has a well-established TestHarness pattern (builder with mock provider/channel, temp SQLite) used by 7 existing E2E tests in `blufio/tests/e2e.rs`. The Prometheus metrics system (`blufio-prometheus`) uses the metrics-rs facade with PrometheusBuilder and already has 10 registered metrics -- extending it for MCP is straightforward.

The tech debt items range from code (wiring StorageAdapter into GatewayState, refactoring SessionActor's 15-arg constructor) to operational (systemd unit file, human verification procedures). Human test procedures (DEBT-04 through DEBT-07) produce documentation artifacts, not code changes.

**Primary recommendation:** Leverage the existing TestHarness pattern for MCP E2E tests, extend the recording.rs helper pattern for MCP metrics, and use Tower middleware for connection limiting. Group human verification items into a single documentation plan since they share the same artifact type.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
None -- all areas delegated to Claude's discretion.

### Claude's Discretion
**E2E Test Strategy (INTG-01, INTG-02, INTG-03)**
- Test approach for MCP stdio E2E (INTG-01): in-process mock vs real subprocess -- prefer consistency with existing TestHarness patterns (mock provider/channel)
- Test approach for agent-uses-external-MCP-tool E2E (INTG-02): mock MCP server vs fixture binary -- prefer in-process for CI speed
- Cross-contamination tests (INTG-03): unit tests on router/handler vs integration with running server -- use whichever level provides thorough coverage with minimal complexity
- MCP Prometheus metrics (INTG-04): which metrics to add (connection count, tool response sizes, context utilization required per spec) -- pick metrics that satisfy requirements without noise

**Human Verification Tasks (DEBT-04, DEBT-05, DEBT-06, DEBT-07)**
- Format for human test procedures -- runbook markdown, semi-automated scripts, or hybrid
- Memory bounds test setup (DEBT-07, 72h) -- local/VPS manual run vs containerized with monitoring
- Session persistence verification depth (DEBT-05) -- session data only vs full conversation continuity
- SIGTERM drain window (DEBT-06) -- 30s vs 60s, pick based on typical LLM response times and systemd conventions

**Connection Limits (INTG-05)**
- Default max concurrent MCP connections -- pick a conservative default for personal agent use case
- Over-limit behavior -- hard reject (503) vs queue with timeout
- Limit scope -- global across transports vs per-transport
- Configuration method -- config file only vs config file + env var override, consistent with existing blufio-config patterns

**Systemd & Deployment (DEBT-02)**
- Target OS -- any Linux with systemd (portable unit file)
- Installation paths -- pick paths that work with single-binary design
- Service user -- dedicated user vs root, follow systemd security best practices
- Restart policy -- on-failure vs always, pick what makes sense for a long-running agent

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| INTG-01 | E2E test: Claude Desktop connects via stdio, lists tools, invokes tool, reads resource | TestHarness + BlufioMcpHandler in-process test pattern |
| INTG-02 | E2E test: Agent uses external MCP tool in a conversation turn | TestHarness + mock MCP server using rmcp in-process |
| INTG-03 | Cross-contamination tests (JSON-RPC to non-MCP endpoints, vice versa) | axum test utilities (TestClient/oneshot pattern) |
| INTG-04 | Prometheus metrics for MCP (connection count, tool response sizes, context utilization) | recording.rs helper pattern, describe_*! macros |
| INTG-05 | Connection count limits enforced (configurable defaults) | Tower ConcurrencyLimit middleware or custom counter |
| DEBT-01 | GET /v1/sessions returns actual session data | Wire StorageAdapter into GatewayState, query from handler |
| DEBT-02 | Commit systemd unit file for production deployment | Standard systemd unit file with hardening directives |
| DEBT-03 | Refactor SessionActor constructor to reduce argument count | SessionActorConfig builder struct pattern |
| DEBT-04 | Live Telegram E2E verification (human test) | Runbook markdown document |
| DEBT-05 | Session persistence verification across restarts (human test) | Runbook markdown document |
| DEBT-06 | SIGTERM drain timing verification (human test) | Runbook markdown document |
| DEBT-07 | Memory bounds measured over 72+ hour runtime | Runbook markdown document with monitoring setup |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tokio | 1.x (workspace) | Async runtime for test harness | Already used throughout |
| axum | 0.7.x (workspace) | HTTP server for gateway/routing tests | Already used for gateway |
| metrics-rs | 0.x (workspace) | Metrics facade for Prometheus | Already used in blufio-prometheus |
| metrics-exporter-prometheus | 0.x (workspace) | Prometheus exporter | Already used |
| rmcp | 0.17.0 (workspace) | MCP protocol for server/client | Already used in blufio-mcp-server/client |
| tower | 0.4.x (workspace) | Middleware stack | Already used transitively via axum |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tempfile | (workspace) | Temp directories for test DBs | All E2E tests |
| serde_json | (workspace) | JSON-RPC message construction | Cross-contamination tests |
| uuid | (workspace) | ID generation in tests | Test fixtures |

### Alternatives Considered
No alternatives needed -- all dependencies are already in the workspace.

## Architecture Patterns

### Pattern 1: MCP E2E Test via In-Process Handler
**What:** Create `BlufioMcpHandler` with a mock-backed `ToolRegistry`, invoke MCP methods directly via the handler interface rather than spawning a subprocess.
**When to use:** INTG-01 (stdio MCP E2E simulation)
**Why:** Consistent with TestHarness pattern. No subprocess management. Fast CI execution. The existing `serve_stdio()` in blufio-mcp-server wraps rmcp, but the handler itself (`BlufioMcpHandler`) can be tested directly.
**Example:**
```rust
// Create handler with test tool registry
let mut tool_registry = ToolRegistry::new();
blufio_skill::builtin::register_builtins(&mut tool_registry);
let tool_registry = Arc::new(RwLock::new(tool_registry));
let handler = BlufioMcpHandler::new(tool_registry.clone(), &mcp_config);
// Test via rmcp's ServerHandler trait methods directly
```

### Pattern 2: Mock External MCP Server for Client E2E
**What:** Create a minimal in-process MCP server using rmcp that exposes a test tool, then have McpClientManager connect to it.
**When to use:** INTG-02 (agent uses external MCP tool)
**Why:** In-process avoids flaky network tests while still exercising the full client connection path. Bind to a random port on localhost.
**Example:**
```rust
// Start a minimal MCP server on a random port
let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
let port = listener.local_addr().unwrap().port();
// Create McpServerEntry pointing to localhost:port
// Connect via McpClientManager::connect_all
```

### Pattern 3: axum Testing for Cross-Contamination
**What:** Build the gateway Router, send test requests directly using axum's test utilities (`Router::into_service()`, `tower::ServiceExt::oneshot()`).
**When to use:** INTG-03 (cross-contamination tests)
**Why:** No need to bind a port. Test request routing at the application layer. Verify JSON-RPC payloads to REST endpoints get 4xx, and REST payloads to /mcp get MCP protocol errors.
**Example:**
```rust
use axum::body::Body;
use http::Request;
use tower::ServiceExt;
let app = build_test_router();
let resp = app.oneshot(Request::post("/v1/messages")
    .header("content-type", "application/json")
    .body(Body::from(json_rpc_payload))
    .unwrap()).await.unwrap();
assert_eq!(resp.status(), StatusCode::BAD_REQUEST); // or 415, 422
```

### Pattern 4: Prometheus Metrics Extension
**What:** Add MCP-specific metrics to `recording.rs` following the existing pattern: `describe_*!` in `register_metrics()`, free functions for recording.
**When to use:** INTG-04
**Why:** Consistent with existing blufio_messages_total, blufio_tokens_total pattern. No new dependencies.
**Metrics to add:**
- `blufio_mcp_connections_total` (counter): total MCP connections by transport
- `blufio_mcp_active_connections` (gauge): current active MCP connections
- `blufio_mcp_tool_response_size_bytes` (histogram): tool response sizes
- `blufio_mcp_context_utilization_ratio` (gauge): context window utilization (0.0-1.0)

### Pattern 5: Connection Limit Middleware
**What:** Use Tower `ConcurrencyLimit` or a custom `AtomicUsize` counter to enforce max concurrent MCP connections.
**When to use:** INTG-05
**Why:** Tower middleware integrates cleanly with axum. The MCP router is already nested at `/mcp` in `server.rs`, so a middleware layer can be added at that nesting point.
**Recommendation:**
- Default: 10 concurrent MCP connections (personal agent use case)
- Over-limit: HTTP 503 Service Unavailable (hard reject, no queue -- simpler and clearer for MCP clients)
- Scope: Global (single limit for all MCP transports -- personal agent has one user)
- Config: `mcp.max_connections` in TOML config file, consistent with existing config pattern

### Pattern 6: SessionActor Constructor Refactoring
**What:** Extract the 15 constructor arguments into a `SessionActorConfig` struct that groups related concerns.
**When to use:** DEBT-03
**Why:** The `#[allow(clippy::too_many_arguments)]` annotation is already there, indicating known tech debt. A config struct with named fields improves readability and makes future additions non-breaking.
**Example:**
```rust
pub struct SessionActorConfig {
    pub session_id: String,
    pub storage: Arc<dyn StorageAdapter + Send + Sync>,
    pub provider: Arc<dyn ProviderAdapter + Send + Sync>,
    pub context_engine: Arc<ContextEngine>,
    pub budget_tracker: Arc<tokio::sync::Mutex<BudgetTracker>>,
    pub cost_ledger: Arc<CostLedger>,
    pub memory_provider: Option<MemoryProvider>,
    pub memory_extractor: Option<Arc<MemoryExtractor>>,
    pub channel: String,
    pub router: Arc<ModelRouter>,
    pub default_model: String,
    pub default_max_tokens: u32,
    pub routing_enabled: bool,
    pub idle_timeout_secs: u64,
    pub tool_registry: Arc<RwLock<ToolRegistry>>,
}

impl SessionActor {
    pub fn new(config: SessionActorConfig) -> Self { ... }
}
```

### Pattern 7: GatewayState with StorageAdapter
**What:** Add `storage: Arc<dyn StorageAdapter + Send + Sync>` to `GatewayState`, wire it in `start_server()`, and use it in `get_sessions()` handler.
**When to use:** DEBT-01
**Why:** Direct fix for the TODO comment at handlers.rs:252. `GatewayState` already carries `inbound_tx`, `response_map`, `ws_senders` -- adding storage is consistent.

### Anti-Patterns to Avoid
- **Spawning real subprocesses in E2E tests:** Fragile, slow, hard to debug in CI. Use in-process handler testing.
- **Hardcoded ports in tests:** Flaky on CI. Always bind to port 0 and discover the assigned port.
- **Mixing MCP metrics with non-MCP metrics names:** Keep the `blufio_mcp_*` prefix distinct from `blufio_*` core metrics.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Concurrency limiting | Custom semaphore counting | Tower ConcurrencyLimit | Edge cases in counting (panic, timeout, cancellation) |
| JSON-RPC parsing in tests | Manual JSON parsing | serde_json + proper type structs | Brittle string matching misses edge cases |
| Prometheus text rendering | Custom /metrics formatter | metrics-exporter-prometheus handle.render() | Already working, well-tested |

## Common Pitfalls

### Pitfall 1: Prometheus Recorder Already Installed
**What goes wrong:** `PrometheusBuilder::new().install_recorder()` can only be called once per process. Tests that initialize PrometheusAdapter will panic if another test already installed a recorder.
**Why it happens:** Rust test harness runs tests in the same process.
**How to avoid:** Use a `std::sync::Once` guard or `#[serial]` test attribute for metrics tests. Alternatively, test metrics recording via the free functions without installing a recorder.
**Warning signs:** `SetRecorderError` panic in CI.

### Pitfall 2: Test Port Conflicts
**What goes wrong:** Tests binding to hardcoded ports fail when another test is running.
**Why it happens:** Parallel test execution.
**How to avoid:** Always use `TcpListener::bind("127.0.0.1:0")` and extract the port. Pass it to clients.
**Warning signs:** "address already in use" errors in CI.

### Pitfall 3: GatewayState Changes Breaking Existing Code
**What goes wrong:** Adding `storage` to `GatewayState` requires updating every place that constructs a `GatewayState`, including tests.
**Why it happens:** Struct construction requires all fields.
**How to avoid:** Update all construction sites. Search for `GatewayState {` in the codebase. Consider adding a builder or Default impl if there are many sites.
**Warning signs:** Compile errors in gateway tests.

### Pitfall 4: SessionActor Refactoring Breaking Callers
**What goes wrong:** Changing SessionActor::new signature breaks all call sites.
**Why it happens:** Constructor is used in TestHarness (harness.rs) and serve.rs.
**How to avoid:** Update both `harness.rs` and `serve.rs` in the same plan. Run full test suite.
**Warning signs:** Compile errors in blufio-test-utils and blufio crates.

### Pitfall 5: MCP Connection Limit Middleware Ordering
**What goes wrong:** Connection limit middleware applied in wrong order doesn't count connections correctly.
**Why it happens:** MCP router is nested under `/mcp` with its own middleware stack (CORS, auth).
**How to avoid:** Apply the concurrency limit layer at the `Router::nest("/mcp", mcp_router)` level in server.rs, before auth and CORS layers.
**Warning signs:** Connections not being counted or limits not enforced.

## Code Examples

### MCP Metrics Registration (recording.rs extension)
```rust
// In register_metrics():
describe_counter!("blufio_mcp_connections_total", "Total MCP connections by transport");
describe_gauge!("blufio_mcp_active_connections", "Currently active MCP connections");
describe_histogram!("blufio_mcp_tool_response_size_bytes", "MCP tool response sizes in bytes");
describe_gauge!("blufio_mcp_context_utilization_ratio", "Context window utilization ratio");

// Recording helpers:
pub fn record_mcp_connection(transport: &str) {
    metrics::counter!("blufio_mcp_connections_total", "transport" => transport.to_string()).increment(1);
}

pub fn set_mcp_active_connections(count: f64) {
    metrics::gauge!("blufio_mcp_active_connections").set(count);
}

pub fn record_mcp_tool_response_size(bytes: f64) {
    metrics::histogram!("blufio_mcp_tool_response_size_bytes").record(bytes);
}

pub fn set_mcp_context_utilization(ratio: f64) {
    metrics::gauge!("blufio_mcp_context_utilization_ratio").set(ratio);
}
```

### Systemd Unit File (DEBT-02)
```ini
[Unit]
Description=Blufio AI Agent
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=blufio
Group=blufio
ExecStart=/usr/local/bin/blufio serve
Restart=on-failure
RestartSec=5
WatchdogSec=300

# Security hardening
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/var/lib/blufio
PrivateTmp=yes
ProtectKernelTunables=yes
ProtectControlGroups=yes

# Environment
Environment=BLUFIO_CONFIG=/etc/blufio/config.toml
Environment=RUST_LOG=blufio=info

# Graceful shutdown
TimeoutStopSec=60
KillSignal=SIGTERM

[Install]
WantedBy=multi-user.target
```

### GET /v1/sessions Wiring (DEBT-01)
```rust
// In GatewayState:
pub struct GatewayState {
    // ... existing fields ...
    pub storage: Arc<dyn StorageAdapter + Send + Sync>,
}

// In handlers.rs:
pub async fn get_sessions(State(state): State<GatewayState>) -> Json<SessionListResponse> {
    let sessions = state.storage.list_sessions(None).await.unwrap_or_default();
    let infos: Vec<SessionInfo> = sessions.into_iter().map(|s| SessionInfo {
        id: s.id,
        channel: s.channel,
        state: s.state,
        created_at: s.created_at,
    }).collect();
    Json(SessionListResponse { sessions: infos })
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Subprocess MCP testing | In-process handler testing | rmcp 0.17+ | Faster CI, no process management |
| Custom connection counters | Tower ConcurrencyLimit | tower 0.4 | Battle-tested middleware |
| Large constructor arg lists | Config/builder structs | Rust idiom | Cleaner API, easier extension |

## Open Questions

1. **MCP stdio E2E test depth**
   - What we know: BlufioMcpHandler implements rmcp's ServerHandler trait. Can be tested in-process.
   - What's unclear: Whether to test the full stdio pipe (stdin/stdout redirection) or just handler-level methods.
   - Recommendation: Test handler-level (initialize, list_tools, call_tool, list_resources, read_resource). The stdio pipe itself is rmcp's responsibility.

2. **Context utilization metric calculation**
   - What we know: INTG-04 requires "context utilization" metric. The context engine has a token budget.
   - What's unclear: Where exactly to hook the measurement -- pre-LLM-call context assembly is the right point.
   - Recommendation: Add measurement in ContextEngine::assemble or in SessionActor::handle_message after context assembly. Report as ratio of used_tokens / max_tokens.

## Sources

### Primary (HIGH confidence)
- Codebase analysis: `blufio/tests/e2e.rs` (7 existing E2E tests with TestHarness)
- Codebase analysis: `blufio-test-utils/src/harness.rs` (TestHarness builder pattern)
- Codebase analysis: `blufio-prometheus/src/recording.rs` (10 existing metrics with helper pattern)
- Codebase analysis: `blufio-gateway/src/server.rs` (MCP router nesting, GatewayState)
- Codebase analysis: `blufio-gateway/src/handlers.rs` (get_sessions TODO at line 252-253)
- Codebase analysis: `blufio-agent/src/session.rs` (SessionActor 15-arg constructor at line 115)
- Codebase analysis: `blufio-mcp-server/src/lib.rs` (serve_stdio, BlufioMcpHandler)
- Codebase analysis: `blufio-mcp-client/src/lib.rs` (McpClientManager, diagnose_server)
- Codebase analysis: `blufio/src/doctor.rs` (check_mcp_servers already implemented)

### Secondary (MEDIUM confidence)
- systemd documentation: standard unit file directives and security hardening

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - all libraries already in workspace
- Architecture: HIGH - patterns derived from existing codebase
- Pitfalls: HIGH - identified from actual codebase structure
- E2E test approach: HIGH - consistent with established TestHarness pattern
- Metrics: HIGH - direct extension of existing recording.rs pattern
- Systemd: MEDIUM - standard practices, no project-specific verification needed

**Research date:** 2026-03-03
**Valid until:** 2026-04-03 (stable -- internal codebase patterns)
