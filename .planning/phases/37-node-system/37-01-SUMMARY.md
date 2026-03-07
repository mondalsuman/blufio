---
phase: 37-node-system
plan: 01
subsystem: node
tags: [ed25519, qrcode, websocket, sqlite, pairing, mutual-auth]

# Dependency graph
requires:
  - phase: 30-event-bus
    provides: EventBus with typed BusEvent pub/sub
  - phase: 31-auth-keypair
    provides: DeviceKeypair with Ed25519 sign/verify
provides:
  - blufio-node crate with types, config, store, and pairing modules
  - V9 SQLite migration for node_pairings, node_groups, pending_approvals
  - NodeConfig in BlufioConfig with heartbeat, reconnect, approval settings
  - NodeEvent extended with Paired, PairingFailed, Stale variants
  - PairingManager with Ed25519 mutual authentication and QR code rendering
affects: [37-02, 37-03, node-mesh, approval-routing]

# Tech tracking
tech-stack:
  added: [qrcode 0.14, tokio-tungstenite 0.28, sysinfo 0.33]
  patterns: [pairing-state-machine, deterministic-fingerprint, config-in-blufio-config-reexport]

key-files:
  created:
    - crates/blufio-node/Cargo.toml
    - crates/blufio-node/src/lib.rs
    - crates/blufio-node/src/types.rs
    - crates/blufio-node/src/config.rs
    - crates/blufio-node/src/store.rs
    - crates/blufio-node/src/pairing.rs
    - crates/blufio-storage/migrations/V9__node_system.sql
  modified:
    - Cargo.toml
    - crates/blufio-config/src/model.rs
    - crates/blufio-config/Cargo.toml
    - crates/blufio-bus/src/events.rs

key-decisions:
  - "Config structs (NodeConfig, etc.) defined in blufio-config to avoid circular dependency; re-exported from blufio-node/config.rs"
  - "DeviceKeypair uses public_bytes() not public_key_bytes() (plan interface mismatch auto-fixed)"
  - "PairingState enum omits Debug derive because DeviceKeypair does not implement Debug"
  - "publish_failure made async to match EventBus::publish async signature"

patterns-established:
  - "Node config pattern: define config structs in blufio-config, re-export from feature crate"
  - "Pairing fingerprint: SHA-256 of sorted concatenated public keys, formatted as XXXX-XXXX-XXXX-XXXX"
  - "tokio-rusqlite error type annotation: |e: tokio_rusqlite::Error<rusqlite::Error>|"

requirements-completed: [NODE-01]

# Metrics
duration: 12min
completed: 2026-03-07
---

# Phase 37 Plan 01: Node System Foundation Summary

**Ed25519 mutual-auth pairing with QR code rendering, SQLite node storage, and typed config/event integration**

## Performance

- **Duration:** 12 min
- **Started:** 2026-03-07T10:56:59Z
- **Completed:** 2026-03-07T11:09:00Z
- **Tasks:** 2
- **Files modified:** 11

## Accomplishments
- Created blufio-node crate with full type system (NodeCapability, NodeStatus, NodeInfo, PairingToken, NodeMessage, ApprovalStatus)
- Implemented PairingManager with Ed25519 mutual authentication, deterministic fingerprint verification, and QR code terminal rendering
- NodeStore with CRUD for pairings, groups, and approvals via tokio-rusqlite
- V9 SQLite migration creating node_pairings, node_groups, and pending_approvals tables
- NodeConfig integrated into BlufioConfig with heartbeat, reconnect, and approval sub-configs
- NodeEvent extended with Paired, PairingFailed, Stale variants for event bus integration

## Task Commits

Each task was committed atomically:

1. **Task 1+2: Create blufio-node crate with types, config, store, pairing, and migration** - `fe705aa` (feat)

**Plan metadata:** (pending)

## Files Created/Modified
- `crates/blufio-node/Cargo.toml` - Crate manifest with Ed25519, QR, WebSocket, SQLite deps
- `crates/blufio-node/src/lib.rs` - Crate root with NodeError enum and module exports
- `crates/blufio-node/src/types.rs` - NodeCapability, NodeStatus, NodeInfo, PairingToken, NodeMessage, ApprovalStatus
- `crates/blufio-node/src/config.rs` - Re-exports NodeConfig from blufio-config
- `crates/blufio-node/src/store.rs` - NodeStore with pairings, groups, approvals CRUD
- `crates/blufio-node/src/pairing.rs` - PairingManager with Ed25519 auth, QR rendering, fingerprint computation
- `crates/blufio-storage/migrations/V9__node_system.sql` - node_pairings, node_groups, pending_approvals tables
- `Cargo.toml` - Added qrcode, tokio-tungstenite, sysinfo workspace deps
- `crates/blufio-config/src/model.rs` - Added NodeConfig, NodeHeartbeatConfig, NodeReconnectConfig, NodeApprovalConfig
- `crates/blufio-config/Cargo.toml` - Added uuid dependency
- `crates/blufio-bus/src/events.rs` - Extended NodeEvent with Paired, PairingFailed, Stale

## Decisions Made
- Config structs defined in blufio-config (not blufio-node) to avoid circular dependency; blufio-node re-exports them
- Used `public_bytes()` instead of plan's `public_key_bytes()` to match actual DeviceKeypair API
- Made `publish_failure` async to match EventBus::publish async signature
- Removed Debug derive from PairingState since DeviceKeypair doesn't implement Debug
- Tasks 1 and 2 combined into single commit since pairing module was needed for lib.rs compilation

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed public_key_bytes -> public_bytes method name**
- **Found during:** Task 1 (pairing.rs implementation)
- **Issue:** Plan referenced `public_key_bytes()` but DeviceKeypair API is `public_bytes()`
- **Fix:** Used correct method name `public_bytes()` throughout pairing.rs
- **Files modified:** crates/blufio-node/src/pairing.rs
- **Verification:** cargo check passes
- **Committed in:** fe705aa

**2. [Rule 3 - Blocking] Added rusqlite dependency and tokio-rusqlite type annotations**
- **Found during:** Task 1 (store.rs compilation)
- **Issue:** rusqlite crate not in dependencies; tokio_rusqlite::Error needed explicit type parameter
- **Fix:** Added rusqlite to Cargo.toml; added `|e: tokio_rusqlite::Error<rusqlite::Error>|` annotations
- **Files modified:** crates/blufio-node/Cargo.toml, crates/blufio-node/src/store.rs
- **Verification:** cargo check passes
- **Committed in:** fe705aa

**3. [Rule 3 - Blocking] Fixed async EventBus::publish calls and Debug derive**
- **Found during:** Task 1 (pairing.rs compilation)
- **Issue:** EventBus::publish is async but calls lacked .await; DeviceKeypair doesn't impl Debug
- **Fix:** Added .await to publish calls; made publish_failure async; removed Debug from PairingState
- **Files modified:** crates/blufio-node/src/pairing.rs
- **Verification:** cargo check passes
- **Committed in:** fe705aa

---

**Total deviations:** 3 auto-fixed (1 bug, 2 blocking)
**Impact on plan:** All fixes necessary for compilation. No scope creep.

## Issues Encountered
None beyond the auto-fixed compilation issues.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- blufio-node crate compiles and all 6 tests pass
- Ready for WebSocket transport layer and node mesh connectivity (plan 02)
- PairingManager ready to be integrated into HTTP/WS handlers

---
*Phase: 37-node-system*
*Completed: 2026-03-07*
