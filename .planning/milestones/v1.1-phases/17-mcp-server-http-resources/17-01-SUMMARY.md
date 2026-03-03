---
phase: 17-mcp-server-http-resources
plan: 01
subsystem: mcp
tags: [rmcp, axum, cors, bearer-auth, streamable-http, tower-http]

# Dependency graph
requires:
  - phase: 16-mcp-server-stdio
    provides: BlufioMcpHandler, serve_stdio, bridge.rs, McpConfig
provides:
  - MCP HTTP transport module (StreamableHttpService wrapper)
  - MCP-specific bearer token auth middleware
  - Restricted CORS layer for /mcp endpoints
  - McpConfig auth_token and cors_origins fields
  - Config validation for MCP auth_token
  - Gateway /mcp route mounting
affects: [17-02 (resources need handler HTTP access), 17-03 (prompts), 17-04 (notifications), 18-mcp-client]

# Tech tracking
tech-stack:
  added: [transport-streamable-http-server rmcp feature, tower-http cors on mcp-server, http crate]
  patterns: [MCP-specific auth isolation from gateway auth, CORS scoped to route group, StreamableHttpService factory pattern]

key-files:
  created:
    - crates/blufio-mcp-server/src/auth.rs
    - crates/blufio-mcp-server/src/transport.rs
  modified:
    - crates/blufio-config/src/model.rs
    - crates/blufio-config/src/validation.rs
    - crates/blufio-mcp-server/Cargo.toml
    - crates/blufio-mcp-server/src/lib.rs
    - crates/blufio-mcp-server/src/handler.rs
    - crates/blufio-gateway/src/server.rs
    - crates/blufio-gateway/src/lib.rs
    - crates/blufio/src/serve.rs

key-decisions:
  - "StreamableHttpService uses factory closure pattern (Fn() -> Result<S>) for per-session handler cloning"
  - "MCP router nested at /mcp BEFORE permissive CorsLayer so MCP routes use restricted CORS"
  - "GatewayChannel.set_mcp_router() method for pre-connect MCP injection"
  - "Signal handler installation moved earlier in serve.rs for MCP cancellation token availability"

patterns-established:
  - "MCP auth middleware: separate McpAuthConfig from gateway AuthConfig for security isolation"
  - "Route-scoped CORS: build_mcp_cors with explicit AllowOrigin list, not permissive"
  - "Factory pattern: Arc<handler> cloned in closure for StreamableHttpService"

requirements-completed: [SRVR-06, SRVR-07, SRVR-16]

# Metrics
duration: 33min
completed: 2026-03-02
---

# Phase 17 Plan 01: MCP HTTP Transport Summary

**Streamable HTTP transport at /mcp with MCP-specific bearer auth and restricted CORS using rmcp StreamableHttpService**

## Performance

- **Duration:** 33 min
- **Started:** 2026-03-02T19:23:16Z
- **Completed:** 2026-03-02T19:56:26Z
- **Tasks:** 2
- **Files modified:** 11

## Accomplishments
- McpConfig extended with auth_token and cors_origins fields, TOML parsing validated
- Config validation rejects mcp.enabled=true without auth_token (fail-closed)
- MCP auth middleware (bearer-only, isolated from gateway auth) with full test coverage
- CORS layer restricts /mcp to configured origins only (empty = reject all)
- StreamableHttpService router builder with SSE keep-alive, stateful sessions
- Gateway /mcp route mounting with MCP-specific CORS before permissive fallback
- serve.rs wires MCP HTTP transport with auth token redaction and cancellation

## Task Commits

Each task was committed atomically:

1. **Task 1: Extend McpConfig and add HTTP transport module** - `cfee1f4` (feat, TDD)
2. **Task 2: Mount MCP HTTP routes on gateway and wire serve.rs** - `160ae25` (feat)

## Files Created/Modified
- `crates/blufio-mcp-server/src/auth.rs` - MCP bearer token auth middleware (McpAuthConfig, mcp_auth_middleware)
- `crates/blufio-mcp-server/src/transport.rs` - HTTP transport (build_mcp_cors, mcp_service_config, build_mcp_router)
- `crates/blufio-config/src/model.rs` - McpConfig auth_token and cors_origins fields
- `crates/blufio-config/src/validation.rs` - MCP auth_token validation check
- `crates/blufio-mcp-server/Cargo.toml` - transport-streamable-http-server feature, axum/tower-http deps
- `crates/blufio-mcp-server/src/lib.rs` - pub mod auth, pub mod transport
- `crates/blufio-mcp-server/src/handler.rs` - Updated test helpers for new McpConfig fields
- `crates/blufio-gateway/src/server.rs` - start_server accepts optional mcp_router
- `crates/blufio-gateway/src/lib.rs` - GatewayChannel.set_mcp_router() method
- `crates/blufio/src/serve.rs` - MCP HTTP wiring with auth redaction

## Decisions Made
- StreamableHttpService requires a factory closure `Fn() -> Result<S>` for per-session handler creation; used `Arc<BlufioMcpHandler>` with clone-in-closure pattern
- MCP router nested at /mcp BEFORE the permissive CorsLayer so that MCP routes get their own restricted CORS
- Added `set_mcp_router` method to GatewayChannel rather than modifying GatewayChannelConfig (avoids Router in Clone type)
- Moved signal handler installation earlier in serve.rs so CancellationToken is available for MCP service config
- Used `LocalSessionManager::default()` for in-memory session management (stateful mode)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] StreamableHttpService API requires factory closure**
- **Found during:** Task 1 (transport module implementation)
- **Issue:** Plan specified `StreamableHttpService::new(handler, config)` but rmcp 0.17 API requires `StreamableHttpService::new(factory_closure, session_manager, config)` -- 3 args with factory pattern
- **Fix:** Used `Arc<BlufioMcpHandler>` with `move || Ok(handler.clone())` factory closure and `Arc<LocalSessionManager>` for session management
- **Files modified:** crates/blufio-mcp-server/src/transport.rs
- **Verification:** cargo build succeeds, transport tests pass
- **Committed in:** cfee1f4

**2. [Rule 3 - Blocking] StreamableHttpServerConfig has json_response field**
- **Found during:** Task 1 (transport module implementation)
- **Issue:** rmcp 0.17 `StreamableHttpServerConfig` has a `json_response` field not mentioned in plan/research
- **Fix:** Set `json_response: false` (use SSE framing, standard behavior for stateful mode)
- **Files modified:** crates/blufio-mcp-server/src/transport.rs
- **Verification:** cargo build succeeds
- **Committed in:** cfee1f4

**3. [Rule 3 - Blocking] Signal handler needed before gateway MCP wiring**
- **Found during:** Task 2 (serve.rs wiring)
- **Issue:** MCP service config requires CancellationToken from signal handler, but handler was installed after gateway section
- **Fix:** Moved `shutdown::install_signal_handler()` earlier in serve.rs (before prometheus/gateway section)
- **Files modified:** crates/blufio/src/serve.rs
- **Verification:** cargo build with and without mcp-server feature succeeds
- **Committed in:** 160ae25

---

**Total deviations:** 3 auto-fixed (3 blocking issues)
**Impact on plan:** All auto-fixes were necessary to match the actual rmcp 0.17 API and correct initialization ordering. No scope creep.

## Issues Encountered
- Initial build failed with `chacha20 ^0.10.0` version resolution error when enabling `transport-streamable-http-server` -- resolved by running `cargo update -p rmcp` to refresh the lock file

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- MCP HTTP transport is mounted and functional at /mcp
- BlufioMcpHandler ready to be extended with resources and prompts (Plan 02/03)
- Auth and CORS are operational; future plans add capabilities without changing transport
- build_mcp_router pattern established for future extensions

## Self-Check: PASSED

All created files verified to exist. All commit hashes verified in git log.

---
*Phase: 17-mcp-server-http-resources*
*Completed: 2026-03-02*
