# Phase 43: Wire EventBus Event Publishers - Context

**Gathered:** 2026-03-08
**Status:** Ready for planning

<domain>
## Phase Boundary

Wire EventBus into AgentLoop and WasmSkillRuntime so `chat.completed` and `tool.invoked` webhook events actually fire. Closes 2 event publisher gaps from v1.3 audit: AgentLoop publishes ChannelEvent::MessageSent after outbound delivery, WasmSkillRuntime publishes SkillEvent::Invoked/Completed around skill execution. No new event types or webhook delivery changes — Phase 42 already handles the event-to-webhook mapping.

</domain>

<decisions>
## Implementation Decisions

### EventBus Injection Pattern
- Setter method on both AgentLoop and WasmSkillRuntime: `set_event_bus(bus: Arc<EventBus>)`
- Consistent with existing ChannelMultiplexer::set_event_bus() pattern
- Field type: `Option<Arc<EventBus>>` — None means no events published (graceful degradation)
- AgentLoop setter called in serve.rs between `AgentLoop::new()` and `agent_loop.run()`
- WasmSkillRuntime setter called in serve.rs after WasmSkillRuntime creation

### AgentLoop Event Publishing
- Publish `ChannelEvent::MessageSent` after `channel.send()` succeeds (not after persist_response)
- Final message only — no events for edit-in-place intermediate updates
- Final response only — one MessageSent at the end of the complete tool loop, not per-iteration
- Use existing MessageSent fields {event_id, timestamp, channel} — no struct modifications
- Publishing is awaited (not spawned) — matches ChannelMultiplexer pattern
- If bus.publish() fails: log warning, continue — event publishing is observability, not core

### WasmSkillRuntime Event Publishing
- Publish `SkillEvent::Invoked` at start of invoke(), before skill execution
- Publish `SkillEvent::Completed` after execution finishes, with is_error=true on failure
- Completed fires always (success and error) — webhook consumers want to know about failures
- Sequential guarantee: Invoked published and awaited before execution, Completed after
- WASM skills only — built-in tools (BashTool, FileTool, HttpTool) go through Tool::invoke() which has no EventBus
- Use existing SkillEvent field sets — no modifications to blufio-bus event types
- If bus.publish() fails: log warning, continue

### SkillInvocation Session ID
- Add `session_id: Option<String>` field to SkillInvocation in blufio-core/types.rs
- Callers with session context populate it; callers without pass None
- Update all existing callers (SkillInvocation is internal, not public API)
- WasmSkillRuntime::invoke() uses this for SkillEvent::Invoked session_id field

### Dependency Changes
- blufio-agent already depends on blufio-bus (no change needed)
- blufio-skill needs blufio-bus added as required dependency in Cargo.toml
- No feature gating — EventBus is always compiled in, Option<Arc<EventBus>> handles runtime off-case

### Webhook Delivery Mapping
- Already complete in Phase 42 (delivery.rs:180-206) — no changes needed
- ChannelEvent::MessageSent → "chat.completed"
- SkillEvent::Completed → "tool.invoked"

### Error Handling
- Completed event with is_error=true on invoke() failure — no separate Failed variant
- Graceful no-bus behavior tested: AgentLoop and WasmSkillRuntime work normally when EventBus is None

### Claude's Discretion
- Exact placement of set_event_bus() call in serve.rs relative to other initialization
- How to surface WasmSkillRuntime for set_event_bus() wiring (may need to expose runtime instance)
- Test helper utilities for asserting EventBus events

</decisions>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches. Follows established wiring patterns from Phase 40 (global EventBus), Phase 41 (provider registry), Phase 42 (gateway stores + webhook delivery).

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ChannelMultiplexer::set_event_bus()` (channel_mux.rs:80): Established pattern for setter-based EventBus injection
- `blufio_bus::EventBus::publish()`: Async method, broadcast send, fast — safe to await inline
- `blufio_bus::events::{new_event_id, now_timestamp}`: Helper functions for event construction
- `ChannelEvent::MessageSent`: Already defined with {event_id, timestamp, channel}
- `SkillEvent::{Invoked, Completed}`: Already defined with correct fields
- `run_webhook_delivery()` (delivery.rs:170): Already subscribes to bus and maps events to webhooks

### Established Patterns
- Setter-based injection: `set_event_bus()`, `set_providers()`, `set_tools()`, `set_storage()` — all async, store in Option<Arc<...>>
- Event publishing: `if let Some(ref bus) = event_bus { bus.publish(...).await; }` in ChannelMultiplexer
- Non-fatal event publishing: log warning on publish failure, never break core flow
- `Arc<EventBus>` shared via clone across subsystems

### Integration Points
- serve.rs:1004-1041: AgentLoop::new() → set_event_bus() → run() wiring location
- serve.rs WasmSkillRuntime: Currently not directly created in serve.rs — needs surfacing for setter
- AgentLoop::handle_inbound() line ~527: After channel.send() succeeds — publish point for MessageSent
- WasmSkillRuntime::invoke() (sandbox.rs:183): Publish Invoked before, Completed after
- blufio-core/types.rs:425: SkillInvocation struct — add session_id field

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 43-wire-eventbus-publishers*
*Context gathered: 2026-03-08*
