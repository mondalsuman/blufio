# Phase 40: Wire Global EventBus & Bridge - Research

**Researched:** 2026-03-07
**Domain:** Runtime wiring -- EventBus unification and bridge startup in serve.rs
**Confidence:** HIGH

## Summary

This phase is pure runtime wiring in the binary crate (`crates/blufio/src/serve.rs`). Both the EventBus crate (Phase 29) and the bridge crate (Phase 34) are fully implemented and tested. The work involves: (1) creating a single `Arc<EventBus>` early in `run_serve()`, (2) replacing the node-scoped `node_event_bus` at line 786 with a reference to the global bus, (3) calling `blufio_bridge::spawn_bridge()` with the global bus and config bridge groups, (4) spawning a consumer task that reads `BridgedMessage` values from the receiver and dispatches them via the ChannelMultiplexer, and (5) publishing `ChannelEvent::MessageReceived` events to the bus when inbound messages arrive.

The existing code is well-structured for this change. All subsystems that consume `Arc<EventBus>` (ConnectionManager, HeartbeatMonitor, PairingManager) already accept it via constructor injection. The bridge crate's `spawn_bridge()` returns `(mpsc::Receiver<BridgedMessage>, JoinHandle<()>)` -- the receiver must be drained by a consumer task that converts `BridgedMessage` into `OutboundMessage` and calls `ChannelMultiplexer::send()`. The bridge feature flag already exists in `Cargo.toml` (line 24: `bridge = ["dep:blufio-bridge"]`) and is in the default features.

**Primary recommendation:** Create the global EventBus before adapter initialization, pass it to the node system instead of creating a separate one, wire bridge startup after multiplexer connect, and spawn a bridge dispatch task. Event publishing should be added to the AgentLoop's message receive path for ChannelEvent::MessageReceived.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
None -- user delegated all implementation decisions to Claude.

### Claude's Discretion
- Bus unification strategy (replace node-scoped bus vs keep both)
- Global bus capacity (currently node bus uses 128)
- How Arc<EventBus> flows through serve.rs to all subsystems
- Bridge dispatch pattern (dedicated task polling BridgedMessage receiver)
- Event publishing scope (which events published in this phase)
- Bridge configuration passthrough from BlufioConfig
- Feature flag gating for bridge functionality

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| INFRA-01 | Internal event bus using tokio broadcast with lag handling | EventBus crate exists with broadcast + lag logging in router.rs:91-93. This phase wires it globally in serve.rs so all subsystems share it. |
| INFRA-02 | Event bus publishes typed events (session, channel, skill, node, webhook, batch) | All 6 BusEvent variants exist in blufio-bus/events.rs. This phase wires ChannelEvent::MessageReceived publishing; other domains publish naturally when their subsystems gain bus access. |
| INFRA-03 | Event bus uses mpsc for reliable subscribers (webhook delivery) | EventBus::subscribe_reliable() exists and is tested. Bridge router uses broadcast subscription. Reliable path available for webhook subscriber wiring in Phase 42. |
| INFRA-06 | Cross-channel bridging with configurable bridge rules in TOML | BridgeManager, spawn_bridge(), run_bridge_loop(), BridgeGroupConfig all exist. This phase wires spawn_bridge() in serve.rs and dispatches BridgedMessage to ChannelMultiplexer. |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| blufio-bus | 0.1.0 | Dual-channel event bus (broadcast + mpsc) | Project crate, Phase 29 |
| blufio-bridge | 0.1.0 | Cross-channel message bridge | Project crate, Phase 34 |
| tokio::sync::broadcast | tokio 1.49 | Fire-and-forget pub/sub | Already used by EventBus |
| tokio::sync::mpsc | tokio 1.49 | Reliable delivery + bridge output channel | Already used by EventBus and bridge |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| blufio-config | 0.1.0 | BridgeGroupConfig + BlufioConfig.bridge | Config parsing already done |
| blufio-agent | 0.1.0 | ChannelMultiplexer for bridge dispatch | Outbound message routing |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Single global EventBus | Separate bus per subsystem | Global is simpler, enables cross-domain subscribers (webhooks), matches CONTEXT.md direction |
| Broadcast for bridge | Reliable mpsc for bridge | Bridge can tolerate lag (logs warning, non-critical), broadcast is simpler |

**Installation:**
No new dependencies needed. `blufio-bus` is already a non-optional dependency. `blufio-bridge` is already an optional dependency gated on the `bridge` feature (which is in `default` features).

## Architecture Patterns

### Recommended Wiring Sequence in serve.rs

```
run_serve() {
  // ... existing init (storage, cost, context, tools, mcp, provider) ...

  // NEW: Create global EventBus (early, before any subsystem that needs it)
  let event_bus = Arc::new(blufio_bus::EventBus::new(1024));

  // ... existing channel adapter setup (telegram, discord, slack, etc.) ...

  // ... existing gateway setup ...

  // Connect multiplexer
  mux.connect().await?;

  // CHANGED: Node system uses global event_bus instead of node_event_bus
  // (replace line 786: let node_event_bus = Arc::new(blufio_bus::EventBus::new(128));)

  // NEW: Spawn bridge (after mux.connect() so channels are available)
  #[cfg(feature = "bridge")]
  let _bridge_handle = { ... spawn_bridge + dispatch task ... };

  // ... existing agent loop creation and run ...
}
```

### Pattern 1: Global EventBus Creation
**What:** Single `Arc<EventBus>` created early in `run_serve()`, shared via `Arc::clone()` to all subsystems.
**When to use:** Always -- this is the core wiring pattern.
**Example:**
```rust
// Source: blufio-bus/src/lib.rs (EventBus::new)
// Create with capacity 1024 (up from node's 128 -- global bus handles
// more event types and subsystems)
let event_bus = Arc::new(blufio_bus::EventBus::new(1024));
info!("global event bus created");
```

### Pattern 2: Node System Bus Replacement
**What:** Replace the node-scoped `node_event_bus` at serve.rs:786 with the global bus reference.
**When to use:** Node system initialization block.
**Example:**
```rust
// BEFORE (serve.rs:786):
// let node_event_bus = Arc::new(blufio_bus::EventBus::new(128));

// AFTER:
let conn_manager = Arc::new(blufio_node::ConnectionManager::new(
    node_store.clone(),
    event_bus.clone(),  // was: node_event_bus.clone()
    config.node.clone(),
));

let heartbeat_monitor = blufio_node::HeartbeatMonitor::new(
    conn_manager.clone(),
    event_bus.clone(),  // was: node_event_bus.clone()
    config.node.clone(),
);
```

### Pattern 3: Bridge Spawn and Dispatch
**What:** Call `spawn_bridge()` with global bus and bridge config, then spawn a consumer task.
**When to use:** After `mux.connect()` succeeds, before `agent_loop.run()`.
**Example:**
```rust
// Source: blufio-bridge/src/lib.rs (spawn_bridge signature)
#[cfg(feature = "bridge")]
let _bridge_handle = if !config.bridge.is_empty() {
    let (mut bridge_rx, bridge_task) = blufio_bridge::spawn_bridge(
        event_bus.clone(),
        config.bridge.clone(),
    );

    // Consumer task: drain BridgedMessage and dispatch via mux
    // NOTE: mux is moved into AgentLoop, so we need the connected_channels
    // or a separate send mechanism. See "Bridge Dispatch Architecture" below.
    // ...

    Some(bridge_task)
} else {
    info!("no bridge groups configured, bridge disabled");
    None
};
```

### Pattern 4: ChannelEvent Publishing
**What:** Publish `ChannelEvent::MessageReceived` to the event bus when inbound messages arrive.
**When to use:** In the AgentLoop message receive path or in ChannelMultiplexer's receive tasks.
**Example:**
```rust
// Source: blufio-bus/src/events.rs (ChannelEvent::MessageReceived)
event_bus.publish(BusEvent::Channel(ChannelEvent::MessageReceived {
    event_id: blufio_bus::new_event_id(),
    timestamp: blufio_bus::now_timestamp(),
    channel: msg.channel.clone(),
    sender_id: msg.sender_id.clone(),
    content: match &msg.content {
        MessageContent::Text(text) => Some(text.clone()),
        _ => None,
    },
    sender_name: None, // or extract from metadata
    is_bridged: false,
})).await;
```

### Bridge Dispatch Architecture

**The key challenge:** The `ChannelMultiplexer` is moved into `AgentLoop` via `Box::new(mux)` at line 872. The bridge dispatch task needs to send outbound messages to specific channels, but cannot access the mux directly.

**Recommended approach:** Create a separate `Arc<Vec<(String, Arc<dyn ChannelAdapter + Send + Sync>)>>` reference to the connected channels *before* the mux is moved into AgentLoop. The ChannelMultiplexer already stores `connected_channels` as `Arc<Vec<...>>`. Options:

1. **Add accessor to ChannelMultiplexer** -- Add a `pub fn connected_channels(&self)` method that returns `Arc::clone(&self.connected_channels)`. Grab a reference after `mux.connect()`, before `Box::new(mux)`. The bridge dispatch task uses this to send directly to channel adapters by name.

2. **Use mpsc channel indirection** -- Create an mpsc channel for bridge outbound messages. Pass the sender to the bridge dispatch task, pass the receiver to AgentLoop. AgentLoop processes both inbound channel messages and bridge outbound messages in its select loop.

3. **Extract send-only handle before moving mux** -- After `mux.connect()`, extract a lightweight send handle (channel name -> Arc<dyn ChannelAdapter>) and pass it to the bridge dispatch task.

**Recommendation:** Option 1 is simplest and requires minimal API change. The connected_channels Arc is already immutable after connect(), so sharing it is safe.

### Anti-Patterns to Avoid
- **Creating a second EventBus for the node system:** The whole point of this phase is bus unification. Delete `node_event_bus` creation, pass the global bus.
- **Blocking the bridge dispatch task:** Use `tokio::spawn` for the bridge consumer. Never block the agent loop.
- **Publishing events synchronously on the hot path:** `EventBus::publish()` is async (acquires RwLock for reliable subscribers). Keep it fast but be aware of the lock.
- **Forgetting `is_bridged: true` on dispatched messages:** When the bridge dispatch task sends bridged messages via channel adapters, the resulting ChannelEvent::MessageReceived MUST NOT set `is_bridged: false` or the bridge will infinite-loop.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Event bus | Custom broadcast | `blufio_bus::EventBus` | Already built and tested (Phase 29) |
| Bridge routing | Custom message forwarding | `blufio_bridge::spawn_bridge()` | Already built with loop prevention, attribution, group filtering (Phase 34) |
| Channel routing | Custom channel lookup | `ChannelMultiplexer` connected_channels | Already does name-based routing |
| Bridge message formatting | Custom formatter | `blufio_bridge::formatter::format_bridged_message()` | Already handles attribution |

**Key insight:** Both crates are complete and tested. This phase is purely about wiring -- connecting inputs to outputs in serve.rs.

## Common Pitfalls

### Pitfall 1: Bridge Infinite Loop
**What goes wrong:** Bridge receives a message, forwards it to another channel, that channel's adapter publishes a ChannelEvent::MessageReceived, bridge forwards it again, forever.
**Why it happens:** Not marking bridged messages with `is_bridged: true` when dispatching.
**How to avoid:** When the bridge dispatch task sends an `OutboundMessage`, any resulting ChannelEvent published for that message MUST have `is_bridged: true`. The bridge router already checks `is_bridged` and skips. BUT: the ChannelEvent is published by the *inbound* path, not the outbound path. So bridge dispatch does NOT trigger the inbound path -- it calls `channel.send()` directly, which is outbound-only. Loop prevention is handled by `BridgeManager::should_bridge()` checking `is_bridged` on the original event. The bridge dispatch does NOT re-publish events to the bus.
**Warning signs:** Messages duplicating across channels endlessly.

### Pitfall 2: Mux Moved Before Bridge Can Send
**What goes wrong:** `Box::new(mux)` moves the multiplexer into AgentLoop. Bridge dispatch task can no longer access it.
**Why it happens:** Ordering issue -- bridge needs to send via channels, but the mux is consumed.
**How to avoid:** Extract connected channel references AFTER `mux.connect()` but BEFORE `Box::new(mux)`. Use `Arc<Vec<(String, Arc<dyn ChannelAdapter>)>>` for the bridge dispatch task.
**Warning signs:** Compile error about moved value.

### Pitfall 3: EventBus Capacity Too Small
**What goes wrong:** Bridge subscriber lags behind and misses events.
**Why it happens:** Broadcast channel capacity too small for the global bus handling all event types.
**How to avoid:** Use capacity 1024 (up from node's 128). The bridge router already handles `RecvError::Lagged(n)` with a warning log (router.rs:91-93), so this is non-fatal but undesirable.
**Warning signs:** "bridge subscriber lagged behind event bus" warnings in logs.

### Pitfall 4: Feature Flag Gating Mismatch
**What goes wrong:** Compile errors when bridge feature is disabled.
**Why it happens:** Code referencing `blufio_bridge` without `#[cfg(feature = "bridge")]` guards.
**How to avoid:** All bridge-related code in serve.rs MUST be behind `#[cfg(feature = "bridge")]`, matching the existing pattern for all other optional adapters.
**Warning signs:** Compilation failure with `--no-default-features`.

### Pitfall 5: Event Publishing in Wrong Location
**What goes wrong:** Events not published, bridge receives nothing, bridging silently fails.
**Why it happens:** Publishing `ChannelEvent::MessageReceived` in a path that messages don't actually traverse.
**How to avoid:** Publish in the ChannelMultiplexer's per-channel receive tasks (where `msg.channel` is already tagged) or in AgentLoop's message receive handler. The multiplexer receive tasks are ideal because they already tag messages with channel names.
**Warning signs:** Bridge loop starts but never forwards any messages.

## Code Examples

Verified patterns from the actual codebase:

### EventBus Creation and Sharing
```rust
// Source: crates/blufio-bus/src/lib.rs:58-64
let event_bus = Arc::new(blufio_bus::EventBus::new(1024));

// Share with node system (replaces node_event_bus):
let conn_manager = Arc::new(blufio_node::ConnectionManager::new(
    node_store.clone(),
    event_bus.clone(),
    config.node.clone(),
));
```

### spawn_bridge() Call
```rust
// Source: crates/blufio-bridge/src/lib.rs:90-100
let (mut bridge_rx, bridge_handle) = blufio_bridge::spawn_bridge(
    event_bus.clone(),
    config.bridge.clone(),
);
```

### BridgedMessage Consumer Task
```rust
// BridgedMessage struct (from crates/blufio-bridge/src/router.rs:19-24):
// pub struct BridgedMessage {
//     pub target_channel: String,
//     pub content: String,
// }

// Consumer task dispatches to channels:
let channels_ref = connected_channels.clone(); // Arc<Vec<(String, Arc<dyn ChannelAdapter>)>>
tokio::spawn(async move {
    while let Some(bridged_msg) = bridge_rx.recv().await {
        // Find the target channel adapter
        let target = channels_ref.iter().find(|(name, _)| name == &bridged_msg.target_channel);
        if let Some((_, adapter)) = target {
            let outbound = OutboundMessage {
                session_id: None,
                channel: bridged_msg.target_channel.clone(),
                content: bridged_msg.content,
                reply_to: None,
                parse_mode: None,
                metadata: Some(serde_json::json!({"is_bridged": true}).to_string()),
            };
            if let Err(e) = adapter.send(outbound).await {
                warn!(
                    target = %bridged_msg.target_channel,
                    error = %e,
                    "bridge dispatch failed"
                );
            }
        } else {
            warn!(
                target = %bridged_msg.target_channel,
                "bridge target channel not found in multiplexer"
            );
        }
    }
    info!("bridge dispatch task completed");
});
```

### ChannelEvent Publishing in Mux Receive Tasks
```rust
// Source: crates/blufio-agent/src/channel_mux.rs:179-221 (existing receive task pattern)
// The event_bus Arc needs to be passed into ChannelMultiplexer or cloned into each task.
// Publishing location: inside the tokio::spawn in connect() after receiving a message.

// After msg.channel = channel_name.clone():
if let blufio_core::types::MessageContent::Text(ref text) = msg.content {
    event_bus_clone.publish(blufio_bus::BusEvent::Channel(
        blufio_bus::ChannelEvent::MessageReceived {
            event_id: blufio_bus::new_event_id(),
            timestamp: blufio_bus::now_timestamp(),
            channel: channel_name.clone(),
            sender_id: msg.sender_id.clone(),
            content: Some(text.clone()),
            sender_name: None,
            is_bridged: false,
        },
    )).await;
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Node-scoped EventBus (Phase 37) | Global EventBus (Phase 40) | This phase | All subsystems share events, enables webhooks + bridging |
| No bridge wiring | spawn_bridge() in serve.rs | This phase | Cross-channel message bridging active |
| No event publishing | ChannelEvent publishing | This phase | Bus has actual events flowing through it |

**No deprecated items** -- all APIs are current as of Phase 39.

## Open Questions

1. **Where to add EventBus to ChannelMultiplexer?**
   - What we know: The mux's receive tasks need to publish events. The mux doesn't currently take an EventBus.
   - What's unclear: Whether to modify ChannelMultiplexer's constructor to accept `Option<Arc<EventBus>>`, or publish in AgentLoop's message handler instead.
   - Recommendation: Modify ChannelMultiplexer to accept an optional `Arc<EventBus>` via a setter method (like `set_event_bus()`), consistent with the gateway's `set_storage()` pattern. This publishes events at the earliest point (before AgentLoop processes them).

2. **ChannelMultiplexer connected_channels access for bridge dispatch**
   - What we know: `connected_channels` is `Arc<Vec<...>>` internally, set after `connect()`.
   - What's unclear: No public accessor exists.
   - Recommendation: Add `pub fn connected_channels_ref(&self) -> Arc<Vec<(String, Arc<dyn ChannelAdapter + Send + Sync>)>>` to ChannelMultiplexer. Call after `connect()`, before moving mux into AgentLoop.

3. **Sender name for ChannelEvent::MessageReceived**
   - What we know: The bridge formats messages with `sender_name`. InboundMessage doesn't have a dedicated `sender_name` field -- only `sender_id`.
   - What's unclear: Whether sender_name should be extracted from metadata or left as None.
   - Recommendation: Set `sender_name: None` in this phase. Channel adapters can populate the metadata with display names, but that's adapter-specific enhancement for later.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (Rust built-in) |
| Config file | Workspace Cargo.toml |
| Quick run command | `cargo test -p blufio-bus && cargo test -p blufio-bridge` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| INFRA-01 | Global EventBus with broadcast + lag handling | unit | `cargo test -p blufio-bus` | Yes (8 tests) |
| INFRA-02 | Typed events (all 6 domains) | unit | `cargo test -p blufio-bus -- events` | Yes (5 tests) |
| INFRA-03 | Reliable mpsc subscribers | unit | `cargo test -p blufio-bus -- reliable` | Yes (2 tests) |
| INFRA-06 | Bridge rules + routing | unit | `cargo test -p blufio-bridge` | Yes (6 tests) |
| INFRA-06 | Bridge dispatch in serve.rs | integration | `cargo test -p blufio -- bridge` | No -- Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p blufio-bus && cargo test -p blufio-bridge && cargo check -p blufio`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] Integration test for bridge dispatch is not feasible without a full TestHarness extension -- verify via `cargo check -p blufio` (compile check) and manual inspection of wiring.
- [ ] ChannelMultiplexer `set_event_bus()` method needs unit test.
- [ ] ChannelMultiplexer `connected_channels_ref()` method needs unit test.

*(The existing unit tests in blufio-bus and blufio-bridge already cover the bus and bridge logic. This phase's validation is primarily that serve.rs compiles and wires things correctly.)*

## Sources

### Primary (HIGH confidence)
- `crates/blufio-bus/src/lib.rs` -- EventBus API (new, publish, subscribe, subscribe_reliable)
- `crates/blufio-bus/src/events.rs` -- All 6 BusEvent variants and ChannelEvent::MessageReceived fields
- `crates/blufio-bridge/src/lib.rs` -- spawn_bridge() signature, BridgeManager API
- `crates/blufio-bridge/src/router.rs` -- run_bridge_loop(), BridgedMessage struct, lag handling
- `crates/blufio/src/serve.rs` -- Full serve startup flow, node_event_bus at line 786, mux wiring
- `crates/blufio-agent/src/channel_mux.rs` -- ChannelMultiplexer API, connect(), send(), receive()
- `crates/blufio/Cargo.toml` -- bridge feature flag already defined, blufio-bus is non-optional
- `crates/blufio-config/src/model.rs` -- BlufioConfig.bridge HashMap<String, BridgeGroupConfig>

### Secondary (MEDIUM confidence)
- None needed -- all research is based on direct source code inspection.

### Tertiary (LOW confidence)
- None.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all crates are already in the project, APIs verified from source
- Architecture: HIGH -- serve.rs wiring patterns are well-established (Arc sharing, feature gates, tokio::spawn)
- Pitfalls: HIGH -- identified from direct code analysis (move semantics, loop prevention, feature flags)

**Research date:** 2026-03-07
**Valid until:** 2026-04-07 (stable -- internal project code, no external API changes)
