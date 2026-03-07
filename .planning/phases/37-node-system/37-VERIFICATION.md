---
phase: 37-node-system
verified: 2026-03-07T16:50:00Z
status: gaps_found
score: 17/19 must-haves verified
re_verification: true
gaps:
  - truth: "ApprovalRouter subscribes to event bus for triggering approval broadcasts"
    status: failed
    reason: "ApprovalRouter has no event bus subscription. It exposes request_approval() as a direct call but does not listen on the event bus for triggering events. This is an implementation gap -- the module is designed for direct invocation only."
    artifacts:
      - path: "crates/blufio-node/src/approval.rs"
        issue: "No EventBus dependency or subscription in ApprovalRouter::new() (line 61); only conn_manager, store, and config. Requires external caller to invoke request_approval() (line 86)."
    missing:
      - "ApprovalRouter should subscribe to EventBus for events that trigger approval broadcasts, or this truth should be dropped if direct invocation is the intended pattern"
    requirement_impact: "NODE-05 core requirement (broadcast, first-wins, timeout) IS satisfied via direct invocation pattern. This gap affects an internal wiring truth, not the external requirement."
  - truth: "ConnectionManager forwards ApprovalResponse messages to ApprovalRouter.handle_response()"
    status: partial
    reason: "ConnectionManager has approval_router field (line 39) and set_approval_router() setter (line 77), but reconnect_with_backoff() does not use it. ApprovalResponse messages at line 317 are logged only, not forwarded to ApprovalRouter.handle_response()."
    artifacts:
      - path: "crates/blufio-node/src/connection.rs"
        issue: "Lines 317-332: ApprovalResponse match arm only logs via debug!(), does not call approval_router.handle_response(). Comment at line 328-331 acknowledges the gap explicitly."
    missing:
      - "Pass approval_router into reconnect_with_backoff() and call handle_response() when ApprovalResponse is received"
    requirement_impact: "NODE-05 broadcast+first-wins is implemented in approval.rs. This gap only affects cross-node approval forwarding via WebSocket -- a secondary integration path."
---

# Phase 37: Node System Verification Report

**Phase Goal:** Implement multi-device node system with pairing, heartbeat monitoring, fleet management, and approval routing
**Verified:** 2026-03-07T16:50:00Z
**Status:** gaps_found
**Re-verification:** Yes -- re-verified from 2026-03-07T12:00:00Z initial report

## Re-verification Notes

Re-verified with fresh `cargo test -p blufio-node` run (8 tests pass, 0 failures) and full source code re-read of all 8 modules. Line numbers updated to current codebase. No code changes since initial verification. Both gaps from initial report confirmed as implementation gaps (not test gaps) -- flagged per CONTEXT decision to not implement missing features.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | blufio-node crate exists and compiles as workspace member | VERIFIED | `cargo test -p blufio-node` succeeds (8 tests pass); workspace uses `members = ["crates/*"]` |
| 2 | Ed25519 mutual authentication via DeviceKeypair sign/verify_strict produces a deterministic fingerprint for MITM prevention | VERIFIED | pairing.rs: `verify_peer_signature()` (line 98) uses `VerifyingKey::verify()` (line 109), `compute_pairing_fingerprint()` (line 209) uses SHA-256 of sorted keys, 6 tests pass |
| 3 | Pairing tokens are 32 random bytes (hex-encoded), expire after 15 minutes, and are single-use | VERIFIED | types.rs: `PairingToken::generate()` (line 114) uses `OsRng.fill_bytes(&mut [0u8; 32])` (line 117), 15*60 expiry (line 120), `consume()` sets `used=true` (line 132) |
| 4 | Pairing state machine drives flow from token generation through key exchange to mutual confirmation | VERIFIED | pairing.rs: `PairingState` enum (line 24) with AwaitingPeer, KeyExchange, AwaitingConfirmation, Complete, Failed states; PairingManager methods drive flow |
| 5 | QR code renders pairing URI in terminal using Dense1x2 Unicode half-blocks | VERIFIED | pairing.rs: `render_pairing_qr()` (line 235) uses `qrcode::render::unicode::Dense1x2` (line 237) with inverted colors (lines 243-244), test confirms no panic |
| 6 | Node pairings persist in SQLite with public_key_hex, capabilities, endpoint, last_seen | VERIFIED | V9 migration: `node_pairings` table (line 3) with all columns; store.rs CRUD operations (save_pairing line 27, list_pairings line 58, get_pairing line 94) with proper SQL |
| 7 | NodeConfig and ApprovalConfig structs exist in blufio-config model with deny_unknown_fields | VERIFIED | model.rs: NodeConfig (line 1464), NodeHeartbeatConfig (line 1519), NodeReconnectConfig (line 1549), NodeApprovalConfig (line 1588) all have `#[serde(deny_unknown_fields)]` |
| 8 | NodeEvent enum extended with Paired, PairingFailed, Stale variants | VERIFIED | events.rs: Paired (line 159), PairingFailed (line 170), Stale (line 179), Connected (line 139), Disconnected (line 148) with proper fields |
| 9 | WebSocket connection manager maintains connections with exponential backoff reconnection and jitter | VERIFIED | connection.rs: `reconnect_with_backoff()` (line 251) with `delay = (delay * 2).min(max_delay)` (line 384) and random jitter (lines 378-382); DashMap connection registry (line 29) |
| 10 | Nodes exchange Hello messages with capability declaration on connect | VERIFIED | connection.rs: `connect_to_peer()` (line 391) sends `NodeMessage::Hello` (line 406) with node_id, capabilities, version immediately after WS connect |
| 11 | Heartbeat sends every 30s with battery, memory, uptime; stale detection at 90s threshold | VERIFIED | heartbeat.rs: `tokio::time::interval(interval)` (line 82) where default is 30s; broadcasts Heartbeat (line 91) with all fields; stale check at configurable threshold (line 108, default 90s) |
| 12 | sysinfo calls run in spawn_blocking to avoid blocking async runtime | VERIFIED | heartbeat.rs: `collect_metrics()` (line 32) wraps `System::new()` + `refresh_memory()` in `tokio::task::spawn_blocking` (line 33) |
| 13 | blufio nodes list shows table with Name, Status, Capabilities, Battery, Memory columns and supports --json | VERIFIED | fleet.rs: `format_nodes_table()` (line 24) formats 5 columns (line 32-34); main.rs: `--json` flag triggers `format_nodes_json()` (line 69) |
| 14 | blufio nodes group create/delete/list manages named groups with --nodes flag | VERIFIED | main.rs: `NodeGroupCommands` with Create(name, nodes), Delete(name), List; fleet.rs: `create_group()` (line 133), `delete_group()` (line 146), `list_groups()` (line 151) backed by store |
| 15 | blufio nodes exec streams per-node output prefixed with [node-name] | VERIFIED | fleet.rs: `exec_on_nodes()` (line 77) sends ExecRequest to targets; main.rs dispatches the command. Note: streaming output display with [node-name] prefix is in the message type (ExecOutput.node_id at types.rs line 199) but actual prefix formatting would be on receive side |
| 16 | blufio nodes pair initiates pairing and displays QR code or token | VERIFIED | main.rs: `NodesCommands::Pair` (line 971) creates PairingManager, calls `initiate_pairing()`, prints QR or fallback token |
| 17 | Node CLI commands wired into main binary clap structure | VERIFIED | main.rs: `#[cfg(feature = "node")] Nodes { action: NodesCommands }` (lines 124-127) with List, Pair, Remove, Group, Exec subcommands (line 338) |
| 18 | Approval requests broadcast to all connected operator devices via WebSocket; first-wins semantics; timeout-then-deny | VERIFIED | approval.rs: `request_approval()` (line 86) calls `conn_manager.broadcast()` (line 118), DashMap::remove for atomic first-wins (line 181), tokio::spawn timeout task with auto-deny (line 133-164) |
| 19 | ApprovalRouter subscribes to event bus for triggering approval broadcasts | FAILED | ApprovalRouter has no EventBus dependency or subscription; it is purely invoked via `request_approval()` method. See gap analysis below. |

**Score:** 17/19 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/blufio-node/Cargo.toml` | Crate manifest | VERIFIED | All deps including ed25519-dalek, qrcode, tokio-tungstenite, sysinfo, dashmap |
| `crates/blufio-node/src/lib.rs` | Crate root with modules and error type | VERIFIED | 57 lines, 8 modules declared, re-exports, NodeError enum |
| `crates/blufio-node/src/types.rs` | Node types | VERIFIED | 233 lines, NodeCapability, NodeStatus, NodeInfo, PairingToken, NodeMessage, ApprovalStatus |
| `crates/blufio-node/src/config.rs` | Config re-exports | VERIFIED | Re-exports from blufio-config |
| `crates/blufio-node/src/store.rs` | SQLite persistence | VERIFIED | 335 lines, full CRUD for pairings/groups/approvals |
| `crates/blufio-node/src/pairing.rs` | Pairing state machine | VERIFIED | 315 lines, PairingManager with Ed25519 auth, QR, fingerprint, 6 tests |
| `crates/blufio-node/src/connection.rs` | WebSocket connection manager | VERIFIED | 462 lines, ConnectionManager with DashMap, reconnection, Hello, heartbeat handling |
| `crates/blufio-node/src/heartbeat.rs` | Heartbeat monitor | VERIFIED | 139 lines, HeartbeatMonitor with spawn_blocking metrics, stale detection, 1 test |
| `crates/blufio-node/src/fleet.rs` | Fleet management | VERIFIED | 174 lines, list/group/exec operations with table/JSON formatting |
| `crates/blufio-node/src/approval.rs` | Approval routing | VERIFIED | 279 lines, ApprovalRouter with broadcast, first-wins, timeout, 1 test |
| `crates/blufio-storage/migrations/V9__node_system.sql` | SQLite migration | VERIFIED | 31 lines, node_pairings, node_groups, pending_approvals tables |
| `crates/blufio/src/main.rs` | CLI integration | VERIFIED | NodesCommands enum (line 338) with all subcommands, handle_nodes_command handler (line 943) |
| `crates/blufio/src/serve.rs` | Server integration | VERIFIED | Node system init with NodeStore (line 785), ConnectionManager (line 788), HeartbeatMonitor (line 798) on startup |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| blufio-node | blufio-auth-keypair | DeviceKeypair import | WIRED | pairing.rs line 11 imports and uses DeviceKeypair for sign/verify |
| blufio-node | blufio-bus | EventBus publish | WIRED | pairing.rs, connection.rs, heartbeat.rs all publish NodeEvent variants |
| ConnectionManager | DashMap connections | mpsc::Sender registry | WIRED | DashMap<NodeId, mpsc::Sender<NodeMessage>> (line 29) with register/remove/send_to/broadcast |
| HeartbeatMonitor | tokio::time::interval | interval tick loop | WIRED | heartbeat.rs run() (line 79) uses tokio::time::interval matching agent pattern |
| Fleet CLI | clap Subcommand | derive pattern | WIRED | main.rs: NodesCommands with #[command(subcommand)] matching SkillCommands pattern |
| Node events | EventBus | Connected, Disconnected, Stale | WIRED | connection.rs publishes Connected/Disconnected; heartbeat.rs publishes Stale |
| ApprovalRouter | ConnectionManager.broadcast() | WebSocket delivery | WIRED | approval.rs line 118 calls self.conn_manager.broadcast(message) |
| ApprovalRouter | DashMap | compare-and-swap for first-wins | WIRED | approval.rs line 181 uses self.active.remove() for atomic first-wins |
| ApprovalRouter | tokio::time | timeout-then-deny | WIRED | tokio::spawn (line 133) with tokio::time::sleep(timeout_secs) (line 134) |
| NodeStore | save_approval/resolve_approval | SQLite persistence | WIRED | store.rs has both methods (lines 281, 309); approval.rs calls them |
| Config | NodeApprovalConfig | broadcast_actions, timeout_secs | WIRED | approval.rs reads config.broadcast_actions (line 79) and config.timeout_secs (line 92) |
| ConnectionManager | ApprovalRouter | forwarding ApprovalResponse | PARTIAL | approval_router field exists (line 39) with setter (line 77), but reconnect_with_backoff does not forward responses (line 317-332 logs only) |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-----------|-------------|--------|----------|
| NODE-01 | 37-01 | Node pairing via Ed25519 mutual authentication (QR or shared token) | SATISFIED | PairingManager with Ed25519 verify (line 98), QR rendering (line 235), token generation/validation (types.rs line 114). 6 pairing tests pass. |
| NODE-02 | 37-02 | Node connection via WebSocket with capability declaration | SATISFIED | ConnectionManager with tokio-tungstenite (connection.rs line 396), Hello message with capabilities (line 406-410). DashMap registry. |
| NODE-03 | 37-02 | Node heartbeat monitoring (battery, memory, connectivity, stale detection) | SATISFIED | HeartbeatMonitor (heartbeat.rs line 56) with 30s interval, spawn_blocking sysinfo (line 33), 90s stale threshold (line 108). 1 heartbeat test passes. |
| NODE-04 | 37-02 | Node fleet management CLI (blufio nodes list/group/exec) | SATISFIED | Full CLI subcommand tree (main.rs line 338) with list (table/json), group CRUD, exec routing. Fleet functions in fleet.rs. |
| NODE-05 | 37-03 | Approval routing broadcasts to all connected operator devices | SATISFIED | ApprovalRouter (approval.rs line 48) with broadcast (line 118), first-wins via DashMap::remove (line 181), timeout-then-deny (lines 133-164), SQLite persistence. Note: Two internal wiring gaps exist (event bus subscription, WebSocket forwarding) but the core broadcast+first-wins+timeout requirement IS implemented. |

All 5 requirements are accounted for. No orphaned requirements found.

**NODE-05 gap impact assessment:** The two documented gaps (Truths 19 and Key Link #12) affect secondary integration paths -- event bus triggering and cross-node WebSocket forwarding. The core NODE-05 requirement (broadcast to all connected devices, first-wins semantics, timeout-then-deny) is fully implemented in approval.rs and can be invoked directly. These gaps would matter in a multi-node deployment where approval responses arrive over WebSocket, but the approval routing logic itself is complete.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| connection.rs | 343 | `_ => {}` catch-all | Info | Legitimate pattern for unhandled message types in WS receive loop |
| connection.rs | 317-332 | ApprovalResponse logged but not forwarded | Warning | Approval responses from peers are not forwarded to ApprovalRouter.handle_response() |
| connection.rs | 328-331 | Code comment acknowledges gap | Info | Comment explicitly states "In a full implementation, the ApprovalRouter would subscribe to incoming messages or be passed as a dependency" |

No TODOs, FIXMEs, placeholders, or empty implementations found anywhere in the crate.

### Test Results (Re-verification Run)

```
cargo test -p blufio-node
running 8 tests
test approval::tests::approval_outcome_clone ... ok
test pairing::tests::pairing_token_validity ... ok
test pairing::tests::challenge_is_order_independent ... ok
test pairing::tests::pairing_token_consume ... ok
test pairing::tests::fingerprint_is_deterministic_regardless_of_order ... ok
test pairing::tests::fingerprint_format_is_four_groups ... ok
test heartbeat::tests::collect_metrics_does_not_panic ... ok
test pairing::tests::qr_code_renders_without_panic ... ok
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Human Verification Required

### 1. WebSocket Pairing Flow End-to-End

**Test:** Start two Blufio instances, run `blufio nodes pair` on one, scan QR/enter token on other
**Expected:** Both nodes paired, stored in SQLite, fingerprint displayed and confirmed on both sides
**Why human:** Requires two running instances and interactive QR scan/token entry

### 2. Heartbeat and Stale Detection Timing

**Test:** Connect two nodes, disconnect one, wait >90 seconds
**Expected:** Disconnected node transitions to Stale status in `blufio nodes list`
**Why human:** Requires real-time observation of timing behavior over 90+ seconds

### 3. Approval First-Wins Race Condition

**Test:** Connect 3+ devices, trigger approval broadcast, rapidly approve from 2 devices simultaneously
**Expected:** Only one device wins; others receive ApprovalHandled notification
**Why human:** Race condition testing requires concurrent device interaction

### Gaps Summary

Two gaps confirmed from initial verification, both related to approval routing wiring:

1. **ApprovalRouter event bus subscription** (Truth 19 -- FAILED): The ApprovalRouter is designed as a direct-call API (`request_approval()` at line 86) rather than subscribing to the event bus for automatic triggering. This is an **implementation gap** -- the code comment at connection.rs lines 328-331 explicitly acknowledges this. Per CONTEXT directive, flagged as UNVERIFIED without attempting implementation.

2. **ConnectionManager -> ApprovalRouter forwarding** (Key Link -- PARTIAL): The `ConnectionManager` has the `approval_router` field (line 39) and `set_approval_router()` setter (line 77), but `reconnect_with_backoff()` does not receive or use this field. `ApprovalResponse` messages at line 317 are logged via debug!() but not passed to `handle_response()`. This is an **implementation gap** that would prevent approval responses received over WebSocket from triggering first-wins resolution. Per CONTEXT directive, flagged as UNVERIFIED without attempting implementation.

Both gaps share a root cause: the approval routing module is fully implemented in isolation but not fully wired into the connection layer's message dispatch loop.

---

_Verified: 2026-03-07T16:50:00Z (re-verification)_
_Initial verification: 2026-03-07T12:00:00Z_
_Verifier: Claude (gsd-executor)_
