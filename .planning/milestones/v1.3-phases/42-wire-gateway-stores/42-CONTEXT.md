# Phase 42: Wire Gateway Stores - Context

**Gathered:** 2026-03-07
**Status:** Ready for planning

<domain>
## Phase Boundary

Instantiate ApiKeyStore, WebhookStore, and BatchStore in serve.rs, wire them into GatewayState via GatewayChannel setters, wire the global EventBus into the gateway for webhook delivery, and spawn the webhook delivery loop. Covers requirements API-11 through API-18. No new crate code — pure wiring in the binary crate.

</domain>

<decisions>
## Implementation Decisions

### Claude's Discretion

The user delegated all implementation decisions to Claude (consistent with Phase 40 and 41 context patterns). The following areas should be resolved during planning based on codebase patterns:

**Store Instantiation:**
- All three stores (ApiKeyStore, WebhookStore, BatchStore) take `tokio_rusqlite::Connection` in constructors
- Whether to reuse the existing storage connection or create dedicated connections per store
- How to handle table creation/migrations (stores may need `CREATE TABLE IF NOT EXISTS` on first use)
- Feature gating: whether stores are behind `#[cfg(feature = "gateway")]` like other gateway wiring

**GatewayChannel Setters:**
- GatewayChannel currently has NO setter methods for stores — `key_store`, `webhook_store`, `batch_store`, and `event_bus` are all hardcoded to `None` in `connect()` (lib.rs:256-269)
- Need to add `set_api_key_store()`, `set_webhook_store()`, `set_batch_store()`, `set_event_bus()` methods following the existing pattern (set_providers, set_tools, set_storage)
- Each setter takes Arc-wrapped store and stores in Mutex<Option<...>>

**Webhook Delivery Wiring:**
- `deliver_with_retry()` in `webhooks/delivery.rs` takes `store: &WebhookStore` and `bus: Option<&EventBus>`
- Need a spawned tokio task that subscribes to EventBus, filters relevant events, and calls `deliver_with_retry()` for matching webhooks
- The global EventBus (created in Phase 40) should be passed to GatewayState.event_bus

**Wiring Order in serve.rs:**
- Store creation: after vault unlock (for DB access), before gateway connect()
- Following Phase 41 pattern: create stores, then call setters on GatewayChannel
- Event bus already exists as global `Arc<EventBus>` from Phase 40 wiring

</decisions>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches. Follows the established wiring patterns from Phase 40 (EventBus) and Phase 41 (ProviderRegistry + ToolRegistry).

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ApiKeyStore::new(conn)`: crates/blufio-gateway/src/api_keys/store.rs — takes tokio_rusqlite::Connection
- `WebhookStore::new(conn)`: crates/blufio-gateway/src/webhooks/store.rs — takes tokio_rusqlite::Connection
- `BatchStore::new(conn)`: crates/blufio-gateway/src/batch/store.rs — takes tokio_rusqlite::Connection
- `deliver_with_retry()`: crates/blufio-gateway/src/webhooks/delivery.rs — webhook delivery with HMAC signing and exponential backoff
- `deliver_single()`: crates/blufio-gateway/src/webhooks/delivery.rs — single webhook delivery
- `sign_payload()`: HMAC-SHA256 signing for webhook payloads
- Global `Arc<EventBus>`: created in Phase 40 wiring in serve.rs

### Established Patterns
- GatewayChannel setter pattern: `set_providers()`, `set_tools()`, `set_storage()` — all async, take Arc-wrapped values, store in Mutex<Option<...>>
- GatewayState construction in `connect()` (lib.rs:249-270): takes stored values from Mutex fields
- Feature-gated wiring: `#[cfg(feature = "gateway")]` blocks in serve.rs
- Store fields already exist in GatewayState: `key_store: None` (in AuthConfig), `webhook_store: None`, `batch_store: None`, `event_bus: None`

### Integration Points
- serve.rs gateway block: after `set_providers()` and `set_tools()` calls (added in Phase 41), before `mux.add_channel()`
- GatewayChannel (lib.rs): needs 4 new setter methods + 4 new Mutex fields for stores and event_bus
- GatewayState.auth.key_store: `Option<Arc<ApiKeyStore>>` field in AuthConfig (server.rs:33)
- GatewayState.webhook_store: `Option<Arc<WebhookStore>>` (server.rs:65)
- GatewayState.batch_store: `Option<Arc<BatchStore>>` (server.rs:67)
- GatewayState.event_bus: `Option<Arc<EventBus>>` (server.rs:69)

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 42-wire-gateway-stores*
*Context gathered: 2026-03-07*
