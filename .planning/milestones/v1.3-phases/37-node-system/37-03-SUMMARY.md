---
phase: 37-node-system
plan: 03
subsystem: node
tags: [websocket, approval, dashmap, first-wins, timeout, tokio]

# Dependency graph
requires:
  - phase: 37-node-system
    provides: NodeStore (save_approval, resolve_approval), ConnectionManager (broadcast, send_to), NodeMessage types, NodeApprovalConfig
provides:
  - ApprovalRouter with broadcast, first-wins, and timeout-then-deny
  - ApprovalOutcome for async notification of approval results
  - ConnectionManager integration for approval message handling
affects: [serve, gateway-approval-integration]

# Tech tracking
tech-stack:
  added: []
  patterns: [DashMap atomic remove for first-wins, oneshot channel for async result delivery, tokio spawn for timeout tasks]

key-files:
  created: [crates/blufio-node/src/approval.rs]
  modified: [crates/blufio-node/src/lib.rs, crates/blufio-node/src/connection.rs]

key-decisions:
  - "First-wins via DashMap::remove (atomic remove guarantees only one responder wins)"
  - "Timeout task per approval request using tokio::spawn with tokio::time::sleep"
  - "ApprovalOutcome delivered via oneshot channel to original requester"
  - "ConnectionManager gets optional approval_router via setter (avoids circular construction)"

patterns-established:
  - "First-wins pattern: DashMap::remove for atomic claim, losers get notification"
  - "Timeout-then-deny: spawned task auto-resolves after configurable seconds"

requirements-completed: [NODE-05]

# Metrics
duration: 2min
completed: 2026-03-07
---

# Phase 37 Plan 03: Approval Routing Summary

**ApprovalRouter with first-wins broadcast semantics, DashMap atomic resolution, and configurable timeout-then-deny**

## Performance

- **Duration:** 2 min
- **Started:** 2026-03-07T11:24:08Z
- **Completed:** 2026-03-07T11:26:28Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- ApprovalRouter broadcasts approval requests to all connected operator devices via ConnectionManager
- First-wins semantics via DashMap atomic remove ensures only first responder wins
- Timeout task auto-denies pending approvals after configurable timeout (default 5 min)
- Late responders receive ApprovalHandled notification
- ConnectionManager wired with approval message handling and optional ApprovalRouter setter

## Task Commits

Each task was committed atomically:

1. **Task 1: Implement ApprovalRouter with broadcast, first-wins, and timeout** - `ee0f987` (feat)
2. **Task 2: Wire approval module into lib.rs and integrate with connection message handler** - `8af0b41` (feat)

## Files Created/Modified
- `crates/blufio-node/src/approval.rs` - ApprovalRouter with broadcast, first-wins resolution, timeout-then-deny, and ApprovalOutcome type
- `crates/blufio-node/src/lib.rs` - Module declaration and re-exports for ApprovalRouter, ApprovalOutcome
- `crates/blufio-node/src/connection.rs` - Approval router field, setter, and ApprovalResponse/ApprovalHandled message handling

## Decisions Made
- First-wins via DashMap::remove (atomic remove guarantees only one responder wins the race)
- Timeout task per approval request using tokio::spawn (each request gets its own timeout)
- ApprovalOutcome delivered via oneshot channel to original requester (async notification)
- ConnectionManager gets optional approval_router via setter to avoid circular construction dependency

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Approval routing complete, ready for serve.rs integration where ApprovalRouter is instantiated with ConnectionManager and NodeStore
- All node system plans (37-01, 37-02, 37-03) complete

---
*Phase: 37-node-system*
*Completed: 2026-03-07*
