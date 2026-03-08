---
phase: 44-node-approval-wiring
plan: 01
subsystem: node
tags: [event-bus, approval-routing, websocket, oncelock, bus-event]

# Dependency graph
requires:
  - phase: 37-node-fleet
    provides: ApprovalRouter with handle_response(), ConnectionManager with approval_router field
  - phase: 40-event-bus
    provides: Global EventBus with subscribe_reliable()
provides:
  - BusEvent::event_type_string() method mapping all 15 leaf variants to dot-separated strings
  - ConnectionManager::set_approval_router(&self) via OnceLock (Arc-compatible)
  - ApprovalResponse forwarding to ApprovalRouter::handle_response() in reconnect_with_backoff
affects: [44-02-PLAN, serve.rs wiring]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "OnceLock for set-once fields on Arc-wrapped structs (replaces Option + &mut self setter)"
    - "BusEvent::event_type_string() for domain.action string mapping"

key-files:
  created: []
  modified:
    - crates/blufio-bus/src/events.rs
    - crates/blufio-node/src/connection.rs

key-decisions:
  - "OnceLock<Arc<ApprovalRouter>> replaces Option<Arc<>> for Arc-compatible set_approval_router(&self)"
  - "event_type_string returns &'static str (zero allocation) since all values are string literals"

patterns-established:
  - "BusEvent::event_type_string() exhaustive match for compile-time safety on future variants"
  - "OnceLock pattern for Arc-compatible late initialization of optional dependencies"

requirements-completed: [NODE-05]

# Metrics
duration: 4min
completed: 2026-03-08
---

# Phase 44 Plan 01: Node Approval Wiring Summary

**BusEvent::event_type_string() mapping all 15 variants to dot-separated strings, plus ConnectionManager ApprovalResponse forwarding via OnceLock-based router**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-08T20:13:24Z
- **Completed:** 2026-03-08T20:18:17Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- BusEvent::event_type_string() covers all 15 leaf variants with exhaustive match returning &'static str
- ConnectionManager::set_approval_router() now takes &self via OnceLock (Arc-compatible, no &mut self)
- reconnect_with_backoff accepts and uses approval_router parameter for ApprovalResponse forwarding
- ApprovalResponse messages forwarded to router.handle_response() with debug logging for first-wins resolution

## Task Commits

Each task was committed atomically:

1. **Task 1: Add BusEvent::event_type_string() method** - `0ed848c` (test: failing test) + `e7f293e` (feat: implementation)
2. **Task 2: Wire ApprovalResponse forwarding in ConnectionManager** - `716f4b7` (feat)

_Note: Task 1 used TDD -- separate test and implementation commits._

## Files Created/Modified
- `crates/blufio-bus/src/events.rs` - Added event_type_string() method with exhaustive match on all 15 BusEvent leaf variants, plus comprehensive test
- `crates/blufio-node/src/connection.rs` - Changed approval_router to OnceLock, set_approval_router to &self, added approval_router param to reconnect_with_backoff, wired ApprovalResponse forwarding

## Decisions Made
- Used OnceLock<Arc<ApprovalRouter>> instead of Option<Arc<>> to make set_approval_router take &self (Arc-compatible) without interior mutability overhead of Mutex/RwLock
- event_type_string() returns &'static str (not String) to avoid allocation since all values are compile-time string literals

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Plan 02 can now wire ApprovalRouter creation and EventBus subscription in serve.rs
- set_approval_router(&self) is ready to be called on Arc<ConnectionManager>
- reconnect_with_backoff will receive the approval_router from reconnect_all after Plan 02 wiring

## Self-Check: PASSED

All files exist, all commits verified.

---
*Phase: 44-node-approval-wiring*
*Completed: 2026-03-08*
