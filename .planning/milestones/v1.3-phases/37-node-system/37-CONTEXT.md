# Phase 37: Node System - Context

**Gathered:** 2026-03-07
**Status:** Ready for planning

<domain>
## Phase Boundary

Paired device mesh with Ed25519 mutual authentication. Users can pair multiple Blufio instances, manage a node fleet via CLI, and have approval requests broadcast to all connected operator devices. Session sharing and coordinated approvals across the mesh.

</domain>

<decisions>
## Implementation Decisions

### Pairing Flow
- QR code (unicode blocks in terminal) for local/mobile devices, with CLI token fallback for headless/remote pairing
- Mutual confirmation: both devices display a fingerprint/code and both operators must confirm (prevents MITM)
- Pairing tokens expire after 15 minutes, single-use; silent expiry with clear error message, user runs `blufio nodes pair` again
- Pairings persist in SQLite/vault storage; nodes auto-reconnect on restart using stored Ed25519 keys

### Capability Model
- Extensible with registry: core capabilities (camera, screen, location, exec) built-in as enum variants, plus custom string capabilities for plugins/future use
- Manual declaration in TOML config (e.g., `capabilities = ["exec", "screen"]`); no auto-detection
- Capabilities enforce permissions: if a node doesn't declare a capability, it cannot receive requests for it
- "exec" capability means both shell commands (`blufio nodes exec node-2 -- ls /tmp`) and Blufio operations (skills, sessions, agent tasks) routed from other nodes

### Fleet CLI
- `blufio nodes list`: table format by default (Name, Status, Capabilities, Battery, Memory), `--json` flag for machine parsing
- `blufio nodes group`: named groups (create, delete, list) — `blufio nodes group create mobile --nodes node-1,node-3`, then target groups with `blufio nodes exec mobile -- ...`
- `blufio nodes exec`: streamed per-node output as results arrive, prefixed with node name (`[node-1] output...`)
- Node status (battery, memory, connectivity) reported via heartbeat messages over WebSocket; `nodes list` shows last-known state

### Approval Routing
- Configurable per-action type in TOML config — operator specifies which action types require broadcast approval
- First-wins semantics: first device to approve/deny wins, other devices see "Already handled by [device]"
- Timeout then deny: after configurable timeout (default 5 min), auto-deny the pending action; user can retry
- Approval requests pushed as WebSocket messages to connected devices; devices display via their channel integration

### Claude's Discretion
- WebSocket reconnection strategy and backoff parameters
- Heartbeat interval and stale detection thresholds
- QR code encoding format and terminal rendering details
- Node ID generation scheme
- Internal message serialization format for node-to-node communication
- Exact TOML config schema structure for node settings

</decisions>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `blufio-auth-keypair` crate: `DeviceKeypair` with Ed25519 sign/verify/verify_strict, `SignedAgentMessage` for authenticated inter-agent messages — directly reusable for node mutual authentication
- `blufio-bus` crate: `EventBus` with broadcast + reliable mpsc channels, already has `NodeEvent::Connected` and `NodeEvent::Disconnected` variants — ready for node lifecycle events
- `blufio-gateway/ws.rs`: Axum WebSocket handler with mpsc response channels and `GatewayState` — pattern to follow for node WebSocket connections
- `blufio-gateway/api_keys`: Scoped API keys with validation — pattern for capability-based access control
- `blufio-agent/heartbeat.rs`: `HeartbeatRunner` with skip-when-unchanged hash logic — pattern for node heartbeat monitoring

### Established Patterns
- Axum-based HTTP/WS server with shared `GatewayState` (DashMap for ws_senders)
- Ed25519 for device identity with strict verification mode
- Event bus for inter-component communication (dual-channel: broadcast + reliable mpsc)
- TOML config via `blufio-config/model.rs`
- `PluginAdapter` / `AuthAdapter` trait pattern for extensible auth
- clap for CLI argument parsing in main binary

### Integration Points
- New `blufio-node` crate will depend on `blufio-auth-keypair`, `blufio-bus`, `blufio-config`, `blufio-core`
- Node CLI subcommands added to main `blufio` binary's clap setup
- Node events published to existing `EventBus` using `BusEvent::Node(...)` variants (may need to extend `NodeEvent` enum with more variants)
- Node pairings stored via `StorageAdapter` (SQLite) alongside existing session/message data
- Approval routing integrates with existing gateway WebSocket connections

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 37-node-system*
*Context gathered: 2026-03-07*
