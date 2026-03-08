# Phase 44: Node Approval Wiring - Context

**Gathered:** 2026-03-08
**Status:** Ready for planning

<domain>
## Phase Boundary

Wire ApprovalRouter into EventBus for event-driven triggering and fix ConnectionManager forwarding of ApprovalResponse messages. This is gap closure from Phase 37 — all behavioral decisions (first-wins, timeout-then-deny, broadcast_actions config) are already implemented. This phase connects the existing pieces.

</domain>

<decisions>
## Implementation Decisions

### EventBus subscription pattern
- ApprovalRouter subscribes to all BusEvents via EventBus, filters by matching event type against broadcast_actions config strings
- Uses reliable mpsc subscription (not broadcast) — approval events are security-sensitive and must not be silently dropped
- Subscription spawned in serve.rs after node init, following Phase 40/43 wiring pattern (centralized wiring, not self-subscription)

### Event naming convention
- BusEvent variants map to broadcast_actions strings using dot-separated lowercase: `SkillEvent::Invoked` → `"skill.invoked"`, `NodeEvent::Connected` → `"node.connected"`, `SessionEvent::Started` → `"session.started"`
- Matches webhook event naming convention (API-16)
- Operator configures: `broadcast_actions = ["skill.invoked", "session.started"]`

### Event scope
- All BusEvent types are eligible for approval routing — no hardcoded subset
- Maximum flexibility: operator controls which action types need approval via TOML config
- No code changes needed when new event types are added in the future

### Approval triggering model
- Post-action notification: EventBus events fire AFTER the action occurs, ApprovalRouter notifies operators of what happened
- Matches EventBus fire-and-forget design — events are observations, not gates
- Pre-action intercept would require a different architecture (call-site changes); out of scope

### ConnectionManager forwarding
- Clone `Arc<ApprovalRouter>` into `reconnect_with_backoff` function as an additional parameter
- Message loop's `ApprovalResponse` match arm calls `router.handle_response()` directly
- Same pattern as existing `Arc<EventBus>` parameter in `reconnect_with_backoff`

### Claude's Discretion
- Event type string extraction implementation (match on BusEvent variants to produce dot-separated string)
- Error handling for subscription task
- Startup ordering details within serve.rs node initialization block

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ApprovalRouter` (`crates/blufio-node/src/approval.rs`): Complete implementation with `requires_approval()`, `request_approval()`, `handle_response()` — just needs EventBus connection
- `ConnectionManager` (`crates/blufio-node/src/connection.rs`): Has `approval_router: Option<Arc<ApprovalRouter>>` field and `set_approval_router()` — plumbing prepared but unused
- `EventBus` (`crates/blufio-bus`): `subscribe_reliable()` returns mpsc::Receiver<BusEvent> — used by webhook delivery (Phase 42)

### Established Patterns
- Phase 40: Global EventBus created in serve.rs, shared via `Arc<EventBus>` to all subsystems
- Phase 42: Webhook delivery spawns background task with `event_bus.subscribe_reliable()` — same pattern for approval subscription
- Phase 43: EventBus.publish() is fire-and-forget, returns `()`
- `reconnect_with_backoff` already takes `Arc<EventBus>`, `Arc<NodeStore>`, `Arc<DashMap<...>>` — adding `Option<Arc<ApprovalRouter>>` follows the same pattern

### Integration Points
- `serve.rs`: ApprovalRouter creation, EventBus subscription spawn, ConnectionManager.set_approval_router()
- `connection.rs:317-329`: ApprovalResponse match arm needs `router.handle_response()` call
- `connection.rs:251`: `reconnect_with_backoff` signature needs `Option<Arc<ApprovalRouter>>` parameter

</code_context>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches. This is pure wiring following established patterns from Phases 40-43.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 44-node-approval-wiring*
*Context gathered: 2026-03-08*
