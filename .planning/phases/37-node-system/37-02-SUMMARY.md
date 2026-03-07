---
phase: 37-node-system
plan: 02
subsystem: node
tags: [websocket, dashmap, heartbeat, sysinfo, tokio-tungstenite, clap, fleet]

# Dependency graph
requires:
  - phase: 37-01
    provides: NodeStore, NodeInfo, NodeMessage types, PairingManager, Ed25519 pairing
provides:
  - ConnectionManager with DashMap-based connection registry and exponential backoff reconnection
  - HeartbeatMonitor with spawn_blocking sysinfo metrics collection and stale detection
  - Fleet management operations (list, group, exec) with table and JSON formatting
  - CLI subcommands (nodes list/pair/remove/group/exec) wired into main binary
  - Node system startup in serve.rs with feature-gated initialization
affects: [37-03, node-websocket-handler, approval-routing]

# Tech tracking
tech-stack:
  added: [tokio-tungstenite, sysinfo, rand]
  patterns: [DashMap connection registry, exponential backoff with jitter, spawn_blocking for sysinfo, feature-gated CLI commands]

key-files:
  created:
    - crates/blufio-node/src/connection.rs
    - crates/blufio-node/src/heartbeat.rs
    - crates/blufio-node/src/fleet.rs
  modified:
    - crates/blufio-node/src/lib.rs
    - crates/blufio/src/main.rs
    - crates/blufio/src/serve.rs
    - crates/blufio/Cargo.toml

key-decisions:
  - "register_connection and remove_connection made async (not sync) because EventBus::publish is async"
  - "CLI handlers load config and open DB directly (not through running server) for offline node management"
  - "Node system in serve.rs gets its own EventBus instance and DB connection (isolation from main agent)"

patterns-established:
  - "Feature-gated node subsystem: #[cfg(feature = 'node')] matching adapter pattern"
  - "ConnectionManager pattern: DashMap<NodeId, mpsc::Sender<NodeMessage>> for concurrent connection tracking"
  - "HeartbeatMonitor pattern: tokio::time::interval + spawn_blocking sysinfo + stale threshold check"

requirements-completed: [NODE-02, NODE-03, NODE-04]

# Metrics
duration: 11min
completed: 2026-03-07
---

# Phase 37 Plan 02: Connection, Heartbeat, and Fleet CLI Summary

**WebSocket connection manager with DashMap registry and exponential backoff, heartbeat monitor with spawn_blocking sysinfo metrics, and fleet CLI commands for node list/group/exec**

## Performance

- **Duration:** 11 min
- **Started:** 2026-03-07T11:10:34Z
- **Completed:** 2026-03-07T11:21:34Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- ConnectionManager with DashMap-based connection tracking, WebSocket reconnection with exponential backoff and jitter
- HeartbeatMonitor sending metrics every 30s via spawn_blocking, detecting stale nodes at 90s threshold
- Fleet module with list/group/exec operations, table and JSON formatting
- Full CLI subcommand tree (nodes list/pair/remove/group/exec) wired into main binary
- Node system startup in serve.rs with HeartbeatMonitor spawned as background task

## Task Commits

Each task was committed atomically:

1. **Task 1: Implement connection manager and heartbeat monitor** - `6b0ea4a` (feat)
2. **Task 2: Wire node system into serve.rs and implement CLI handlers** - `3a831a5` (feat)

## Files Created/Modified
- `crates/blufio-node/src/connection.rs` - WebSocket connection manager with DashMap, reconnection backoff, message routing
- `crates/blufio-node/src/heartbeat.rs` - Heartbeat monitor with spawn_blocking sysinfo metrics and stale detection
- `crates/blufio-node/src/fleet.rs` - Fleet management: list nodes, format table/JSON, exec on nodes, group CRUD
- `crates/blufio-node/src/lib.rs` - Added module declarations and re-exports for connection, heartbeat, fleet
- `crates/blufio/src/main.rs` - NodesCommands enum, handle_nodes_command function, CLI dispatch
- `crates/blufio/src/serve.rs` - Node system initialization on serve startup
- `crates/blufio/Cargo.toml` - Added blufio-node optional dependency with "node" feature flag

## Decisions Made
- Made register_connection and remove_connection async because EventBus::publish requires await
- CLI handlers open their own DB connection for offline node management (no running server needed)
- Node system in serve.rs gets its own EventBus instance and DB connection for isolation

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed sync methods calling async EventBus::publish**
- **Found during:** Task 1 (connection.rs implementation)
- **Issue:** Plan had register_connection and remove_connection as sync methods calling async publish
- **Fix:** Made both methods async with proper .await on publish calls
- **Files modified:** crates/blufio-node/src/connection.rs
- **Verification:** cargo check -p blufio-node passes
- **Committed in:** 6b0ea4a (Task 1 commit)

**2. [Rule 1 - Bug] Removed unused Arc import in fleet.rs**
- **Found during:** Task 1 (fleet.rs implementation)
- **Issue:** Plan included unused `use std::sync::Arc` import
- **Fix:** Removed the unused import
- **Files modified:** crates/blufio-node/src/fleet.rs
- **Verification:** cargo check -p blufio-node passes with no warnings
- **Committed in:** 6b0ea4a (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (2 bug fixes)
**Impact on plan:** Both fixes necessary for correctness. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Connection manager and heartbeat monitor ready for WebSocket server handler (37-03)
- Fleet operations ready for approval routing integration
- Node system initializes on serve startup when config.node.enabled = true

---
*Phase: 37-node-system*
*Completed: 2026-03-07*
