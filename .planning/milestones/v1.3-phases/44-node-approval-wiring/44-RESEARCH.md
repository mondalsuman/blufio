# Phase 44: Node Approval Wiring - Research

**Researched:** 2026-03-08
**Domain:** Rust async wiring -- EventBus subscription + WebSocket message forwarding
**Confidence:** HIGH

## Summary

This phase is pure wiring work connecting two existing, fully-implemented subsystems: the `ApprovalRouter` (Phase 37) and the global `EventBus` (Phase 40). All behavioral logic (first-wins resolution, timeout-then-deny, broadcast to operators, `broadcast_actions` config filtering) already exists in `crates/blufio-node/src/approval.rs`. The two gaps are: (1) no background task subscribes to EventBus events and routes them through the ApprovalRouter, and (2) the `reconnect_with_backoff` message loop in `connection.rs` logs `ApprovalResponse` messages but does not forward them to `ApprovalRouter::handle_response()`.

Both changes follow established patterns already in the codebase. The EventBus subscription pattern is identical to `run_webhook_delivery` in `crates/blufio-gateway/src/webhooks/delivery.rs` (Phase 42/43). The ConnectionManager forwarding follows the existing `Arc<EventBus>` parameter pattern in `reconnect_with_backoff`. The implementation scope is small (two files changed, ~50 lines net).

**Primary recommendation:** Follow the webhook delivery engine pattern exactly -- subscribe via `event_bus.subscribe_reliable()`, spawn a background task that matches on `BusEvent` variants, extract a dot-separated event type string, check `requires_approval()`, and call `request_approval()`. For ConnectionManager forwarding, pass `Option<Arc<ApprovalRouter>>` into `reconnect_with_backoff` and call `router.handle_response()` in the `ApprovalResponse` match arm.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- ApprovalRouter subscribes to all BusEvents via EventBus, filters by matching event type against broadcast_actions config strings
- Uses reliable mpsc subscription (not broadcast) -- approval events are security-sensitive and must not be silently dropped
- Subscription spawned in serve.rs after node init, following Phase 40/43 wiring pattern (centralized wiring, not self-subscription)
- BusEvent variants map to broadcast_actions strings using dot-separated lowercase: `SkillEvent::Invoked` -> `"skill.invoked"`, `NodeEvent::Connected` -> `"node.connected"`, `SessionEvent::Started` -> `"session.started"`
- Matches webhook event naming convention (API-16)
- Operator configures: `broadcast_actions = ["skill.invoked", "session.started"]`
- All BusEvent types are eligible for approval routing -- no hardcoded subset
- Maximum flexibility: operator controls which action types need approval via TOML config
- No code changes needed when new event types are added in the future
- Post-action notification: EventBus events fire AFTER the action occurs, ApprovalRouter notifies operators of what happened
- Matches EventBus fire-and-forget design -- events are observations, not gates
- Pre-action intercept would require a different architecture (call-site changes); out of scope
- Clone `Arc<ApprovalRouter>` into `reconnect_with_backoff` function as an additional parameter
- Message loop's `ApprovalResponse` match arm calls `router.handle_response()` directly
- Same pattern as existing `Arc<EventBus>` parameter in `reconnect_with_backoff`

### Claude's Discretion
- Event type string extraction implementation (match on BusEvent variants to produce dot-separated string)
- Error handling for subscription task
- Startup ordering details within serve.rs node initialization block

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| NODE-05 | Approval routing broadcasts to all connected operator devices | Core logic already implemented (Phase 37). This phase wires the EventBus subscription (Plan 01) so events trigger approval routing, and fixes ConnectionManager forwarding (Plan 02) so operator responses reach `handle_response()`. Both gaps identified in Phase 39 re-verification. |

</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| blufio-bus | workspace | EventBus with `subscribe_reliable()` | Already the project's event system; provides guaranteed-delivery mpsc channel |
| blufio-node | workspace | ApprovalRouter, ConnectionManager | Existing node subsystem with all behavioral logic |
| tokio | workspace | async runtime, mpsc channels, spawn | Project standard async runtime |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tracing | workspace | Structured logging | All new code paths need info/debug/warn logging |
| dashmap | workspace | Concurrent map for connections | Already used by ConnectionManager |

### Alternatives Considered
None -- this is pure internal wiring using existing project infrastructure.

## Architecture Patterns

### Recommended Change Structure
```
crates/blufio-node/src/
├── approval.rs         # NO CHANGES -- already complete
├── connection.rs       # MODIFY: add approval_router param to reconnect_with_backoff,
│                       #          forward ApprovalResponse in message loop
├── types.rs            # NO CHANGES
└── lib.rs              # POSSIBLY: add event_type_string helper or put in approval.rs

crates/blufio/src/
└── serve.rs            # MODIFY: create ApprovalRouter, set on ConnectionManager,
                        #          spawn EventBus subscription task
```

### Pattern 1: EventBus Reliable Subscription (from webhook delivery)
**What:** Background task subscribes to EventBus via `subscribe_reliable()`, matches events, performs action
**When to use:** When a subsystem needs guaranteed delivery of all EventBus events
**Example:**
```rust
// Source: crates/blufio-gateway/src/webhooks/delivery.rs:170-259
pub async fn run_approval_subscription(
    bus: Arc<blufio_bus::EventBus>,
    router: Arc<ApprovalRouter>,
) {
    let mut rx = bus.subscribe_reliable(256).await;
    tracing::info!("approval event subscription started");

    while let Some(event) = rx.recv().await {
        let event_type = bus_event_to_type_string(&event);
        if router.requires_approval(&event_type) {
            let description = format!("{:?}", event);
            if let Err(e) = router.request_approval(&event_type, &description).await {
                tracing::error!(error = %e, "failed to request approval for {}", event_type);
            }
        }
    }
    tracing::warn!("approval event subscription stopped -- event bus closed");
}
```

### Pattern 2: BusEvent to Dot-Separated String Mapping
**What:** Convert BusEvent enum variants to `"domain.action"` strings matching `broadcast_actions` config
**When to use:** Needed for the approval subscription task to filter events
**Example:**
```rust
// New helper function -- maps all BusEvent variants exhaustively
fn bus_event_to_type_string(event: &BusEvent) -> String {
    match event {
        BusEvent::Session(SessionEvent::Created { .. }) => "session.created".to_string(),
        BusEvent::Session(SessionEvent::Closed { .. }) => "session.closed".to_string(),
        BusEvent::Channel(ChannelEvent::MessageReceived { .. }) => "channel.message_received".to_string(),
        BusEvent::Channel(ChannelEvent::MessageSent { .. }) => "channel.message_sent".to_string(),
        BusEvent::Skill(SkillEvent::Invoked { .. }) => "skill.invoked".to_string(),
        BusEvent::Skill(SkillEvent::Completed { .. }) => "skill.completed".to_string(),
        BusEvent::Node(NodeEvent::Connected { .. }) => "node.connected".to_string(),
        BusEvent::Node(NodeEvent::Disconnected { .. }) => "node.disconnected".to_string(),
        BusEvent::Node(NodeEvent::Paired { .. }) => "node.paired".to_string(),
        BusEvent::Node(NodeEvent::PairingFailed { .. }) => "node.pairing_failed".to_string(),
        BusEvent::Node(NodeEvent::Stale { .. }) => "node.stale".to_string(),
        BusEvent::Webhook(WebhookEvent::Triggered { .. }) => "webhook.triggered".to_string(),
        BusEvent::Webhook(WebhookEvent::DeliveryAttempted { .. }) => "webhook.delivery_attempted".to_string(),
        BusEvent::Batch(BatchEvent::Submitted { .. }) => "batch.submitted".to_string(),
        BusEvent::Batch(BatchEvent::Completed { .. }) => "batch.completed".to_string(),
    }
}
```

### Pattern 3: serve.rs Centralized Wiring (from Phases 40/43)
**What:** Create subsystem components, wire dependencies via `Arc` sharing, spawn background tasks
**When to use:** All subsystem initialization in serve.rs
**Example:**
```rust
// Source: serve.rs:912-940 (existing node system block)
// Current code creates conn_manager but no ApprovalRouter.
// Enhanced version:
#[cfg(feature = "node")]
if config.node.enabled {
    // ... existing NodeStore and ConnectionManager creation ...

    // Create approval router (needs conn_manager, store, config)
    let approval_router = Arc::new(blufio_node::ApprovalRouter::new(
        conn_manager.clone(),
        node_store.clone(),
        config.node.approval.clone(),
    ));

    // Wire into ConnectionManager (requires &mut so must be done before Arc::new)
    // NOTE: ConnectionManager is already Arc<> -- need to handle this
    conn_manager.set_approval_router(approval_router.clone());

    // Spawn EventBus subscription for approval routing
    {
        let approval_bus = event_bus.clone();
        let approval_router_clone = approval_router.clone();
        tokio::spawn(async move {
            // subscription function here
        });
        info!("approval event subscription spawned");
    }
}
```

### Pattern 4: ConnectionManager Parameter Extension (from reconnect_with_backoff)
**What:** Add `Option<Arc<ApprovalRouter>>` parameter to `reconnect_with_backoff` free function
**When to use:** When the message receive loop needs access to the approval router
**Example:**
```rust
// Source: connection.rs:251-258 (current signature)
async fn reconnect_with_backoff(
    peer: &NodeInfo,
    endpoint: &str,
    connections: Arc<DashMap<NodeId, mpsc::Sender<NodeMessage>>>,
    node_states: Arc<DashMap<NodeId, NodeRuntimeState>>,
    event_bus: Arc<EventBus>,
    store: Arc<NodeStore>,
    config: &NodeConfig,
    approval_router: Option<Arc<crate::approval::ApprovalRouter>>,  // NEW
)

// In the ApprovalResponse match arm (connection.rs:317-332):
NodeMessage::ApprovalResponse {
    ref request_id,
    approved,
    ref responder_node,
} => {
    debug!(
        request_id = %request_id,
        approved = approved,
        responder = %responder_node,
        "received approval response from peer"
    );
    if let Some(ref router) = approval_router {
        match router.handle_response(request_id, approved, responder_node).await {
            Ok(was_first) => {
                debug!(request_id = %request_id, was_first = was_first, "approval response forwarded");
            }
            Err(e) => {
                warn!(request_id = %request_id, error = %e, "failed to handle approval response");
            }
        }
    }
}
```

### Anti-Patterns to Avoid
- **Self-subscription inside ApprovalRouter::new():** The project pattern (Phases 40/43) is centralized wiring in serve.rs, not self-subscription inside constructors. ApprovalRouter should not know about EventBus.
- **Using broadcast (fire-and-forget) for approval events:** Approval events are security-sensitive; must use `subscribe_reliable()` to guarantee delivery. The CONTEXT.md locks this decision.
- **Hardcoding event type whitelist:** The operator controls which events need approval via `broadcast_actions` TOML config. The code must be exhaustive over BusEvent variants and filter dynamically.
- **Blocking inside the subscription loop:** `request_approval()` is async and spawns a timeout task. The subscription loop should await it but not block on the oneshot result. The approval fires and the loop continues.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Event bus subscription | Custom channel management | `EventBus::subscribe_reliable(256)` | Already handles mpsc setup, capacity, delivery guarantees |
| First-wins approval | Custom mutex/atomics | `ApprovalRouter::handle_response()` uses `DashMap::remove()` | Atomic check-and-remove already correct |
| Event type naming | Ad-hoc string formatting | Exhaustive match on BusEvent variants | Compiler catches missing variants when new events added |

**Key insight:** All behavioral logic exists. This phase only connects existing pieces. There should be zero new business logic.

## Common Pitfalls

### Pitfall 1: ConnectionManager is Already Arc -- Cannot Call &mut self Methods
**What goes wrong:** `set_approval_router(&mut self, ...)` requires `&mut self`, but by the time we want to set the router, `conn_manager` may already be wrapped in `Arc`.
**Why it happens:** `ConnectionManager::new()` returns `Self`, then `Arc::new()` wraps it in serve.rs. After Arc wrapping, `set_approval_router` cannot be called because Arc only gives shared references.
**How to avoid:** Create `ApprovalRouter` AFTER creating `ConnectionManager` but BEFORE wrapping in `Arc::new()`. The serve.rs code currently does `Arc::new(ConnectionManager::new(...))` on one line. Split it:
```rust
let mut conn_manager = blufio_node::ConnectionManager::new(node_store.clone(), event_bus.clone(), config.node.clone());
let approval_router = Arc::new(blufio_node::ApprovalRouter::new(/* needs Arc<ConnectionManager>... */));
```
**However**, there is a circular dependency: `ApprovalRouter::new()` takes `Arc<ConnectionManager>`, but `ConnectionManager::set_approval_router()` takes `&mut self`. Solution: create `ConnectionManager` first, wrap in `Arc`, create `ApprovalRouter` with the `Arc<ConnectionManager>`, then use the existing `set_approval_router` via `Arc::get_mut()` (only works if no other Arc clones exist yet) or change the setter to work differently.
**Warning signs:** Compilation error on `set_approval_router` after `Arc::new()`.

### Pitfall 2: Circular Reference Between ApprovalRouter and ConnectionManager
**What goes wrong:** `ApprovalRouter` holds `Arc<ConnectionManager>`, and `ConnectionManager` holds `Option<Arc<ApprovalRouter>>`. This is a circular `Arc` reference that prevents memory deallocation.
**Why it happens:** Both need references to each other for broadcasting and response handling.
**How to avoid:** This is acceptable in this project because both live for the entire process lifetime (created at startup, never dropped until shutdown). The circular Arc does not cause a leak since the process exits. This is a conscious design choice documented in Phase 37 STATE.md notes. Do NOT refactor to use `Weak` -- it adds complexity for no benefit in a long-lived server.
**Warning signs:** None -- this is intentional.

### Pitfall 3: reconnect_with_backoff Call Site Must Pass ApprovalRouter
**What goes wrong:** After adding the new parameter to `reconnect_with_backoff`, the call site in `reconnect_all()` (line 102) must also pass it. But `reconnect_all()` doesn't have access to the approval router.
**Why it happens:** `reconnect_all()` is a method on `ConnectionManager`, which holds `approval_router: Option<Arc<ApprovalRouter>>`. It can access `self.approval_router.clone()`.
**How to avoid:** Pass `self.approval_router.clone()` at the call site in `reconnect_all()`. The field already exists on `ConnectionManager`.
**Warning signs:** Compilation error about missing argument.

### Pitfall 4: Event Description for request_approval Should Be Human-Readable
**What goes wrong:** Using `format!("{:?}", event)` produces verbose Debug output with all fields.
**Why it happens:** Quick implementation without considering readability.
**How to avoid:** Build a concise description string from the event's key fields (e.g., "skill.invoked: weather in session sess-123"). The description is what operators see in approval requests.
**Warning signs:** Operators receive unreadable approval notifications.

### Pitfall 5: subscribe_reliable Buffer Size
**What goes wrong:** If the buffer is too small and ApprovalRouter processing is slow, the reliable sender logs `"reliable subscriber dropped event -- channel full or closed"` errors.
**Why it happens:** Approval requests involve async SQLite writes and WebSocket broadcasts, which may be slower than event publishing rate.
**How to avoid:** Use buffer size 256 (same as webhook delivery). Approval events are filtered by `requires_approval()` so only configured event types trigger the slow path.
**Warning signs:** Log messages about dropped reliable events.

## Code Examples

Verified patterns from the project codebase:

### Webhook Delivery Subscription (Reference Pattern)
```rust
// Source: crates/blufio-gateway/src/webhooks/delivery.rs:170-175
pub async fn run_webhook_delivery(
    bus: Arc<blufio_bus::EventBus>,
    store: Arc<WebhookStore>,
    client: reqwest::Client,
) {
    let mut rx = bus.subscribe_reliable(256).await;
    tracing::info!("webhook delivery engine started");
    while let Some(event) = rx.recv().await {
        // match on event variants, process...
    }
}
```

### Webhook Delivery Spawn in serve.rs (Reference Pattern)
```rust
// Source: crates/blufio/src/serve.rs:675-690
{
    let delivery_bus = event_bus.clone();
    let delivery_store = webhook_store;
    tokio::spawn(async move {
        blufio_gateway::webhooks::delivery::run_webhook_delivery(
            delivery_bus,
            delivery_store,
            reqwest::Client::new(),
        )
        .await;
    });
    info!("webhook delivery engine spawned");
}
```

### Node System Init in serve.rs (Current Code to Modify)
```rust
// Source: crates/blufio/src/serve.rs:912-940
#[cfg(feature = "node")]
if config.node.enabled {
    info!(port = config.node.listen_port, "starting node system");
    let node_conn = blufio_storage::open_connection(&config.storage.database_path).await?;
    let node_store = Arc::new(blufio_node::NodeStore::new(node_conn));
    let conn_manager = Arc::new(blufio_node::ConnectionManager::new(
        node_store.clone(),
        event_bus.clone(),
        config.node.clone(),
    ));
    conn_manager.reconnect_all().await;
    let heartbeat_monitor = blufio_node::HeartbeatMonitor::new(
        conn_manager.clone(),
        event_bus.clone(),
        config.node.clone(),
    );
    tokio::spawn(async move {
        heartbeat_monitor.run().await;
    });
    info!("node system started");
}
```

### ApprovalRouter Constructor (Current API)
```rust
// Source: crates/blufio-node/src/approval.rs:61-72
pub fn new(
    conn_manager: Arc<ConnectionManager>,
    store: Arc<NodeStore>,
    config: NodeApprovalConfig,
) -> Self
```

### ConnectionManager set_approval_router (Current API)
```rust
// Source: crates/blufio-node/src/connection.rs:77-79
pub fn set_approval_router(&mut self, router: Arc<crate::approval::ApprovalRouter>) {
    self.approval_router = Some(router);
}
```

### ApprovalResponse Match Arm (Current Code -- Needs Forwarding)
```rust
// Source: crates/blufio-node/src/connection.rs:317-332
NodeMessage::ApprovalResponse {
    ref request_id,
    approved,
    ref responder_node,
} => {
    debug!(
        request_id = %request_id,
        approved = approved,
        responder = %responder_node,
        "received approval response from peer"
    );
    // Currently just logs -- needs router.handle_response() call
}
```

### BusEvent Variants (Complete Enum for Exhaustive Match)
```rust
// Source: crates/blufio-bus/src/events.rs:25-39
pub enum BusEvent {
    Session(SessionEvent),   // Created, Closed
    Channel(ChannelEvent),   // MessageReceived, MessageSent
    Skill(SkillEvent),       // Invoked, Completed
    Node(NodeEvent),         // Connected, Disconnected, Paired, PairingFailed, Stale
    Webhook(WebhookEvent),   // Triggered, DeliveryAttempted
    Batch(BatchEvent),       // Submitted, Completed
}
// Total: 15 leaf variants to map to dot-separated strings
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| ApprovalRouter unconnected to EventBus | Will subscribe via reliable mpsc | Phase 44 | Events trigger approval routing |
| ConnectionManager logs ApprovalResponse | Will forward to handle_response() | Phase 44 | First-wins resolution actually works for WebSocket peers |

**Deprecated/outdated:** Nothing deprecated. Both subsystems are current (Phase 37 for nodes, Phase 40 for EventBus).

## Open Questions

1. **Arc<ConnectionManager> vs &mut self for set_approval_router**
   - What we know: `set_approval_router` takes `&mut self`. Current serve.rs wraps ConnectionManager in `Arc` immediately. `ApprovalRouter::new` needs `Arc<ConnectionManager>`.
   - What's unclear: Exact initialization order to satisfy both constraints.
   - Recommendation: Create `ConnectionManager` (not Arc), then wrap in Arc. Create `ApprovalRouter` with `Arc<ConnectionManager>` clone. Use `Arc::get_mut(&mut conn_manager_arc)` to call `set_approval_router` before any clones are made. If `Arc::get_mut` is awkward, change `set_approval_router` to use interior mutability (`RwLock` or `std::sync::Mutex`) -- but Pitfall 1 analysis shows `Arc::get_mut` should work because no clones exist yet at that point in initialization.

2. **Where to place `bus_event_to_type_string` helper**
   - What we know: Could go in `blufio-bus/src/events.rs` (near the type definition) or in `blufio-node/src/approval.rs` (near the consumer).
   - What's unclear: Project convention for such helpers.
   - Recommendation: Place in `blufio-bus/src/events.rs` as a method `impl BusEvent { pub fn event_type_string(&self) -> &'static str }`. This keeps naming close to the enum definition and allows future consumers (not just approvals) to reuse it. Using `&'static str` avoids allocation since all strings are literals.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test + tokio::test |
| Config file | Cargo workspace, per-crate test modules |
| Quick run command | `cargo test -p blufio-node` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| NODE-05a | BusEvent to type string mapping covers all 15 variants | unit | `cargo test -p blufio-bus -- bus_event_type_string -x` | Wave 0 |
| NODE-05b | Approval subscription filters by broadcast_actions config | unit | `cargo test -p blufio-node -- approval_subscription -x` | Wave 0 |
| NODE-05c | ApprovalResponse forwarded to handle_response | unit | `cargo test -p blufio-node -- approval_response_forward -x` | Wave 0 |
| NODE-05d | Full compilation with node feature | smoke | `cargo check --features node` | Existing |

### Sampling Rate
- **Per task commit:** `cargo test -p blufio-node && cargo test -p blufio-bus`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `blufio-bus/src/events.rs` test for `event_type_string()` -- covers all 15 variants
- [ ] `blufio-node/src/approval.rs` or `connection.rs` test for response forwarding logic
- [ ] Compilation smoke test: `cargo check -p blufio --features node`

## Sources

### Primary (HIGH confidence)
- `crates/blufio-bus/src/lib.rs` -- EventBus API: `subscribe_reliable(buffer) -> mpsc::Receiver<BusEvent>`
- `crates/blufio-bus/src/events.rs` -- All 15 BusEvent leaf variants (6 domains, 15 total)
- `crates/blufio-node/src/approval.rs` -- ApprovalRouter: `requires_approval()`, `request_approval()`, `handle_response()`
- `crates/blufio-node/src/connection.rs` -- ConnectionManager: `set_approval_router()`, `reconnect_with_backoff()`, ApprovalResponse match arm at line 317
- `crates/blufio/src/serve.rs` -- Node system init block at line 912, webhook delivery spawn at line 675
- `crates/blufio-gateway/src/webhooks/delivery.rs` -- Reference pattern for EventBus subscription
- `crates/blufio-config/src/model.rs` -- `NodeApprovalConfig` with `broadcast_actions: Vec<String>` and `timeout_secs: u64`

### Secondary (MEDIUM confidence)
None needed -- all code is local to the project.

### Tertiary (LOW confidence)
None.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all components already exist in the codebase, no new dependencies
- Architecture: HIGH -- patterns directly copied from webhook delivery (Phase 42/43)
- Pitfalls: HIGH -- identified from direct code analysis (Arc/mut conflict, circular reference, buffer sizing)

**Research date:** 2026-03-08
**Valid until:** Indefinite -- this is internal wiring of stable project components
