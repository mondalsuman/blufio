---
phase: 40-wire-global-eventbus-bridge
plan: 02
subsystem: infra
tags: [eventbus, bridge, channel-dispatch, cross-channel, feature-flag]

# Dependency graph
requires:
  - phase: 40-wire-global-eventbus-bridge
    plan: 01
    provides: "Global Arc<EventBus> in serve.rs, ChannelMultiplexer.connected_channels_ref() accessor"
  - phase: 34-cross-channel-bridge
    provides: "blufio_bridge::spawn_bridge(), BridgedMessage struct, bridge routing logic"
provides:
  - "spawn_bridge() called in serve.rs when bridge groups configured"
  - "Bridge dispatch task routes BridgedMessage to channel adapters via connected_channels_ref()"
  - "Bridge is_bridged metadata on dispatched messages for loop prevention"
affects: [webhooks, runtime, deployment]

# Tech tracking
tech-stack:
  added: []
  patterns: [bridge-dispatch-via-connected-channels, feature-gated-subsystem-wiring]

key-files:
  created: []
  modified:
    - crates/blufio/src/serve.rs

key-decisions:
  - "Bridge dispatch calls adapter.send() directly (outbound-only), not via mux inbound path -- prevents infinite loops"
  - "connected_channels_ref() captured before mux moves into AgentLoop -- Arc<Vec> snapshot is immutable and safe"
  - "Dispatched messages include metadata is_bridged:true as defense-in-depth loop prevention"

patterns-established:
  - "Feature-gated subsystem wiring: #[cfg(feature = \"bridge\")] on let binding with if/else for no-op"
  - "Bridge dispatch: grab channel references before mux moves, spawn independent consumer task"

requirements-completed: [INFRA-06]

# Metrics
duration: 7min
completed: 2026-03-07
---

# Phase 40 Plan 02: Bridge Dispatch Wiring Summary

**spawn_bridge() wired in serve.rs with dispatch task routing BridgedMessage to channel adapters via connected_channels_ref(), all behind #[cfg(feature = "bridge")]**

## Performance

- **Duration:** 7 min
- **Started:** 2026-03-07T19:42:53Z
- **Completed:** 2026-03-07T19:50:17Z
- **Tasks:** 2
- **Files modified:** 1

## Accomplishments
- Wired blufio_bridge::spawn_bridge() into serve.rs, called after mux.connect() when config.bridge is non-empty
- Spawned bridge dispatch consumer task that reads BridgedMessage values and routes them to target channel adapters by name
- Captured connected_channels_ref() before mux moves into AgentLoop, providing immutable Arc snapshot for the dispatch task
- Added is_bridged metadata on dispatched OutboundMessage for defense-in-depth loop prevention
- Verified all bridge code is cleanly gated behind #[cfg(feature = "bridge")] -- compiles with and without bridge feature
- Full workspace test suite passes with zero failures

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire spawn_bridge() and bridge dispatch task in serve.rs** - `8dabaf6` (feat)
2. **Task 2: Verify feature-flag correctness and full workspace compilation** - No code changes required (verification-only task, all checks passed)

## Files Created/Modified
- `crates/blufio/src/serve.rs` - Added bridge wiring block: spawn_bridge() call, dispatch consumer task with channel adapter routing, feature-flag gating
- `Cargo.lock` - Updated dependency graph (blufio-bus added to blufio-agent from Plan 01)

## Decisions Made
- Bridge dispatch uses adapter.send() directly (not mux inbound) to prevent infinite loop -- outbound-only path does not trigger ChannelEvent::MessageReceived re-publishing
- connected_channels_ref() returns Arc<Vec<...>> which is an immutable snapshot after connect(), safe to share with spawned task
- Dispatched messages include serde_json metadata `{"is_bridged": true}` as defense-in-depth (primary loop prevention is in BridgeManager::should_bridge checking is_bridged on ChannelEvent)
- _bridge_handles keeps JoinHandles alive for run_serve() lifetime via Some((bridge_task, dispatch_task))

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

Pre-existing: `cargo check --no-default-features --features "telegram,anthropic,sqlite,onnx,gateway,node"` (without keypair) fails due to unguarded `blufio_auth_keypair` reference in gateway setup. This is not related to bridge changes and is a pre-existing feature-flag interaction. Adding `keypair` to features resolves it. Not fixed as it's out of scope.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Bridge system is fully wired: EventBus -> spawn_bridge() -> BridgedMessage -> dispatch to channel adapters
- INFRA-06 (cross-channel bridging) requirement is now complete end-to-end
- Phase 40 is fully complete (both plans done)
- Ready for Phase 41 or any remaining gap-closure work

## Self-Check: PASSED

All files verified present, all commits verified in git log. Bridge wiring confirmed in serve.rs (spawn_bridge call, dispatch_task, cfg(feature = "bridge") gate).

---
*Phase: 40-wire-global-eventbus-bridge*
*Completed: 2026-03-07*
