---
phase: 43-wire-eventbus-publishers
plan: 01
subsystem: api
tags: [eventbus, webhooks, wasm, agent-loop, event-publishing]

# Dependency graph
requires:
  - phase: 40-global-eventbus
    provides: EventBus, BusEvent, ChannelEvent, SkillEvent types
  - phase: 42-wire-gateway-stores
    provides: Webhook delivery subscribing to EventBus events
provides:
  - AgentLoop publishes ChannelEvent::MessageSent after final response delivery
  - WasmSkillRuntime publishes SkillEvent::Invoked/Completed around skill execution
  - SkillInvocation carries session_id for event correlation
  - serve.rs wires EventBus into AgentLoop
affects: [webhook-delivery, skill-lifecycle, event-observability]

# Tech tracking
tech-stack:
  added: [blufio-bus dependency in blufio-skill]
  patterns: [setter-based EventBus injection, fire-and-forget publish]

key-files:
  created: []
  modified:
    - crates/blufio-core/src/types.rs
    - crates/blufio-skill/Cargo.toml
    - crates/blufio-skill/src/sandbox.rs
    - crates/blufio-agent/src/lib.rs
    - crates/blufio/src/serve.rs

key-decisions:
  - "EventBus.publish() is fire-and-forget (returns ()), not Result -- publish calls use simple await without error handling"
  - "WasmSkillRuntime not created in production serve.rs yet -- set_event_bus ready for wiring when skill loading is implemented"
  - "MessageSent published after both send and edit-in-place paths complete, before persist_response"

patterns-established:
  - "EventBus publish pattern: if let Some(ref bus) = self.event_bus { bus.publish(...).await; }"
  - "Setter injection on subsystem structs: set_event_bus(&mut self, bus: Arc<EventBus>)"

requirements-completed: [API-16]

# Metrics
duration: 7min
completed: 2026-03-08
---

# Phase 43 Plan 01: Wire EventBus Publishers Summary

**EventBus wired into AgentLoop (MessageSent) and WasmSkillRuntime (Invoked/Completed) with fire-and-forget publish, closing 2 event publisher gaps for webhook delivery**

## Performance

- **Duration:** 7 min
- **Started:** 2026-03-08T12:24:53Z
- **Completed:** 2026-03-08T12:31:53Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- AgentLoop publishes ChannelEvent::MessageSent after final response delivery (send or edit path)
- WasmSkillRuntime publishes SkillEvent::Invoked before and SkillEvent::Completed after execution
- SkillInvocation extended with session_id: Option<String> for event correlation (18 callers updated)
- serve.rs wires EventBus into AgentLoop; WasmSkillRuntime setter ready for future wiring
- All 192+ workspace tests pass, clean build

## Task Commits

Each task was committed atomically:

1. **Task 1: Add EventBus to AgentLoop, WasmSkillRuntime, and SkillInvocation** - `8d75f4a` (feat)
2. **Task 2: Wire EventBus into AgentLoop in serve.rs** - `b76ca9f` (feat)

## Files Created/Modified
- `crates/blufio-core/src/types.rs` - Added session_id: Option<String> to SkillInvocation
- `crates/blufio-skill/Cargo.toml` - Added blufio-bus dependency
- `crates/blufio-skill/src/sandbox.rs` - EventBus field, setter, Invoked/Completed publishing in invoke()
- `crates/blufio-agent/src/lib.rs` - EventBus field, setter, MessageSent publishing after final delivery
- `crates/blufio/src/serve.rs` - agent_loop.set_event_bus(event_bus.clone()) wiring

## Decisions Made
- EventBus.publish() returns () not Result -- adapted plan's if-let-Err pattern to simple await
- WasmSkillRuntime is not constructed in production serve.rs (only tests) -- set_event_bus setter is ready but not wired in serve.rs yet; will be wired when production skill loading is implemented
- MessageSent published after both the send and edit-in-place code paths complete, before persist_response

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed publish call pattern for fire-and-forget EventBus**
- **Found during:** Task 1
- **Issue:** Plan specified `if let Err(e) = bus.publish(...)` pattern but EventBus::publish() returns () not Result
- **Fix:** Changed to simple `bus.publish(...).await;` without error handling wrapper
- **Files modified:** crates/blufio-skill/src/sandbox.rs, crates/blufio-agent/src/lib.rs
- **Verification:** cargo test --workspace passes, build clean
- **Committed in:** 8d75f4a (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Necessary correction for API compatibility. No scope creep.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- EventBus publishers wired -- chat.completed and tool.invoked webhooks will fire once WasmSkillRuntime is constructed in production
- AgentLoop MessageSent fully wired end-to-end via serve.rs
- Ready for Phase 43 Plan 02 or Phase 44+

---
*Phase: 43-wire-eventbus-publishers*
*Completed: 2026-03-08*
