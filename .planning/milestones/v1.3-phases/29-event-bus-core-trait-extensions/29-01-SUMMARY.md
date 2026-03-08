---
phase: 29-event-bus-core-trait-extensions
plan: 01
status: complete
completed: "2026-03-05"
commit: 8846265
---

# Plan 01 Summary: Event Bus Crate (blufio-bus)

## What was done

Created the `blufio-bus` crate providing an internal typed event bus using tokio broadcast + mpsc channels.

### Key artifacts

- **`crates/blufio-bus/Cargo.toml`** -- New crate with serde, chrono, uuid, tokio (sync), tracing dependencies
- **`crates/blufio-bus/src/events.rs`** -- BusEvent enum with 6 domain variants (Session, Channel, Skill, Node, Webhook, Batch), each with 2 sub-variants carrying event_id + timestamp + domain fields. Helper functions: `new_event_id()`, `now_timestamp()`
- **`crates/blufio-bus/src/lib.rs`** -- EventBus struct with dual-channel pub/sub:
  - `publish()` fans out to both broadcast and all registered mpsc senders
  - `subscribe()` returns broadcast::Receiver for fire-and-forget consumers
  - `subscribe_reliable()` returns mpsc::Receiver for guaranteed-delivery consumers
  - `subscriber_count()` reports active broadcast subscriber count

### Requirements satisfied

- **INFRA-01**: Internal event bus exists with typed events for session, channel, skill, node, webhook, batch
- **INFRA-02**: Critical subscribers use mpsc (never silently drop); fire-and-forget use broadcast with logged errors
- **INFRA-03**: EventBus can be shared via Arc across threads (Send + Sync verified by compile-time assertion)

### Test results

13 tests pass (12 unit tests + 1 doctest):
- Event variant construction, Clone, Serialize/Deserialize roundtrip
- Broadcast subscriber receives events
- Reliable (mpsc) subscriber receives events
- Multiple broadcast subscribers each receive the same event
- Reliable and broadcast coexist
- Send + Sync compile-time assertion
- Subscriber count tracking
- Publishing with no subscribers does not panic
