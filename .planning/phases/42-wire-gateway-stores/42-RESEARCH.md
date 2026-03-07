# Phase 42: Wire Gateway Stores - Research

**Researched:** 2026-03-07
**Domain:** Rust binary-crate wiring -- ApiKeyStore, WebhookStore, BatchStore, and EventBus into GatewayState
**Confidence:** HIGH

## Summary

Phase 42 is the final gap-closure wiring phase: it connects three existing SQLite-backed stores (ApiKeyStore, WebhookStore, BatchStore) and the global EventBus into GatewayChannel, then spawns the webhook delivery background loop. All store implementations exist in blufio-gateway (Phase 32), the EventBus exists globally in serve.rs (Phase 40), the GatewayState fields for these four values already exist (hardcoded to `None` in lib.rs:256-269), and the handler code already consumes them via `state.auth.key_store`, `state.webhook_store`, `state.batch_store`, and `state.event_bus`. The only missing piece is the wiring: setter methods on GatewayChannel and the initialization calls in serve.rs.

This is a pure wiring phase with zero new crate code. The work consists of: (1) adding four new `Mutex<Option<...>>` fields and four setter methods to `GatewayChannel` following the exact pattern of `set_providers()`, `set_tools()`, and `set_storage()`, (2) opening a `tokio_rusqlite::Connection` for the stores via the existing `blufio_storage::open_connection()` factory (which handles encryption via `BLUFIO_DB_KEY`), (3) instantiating the three stores and calling the setters in serve.rs within the existing `#[cfg(feature = "gateway")]` block, (4) passing the global `Arc<EventBus>` to the gateway via a setter, and (5) spawning `run_webhook_delivery()` as a background tokio task.

**Primary recommendation:** Follow the established Phase 41 pattern exactly -- add fields + setters to GatewayChannel, populate them in serve.rs between gateway construction and `mux.add_channel()`. Open a single new `tokio_rusqlite::Connection` shared across all three stores (they already use SQLite's single-writer pattern via tokio_rusqlite's background thread). The V7 migration (which creates the tables) is already run by `Database::open()` during SqliteStorage initialization, so no additional migration is needed.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions
None -- user delegated all implementation decisions to Claude.

### Claude's Discretion
- All three stores (ApiKeyStore, WebhookStore, BatchStore) take `tokio_rusqlite::Connection` in constructors
- Whether to reuse the existing storage connection or create dedicated connections per store
- How to handle table creation/migrations (stores may need `CREATE TABLE IF NOT EXISTS` on first use)
- Feature gating: whether stores are behind `#[cfg(feature = "gateway")]` like other gateway wiring
- GatewayChannel setter methods: `set_api_key_store()`, `set_webhook_store()`, `set_batch_store()`, `set_event_bus()`
- Each setter takes Arc-wrapped store and stores in `Mutex<Option<...>>`
- Wiring order in serve.rs: after vault unlock (for DB access), before `gateway.connect()` / `mux.add_channel()`
- Webhook delivery: spawned tokio task using `run_webhook_delivery()` from `delivery.rs`
- The global `EventBus` (created in Phase 40) should be passed to GatewayState.event_bus

### Deferred Ideas (OUT OF SCOPE)
None.

</user_constraints>

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| API-11 | User can create scoped API keys via POST /v1/api-keys | ApiKeyStore wired into AuthConfig.key_store enables the handler at `api_keys::handlers::post_create_api_key` |
| API-12 | API keys support scope restrictions (chat.completions, tools.invoke, admin) | Scopes already implemented in ApiKey model + AuthContext; store wiring enables persistence |
| API-13 | API keys support per-key rate limiting (requests per minute) | Rate limit middleware at `rate_limit.rs:69` reads from `state.auth.key_store`; wiring enables it |
| API-14 | API keys support expiration and revocation | ApiKeyStore.revoke() and is_valid() exist; wiring enables DELETE /v1/api-keys/:id |
| API-15 | User can register webhooks via POST /v1/webhooks | WebhookStore wired into GatewayState.webhook_store enables `webhooks::handlers::post_create_webhook` |
| API-16 | Webhooks deliver events with HMAC-SHA256 signing and exponential backoff retry | `run_webhook_delivery()` + `deliver_with_retry()` exist; spawning the delivery loop enables live delivery |
| API-17 | User can submit batch requests via POST /v1/batch | BatchStore wired into GatewayState.batch_store enables `batch::handlers::post_create_batch` |
| API-18 | Batch results available with per-item success/error status | BatchStore.get_batch() returns items; wiring enables GET /v1/batch/:id |

</phase_requirements>

## Standard Stack

### Core (Already Exists -- No New Dependencies)

| Library | Version | Purpose | Status |
|---------|---------|---------|--------|
| blufio-gateway | workspace | GatewayChannel, GatewayState, ApiKeyStore, WebhookStore, BatchStore, delivery engine | Exists, target of setters |
| blufio-storage | workspace | `open_connection()` factory for encrypted SQLite | Exists, used for store connections |
| blufio-bus | workspace | EventBus with reliable mpsc subscribers | Exists, global instance created in Phase 40 |
| blufio-config | workspace | BlufioConfig.storage.database_path | Exists, provides DB path |
| tokio-rusqlite | workspace | Async SQLite wrapper (background thread model) | Exists as transitive dependency |
| reqwest | workspace | HTTP client for webhook delivery | Exists in blufio-gateway's Cargo.toml |

### No New External Dependencies

This phase adds zero new crates. All dependencies are already in the workspace.

## Architecture Patterns

### Recommended Module Structure

```
crates/blufio-gateway/src/lib.rs   # MODIFIED -- add 4 Mutex fields + 4 setter methods
crates/blufio/src/serve.rs         # MODIFIED -- instantiate stores, call setters, spawn delivery
```

Only two files need modification. No new files.

### Pattern 1: GatewayChannel Setter Methods (4 new setters)

**What:** Add four new fields and four async setter methods to GatewayChannel, following the exact pattern of `set_storage()`, `set_providers()`, and `set_tools()`.

**When to use:** This is the standard injection pattern for GatewayChannel -- all optional subsystems use this.

```rust
// In GatewayChannel struct definition (lib.rs):
// ADD these fields:
api_key_store: Mutex<Option<Arc<ApiKeyStore>>>,
webhook_store: Mutex<Option<Arc<WebhookStore>>>,
batch_store: Mutex<Option<Arc<BatchStore>>>,
event_bus: Mutex<Option<Arc<blufio_bus::EventBus>>>,

// In GatewayChannel::new():
// ADD these initializations:
api_key_store: Mutex::new(None),
webhook_store: Mutex::new(None),
batch_store: Mutex::new(None),
event_bus: Mutex::new(None),

// ADD these setter methods (following set_storage/set_providers/set_tools pattern):
pub async fn set_api_key_store(&self, store: Arc<ApiKeyStore>) {
    let mut s = self.api_key_store.lock().await;
    *s = Some(store);
}

pub async fn set_webhook_store(&self, store: Arc<WebhookStore>) {
    let mut s = self.webhook_store.lock().await;
    *s = Some(store);
}

pub async fn set_batch_store(&self, store: Arc<BatchStore>) {
    let mut s = self.batch_store.lock().await;
    *s = Some(store);
}

pub async fn set_event_bus(&self, bus: Arc<blufio_bus::EventBus>) {
    let mut s = self.event_bus.lock().await;
    *s = Some(bus);
}
```

### Pattern 2: GatewayState Population in connect()

**What:** In the `connect()` method, take values from the new Mutex fields and populate GatewayState.

**Where:** lib.rs, within the `connect()` method, lines 244-270.

```rust
// In connect(), alongside existing .take() calls (lines 245-247):
let api_key_store = self.api_key_store.lock().await.take();
let webhook_store = self.webhook_store.lock().await.take();
let batch_store = self.batch_store.lock().await.take();
let event_bus = self.event_bus.lock().await.take();

// In GatewayState construction:
auth: AuthConfig {
    bearer_token: self.config.bearer_token.clone(),
    keypair_public_key: self.config.keypair_public_key,
    key_store: api_key_store,  // WAS: None
},
// ...
webhook_store,  // WAS: None
batch_store,    // WAS: None
event_bus,      // WAS: None
```

### Pattern 3: Store Instantiation in serve.rs

**What:** Open a new tokio_rusqlite::Connection via `blufio_storage::open_connection()`, instantiate all three stores, and call the setters.

**Where:** serve.rs, within the `#[cfg(feature = "gateway")]` block, after gateway construction (line 635) and before `mux.add_channel()` (line 714).

```rust
// Open a dedicated connection for gateway stores (same DB, separate connection).
// The V7 migration has already been run by SqliteStorage::initialize().
let store_conn = blufio_storage::open_connection(&config.storage.database_path).await?;

// Instantiate stores.
let api_key_store = Arc::new(blufio_gateway::api_keys::store::ApiKeyStore::new(store_conn.clone()));
let webhook_store = Arc::new(blufio_gateway::webhooks::store::WebhookStore::new(store_conn.clone()));
let batch_store = Arc::new(blufio_gateway::batch::store::BatchStore::new(store_conn));

// Wire stores into gateway.
gateway.set_api_key_store(api_key_store).await;
gateway.set_webhook_store(webhook_store.clone()).await;
gateway.set_batch_store(batch_store).await;
gateway.set_event_bus(event_bus.clone()).await;
info!("gateway stores wired (api_keys, webhooks, batch, event_bus)");

// Spawn webhook delivery background loop.
let delivery_bus = event_bus.clone();
let delivery_store = webhook_store;
tokio::spawn(async move {
    blufio_gateway::webhooks::delivery::run_webhook_delivery(
        delivery_bus,
        delivery_store,
        reqwest::Client::new(),
    ).await;
});
info!("webhook delivery engine spawned");
```

### Pattern 4: Connection Strategy -- Single New Connection, Shared Across Stores

**What:** Open ONE new `tokio_rusqlite::Connection` via `open_connection()` and pass clones to all three stores.

**Why this approach:**
- `tokio_rusqlite::Connection` is internally `Arc`-wrapped -- cloning is cheap (just Arc::clone)
- All writes go through tokio_rusqlite's single background thread, so there are no SQLITE_BUSY conflicts
- The main SqliteStorage connection handles sessions/context; the stores connection handles API keys/webhooks/batch -- logical separation
- Using `open_connection()` ensures encryption (BLUFIO_DB_KEY) is handled correctly
- Migrations (V7) are already applied by `SqliteStorage::initialize()` before gateway setup

**Alternative considered:** Reusing SqliteStorage's internal connection. Rejected because:
- SqliteStorage's `Database` struct wraps the connection; no public accessor that returns `tokio_rusqlite::Connection` directly
- Opening a second connection to the same WAL-mode DB is standard SQLite practice
- CostLedger already follows this pattern (line 178: "opens its own connection to the same DB")

### Anti-Patterns to Avoid

- **Creating tables in store constructors:** The V7 migration is already run during SqliteStorage initialization. Do not add `CREATE TABLE IF NOT EXISTS` calls to store constructors -- the tables already exist.
- **Opening one connection per store:** Wasteful. tokio_rusqlite::Connection is Arc-based; clone it.
- **Adding blufio-storage as a dependency to blufio-gateway:** Not needed. Only the binary crate (blufio) calls `open_connection()`. The stores receive a pre-opened `tokio_rusqlite::Connection`.
- **Putting store instantiation before SqliteStorage::initialize():** Migrations must run first (line 167: `storage.initialize().await`). Store instantiation must come after.
- **Forgetting to clone webhook_store for the delivery task:** The delivery loop needs `Arc<WebhookStore>`; the gateway setter also takes `Arc<WebhookStore>`. Clone before passing to both.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Store implementations | New store code | Existing ApiKeyStore, WebhookStore, BatchStore | Phase 32 already implemented all three with full test coverage (53 tests) |
| Webhook delivery | Custom delivery loop | `run_webhook_delivery()` from delivery.rs | Already handles EventBus subscription, event filtering, HMAC signing, retry, dead letter queue |
| DB connection management | Custom connection pool | `blufio_storage::open_connection()` | Handles encryption, parent dir creation, error mapping |
| Gateway state injection | Custom DI/factory | GatewayChannel setter methods | Established pattern used by 6 other setters |
| Event bus subscription | Custom mpsc wiring | `bus.subscribe_reliable(256)` | Already implemented in EventBus with lag protection |

**Key insight:** Every component this phase needs already exists. The only new code is 4 struct fields, 4 setter methods, and ~25 lines of wiring in serve.rs.

## Common Pitfalls

### Pitfall 1: tokio_rusqlite::Connection Clone Semantics
**What goes wrong:** Assuming `Connection::clone()` creates a new SQLite connection (it does not -- it clones the inner Arc).
**Why it happens:** Unfamiliarity with tokio_rusqlite internals.
**How to avoid:** Understand that `tokio_rusqlite::Connection` wraps `Arc<InnerConnection>`. Cloning shares the same background thread and the same SQLite connection. This is exactly what we want -- all three stores share one SQLite connection via one background thread.
**Warning signs:** None -- this "pitfall" is actually the desired behavior.

### Pitfall 2: Ordering -- Stores Before Migrations
**What goes wrong:** If stores are instantiated before `SqliteStorage::initialize()` runs the V7 migration, the tables don't exist yet.
**Why it happens:** Moving wiring code too early in serve.rs.
**How to avoid:** Store instantiation MUST happen within the `#[cfg(feature = "gateway")]` block (line 601+), which is well after `storage.initialize().await` (line 167). The existing code structure naturally prevents this, but be aware.
**Warning signs:** "no such table: api_keys" errors at runtime.

### Pitfall 3: Missing reqwest::Client for Webhook Delivery
**What goes wrong:** `run_webhook_delivery()` takes a `reqwest::Client` parameter. Forgetting to create one or accidentally using a client with wrong timeout settings.
**Why it happens:** The delivery function already sets per-request 10-second timeouts internally, but the base `Client` needs to be created.
**How to avoid:** Use `reqwest::Client::new()` which provides reasonable defaults. The delivery engine sets its own per-request timeouts.
**Warning signs:** Compilation error on missing argument.

### Pitfall 4: import Paths for Store Types
**What goes wrong:** Incorrect import paths in serve.rs for the store types.
**Why it happens:** Stores are nested under submodules in blufio-gateway.
**How to avoid:** Use the correct paths:
- `blufio_gateway::api_keys::store::ApiKeyStore`
- `blufio_gateway::webhooks::store::WebhookStore`
- `blufio_gateway::batch::store::BatchStore`
- `blufio_gateway::webhooks::delivery::run_webhook_delivery`
**Warning signs:** Compilation error on unresolved imports.

### Pitfall 5: Webhook Delivery Task Lifetime
**What goes wrong:** The spawned webhook delivery task exits when the EventBus is dropped (the mpsc receiver returns None).
**Why it happens:** Normal tokio task lifecycle.
**How to avoid:** This is actually correct behavior -- `run_webhook_delivery` logs "webhook delivery engine stopped -- event bus closed" and exits cleanly. The EventBus lives as an `Arc` in serve.rs for the entire server lifetime, so the delivery task runs until shutdown. No join handle management needed.
**Warning signs:** "webhook delivery engine stopped" log during normal operation (would indicate premature Arc drop).

### Pitfall 6: AuthConfig.key_store Needs to Pass Through Setter
**What goes wrong:** The `key_store` field is inside `AuthConfig`, not a top-level `GatewayState` field. The setter must store it so that `connect()` can place it into `AuthConfig`.
**Why it happens:** Different nesting -- `webhook_store`, `batch_store`, `event_bus` are top-level in GatewayState, but `key_store` is inside `GatewayState.auth.key_store`.
**How to avoid:** The setter stores the value in `Mutex<Option<Arc<ApiKeyStore>>>` on GatewayChannel. In `connect()`, take it out and put it in `AuthConfig { key_store: api_key_store, ... }`.
**Warning signs:** key_store always None at runtime despite being set.

## Code Examples

### Existing Setter Pattern (from lib.rs)

Source: `crates/blufio-gateway/src/lib.rs` lines 156-186

```rust
// Example: set_storage (the pattern to follow)
pub async fn set_storage(&self, storage: Arc<dyn StorageAdapter + Send + Sync>) {
    let mut s = self.storage.lock().await;
    *s = Some(storage);
}

// Example: set_providers
pub async fn set_providers(&self, providers: Arc<dyn ProviderRegistry + Send + Sync>) {
    let mut p = self.providers.lock().await;
    *p = Some(providers);
}
```

### Existing connect() Pattern (from lib.rs)

Source: `crates/blufio-gateway/src/lib.rs` lines 237-270

```rust
// Existing pattern: take optional adapters
let storage = self.storage.lock().await.take();
let providers = self.providers.lock().await.take();
let tools = self.tools.lock().await.take();

let state = GatewayState {
    // ...
    auth: AuthConfig {
        bearer_token: self.config.bearer_token.clone(),
        keypair_public_key: self.config.keypair_public_key,
        key_store: None,  // <-- CHANGE TO: api_key_store
    },
    // ...
    webhook_store: None,  // <-- CHANGE TO: webhook_store
    batch_store: None,    // <-- CHANGE TO: batch_store
    event_bus: None,      // <-- CHANGE TO: event_bus
};
```

### Existing Gateway Wiring in serve.rs

Source: `crates/blufio/src/serve.rs` lines 635-714

```rust
// Existing pattern: create gateway, call setters, add to mux
let mut gateway = GatewayChannel::new(gateway_config);
gateway.set_storage(storage.clone()).await;
if let Some(ref providers) = provider_registry {
    gateway.set_providers(providers.clone()).await;
}
gateway.set_tools(tool_registry.clone()).await;
gateway.set_api_tools_allowlist(config.gateway.api_tools_allowlist.clone());
// ... MCP, WhatsApp wiring ...
mux.add_channel("gateway".to_string(), Box::new(gateway));
```

### Handler Consumption Pattern (already exists)

Source: `crates/blufio-gateway/src/api_keys/handlers.rs` line 25

```rust
// Handlers already handle the None case with error response:
let key_store = state.auth.key_store.as_ref().ok_or_else(|| {
    (StatusCode::SERVICE_UNAVAILABLE, "API key store not configured")
})?;
```

Source: `crates/blufio-gateway/src/webhooks/handlers.rs` line 39

```rust
let webhook_store = state.webhook_store.as_ref().ok_or_else(|| {
    (StatusCode::SERVICE_UNAVAILABLE, "Webhook store not configured")
})?;
```

### run_webhook_delivery Signature

Source: `crates/blufio-gateway/src/webhooks/delivery.rs` lines 170-174

```rust
pub async fn run_webhook_delivery(
    bus: Arc<blufio_bus::EventBus>,
    store: Arc<WebhookStore>,
    client: reqwest::Client,
) {
    let mut rx = bus.subscribe_reliable(256).await;
    // ... event loop ...
}
```

### open_connection Factory

Source: `crates/blufio-storage/src/database.rs` line 103

```rust
pub async fn open_connection(path: &str) -> Result<tokio_rusqlite::Connection, BlufioError> {
    // Handles BLUFIO_DB_KEY encryption, parent dir creation, WAL mode check
}
```

### CostLedger Precedent (same pattern)

Source: `crates/blufio/src/serve.rs` line 178

```rust
// CostLedger already opens its own connection to the same DB:
let cost_ledger = Arc::new(CostLedger::open(&config.storage.database_path).await?);
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Stores hardcoded to None in GatewayState | Stores wired via setters | Phase 42 (this phase) | Enables API-11 through API-18 |
| No webhook delivery loop | Background task with EventBus subscription | Phase 42 (this phase) | Enables live webhook delivery (API-16) |
| API key auth only via master bearer token | Scoped API key auth via ApiKeyStore | Phase 42 (this phase) | Enables scoped access (API-11..14) |
| Batch endpoints return 503 | BatchStore enables full batch lifecycle | Phase 42 (this phase) | Enables batch processing (API-17..18) |

## Open Questions

None. All implementation details are clear from the existing code. The patterns are well-established from Phase 40 and 41, and all target types/functions have been verified directly from source.

## Validation Architecture

> nyquist_validation not explicitly set to false in config.json -- section included.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in #[cfg(test)] + tokio::test |
| Config file | Workspace Cargo.toml |
| Quick run command | `cargo test -p blufio-gateway --lib` |
| Full suite command | `cargo test -p blufio-gateway` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| API-11 | ApiKeyStore.create() + lookup() works end-to-end | unit | `cargo test -p blufio-gateway --lib api_keys::store -x` | Exists (10 tests) |
| API-12 | Scoped auth context enforces scope restrictions | unit | `cargo test -p blufio-gateway --lib api_keys::tests -x` | Exists (11 tests) |
| API-13 | Rate limit counter increment + cleanup | unit | `cargo test -p blufio-gateway --lib api_keys::store::tests::rate_limit -x` | Exists (3 tests) |
| API-14 | API key revocation invalidates key | unit | `cargo test -p blufio-gateway --lib api_keys::store::tests::revoke -x` | Exists |
| API-15 | WebhookStore.create() + list() + delete() | unit | `cargo test -p blufio-gateway --lib webhooks::store -x` | Exists (7 tests) |
| API-16 | HMAC signing + delivery retry constants | unit | `cargo test -p blufio-gateway --lib webhooks::delivery -x` | Exists (6 tests) |
| API-17 | BatchStore.create_batch() + update_item() + finalize() | unit | `cargo test -p blufio-gateway --lib batch::store -x` | Exists (5 tests) |
| API-18 | Batch status includes per-item results | unit | `cargo test -p blufio-gateway --lib batch -x` | Exists (9 tests) |
| WIRING | GatewayChannel new setters compile + GatewayState population | build | `cargo check -p blufio-gateway --lib` | N/A (compilation check) |
| WIRING | serve.rs compiles with store wiring | build | `cargo check -p blufio --features gateway` | N/A (compilation check) |

### Sampling Rate
- **Per task commit:** `cargo test -p blufio-gateway --lib` (120 tests, fast)
- **Per wave merge:** `cargo test -p blufio-gateway && cargo check -p blufio --features gateway`
- **Phase gate:** Full suite green before /gsd:verify-work

### Wave 0 Gaps
None -- existing test infrastructure covers all phase requirements. The stores have comprehensive unit tests. This phase adds wiring code (no new testable logic), verified by compilation checks.

## Sources

### Primary (HIGH confidence)
- `crates/blufio-gateway/src/lib.rs` -- GatewayChannel struct, existing setters, connect() method (read directly)
- `crates/blufio-gateway/src/server.rs` -- GatewayState struct with webhook_store, batch_store, event_bus fields (read directly)
- `crates/blufio-gateway/src/auth.rs` -- AuthConfig struct with key_store field (read directly)
- `crates/blufio-gateway/src/api_keys/store.rs` -- ApiKeyStore constructor and all methods (read directly)
- `crates/blufio-gateway/src/webhooks/store.rs` -- WebhookStore constructor and all methods (read directly)
- `crates/blufio-gateway/src/batch/store.rs` -- BatchStore constructor and all methods (read directly)
- `crates/blufio-gateway/src/webhooks/delivery.rs` -- run_webhook_delivery(), deliver_with_retry(), sign_payload() (read directly)
- `crates/blufio/src/serve.rs` -- Current initialization patterns, gateway wiring block, EventBus creation (read directly)
- `crates/blufio-storage/src/database.rs` -- open_connection() factory function (read directly)
- `crates/blufio-storage/migrations/V7__api_keys_webhooks_batch.sql` -- Table definitions for all three stores (read directly)
- `crates/blufio-gateway/src/api_keys/handlers.rs` -- Handler consumption of key_store (grepped)
- `crates/blufio-gateway/src/webhooks/handlers.rs` -- Handler consumption of webhook_store (grepped)
- `crates/blufio-gateway/src/batch/handlers.rs` -- Handler consumption of batch_store and event_bus (grepped)
- `crates/blufio-gateway/src/rate_limit.rs` -- Rate limit middleware consumption of key_store (grepped)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all crates and functions exist, read directly from source
- Architecture: HIGH -- follows established setter pattern used by 6 existing setters, no ambiguity
- Pitfalls: HIGH -- derived from reading actual code structure, connection semantics, and migration ordering
- Wiring: HIGH -- exact field names, method signatures, and import paths confirmed from source

**Research date:** 2026-03-07
**Valid until:** 2026-04-07 (stable internal architecture, no external dependency changes)
