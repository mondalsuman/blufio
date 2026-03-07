---
phase: 40-wire-global-eventbus-bridge
plan: 01
subsystem: infra
tags: [eventbus, channel-multiplexer, pubsub, tokio-broadcast]

# Dependency graph
requires:
  - phase: 29-internal-event-bus
    provides: "EventBus struct, BusEvent/ChannelEvent types, new_event_id/now_timestamp helpers"
provides:
  - "Global Arc<EventBus> created in serve.rs with capacity 1024"
  - "ChannelMultiplexer.set_event_bus() setter for wiring"
  - "ChannelMultiplexer.connected_channels_ref() accessor for bridge dispatch"
  - "ChannelEvent::MessageReceived published for every inbound text message"
  - "Node system uses global bus instead of node-scoped bus"
affects: [40-02-bridge-wiring, webhooks, bridging, node-events]

# Tech tracking
tech-stack:
  added: [blufio-bus dependency in blufio-agent]
  patterns: [global-eventbus-injection, per-channel-event-publishing]

key-files:
  created: []
  modified:
    - crates/blufio-agent/src/channel_mux.rs
    - crates/blufio-agent/Cargo.toml
    - crates/blufio/src/serve.rs

key-decisions:
  - "Global EventBus capacity 1024 (up from node-scoped 128) since it handles all event types"
  - "EventBus wired to mux before channel setup so per-channel spawn tasks have the bus clone"
  - "blufio-bus added as dependency to blufio-agent (was only in blufio main crate)"

patterns-established:
  - "Global bus injection: create Arc<EventBus> early in run_serve(), pass to subsystems via setter methods"
  - "Per-channel event publishing: clone event_bus into tokio::spawn tasks, publish on text message receive"

requirements-completed: [INFRA-01, INFRA-02, INFRA-03]

# Metrics
duration: 7min
completed: 2026-03-07
---

# Phase 40 Plan 01: Wire Global EventBus Summary

**Global Arc<EventBus> created in serve.rs (capacity 1024), wired to ChannelMultiplexer for text message event publishing, replacing node-scoped bus**

## Performance

- **Duration:** 7 min
- **Started:** 2026-03-07T19:32:10Z
- **Completed:** 2026-03-07T19:40:08Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- Added `set_event_bus()` and `connected_channels_ref()` methods to ChannelMultiplexer
- ChannelMultiplexer publishes `ChannelEvent::MessageReceived` to the global bus for every inbound text message
- Created global `Arc<EventBus>` with capacity 1024 in `run_serve()`, replacing the node-scoped `node_event_bus` (capacity 128)
- ConnectionManager and HeartbeatMonitor now use the single global bus

## Task Commits

Each task was committed atomically:

1. **Task 1: Add set_event_bus() and connected_channels_ref() to ChannelMultiplexer** - `fd1b132` (feat)
2. **Task 2: Create global EventBus in serve.rs and replace node-scoped bus** - `95190f5` (feat)

## Files Created/Modified
- `crates/blufio-agent/Cargo.toml` - Added blufio-bus dependency
- `crates/blufio-agent/src/channel_mux.rs` - Added event_bus field, set_event_bus() setter, connected_channels_ref() accessor, ChannelEvent publishing in receive tasks, 2 unit tests
- `crates/blufio/src/serve.rs` - Global EventBus creation (capacity 1024), mux.set_event_bus() wiring, node_event_bus replaced with global event_bus

## Decisions Made
- Global EventBus capacity set to 1024 (up from node's 128) since it handles all event types globally
- EventBus wired to mux before channel setup so per-channel spawn tasks get the bus clone
- blufio-bus added as a dependency to blufio-agent crate (previously only in blufio main crate)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added blufio-bus dependency to blufio-agent/Cargo.toml**
- **Found during:** Task 1
- **Issue:** blufio-bus was not listed as a dependency of blufio-agent, causing compilation failure when importing blufio_bus types
- **Fix:** Added `blufio-bus = { path = "../blufio-bus" }` to blufio-agent/Cargo.toml dependencies
- **Files modified:** crates/blufio-agent/Cargo.toml
- **Verification:** cargo check -p blufio-agent compiles cleanly
- **Committed in:** fd1b132 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Essential fix for compilation. Plan noted to check Cargo.toml and add if needed. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Global EventBus is active and publishing ChannelEvent::MessageReceived for text messages
- connected_channels_ref() is available for bridge dispatch
- Plan 02 (bridge wiring) can now subscribe to the bus and route messages between channels
- All workspace tests pass with no regressions

## Self-Check: PASSED

All files verified present, all commits verified in git log.

---
*Phase: 40-wire-global-eventbus-bridge*
*Completed: 2026-03-07*
