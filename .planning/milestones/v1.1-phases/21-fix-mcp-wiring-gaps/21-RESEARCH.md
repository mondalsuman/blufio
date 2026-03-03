# Phase 21: Fix MCP Wiring Gaps - Research

**Researched:** 2026-03-03
**Domain:** Rust async systems wiring — connecting existing subsystems to the running server
**Confidence:** HIGH

## Summary

Phase 21 is a pure wiring phase: all building blocks (HealthTracker, PinStore, CostLedger, Prometheus recording helpers, trust zone guidance text) already exist in the codebase with comprehensive tests. The work is connecting these disconnected pieces at the correct integration points in serve.rs, manager.rs, external_tool.rs, and the context engine.

Five distinct wiring tasks exist: (1) spawn HealthTracker background task in serve.rs with actual ping checks, (2) instantiate PinStore in serve.rs and pass it to McpClientManager::connect_all() for hash verification during discovery, (3) add server_name field to CostLedger/CostRecord for per-server attribution, (4) create a TrustZoneProvider implementing ConditionalProvider to inject trust zone guidance into agent prompts, (5) add Prometheus metric recording calls at MCP client/server integration points.

**Primary recommendation:** Execute as a 2-wave plan — Wave 1 handles the foundational wiring (PinStore, HealthTracker, CostLedger migration, Prometheus metrics) and Wave 2 handles the trust zone context provider that depends on knowing which external tools are registered.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **Degradation Response**: Remove degraded server's tools from LLM context immediately. Log-only notifications (tracing::warn on degradation, tracing::info on recovery). Auto re-register tools when server recovers with PinStore hash verification during rediscovery. Make health check interval configurable via `mcp.health_check_interval_secs` in TOML config (default 60s).
- **Rug-Pull Handling**: Hard-block mutated tools — disable immediately on pin mismatch, remove from LLM context, log SECURITY-level warning. Block entire server when any tool has a pin mismatch. Operator must run `blufio mcp re-pin` to re-trust. Check pins on rediscovery only (initial connect + reconnection), not on every tool call. Use same SQLite database as main storage — add mcp_tool_pins table via migration.
- **Trust Zone Prompting**: System prompt section injection when external MCP tools are present. List specific external tool names. Factual/neutral tone. Operators can mark servers as trusted via `trusted = true` in `[[mcp.servers]]` TOML config. No trust zone warning for tools from trusted servers.
- **Per-Server Budget**: Attribution only — track which server each cost came from (add server_name to CostLedger), no enforcement. Use tool response byte size as cost proxy (~4 bytes per token). Add `blufio cost --by-server` CLI subcommand. No anomaly detection or threshold alerts.
- **Prometheus Metric Call Sites**: Wire existing recording helpers at appropriate points in MCP client and server code paths.

### Claude's Discretion
- Specific call site locations for Prometheus metric recording helpers

### Deferred Ideas (OUT OF SCOPE)
- None — discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| CLNT-06 | Connection lifecycle management (ping health checks, exponential backoff, graceful degradation) | HealthTracker + spawn_health_monitor() exist in health.rs. Need to: wire into serve.rs, make spawn_health_monitor do actual pings via RunningService, pass HealthTracker to manager for tool removal/re-registration |
| CLNT-07 | SHA-256 hash pinning of tool definitions at discovery (stored in SQLite) | PinStore exists in pin_store.rs with full CRUD + verify_or_store(). compute_tool_pin() exists in pin.rs. Need to: add V6 migration for mcp_tool_pins table, instantiate PinStore in serve.rs, pass to connect_all(), call verify_or_store() in discover_and_register() |
| CLNT-10 | External tools labeled as separate trust zone in prompt context | EXTERNAL_TOOL_TRUST_GUIDANCE const + external_tools_section_header() exist in manager.rs. Need to: create TrustZoneProvider implementing ConditionalProvider, add `trusted` field to McpServerEntry, register provider in serve.rs |
| CLNT-12 | Per-server budget tracking in unified cost ledger | CostLedger exists in ledger.rs with full SQLite persistence. Need to: add `server_name` field to CostRecord, add V6 migration column, record cost in ExternalTool::invoke(), add `by_server_total()` query, add CLI subcommand |
| INTG-04 | Prometheus metrics for MCP (connection count, tool response sizes, context utilization) | Recording helpers exist in recording.rs (record_mcp_connection, record_mcp_tool_response_size, set_mcp_active_connections, set_mcp_context_utilization). All defined but never called. Need to: add calls at connect, tool invoke, and context assembly points |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tokio-rusqlite | workspace | Async SQLite access for PinStore and CostLedger | Already used by both modules |
| tokio-util | workspace | CancellationToken for background task shutdown | Already used in health.rs |
| metrics-rs | workspace | Facade for Prometheus metric recording | Already used in recording.rs |
| ring | workspace | SHA-256 digest for pin computation | Already used in pin.rs |
| refinery | workspace | Embedded SQL migrations | Already used in blufio-storage |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| async_trait | workspace | Trait async support for ConditionalProvider | Trust zone provider implementation |
| tracing | workspace | Structured logging | All subsystems already use it |

### Alternatives Considered
None — all dependencies are already in the workspace. This is a wiring phase, not a library selection phase.

## Architecture Patterns

### Pattern 1: ConditionalProvider for Trust Zone Injection
**What:** Implement `ConditionalProvider` trait (like SkillProvider) that injects trust zone guidance text into the agent's prompt when external tools are present.
**When to use:** When external MCP tools are registered in the ToolRegistry.
**Example:**
```rust
pub struct TrustZoneProvider {
    registry: Arc<RwLock<ToolRegistry>>,
    trusted_servers: HashSet<String>,
}

#[async_trait]
impl ConditionalProvider for TrustZoneProvider {
    async fn provide_context(&self, _session_id: &str) -> Result<Vec<ProviderMessage>, BlufioError> {
        let registry = self.registry.read().await;
        let external = registry.external_tools(); // filter non-trusted
        if external.is_empty() {
            return Ok(vec![]);
        }
        // Build trust zone section with tool names
        let text = format_trust_zone_section(&external, &self.trusted_servers);
        Ok(vec![ProviderMessage { role: "user".into(), content: vec![ContentBlock::Text { text }] }])
    }
}
```

### Pattern 2: PinStore Integration in discover_and_register()
**What:** Pass PinStore to `discover_and_register()`, call `verify_or_store()` for each tool during discovery. On Mismatch, skip tool registration and block entire server.
**Where:** `manager.rs::discover_and_register()` currently has a TODO comment at the `_pin` variable (line 313).

### Pattern 3: Background Health Monitor with Actual Pings
**What:** Enhanced `spawn_health_monitor()` that takes `Arc<McpClientManager>` or server sessions, calls `session.ping()` on each tick, and uses HealthTracker to track state transitions.
**Where:** serve.rs after `McpClientManager::connect_all()` returns.

### Pattern 4: V6 Migration for mcp_tool_pins + server_name
**What:** Single SQL migration file adding the mcp_tool_pins table and server_name column to cost_ledger.
**Where:** `crates/blufio-storage/migrations/V6__mcp_wiring.sql`

### Anti-Patterns to Avoid
- **Creating new database connections**: PinStore.open() creates its own connection, but for the wiring we should use the existing shared database path from config, not create independent connections to different files.
- **Checking pins on every tool call**: The CONTEXT.md explicitly says pins are checked on rediscovery only (initial connect + reconnection), NOT on every invoke().
- **Injecting trust zone as system_blocks**: Use ConditionalProvider (user-role message in conditional zone), not static zone modification.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Async SQLite | Custom thread pool | tokio-rusqlite | Already in use, handles the single-writer limitation |
| Background task lifecycle | Custom spawn+cancel | CancellationToken pattern | Established pattern in health.rs and gateway |
| Metric recording | Raw metrics::counter!/gauge! calls | recording.rs helpers | Consistent naming, already registered |
| Pin hashing | Custom hash logic | compute_tool_pin() from pin.rs | Canonical JSON + SHA-256 already implemented |

## Common Pitfalls

### Pitfall 1: PinStore Using Wrong Database Path
**What goes wrong:** PinStore creates its own SQLite connection to a different file than the main database, leading to pins being stored in a file that doesn't get backed up or migrated.
**Why it happens:** PinStore::open() takes an arbitrary path. Easy to pass a different path than the main storage.
**How to avoid:** Use the same database path from BlufioConfig that CostLedger and StorageAdapter use. The mcp_tool_pins table should be created via the same refinery migration system.
**Warning signs:** Pin data disappears after restart, or `blufio mcp re-pin` has no effect.

### Pitfall 2: Holding RwLock During Async Operations
**What goes wrong:** Deadlock or contention if ToolRegistry write lock is held while performing async operations (ping, network calls).
**Why it happens:** The HealthTracker needs to remove/re-add tools, which requires write access to ToolRegistry.
**How to avoid:** Acquire lock, perform modification, drop lock before any async operations. Never hold a RwLock across `.await` points.
**Warning signs:** Server hangs on health check tick.

### Pitfall 3: Migration Ordering with cost_ledger server_name Column
**What goes wrong:** Adding a NOT NULL column to an existing table with data fails.
**Why it happens:** Existing rows have no server_name value.
**How to avoid:** Add the column as nullable (`server_name TEXT`) or with a default value. For cost attribution, nullable is correct since existing LLM cost records have no server association.
**Warning signs:** Migration failure on startup with existing data.

### Pitfall 4: Health Monitor Blocking on Failed Pings
**What goes wrong:** If a server is unreachable, the ping call blocks for the full timeout, delaying checks for other servers.
**Why it happens:** Sequential health checks within the tick handler.
**How to avoid:** Use `tokio::time::timeout` with a short timeout (5s) per ping, and check all servers concurrently within each tick.
**Warning signs:** Health checks take longer than the interval.

### Pitfall 5: Feature Gate Inconsistency
**What goes wrong:** Code compiles with `mcp-client` feature but fails without it.
**Why it happens:** New wiring code references types from blufio-mcp-client without proper `#[cfg(feature = "mcp-client")]` guards.
**How to avoid:** Wrap all MCP client wiring in `#[cfg(feature = "mcp-client")]` blocks, matching the existing pattern in serve.rs.
**Warning signs:** CI build failure on non-MCP feature combinations.

## Code Examples

### PinStore Integration in discover_and_register
```rust
// In manager.rs discover_and_register(), replace the TODO:
let pin_hash = compute_tool_pin(&tool_name, tool.description.as_deref(), &schema);
if let Some(pin_store) = pin_store {
    match pin_store.verify_or_store(&server.name, &tool_name, &pin_hash).await {
        Ok(PinVerification::FirstSeen) => { /* continue registration */ }
        Ok(PinVerification::Verified) => { /* continue registration */ }
        Ok(PinVerification::Mismatch { stored, computed }) => {
            error!(server = %server.name, tool = %tool_name,
                   "SECURITY: tool schema mutated - blocking entire server");
            return Err(BlufioError::Skill {
                message: format!("rug pull detected on server '{}'", server.name),
                source: None,
            });
        }
        Err(e) => {
            warn!(server = %server.name, error = %e, "pin verification failed, continuing");
        }
    }
}
```

### Health Monitor with Real Pings
```rust
pub fn spawn_health_monitor(
    sessions: HashMap<String, Arc<RunningService<RoleClient, ()>>>,
    tracker: Arc<RwLock<HealthTracker>>,
    tool_registry: Arc<RwLock<ToolRegistry>>,
    cancel: CancellationToken,
    interval_secs: u64,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        interval.tick().await; // skip initial
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    for (name, session) in &sessions {
                        let ping_result = tokio::time::timeout(
                            Duration::from_secs(5),
                            session.ping()
                        ).await;
                        let mut tracker = tracker.write().await;
                        match ping_result {
                            Ok(Ok(_)) => tracker.mark_healthy(name),
                            _ => { tracker.mark_degraded(name, "ping failed".into()); }
                        }
                    }
                }
                _ = cancel.cancelled() => break,
            }
        }
    })
}
```

### Prometheus Metric Call Sites
```rust
// In connect_server() on success:
blufio_prometheus::recording::record_mcp_connection(&server.transport);

// In ExternalTool::invoke() after response:
let response_bytes = content.len() as f64;
blufio_prometheus::recording::record_mcp_tool_response_size(response_bytes);

// After connect_all() in serve.rs:
blufio_prometheus::recording::set_mcp_active_connections(result.connected as f64);
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Tools just discovered | Tool pins stored in SQLite | Phase 18 (PinStore built) | Rug pull detection now possible but not wired |
| No health monitoring | HealthTracker built with backoff | Phase 18 (health.rs) | Health tracking exists but monitor doesn't ping |
| No cost attribution | CostLedger records costs | Phase 4 (ledger.rs) | Per-server attribution needs server_name field |
| Trust zone guidance text exists | Only as const string | Phase 18 (manager.rs) | Not yet injected into prompts |

## Open Questions

1. **Health monitor tool re-registration on recovery**
   - What we know: When a server recovers, its tools need to be re-discovered and re-registered with pin verification.
   - What's unclear: Whether `list_all_tools()` works on an existing session after ping recovery, or if the session needs to be re-established.
   - Recommendation: Try ping first. If ping succeeds on a previously-degraded server, call `list_all_tools()` on the existing session. If that fails, the session is stale and a full reconnect is needed (out of scope for this wiring phase — log and skip re-registration).

2. **cost_ledger server_name migration vs. new table**
   - What we know: The CONTEXT.md says "add server_name to CostLedger" for attribution.
   - What's unclear: Whether to add a column to the existing cost_ledger table or create a separate mcp_cost_ledger table.
   - Recommendation: Add nullable `server_name TEXT` column to existing cost_ledger table via V6 migration. This is simpler and allows unified cost queries. LLM cost records will have NULL server_name; external tool costs will have the server name populated.

## Sources

### Primary (HIGH confidence)
- Codebase inspection: health.rs, pin_store.rs, pin.rs, ledger.rs, recording.rs, manager.rs, serve.rs, external_tool.rs, conditional.rs, provider.rs
- All findings based on direct code reading — no external library research needed for this wiring phase

### Secondary (MEDIUM confidence)
- N/A — all code exists in-tree

### Tertiary (LOW confidence)
- rmcp `session.ping()` API — inferred from rmcp RunningService trait but not verified via Context7 (the function exists on `RunningService<RoleClient, ()>` based on MCP spec ping/pong requirement)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - all libraries already in workspace
- Architecture: HIGH - patterns follow existing code conventions (ConditionalProvider, CancellationToken, migrations)
- Pitfalls: HIGH - identified from direct code reading of existing patterns

**Research date:** 2026-03-03
**Valid until:** 2026-04-03 (stable — internal wiring only)
