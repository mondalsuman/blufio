---
phase: 03-agent-loop-telegram
plan: 04
subsystem: agent-loop
tags: [graceful-shutdown, session-drain, polling, tokio]

requires:
  - phase: 03-03
    provides: "drain_sessions() stub and SessionActor with state() method"
provides:
  - Poll-based drain_sessions() that monitors SessionState transitions with 100ms interval and 30s timeout
  - Per-session diagnostic logging on shutdown timeout for production debugging
affects: [agent-runtime, graceful-shutdown]

tech-stack:
  added: []
  patterns: [poll-based-drain, deadline-bounded-polling]

key-files:
  created: []
  modified:
    - crates/blufio-agent/src/shutdown.rs

key-decisions:
  - "100ms poll interval chosen for fast exit (~100ms after last session finishes) with negligible CPU (max 300 polls over 30s)"
  - "Both Idle and Draining states treated as 'done' -- Draining sessions won't receive new messages"
  - "Per-session logging on timeout includes session_key, session_id, and current state for debugging"

patterns-established:
  - "Poll-based drain with deadline: poll at fixed interval, check condition, respect timeout upper bound"
  - "Active = not Idle AND not Draining; all other states (Responding, Processing, Receiving, ToolExecuting) are waited on"

requirements-completed: [CORE-03]

duration: 2min
completed: 2026-03-01
---

# Plan 03-04: Gap Closure - drain_sessions Stub Summary

**Poll-based drain_sessions() replacing fixed sleep with 100ms session state polling bounded by 30s timeout**

## Performance

- **Duration:** 2 min
- **Started:** 2026-03-01T20:43:53Z
- **Completed:** 2026-03-01T20:46:48Z
- **Tasks:** 1
- **Files modified:** 1

## Accomplishments

- Replaced fixed `tokio::time::sleep(timeout)` stub with poll-based session state monitoring
- drain_sessions() now returns immediately when all sessions are Idle (no 30-second wait)
- Returns as soon as all sessions transition to Idle or Draining state during the polling window
- Per-session diagnostic logging on timeout (session_key, session_id, state) for production debugging
- Checks all active states (Responding, Processing, Receiving, ToolExecuting), not just Responding

## Task Commits

Each task was committed atomically:

1. **Task 1: Replace drain_sessions stub with poll-based session monitoring** - `ad3d3d2` (feat)

## Files Created/Modified

- `crates/blufio-agent/src/shutdown.rs` - Replaced drain_sessions() fixed sleep with 100ms polling loop bounded by deadline

## Decisions Made

- **100ms poll interval:** Fast enough to exit within ~100ms of last session finishing, negligible CPU at max 300 polls over 30s timeout
- **Idle + Draining = done:** Sessions marked Draining via set_draining() are considered finished (won't receive new messages)
- **Per-session timeout logging:** Each undrained session logged individually with key, ID, and state for production debugging
- **Signature unchanged:** `&HashMap<String, SessionActor>` stays the same -- no breaking change at the call site in AgentLoop::run()

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- CORE-03 (graceful shutdown drains active sessions) is now fully satisfied
- Phase 3 gap is closed -- drain_sessions() correctly monitors session state transitions
- All 44 blufio-agent tests pass, full workspace compilation clean

## Self-Check: PASSED

- FOUND: crates/blufio-agent/src/shutdown.rs
- FOUND: 03-04-SUMMARY.md
- FOUND: ad3d3d2 (task 1 commit)

---
*Plan: 03-04-gap-closure-drain-sessions*
*Completed: 2026-03-01*
