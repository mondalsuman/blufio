# Phase 43: Wire EventBus Event Publishers - Research

**Researched:** 2026-03-08
**Domain:** Rust async event publishing -- wiring EventBus into AgentLoop and WasmSkillRuntime
**Confidence:** HIGH

## Summary

Phase 43 closes the last two event publisher gaps identified in the v1.3 audit. The global EventBus (Phase 40) exists and webhook delivery (Phase 42) already subscribes and maps events to webhooks. What is missing is the actual _publishing_ of events: AgentLoop never publishes `ChannelEvent::MessageSent` after sending a response, and `WasmSkillRuntime` never publishes `SkillEvent::Invoked/Completed` around skill execution.

The implementation is straightforward wiring work. Both structs need an `Option<Arc<EventBus>>` field, a `set_event_bus()` setter (matching the established `ChannelMultiplexer` pattern), and a few `bus.publish()` calls at the right points. The `SkillInvocation` struct also needs a `session_id` field so `SkillEvent::Invoked` can include it. The `blufio-skill` crate needs `blufio-bus` added as a dependency.

**Primary recommendation:** Follow the exact setter-injection pattern from `ChannelMultiplexer::set_event_bus()`. Publish calls are awaited inline with log-and-continue on failure. No new event types or struct modifications beyond the `SkillInvocation.session_id` addition.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Setter method on both AgentLoop and WasmSkillRuntime: `set_event_bus(bus: Arc<EventBus>)`
- Field type: `Option<Arc<EventBus>>` -- None means no events published (graceful degradation)
- AgentLoop setter called in serve.rs between `AgentLoop::new()` and `agent_loop.run()`
- WasmSkillRuntime setter called in serve.rs after WasmSkillRuntime creation
- Publish `ChannelEvent::MessageSent` after `channel.send()` succeeds (not after persist_response)
- Final message only -- no events for edit-in-place intermediate updates
- Final response only -- one MessageSent at the end of the complete tool loop, not per-iteration
- Use existing MessageSent fields {event_id, timestamp, channel} -- no struct modifications
- Publishing is awaited (not spawned) -- matches ChannelMultiplexer pattern
- If bus.publish() fails: log warning, continue
- Publish `SkillEvent::Invoked` at start of invoke(), before skill execution
- Publish `SkillEvent::Completed` after execution finishes, with is_error=true on failure
- Completed fires always (success and error)
- Sequential guarantee: Invoked published and awaited before execution, Completed after
- WASM skills only -- built-in tools have no EventBus
- Add `session_id: Option<String>` field to SkillInvocation in blufio-core/types.rs
- Update all existing callers of SkillInvocation
- blufio-skill needs blufio-bus added as dependency in Cargo.toml
- No feature gating -- Option<Arc<EventBus>> handles runtime off-case
- Webhook delivery mapping already complete in Phase 42 -- no changes needed
- Completed event with is_error=true on invoke() failure -- no separate Failed variant
- Graceful no-bus behavior tested

### Claude's Discretion
- Exact placement of set_event_bus() call in serve.rs relative to other initialization
- How to surface WasmSkillRuntime for set_event_bus() wiring (may need to expose runtime instance)
- Test helper utilities for asserting EventBus events

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| API-16 | Webhooks deliver events with HMAC-SHA256 signing and exponential backoff retry | Phase 42 built webhook delivery; this phase wires the event publishers so events actually reach delivery. MessageSent -> chat.completed, SkillEvent::Completed -> tool.invoked. |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| blufio-bus | workspace | EventBus with broadcast/mpsc channels | Already built in Phase 40; provides `publish()`, `BusEvent`, typed events |
| blufio-core | workspace | SkillInvocation type definition | Owns the types that need session_id addition |
| blufio-agent | workspace | AgentLoop struct | Target for ChannelEvent::MessageSent publishing |
| blufio-skill | workspace | WasmSkillRuntime struct | Target for SkillEvent publishing |
| tokio | workspace | Async runtime | `bus.publish().await` is async |
| tracing | workspace | Structured logging | `warn!()` on publish failure |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| uuid | workspace | Event ID generation via `new_event_id()` | Already re-exported from blufio-bus::events |
| chrono | workspace | Timestamps via `now_timestamp()` | Already re-exported from blufio-bus::events |

### Dependency Change Required
| Crate | Change | Why |
|-------|--------|-----|
| blufio-skill/Cargo.toml | Add `blufio-bus = { path = "../blufio-bus" }` | WasmSkillRuntime needs to import EventBus and event types |

**No new external dependencies.** All libraries are already in the workspace.

## Architecture Patterns

### Pattern 1: Setter-Based EventBus Injection
**What:** Add `event_bus: Option<Arc<EventBus>>` field, expose `set_event_bus(&mut self, bus: Arc<EventBus>)` setter, call in serve.rs between construction and run.
**When to use:** Every subsystem that needs to publish events.
**Established in:** `ChannelMultiplexer::set_event_bus()` at channel_mux.rs:80

```rust
// Field on struct:
event_bus: Option<Arc<blufio_bus::EventBus>>,

// Setter:
pub fn set_event_bus(&mut self, bus: Arc<blufio_bus::EventBus>) {
    self.event_bus = Some(bus);
}
```

### Pattern 2: Non-Fatal Publish with Log
**What:** Await publish inline, log warning on failure, never break core flow.
**When to use:** Every event publish call.
**Established in:** ChannelMultiplexer receive tasks.

```rust
if let Some(ref bus) = self.event_bus {
    if let Err(e) = bus.publish(BusEvent::Channel(ChannelEvent::MessageSent {
        event_id: blufio_bus::events::new_event_id(),
        timestamp: blufio_bus::events::now_timestamp(),
        channel: channel_name.clone(),
    })).await {
        tracing::warn!(error = %e, "failed to publish MessageSent event");
    }
}
```

### Pattern 3: SkillInvocation Session ID Propagation
**What:** Add `session_id: Option<String>` to SkillInvocation, propagate from callers that have session context.
**When to use:** Enabling event correlation between skill invocations and sessions.

```rust
pub struct SkillInvocation {
    pub skill_name: String,
    pub input: serde_json::Value,
    pub session_id: Option<String>,  // NEW
}
```

### Anti-Patterns to Avoid
- **Spawning publish as a background task:** Creates ordering issues. Awaiting inline is fast (broadcast send is non-blocking internally) and preserves event ordering.
- **Publishing on every intermediate edit:** The decision says final message only. Edit-in-place updates are not MessageSent events.
- **Publishing per tool-loop iteration:** One MessageSent at the end of the complete response, not per LLM turn.
- **Panicking on publish failure:** Events are observability, not core flow. Always log-and-continue.

## Key Code Locations

### AgentLoop (blufio-agent/src/lib.rs)
| Item | Line | Notes |
|------|------|-------|
| `struct AgentLoop` | 53 | Add `event_bus: Option<Arc<EventBus>>` field |
| `AgentLoop::new()` | 78-112 | Initialize `event_bus: None` in constructor |
| Final send (non-edit path) | 527 | `self.channel.send(out).await` -- publish MessageSent after this succeeds |
| Final edit path | 530-539 | Also represents final delivery -- publish MessageSent after edit succeeds |
| `handle_inbound()` | ~200 | Has `channel_name: String` available for the event |

### WasmSkillRuntime (blufio-skill/src/sandbox.rs)
| Item | Line | Notes |
|------|------|-------|
| `struct WasmSkillRuntime` | 50 | Add `event_bus: Option<Arc<EventBus>>` field |
| `WasmSkillRuntime::new()` | 63 | Initialize `event_bus: None` |
| `invoke()` | 183 | Publish Invoked before line 185 (verify_before_execution), Completed after execution result |

### serve.rs (crates/blufio/src/serve.rs)
| Item | Line | Notes |
|------|------|-------|
| EventBus creation | 223 | `let event_bus = Arc::new(EventBus::new(1024))` |
| ChannelMultiplexer wiring | 353 | `mux.set_event_bus(event_bus.clone())` -- existing pattern to follow |
| AgentLoop::new() | 1004-1018 | Insert `agent_loop.set_event_bus(event_bus.clone())` between line 1018 and 1041 |
| agent_loop.run() | 1041 | Must call set_event_bus before this |

### SkillInvocation (blufio-core/src/types.rs)
| Item | Line | Notes |
|------|------|-------|
| `struct SkillInvocation` | 425 | Add `session_id: Option<String>` field |

### Event Types (blufio-bus/src/events.rs)
| Item | Line | Notes |
|------|------|-------|
| `ChannelEvent::MessageSent` | 94-101 | Fields: event_id, timestamp, channel -- no changes |
| `SkillEvent::Invoked` | 110-118 | Fields: event_id, timestamp, skill_name, session_id |
| `SkillEvent::Completed` | 121-130 | Fields: event_id, timestamp, skill_name, is_error |

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Event IDs | UUID generation | `blufio_bus::events::new_event_id()` | Already exists, consistent format |
| Timestamps | Manual formatting | `blufio_bus::events::now_timestamp()` | Already exists, ISO 8601 |
| Event types | New event enums | Existing `ChannelEvent::MessageSent`, `SkillEvent::{Invoked,Completed}` | Already defined in Phase 40 |
| Webhook mapping | Event-to-webhook logic | Phase 42 delivery.rs | Already maps MessageSent->chat.completed, Completed->tool.invoked |

## Common Pitfalls

### Pitfall 1: Publishing Before Send Succeeds
**What goes wrong:** Publishing MessageSent before `channel.send()` completes means the webhook fires even if delivery failed.
**Why it happens:** Placing the publish call before the send call.
**How to avoid:** Publish only after `channel.send()` returns Ok. On Err, skip publishing.
**Warning signs:** Webhook consumers receive chat.completed for messages that never reached the user.

### Pitfall 2: Missing SkillInvocation Callers
**What goes wrong:** Adding `session_id` to SkillInvocation breaks all existing construction sites.
**Why it happens:** SkillInvocation is constructed in multiple places (tool invoke, tests, CLI).
**How to avoid:** Search for all `SkillInvocation {` and `SkillInvocation::` patterns across the codebase. Add `session_id: None` to each.
**Warning signs:** Compilation errors after adding the field.

### Pitfall 3: WasmSkillRuntime Not Accessible in serve.rs
**What goes wrong:** WasmSkillRuntime may be created inside ToolRegistry or another abstraction, not directly in serve.rs.
**Why it happens:** The runtime is loaded into the tool registry, not held as a standalone variable.
**How to avoid:** Either (a) call set_event_bus on the runtime before registering it in the tool registry, or (b) expose the runtime through the registry for post-construction wiring.
**Warning signs:** No obvious `WasmSkillRuntime::new()` call in serve.rs.

### Pitfall 4: Double-Publishing on Edit Path
**What goes wrong:** Publishing MessageSent on both the initial send AND the final edit for edit-in-place channels.
**Why it happens:** Two code paths (line 527 non-edit, line 530-539 edit) both represent "final message delivered."
**How to avoid:** Publish exactly once after the final delivery point. The decision says "final message only" -- pick the single point after all sending/editing is complete.
**Warning signs:** Webhook consumers receive duplicate chat.completed events.

### Pitfall 5: SkillEvent::Invoked session_id Type Mismatch
**What goes wrong:** SkillEvent::Invoked has `session_id: String` (not Option), but SkillInvocation has `session_id: Option<String>`.
**Why it happens:** The event type was designed with a required session_id.
**How to avoid:** Use `.unwrap_or_default()` or `"unknown".to_string()` when constructing the Invoked event from an invocation with None session_id.
**Warning signs:** Type error at the publish site.

## Code Examples

### AgentLoop set_event_bus Implementation
```rust
// In struct AgentLoop (lib.rs):
event_bus: Option<Arc<blufio_bus::EventBus>>,

// Setter method:
pub fn set_event_bus(&mut self, bus: Arc<blufio_bus::EventBus>) {
    self.event_bus = Some(bus);
}

// In new(), add to Self { ... }:
event_bus: None,
```

### AgentLoop MessageSent Publishing (after final send)
```rust
// After the final channel.send() or edit_message() succeeds (around line 540):
if let Some(ref bus) = self.event_bus {
    if let Err(e) = bus.publish(blufio_bus::events::BusEvent::Channel(
        blufio_bus::events::ChannelEvent::MessageSent {
            event_id: blufio_bus::events::new_event_id(),
            timestamp: blufio_bus::events::now_timestamp(),
            channel: channel_name.clone(),
        },
    )).await {
        tracing::warn!(error = %e, "failed to publish MessageSent event");
    }
}
```

### WasmSkillRuntime Event Publishing
```rust
// At start of invoke(), after verify_before_execution:
if let Some(ref bus) = self.event_bus {
    if let Err(e) = bus.publish(blufio_bus::events::BusEvent::Skill(
        blufio_bus::events::SkillEvent::Invoked {
            event_id: blufio_bus::events::new_event_id(),
            timestamp: blufio_bus::events::now_timestamp(),
            skill_name: invocation.skill_name.clone(),
            session_id: invocation.session_id.clone().unwrap_or_default(),
        },
    )).await {
        tracing::warn!(error = %e, "failed to publish SkillEvent::Invoked");
    }
}

// ... execution ...

// After execution result (success or error):
if let Some(ref bus) = self.event_bus {
    if let Err(e) = bus.publish(blufio_bus::events::BusEvent::Skill(
        blufio_bus::events::SkillEvent::Completed {
            event_id: blufio_bus::events::new_event_id(),
            timestamp: blufio_bus::events::now_timestamp(),
            skill_name: invocation.skill_name.clone(),
            is_error: result.as_ref().map_or(true, |r| r.is_error),
        },
    )).await {
        tracing::warn!(error = %e, "failed to publish SkillEvent::Completed");
    }
}
```

### serve.rs Wiring
```rust
// After AgentLoop::new() (line 1018), before agent_loop.run() (line 1041):
agent_loop.set_event_bus(event_bus.clone());

// For WasmSkillRuntime -- depends on how runtime is surfaced:
// Option A: Before registering in tool registry
let mut wasm_runtime = WasmSkillRuntime::new()?;
wasm_runtime.set_event_bus(event_bus.clone());
// ... then register in tool_registry ...

// Option B: Via tool registry accessor
// tool_registry.wasm_runtime_mut().set_event_bus(event_bus.clone());
```

### Cargo.toml Addition
```toml
# In blufio-skill/Cargo.toml [dependencies]:
blufio-bus = { path = "../blufio-bus" }
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| No event publishing in AgentLoop | Setter-injected EventBus with inline publish | Phase 43 | Enables chat.completed webhook |
| No event publishing in WasmSkillRuntime | Setter-injected EventBus with Invoked/Completed | Phase 43 | Enables tool.invoked webhook |
| SkillInvocation without session context | session_id: Option<String> field added | Phase 43 | Enables event correlation |

## Open Questions

1. **WasmSkillRuntime accessibility in serve.rs**
   - What we know: WasmSkillRuntime is created somewhere and loaded into the ToolRegistry. serve.rs may not directly hold a reference.
   - What's unclear: Whether the runtime is created in serve.rs or in tool registry initialization code.
   - Recommendation: Search for WasmSkillRuntime::new() across the codebase during planning. If inside tool registry, either call set_event_bus before registration, or add an accessor method to ToolRegistry.

2. **SkillInvocation callers count**
   - What we know: SkillInvocation is constructed in sandbox.rs tests, tool invocation paths, and possibly CLI.
   - What's unclear: Exact number of construction sites.
   - Recommendation: `grep -r "SkillInvocation"` during implementation to find all sites and add `session_id: None`.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in Rust test framework) |
| Config file | Cargo.toml per-crate |
| Quick run command | `cargo test -p blufio-agent --lib && cargo test -p blufio-skill --lib` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| API-16-a | AgentLoop publishes MessageSent after send | unit | `cargo test -p blufio-agent --lib test_event_bus_message_sent -x` | Wave 0 |
| API-16-b | AgentLoop with no bus works normally | unit | `cargo test -p blufio-agent --lib test_no_event_bus -x` | Wave 0 |
| API-16-c | WasmSkillRuntime publishes Invoked before execution | unit | `cargo test -p blufio-skill --lib test_skill_event_invoked -x` | Wave 0 |
| API-16-d | WasmSkillRuntime publishes Completed after execution | unit | `cargo test -p blufio-skill --lib test_skill_event_completed -x` | Wave 0 |
| API-16-e | WasmSkillRuntime Completed has is_error=true on failure | unit | `cargo test -p blufio-skill --lib test_skill_event_completed_error -x` | Wave 0 |
| API-16-f | WasmSkillRuntime with no bus works normally | unit | `cargo test -p blufio-skill --lib test_no_event_bus_skill -x` | Wave 0 |
| API-16-g | SkillInvocation session_id propagated to event | unit | `cargo test -p blufio-skill --lib test_session_id_propagation -x` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p blufio-agent --lib && cargo test -p blufio-skill --lib`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] Test helpers for subscribing to EventBus and asserting published events
- [ ] Unit tests for AgentLoop EventBus publishing (API-16-a, API-16-b)
- [ ] Unit tests for WasmSkillRuntime EventBus publishing (API-16-c through API-16-g)

## Sources

### Primary (HIGH confidence)
- Direct source code inspection of: channel_mux.rs (set_event_bus pattern), lib.rs (AgentLoop struct, handle_inbound), sandbox.rs (WasmSkillRuntime, invoke), events.rs (event type definitions), types.rs (SkillInvocation), serve.rs (wiring), blufio-skill/Cargo.toml (dependencies)
- CONTEXT.md locked decisions from discussion phase

### Secondary (MEDIUM confidence)
- STATE.md accumulated decisions from Phases 40-42

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - all crates and types verified from source
- Architecture: HIGH - setter pattern verified from ChannelMultiplexer, exact line numbers confirmed
- Pitfalls: HIGH - identified from direct code inspection of publish points and struct definitions
- WasmSkillRuntime serve.rs wiring: MEDIUM - runtime may not be directly accessible; needs investigation during planning

**Research date:** 2026-03-08
**Valid until:** 2026-04-08 (internal project, stable patterns)
