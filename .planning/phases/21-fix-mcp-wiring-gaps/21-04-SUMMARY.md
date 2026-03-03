---
phase: 21-fix-mcp-wiring-gaps
plan: 04
subsystem: observability
tags: [prometheus, metrics, mcp, cost-attribution]

# Dependency graph
requires:
  - phase: 21-01-cost-schema
    provides: "CostRecord with_server_name builder and cost ledger with server_name column"
  - phase: 21-02-health-monitor
    provides: "health_check_interval_secs config field on McpConfig"
  - phase: 21-03-trust-zone
    provides: "TrustZoneProvider, trusted config field, PinStore in connect_all"
provides:
  - "Prometheus MCP metrics wired at connection and invocation points"
  - "ExternalTool.server_name field with public getter for cost attribution"
  - "record_mcp_connection() called on successful MCP server connect"
  - "set_mcp_active_connections() called after connect_all() in serve.rs"
  - "record_mcp_tool_response_size() called after each external tool invocation"
affects: [context-engine, cost-attribution]

# Tech tracking
tech-stack:
  added: [blufio-prometheus dependency in blufio-mcp-client]
  patterns: [metric recording at integration boundaries]

key-files:
  created: []
  modified:
    - crates/blufio-mcp-client/Cargo.toml
    - crates/blufio-mcp-client/src/manager.rs
    - crates/blufio-mcp-client/src/external_tool.rs
    - crates/blufio/src/serve.rs
    - crates/blufio-prometheus/src/recording.rs

key-decisions:
  - "Response size metric recorded before truncation to capture true MCP response size"
  - "set_mcp_context_utilization deferred to context engine integration (separate from MCP wiring)"

patterns-established:
  - "Metric recording at integration boundaries: record at point of event, not in caller"

requirements-completed: [INTG-04, CLNT-12]

# Metrics
duration: 8min
completed: 2026-03-03
---

# Phase 21 Plan 04: Prometheus MCP Metric Wiring Summary

**Wired Prometheus MCP metric recording helpers at connection, invocation, and active-connection points, plus ExternalTool server_name for cost attribution**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-03T14:17:42Z
- **Completed:** 2026-03-03T14:25:58Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments
- Wired record_mcp_connection() in manager.rs on successful server connect
- Wired record_mcp_tool_response_size() in external_tool.rs invoke() after response extraction
- Wired set_mcp_active_connections() in serve.rs after connect_all() returns
- Added server_name field and public getter to ExternalTool for cost attribution
- Documented all MCP metric call sites in recording.rs
- Full workspace compiles with all features and all tests pass (63 mcp-client, 28 cost, 4 prometheus)

## Task Commits

Each task was committed atomically:

1. **Task 1: Add Prometheus metric calls to MCP connection and tool invocation paths** - `18cd576` (feat)
2. **Task 2: Verify all metrics emit non-zero values and add integration note** - `c84c04b` (feat)

**Plan metadata:** `66de978` (docs: complete plan)

## Files Created/Modified
- `crates/blufio-mcp-client/Cargo.toml` - Added blufio-prometheus dependency
- `crates/blufio-mcp-client/src/manager.rs` - record_mcp_connection() call after successful server connect
- `crates/blufio-mcp-client/src/external_tool.rs` - server_name field, getter, record_mcp_tool_response_size() in invoke()
- `crates/blufio/src/serve.rs` - set_mcp_active_connections() after connect_all()
- `crates/blufio-prometheus/src/recording.rs` - Call-site documentation for all MCP metrics
- `crates/blufio-mcp-server/src/handler.rs` - Fixed missing health_check_interval_secs in test helper
- `crates/blufio/tests/e2e_mcp_client.rs` - Fixed missing pin_store argument in e2e tests

## Decisions Made
- Response size metric recorded before truncation to capture true MCP response size (not truncated size)
- set_mcp_context_utilization() deferred to context engine integration -- requires token counting during assembly, separate from MCP wiring scope

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed missing health_check_interval_secs in mcp-server test helper**
- **Found during:** Task 2 (full workspace check)
- **Issue:** McpConfig test helper in handler.rs was missing health_check_interval_secs field added in plan 21-02
- **Fix:** Added health_check_interval_secs: 60 to default_config() test helper
- **Files modified:** crates/blufio-mcp-server/src/handler.rs
- **Verification:** cargo check --all-targets --all-features succeeds
- **Committed in:** c84c04b (Task 2 commit)

**2. [Rule 3 - Blocking] Fixed e2e_mcp_client tests missing pin_store argument**
- **Found during:** Task 2 (full workspace check)
- **Issue:** e2e tests calling connect_all() with 2 args instead of 3 (pin_store added in plan 21-03)
- **Fix:** Added None as pin_store argument to 4 call sites
- **Files modified:** crates/blufio/tests/e2e_mcp_client.rs
- **Verification:** cargo check --all-targets --all-features succeeds
- **Committed in:** c84c04b (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Both fixes for pre-existing compilation issues from earlier plans in this phase. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All 4 plans in Phase 21 (Fix MCP Wiring Gaps) are now complete
- MCP wiring gaps are closed: cost schema, health monitor, trust zones, and Prometheus metrics
- Three of four MCP Prometheus metrics are wired (context utilization deferred to context engine integration)
- Ready for Phase 22 or any subsequent work

## Self-Check: PASSED

All files exist, all commits verified, all metric call sites confirmed in source.

---
*Phase: 21-fix-mcp-wiring-gaps*
*Completed: 2026-03-03*
