---
phase: 40-wire-global-eventbus-bridge
verified: 2026-03-07T20:15:00Z
status: passed
score: 8/8 must-haves verified
---

# Phase 40: Wire Global EventBus & Bridge Verification Report

**Phase Goal:** Create a single global EventBus in serve.rs shared across all subsystems, and wire the bridge loop
**Verified:** 2026-03-07T20:15:00Z
**Status:** passed
**Re-verification:** No -- initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | A single global Arc\<EventBus\> is created early in run_serve() and shared to all subsystems | VERIFIED | `serve.rs:218`: `let event_bus = Arc::new(blufio_bus::EventBus::new(1024));` -- created after tool registry, before channel setup. Passed to mux (line 348), node ConnectionManager (line 852), HeartbeatMonitor (line 862), and bridge (line 722). |
| 2 | The node-scoped node_event_bus is removed and replaced with the global bus | VERIFIED | `grep node_event_bus serve.rs` returns zero matches. ConnectionManager and HeartbeatMonitor both receive `event_bus.clone()` (lines 852, 862). |
| 3 | ChannelMultiplexer publishes ChannelEvent::MessageReceived to the bus for every inbound text message | VERIFIED | `channel_mux.rs:222-238`: In each per-channel receive task, checks `if let Some(ref bus) = event_bus_clone` and `if let MessageContent::Text(ref text) = msg.content`, then calls `bus.publish(BusEvent::Channel(ChannelEvent::MessageReceived {...}))`. Includes event_id, timestamp, channel, sender_id, content, is_bridged=false. |
| 4 | ChannelMultiplexer exposes connected_channels_ref() for bridge dispatch | VERIFIED | `channel_mux.rs:89-91`: `pub fn connected_channels_ref(&self) -> Arc<Vec<(String, Arc<dyn ChannelAdapter + Send + Sync>)>>` returns `Arc::clone(&self.connected_channels)`. |
| 5 | Bridge loop starts at serve.rs startup when bridge groups are configured | VERIFIED | `serve.rs:715-770`: Behind `#[cfg(feature = "bridge")]`, checks `!config.bridge.is_empty()`, then calls `blufio_bridge::spawn_bridge(event_bus.clone(), config.bridge.clone())`. Logs "cross-channel bridge started" with group count. Falls through to "no bridge groups configured, bridge disabled" when empty. |
| 6 | BridgedMessage values from the bridge are dispatched to the correct channel adapter | VERIFIED | `serve.rs:727-760`: Dispatch task reads `bridge_rx.recv().await`, finds target adapter via `bridge_channels.iter().find(|(name, _)| name == &bridged_msg.target_channel)`, constructs `OutboundMessage`, calls `adapter.send(outbound).await`. Logs warnings on failure or missing target. |
| 7 | Bridge dispatch does not re-publish events to the bus (no infinite loop) | VERIFIED | Dispatch task calls `adapter.send()` directly (outbound-only path). `serve.rs` contains zero `bus.publish` or `event_bus.*publish` calls -- all publishing is in `channel_mux.rs` receive tasks (inbound path only). Additionally, dispatched messages carry `metadata: {"is_bridged": true}` (line 742) as defense-in-depth. |
| 8 | Bridge is gated behind #[cfg(feature = "bridge")] and is a no-op when no groups configured | VERIFIED | `serve.rs:715`: `#[cfg(feature = "bridge")]` attribute. Line 716: `if !config.bridge.is_empty()` with else branch logging "bridge disabled". Cargo.toml confirms `bridge = ["dep:blufio-bridge"]` as optional feature in default features. |

**Score:** 8/8 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/blufio-agent/src/channel_mux.rs` | set_event_bus() setter and connected_channels_ref() accessor | VERIFIED | 399 lines. Contains `event_bus: Option<Arc<blufio_bus::EventBus>>` field (line 41), `set_event_bus()` (line 80), `connected_channels_ref()` (line 89), ChannelEvent publishing in connect() spawn block (lines 222-238), 2 new unit tests (lines 385-398). All 7 tests pass. |
| `crates/blufio/src/serve.rs` | Global EventBus creation, bridge wiring, node bus replacement | VERIFIED | Line 218: `EventBus::new(1024)`. Line 348: `mux.set_event_bus()`. Lines 715-770: bridge wiring block. Lines 850-862: node system using global `event_bus`. No `node_event_bus` references remain. `cargo check -p blufio` compiles cleanly. |
| `crates/blufio-agent/Cargo.toml` | blufio-bus dependency | VERIFIED | Line 12: `blufio-bus = { path = "../blufio-bus" }` |
| `crates/blufio-bus/src/lib.rs` | EventBus with broadcast + mpsc | VERIFIED | 208 lines. `EventBus::new()`, `publish()`, `subscribe()`, `subscribe_reliable()`, `subscriber_count()`. 12 tests pass including doc test. |
| `crates/blufio-bus/src/events.rs` | Typed events: Session, Channel, Skill, Node, Webhook, Batch | VERIFIED | 357 lines. `BusEvent` enum with 6 variants. Each has sub-enum with concrete event types. All derive Clone + Serialize + Deserialize. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| serve.rs | channel_mux.rs | `mux.set_event_bus(event_bus.clone())` | WIRED | Line 348 in serve.rs |
| channel_mux.rs | blufio_bus::EventBus | `bus.publish(BusEvent::Channel(ChannelEvent::MessageReceived {...}))` | WIRED | Lines 226-237 in channel_mux.rs, inside per-channel tokio::spawn receive task |
| serve.rs (node system) | blufio_bus::EventBus | `event_bus.clone()` replaces node_event_bus | WIRED | Lines 852, 862 -- ConnectionManager and HeartbeatMonitor use global bus. Zero references to `node_event_bus`. |
| serve.rs | blufio_bridge::spawn_bridge | `spawn_bridge(event_bus.clone(), config.bridge.clone())` | WIRED | Line 721 in serve.rs |
| serve.rs (bridge dispatch) | ChannelMultiplexer connected_channels | `connected_channels_ref()` called before mux moves, adapter.send() for dispatch | WIRED | Line 718: `mux.connected_channels_ref()`. Line 745: `adapter.send(outbound).await` |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| INFRA-01 | 40-01 | Internal event bus using tokio broadcast with lag handling | SATISFIED | `blufio-bus/src/lib.rs`: Uses `broadcast::channel(capacity)` (line 59). Lag handling: subscribers receive `RecvError::Lagged(n)`. EventBus is now globally instantiated in serve.rs and actively publishing events. |
| INFRA-02 | 40-01 | Event bus publishes typed events (session, channel, skill, node, webhook, batch) | SATISFIED | `blufio-bus/src/events.rs`: BusEvent enum with 6 variants -- Session, Channel, Skill, Node, Webhook, Batch. ChannelEvent::MessageReceived is actively published by ChannelMultiplexer. NodeEvent types used by ConnectionManager/HeartbeatMonitor. |
| INFRA-03 | 40-01 | Event bus uses mpsc for reliable subscribers (webhook delivery) | SATISFIED | `blufio-bus/src/lib.rs:97`: `subscribe_reliable()` returns `mpsc::Receiver<BusEvent>`. Uses `mpsc::channel(buffer)` for guaranteed delivery. Tested in `test_publish_to_reliable_subscriber` and `test_reliable_and_broadcast_coexist`. |
| INFRA-06 | 40-02 | Cross-channel bridging with configurable bridge rules in TOML | SATISFIED | Bridge crate (Phase 34) provides routing logic. Phase 40 wires it: `spawn_bridge()` called with `config.bridge` (TOML-parsed HashMap\<String, BridgeGroupConfig\>). Dispatch task routes BridgedMessage to channel adapters. Feature-gated, no-op when unconfigured. End-to-end path: channel receive -> EventBus publish -> bridge subscribe -> route -> dispatch -> adapter.send(). |

No orphaned requirements. All 4 requirement IDs from the ROADMAP (INFRA-01, INFRA-02, INFRA-03, INFRA-06) are claimed by plans and verified.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| (none) | - | - | - | No TODO, FIXME, HACK, PLACEHOLDER, or stub patterns found in modified files |

No anti-patterns detected. Both `channel_mux.rs` and the bridge-related sections of `serve.rs` are clean.

### Compilation & Test Verification

| Check | Result |
|-------|--------|
| `cargo check -p blufio` | PASS -- compiles cleanly |
| `cargo test -p blufio-agent -- channel_mux` | PASS -- 7/7 tests (including 2 new: test_set_event_bus, test_connected_channels_ref_empty) |
| `cargo test -p blufio-bus` | PASS -- 12/12 tests + 1 doc test |

### Human Verification Required

### 1. End-to-End Bridge Message Flow

**Test:** Configure two channels (e.g., Telegram + Discord) with a bridge group in TOML. Send a message on one channel.
**Expected:** Message appears on the other channel with bridge attribution. No infinite loop of repeated messages.
**Why human:** Requires running server with real channel adapters and sending actual messages to verify full async pipeline.

### 2. EventBus Under Load

**Test:** Send many messages rapidly on a bridged channel.
**Expected:** No broadcast lag warnings, no dropped messages on reliable subscribers, no task panics.
**Why human:** Requires runtime load testing; cannot verify async backpressure behavior statically.

### Gaps Summary

No gaps found. All 8 observable truths verified. All 5 artifacts confirmed substantive and wired. All 5 key links confirmed connected. All 4 requirement IDs satisfied. No anti-patterns detected. Compilation and tests pass.

---

_Verified: 2026-03-07T20:15:00Z_
_Verifier: Claude (gsd-verifier)_
