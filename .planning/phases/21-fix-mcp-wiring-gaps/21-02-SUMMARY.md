---
phase: 21-fix-mcp-wiring-gaps
plan: 02
subsystem: mcp-client
tags: [health-check, ping, rmcp, background-task, config]

# Dependency graph
requires:
  - phase: 18-mcp-client
    provides: "HealthTracker struct, McpClientManager, connected_session_map()"
provides:
  - "Real ping-based health monitoring for external MCP servers"
  - "Configurable health_check_interval_secs in McpConfig"
  - "HealthTracker spawned in serve.rs with cancellation support"
affects: [21-03-PLAN, 21-04-PLAN]

# Tech tracking
tech-stack:
  added: []
  patterns: ["rmcp ClientRequest::PingRequest for session health probing", "Arc<RwLock<HealthTracker>> shared state for async health monitoring"]

key-files:
  created: []
  modified:
    - "crates/blufio-mcp-client/src/health.rs"
    - "crates/blufio-config/src/model.rs"
    - "crates/blufio/src/serve.rs"

key-decisions:
  - "5-second timeout per ping request to avoid blocking the health loop"
  - "Health monitor spawned after cancel token to enable graceful shutdown via child_token()"
  - "Sessions extracted from McpClientManager via connected_session_map() before health spawn"

patterns-established:
  - "rmcp ping: session.send_request(ClientRequest::PingRequest(Default::default()))"
  - "Health monitor lifecycle: extract sessions -> create tracker -> spawn after cancel token"

requirements-completed: [CLNT-06]

# Metrics
duration: 20min
completed: 2026-03-03
---

# Phase 21 Plan 02: Health Monitor Summary

**Real ping-based MCP health monitoring with configurable interval and HealthTracker wiring in serve.rs**

## Performance

- **Duration:** 20 min
- **Started:** 2026-03-03T13:53:42Z
- **Completed:** 2026-03-03T14:14:07Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- Enhanced spawn_health_monitor to perform actual rmcp ping checks with 5-second timeout per server
- Added health_check_interval_secs config field to McpConfig (default 60 seconds)
- Wired HealthTracker spawn into serve.rs after cancel token creation for graceful shutdown
- State transitions use existing mark_healthy (info) and mark_degraded (warn) tracing

## Task Commits

Each task was committed atomically:

1. **Task 1: Add health_check_interval_secs config and enhance health monitor** - `ec7a551` (feat)
2. **Task 2: Wire HealthTracker and PinStore instantiation into serve.rs** - `97cae69` (feat, from 21-01 parallel execution)

**Plan metadata:** (pending)

## Files Created/Modified
- `crates/blufio-mcp-client/src/health.rs` - Enhanced spawn_health_monitor with real ping checks via rmcp ClientRequest::PingRequest
- `crates/blufio-config/src/model.rs` - Added health_check_interval_secs field with default 60s and serde support
- `crates/blufio/src/serve.rs` - Wired HealthTracker spawn with connected_session_map and child cancel token

## Decisions Made
- Used 5-second timeout for individual ping requests to balance responsiveness vs. false positives
- Health monitor spawned after cancel token is created (not inside MCP client block) to ensure graceful shutdown support
- Connected sessions extracted from McpClientManager as a separate HashMap before spawning monitor task

## Deviations from Plan

### Notes

Task 2 serve.rs changes were partially pre-committed by the parallel 21-01 plan executor (commit `97cae69`). The 21-01 executor already wired PinStore, connected_session_map(), and the health monitor spawn into serve.rs as part of its broader wiring task. This plan's Task 1 provided the enhanced health monitor function that the serve.rs wiring calls.

**Total deviations:** 0 auto-fixed
**Impact on plan:** None. The serve.rs wiring was completed by the 21-01 parallel executor, which is equivalent to plan execution. All success criteria met.

## Issues Encountered
None - plan executed successfully. The serve.rs wiring being pre-committed by 21-01 was a benign overlap, not a conflict.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Health monitoring is fully operational for connected MCP servers
- Ready for 21-03 (unregister_by_namespace) and 21-04 (budget per-server tracking)
- HealthTracker state can be queried to determine which servers are degraded

## Self-Check: PASSED

- All 3 key files exist on disk
- Both commits (ec7a551, 97cae69) found in git log
- health_check_interval_secs present in model.rs and serve.rs
- spawn_health_monitor and PingRequest present in health.rs
- spawn_health_monitor and connected_session_map present in serve.rs

---
*Phase: 21-fix-mcp-wiring-gaps*
*Completed: 2026-03-03*
