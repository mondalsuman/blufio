# Phase 40: Wire Global EventBus & Bridge - Context

**Gathered:** 2026-03-07
**Status:** Ready for planning

<domain>
## Phase Boundary

Create a single global `Arc<EventBus>` in serve.rs shared across all subsystems, replace the node-scoped `node_event_bus`, wire `blufio-bridge::spawn_bridge()` to start at server startup, and ensure channel events are published to the bus so bridging works end-to-end. No new event types or crate changes — pure wiring in the binary crate.

</domain>

<decisions>
## Implementation Decisions

### Claude's Discretion

The user delegated all implementation decisions to Claude (consistent with Phase 29 context). The following areas should be resolved during planning based on codebase patterns:

**Bus Unification:**
- Whether to replace the node-scoped `node_event_bus` (serve.rs:786) with a single global bus or keep both
- Global bus capacity (currently node bus uses 128)
- How `Arc<EventBus>` flows through serve.rs to all subsystems (nodes, gateway, agent, bridge)

**Bridge Dispatch:**
- How `spawn_bridge()` return value (`mpsc::Receiver<BridgedMessage>`) is consumed
- Whether a dedicated tokio task polls bridged messages and dispatches via ChannelMultiplexer
- Integration pattern with existing channel adapter send paths

**Event Publishing Scope:**
- Which subsystems actively publish events in this phase
- At minimum: ChannelEvent::MessageReceived must be published for bridging to work
- Whether to proactively wire Session/Skill/Node/Webhook/Batch publishing or defer to Phase 41+

**Bridge Configuration:**
- How bridge groups from `BlufioConfig` are passed to `spawn_bridge()`
- Whether bridge is no-op when no groups configured (already handled by BridgeManager::is_empty())
- Feature flag gating (if any) for bridge functionality

</decisions>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches. User trusts Claude to make all wiring decisions based on existing codebase patterns in serve.rs and the established crate interfaces.

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `blufio_bus::EventBus`: Complete dual-channel pub/sub (broadcast + reliable mpsc), Send+Sync, ready for Arc wrapping
- `blufio_bridge::spawn_bridge()`: Takes `Arc<EventBus>` + bridge groups, returns `(mpsc::Receiver<BridgedMessage>, JoinHandle<()>)`
- `blufio_bridge::router::run_bridge_loop()`: Subscribes to bus, filters ChannelEvent::MessageReceived, formats with attribution, sends to mpsc
- `BridgeGroupConfig` in blufio-config: Already parsed from TOML `[bridge.groups.<name>]` sections
- `ChannelMultiplexer` in blufio-agent: Aggregates multiple channel adapters, has send capability

### Established Patterns
- Feature-gated adapter initialization: each adapter behind `#[cfg(feature = "...")]` blocks in serve.rs
- `Arc<T>` sharing: storage, vault, context engine, cost ledger all shared via Arc in serve.rs
- Subsystem tasks spawned via `tokio::spawn` with JoinHandles tracked in `tasks` vec
- Node system already uses EventBus pattern (serve.rs:786) — shows the wiring template

### Integration Points
- `serve.rs` startup: Global EventBus created early, before adapter initialization
- `serve.rs:786`: Node EventBus creation — replace with global bus reference
- Channel adapters: Need to publish ChannelEvent::MessageReceived when messages arrive
- Bridge output: BridgedMessage receiver needs a consumer task that calls channel adapter send methods
- `blufio/Cargo.toml`: Needs `blufio-bridge` dependency added

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 40-wire-global-eventbus-bridge*
*Context gathered: 2026-03-07*
