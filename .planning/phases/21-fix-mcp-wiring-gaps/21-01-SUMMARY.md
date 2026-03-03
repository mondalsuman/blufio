---
phase: 21-fix-mcp-wiring-gaps
plan: 01
subsystem: mcp-client
tags: [pinstore, rug-pull, cost-attribution, sqlite, migration]

# Dependency graph
requires:
  - phase: 18-mcp-client
    provides: "PinStore, compute_tool_pin, McpClientManager, ExternalTool"
provides:
  - "V6 migration creating mcp_tool_pins table and cost_ledger server_name column"
  - "CostRecord server_name field with builder and by_server_total() query"
  - "PinStore wired into discover_and_register() for rug-pull detection"
  - "PinStore re-exported from blufio-mcp-client crate"
affects: [21-02, 21-03, 21-04, cost-reporting, mcp-security]

# Tech tracking
tech-stack:
  added: []
  patterns: ["builder pattern with_server_name() on CostRecord", "Optional PinStore parameter in connect_all for graceful degradation"]

key-files:
  created:
    - "crates/blufio-storage/migrations/V6__mcp_wiring.sql"
  modified:
    - "crates/blufio-cost/src/ledger.rs"
    - "crates/blufio-cost/src/budget.rs"
    - "crates/blufio-mcp-client/src/manager.rs"
    - "crates/blufio-mcp-client/src/lib.rs"
    - "crates/blufio/src/serve.rs"

key-decisions:
  - "PinStore opened from database_path in serve.rs with graceful fallback on failure"
  - "connected_session_map() added to McpClientManager for health monitoring (CLNT-06)"

patterns-established:
  - "Optional PinStore: connect_all accepts Option<&PinStore> so callers without pin storage (tests) pass None"
  - "Server-level blocking: on any tool pin mismatch, entire server is rejected (no partial registration)"

requirements-completed: [CLNT-07, CLNT-12]

# Metrics
duration: 18min
completed: 2026-03-03
---

# Phase 21 Plan 01: Wire PinStore + Server Cost Attribution Summary

**PinStore integrated into tool discovery for rug-pull detection, V6 migration with mcp_tool_pins table and server_name cost attribution column**

## Performance

- **Duration:** 18 min
- **Started:** 2026-03-03T13:53:32Z
- **Completed:** 2026-03-03T14:11:32Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments
- V6 migration creates mcp_tool_pins table (composite PK on server_name, tool_name) and adds server_name column to cost_ledger with index
- CostRecord extended with server_name: Option<String>, builder method, and by_server_total() grouped query
- PinStore::verify_or_store() called for every tool during MCP discovery; Mismatch blocks entire server
- serve.rs opens PinStore from database_path and passes to connect_all with graceful fallback

## Task Commits

Each task was committed atomically:

1. **Task 1: Add V6 migration and CostRecord server_name field** - `fbb5146` (feat)
2. **Task 2: Wire PinStore into McpClientManager tool discovery** - `97cae69` (feat)

## Files Created/Modified
- `crates/blufio-storage/migrations/V6__mcp_wiring.sql` - V6 migration: mcp_tool_pins table + cost_ledger server_name column + index
- `crates/blufio-cost/src/ledger.rs` - CostRecord server_name field, with_server_name() builder, by_server_total() query, updated record() INSERT, tests
- `crates/blufio-cost/src/budget.rs` - Updated test schema and sample record to include server_name column
- `crates/blufio-mcp-client/src/manager.rs` - PinStore+PinVerification imports, connect_all pin_store param, verify_or_store integration, connected_session_map()
- `crates/blufio-mcp-client/src/lib.rs` - Re-export PinStore from crate root
- `crates/blufio/src/serve.rs` - Open PinStore from database_path, pass to connect_all

## Decisions Made
- PinStore opened from config.storage.database_path in serve.rs with warn-level fallback if open fails (graceful degradation -- agent still works without pin verification)
- connected_session_map() added to McpClientManager for health monitoring use case (CLNT-06)

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- PinStore is wired and active during tool discovery
- Per-server cost attribution is ready for use by cost reporting features
- Remaining plans in phase 21 can proceed (serve.rs PinStore wiring, integration tests, etc.)

## Self-Check: PASSED

All files verified present. Both commit hashes (fbb5146, 97cae69) confirmed in git log.

---
*Phase: 21-fix-mcp-wiring-gaps*
*Completed: 2026-03-03*
