# Phase 8: Plugin System & Gateway - Research

**Researched:** 2026-03-01
**Domain:** Plugin host architecture, HTTP/WebSocket gateway, Prometheus metrics, adapter registry
**Confidence:** HIGH

## Summary

Phase 8 transforms Blufio from a hardcoded adapter stack into a plugin-based architecture with runtime adapter selection, adds an HTTP/WebSocket gateway as a second channel, and introduces Prometheus metrics. The core challenge is threefold: (1) building a PluginRegistry that replaces the hardcoded adapter initialization in serve.rs with dynamic adapter lookup, (2) implementing the gateway as a ChannelAdapter so it reuses the entire agent loop, and (3) supporting multiple concurrent channels (Telegram + Gateway) in the existing single-channel AgentLoop.

The existing codebase is well-prepared: all seven adapter traits are already defined in blufio-core, the AgentLoop already accepts `Box<dyn ChannelAdapter>`, and the config system uses figment with `deny_unknown_fields`. The main architectural shift is making AgentLoop support multiple channels via a multiplexing pattern, and wrapping all existing adapters behind a PluginRegistry with manifest-based discovery.

**Primary recommendation:** Use axum 0.8 for the HTTP/WebSocket gateway, metrics-rs with metrics-exporter-prometheus for Prometheus, and a compile-time PluginRegistry (no dynamic loading) that wraps existing adapters with plugin metadata.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Compile-time Cargo features for adapter loading -- each adapter is a feature flag, default build includes all 6 standard adapters
- PluginRegistry pattern at startup: adapters register themselves, serve.rs queries the registry
- Plugin manifests extend skill.toml pattern (same format with adapter-specific fields)
- `blufio plugin search` queries hardcoded built-in catalog -- no network calls
- `blufio plugin install/remove` toggles enabled state in blufio.toml config
- `blufio plugin list` shows ALL compiled-in adapters with status: enabled/disabled/not-configured
- `blufio plugin update` is informational (binary updates are whole-binary)
- Gateway implemented as ChannelAdapter -- reuses entire agent loop, session management, tool pipeline
- Full REST API + SSE streaming: POST /v1/messages, GET /v1/sessions, GET /v1/health
- WebSocket at /ws supports bidirectional streaming
- Bearer token authentication from blufio.toml or vault
- Single axum server: /v1/* for API, /ws for WebSocket, /metrics for Prometheus -- one port, different paths
- Keep existing crates + add new crates (blufio-gateway, blufio-prometheus, blufio-auth-keypair)
- Each adapter crate is a Cargo feature, all optional with default features including all 6
- Prometheus adapter: standard /metrics endpoint with counters/gauges
- Auth adapter: bearer token validation backed by device keypair stored in vault

### Claude's Discretion
- PluginRegistry internal architecture and registration API
- Plugin manifest field names and exact TOML structure
- SSE streaming protocol details and event format
- WebSocket message frame format and protocol
- Prometheus metric naming conventions
- Device keypair generation algorithm (Ed25519 vs other)
- Error response format for gateway API
- Gateway crate internal module structure

### Deferred Ideas (OUT OF SCOPE)
- Third-party runtime plugin loading (dynamic libraries, WASM adapters) -- v2 concern
- Challenge-response keypair authentication -- future enhancement
- Structured JSON log export via ObservabilityAdapter -- future phase
- GitHub/HTTP-based plugin registry for community plugins -- v2
- Separate metrics port for production deployments -- configurable in future
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| PLUG-01 | Plugin host loads adapter plugins implementing Channel, Provider, Storage, Embedding, Observability, Auth traits | PluginRegistry architecture with compile-time registration; AdapterType enum already exists |
| PLUG-02 | `blufio plugin list/search/install/remove/update` CLI commands for plugin management | CLI subcommand pattern from existing `blufio skill` commands; toggling enabled state in config |
| PLUG-03 | Plugin manifest (plugin.toml) declares name, version, adapter type, capabilities, minimum Blufio version | Extends skill.toml pattern already in manifest.rs; TOML parsing with serde |
| PLUG-04 | Default install ships with: Telegram, Anthropic, SQLite, local ONNX, Prometheus, device keypair | Cargo features with `default = ["telegram", "anthropic", "sqlite", "onnx", "prometheus", "keypair"]` |
| INFRA-05 | HTTP/WebSocket gateway (axum) for API access alongside channel messaging | axum 0.8 with WebSocket upgrade, SSE via axum::response::Sse, shared state via State extractor |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| axum | 0.8 | HTTP/WebSocket server framework | De facto Rust web framework, built on tower/hyper/tokio, first-party WebSocket support |
| axum-extra | 0.10 | SSE (Server-Sent Events) support | axum's official companion for SSE streaming via `Sse` type |
| tower-http | 0.6 | CORS, tracing, auth middleware | Standard middleware layer for axum apps |
| metrics | 0.24 | Metrics facade (counter, gauge, histogram macros) | Rust standard metrics facade, similar to `log` crate pattern |
| metrics-exporter-prometheus | 0.16 | Prometheus text format exporter | Official metrics-rs Prometheus exporter with HTTP listener or manual render |
| ed25519-dalek | 2.1 | Ed25519 keypair generation and signing | De facto Rust Ed25519 implementation, pure Rust, audited |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tokio-tungstenite | 0.24 | WebSocket protocol (used internally by axum) | Already pulled in by axum's ws feature |
| serde_json | 1 | JSON serialization for API request/response | Already in workspace |
| uuid | 1 | Request ID generation | Already in workspace |
| chrono | 0.4 | Timestamps for API responses | Already in workspace |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| axum | actix-web | actix has higher raw throughput but axum is built on tower/hyper and integrates better with existing tokio ecosystem; axum is more idiomatic |
| metrics-rs | prometheus crate | prometheus crate is more mature but metrics-rs provides a cleaner facade pattern and the exporter integrates with axum; metrics-rs is the modern standard |
| ed25519-dalek | ring | ring is already in workspace for vault crypto, but ed25519-dalek has a cleaner API for keypair management; ring's ed25519 API is lower-level |

**Installation (Cargo.toml additions):**
```toml
axum = { version = "0.8", features = ["ws"] }
axum-extra = { version = "0.10", features = ["typed-header"] }
tower-http = { version = "0.6", features = ["cors", "trace"] }
metrics = "0.24"
metrics-exporter-prometheus = "0.16"
ed25519-dalek = { version = "2.1", features = ["rand_core"] }
```

## Architecture Patterns

### Recommended Crate Structure
```
crates/
├── blufio-gateway/         # HTTP/WebSocket gateway ChannelAdapter
│   ├── src/
│   │   ├── lib.rs          # GatewayChannel struct (ChannelAdapter impl)
│   │   ├── server.rs       # axum server setup, routing
│   │   ├── handlers.rs     # REST API handlers (messages, sessions, health)
│   │   ├── ws.rs           # WebSocket upgrade and message handling
│   │   ├── sse.rs          # SSE streaming response builder
│   │   └── auth.rs         # Bearer token validation middleware
│   └── Cargo.toml
├── blufio-prometheus/      # ObservabilityAdapter for Prometheus metrics
│   ├── src/
│   │   ├── lib.rs          # PrometheusAdapter struct
│   │   └── metrics.rs      # Metric definitions and recording
│   └── Cargo.toml
├── blufio-auth-keypair/    # AuthAdapter for device keypair auth
│   ├── src/
│   │   ├── lib.rs          # KeypairAuthAdapter struct
│   │   └── keypair.rs      # Ed25519 key generation/validation
│   └── Cargo.toml
└── blufio-plugin/          # PluginRegistry, manifest, CLI
    ├── src/
    │   ├── lib.rs           # Re-exports
    │   ├── registry.rs      # PluginRegistry (compile-time adapter catalog)
    │   ├── manifest.rs      # Plugin manifest parser (plugin.toml)
    │   └── catalog.rs       # Built-in plugin catalog for `blufio plugin search`
    └── Cargo.toml
```

### Pattern 1: PluginRegistry (Compile-Time Adapter Catalog)
**What:** A registry that holds metadata about all compiled-in adapters and their enabled/disabled status.
**When to use:** At startup, serve.rs queries the registry to discover which adapters to initialize.
**Example:**
```rust
pub struct PluginRegistry {
    plugins: HashMap<String, PluginEntry>,
}

pub struct PluginEntry {
    pub manifest: PluginManifest,
    pub status: PluginStatus,
    pub factory: Option<Box<dyn PluginFactory>>,
}

pub enum PluginStatus {
    Enabled,
    Disabled,
    NotConfigured, // Compiled in but missing required config (e.g., no API key)
}

pub trait PluginFactory: Send + Sync {
    fn adapter_type(&self) -> AdapterType;
    fn create(&self, config: &BlufioConfig) -> Result<Box<dyn PluginAdapter>, BlufioError>;
}

impl PluginRegistry {
    pub fn new() -> Self { /* ... */ }
    pub fn register(&mut self, manifest: PluginManifest, factory: Box<dyn PluginFactory>) { /* ... */ }
    pub fn get_enabled(&self, adapter_type: AdapterType) -> Vec<&PluginEntry> { /* ... */ }
    pub fn list_all(&self) -> Vec<&PluginEntry> { /* ... */ }
    pub fn set_enabled(&mut self, name: &str, enabled: bool) { /* ... */ }
}
```

### Pattern 2: Multi-Channel AgentLoop via Channel Multiplexer
**What:** Instead of modifying AgentLoop to hold multiple channels, create a ChannelMultiplexer that wraps multiple ChannelAdapters and implements ChannelAdapter itself.
**When to use:** When running Telegram + Gateway simultaneously.
**Example:**
```rust
pub struct ChannelMultiplexer {
    channels: Vec<Box<dyn ChannelAdapter + Send + Sync>>,
    inbound_rx: tokio::sync::mpsc::Receiver<InboundMessage>,
    inbound_tx: tokio::sync::mpsc::Sender<InboundMessage>,
    outbound_senders: HashMap<String, tokio::sync::mpsc::Sender<OutboundMessage>>,
}

// Each channel runs its own receive loop in a background task,
// forwarding InboundMessages to the multiplexer's channel.
// The AgentLoop sees a single ChannelAdapter (the multiplexer).
// Outbound messages are routed back to the originating channel
// by matching the channel field on OutboundMessage.
```

### Pattern 3: Gateway as ChannelAdapter
**What:** The HTTP/WebSocket gateway implements ChannelAdapter, bridging HTTP requests into InboundMessages and routing responses back to HTTP clients.
**When to use:** The gateway appears to the agent loop as just another messaging channel.
**Example:**
```rust
pub struct GatewayChannel {
    inbound_tx: mpsc::Sender<InboundMessage>,
    inbound_rx: Mutex<mpsc::Receiver<InboundMessage>>,
    response_map: Arc<DashMap<String, oneshot::Sender<String>>>,
    // axum server runs in background, sends InboundMessages via inbound_tx
    // and waits for responses via oneshot channels in response_map
}

#[async_trait]
impl ChannelAdapter for GatewayChannel {
    async fn receive(&self) -> Result<InboundMessage, BlufioError> {
        self.inbound_rx.lock().await.recv().await
            .ok_or(BlufioError::Channel { message: "gateway closed".into(), source: None })
    }
    async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> {
        // Route response back to waiting HTTP request or WebSocket connection
        if let Some(session_id) = &msg.session_id {
            if let Some((_, tx)) = self.response_map.remove(session_id) {
                let _ = tx.send(msg.content);
            }
        }
        Ok(MessageId(uuid::Uuid::new_v4().to_string()))
    }
}
```

### Anti-Patterns to Avoid
- **Modifying AgentLoop internals for multi-channel:** Don't change AgentLoop to hold Vec<Box<dyn ChannelAdapter>>. Use a multiplexer pattern that preserves the single-channel interface.
- **Global metrics state without handle:** Don't call PrometheusBuilder::install() which starts its own HTTP listener. Use install_recorder() and get a PrometheusHandle for manual rendering via the axum /metrics endpoint.
- **Blocking the agent loop for HTTP requests:** Don't make the gateway channel synchronously wait for responses. Use oneshot channels for async request/response pairing.
- **Feature flag explosion:** Don't make every dependency conditional. Only the adapter crates themselves should be behind features; core types remain always-compiled.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| WebSocket protocol | Custom frame parsing | axum built-in WebSocket (tungstenite) | Binary framing, ping/pong, close handshake, fragmentation |
| SSE streaming | Custom chunked transfer | axum-extra Sse type | Retry logic, event IDs, proper content-type headers |
| Prometheus format | Custom metric text serialization | metrics-exporter-prometheus | Exposition format v0.0.4 compliance, histogram buckets, proper escaping |
| Ed25519 crypto | Manual key derivation | ed25519-dalek | Constant-time operations, side-channel resistance, RFC 8032 compliance |
| HTTP auth middleware | Per-handler token checking | tower-http or custom axum layer | DRY, consistent error responses, applied at router level |

**Key insight:** The HTTP gateway surface area is deceptively large -- CORS, content negotiation, error serialization, streaming protocols, auth middleware. Using axum's ecosystem handles all of this.

## Common Pitfalls

### Pitfall 1: Channel Multiplexer Deadlock
**What goes wrong:** The multiplexer's `receive()` blocks waiting for a message from any channel, but `send()` needs to route to a specific channel. If channels share locks, deadlock occurs.
**Why it happens:** Mixed ownership of mpsc channels with RwLock contention.
**How to avoid:** Use a single mpsc channel for inbound aggregation. Each channel has its own outbound sender. Route outbound by channel name, not by shared state.
**Warning signs:** AgentLoop hangs after first message when both Telegram and gateway are active.

### Pitfall 2: Gateway Response Routing
**What goes wrong:** HTTP POST /v1/messages sends a request, but the response comes back through AgentLoop's normal send() path which doesn't know about the waiting HTTP handler.
**Why it happens:** The ChannelAdapter interface is fire-and-forget (send returns MessageId, not a response channel).
**How to avoid:** Use a response map (DashMap<request_id, oneshot::Sender>) in GatewayChannel. The HTTP handler creates a oneshot channel, stores the sender in the map keyed by a synthetic session_id or request_id, and awaits the receiver. When send() is called, it looks up and delivers the response.
**Warning signs:** HTTP requests hang forever, or responses are delivered to wrong clients.

### Pitfall 3: PrometheusBuilder::install() Port Conflict
**What goes wrong:** Calling PrometheusBuilder::install() starts a separate HTTP listener on port 9000, conflicting with the axum server or requiring two ports.
**Why it happens:** Default metrics-exporter-prometheus behavior.
**How to avoid:** Use PrometheusBuilder::new().install_recorder() to get a PrometheusHandle without starting a listener. Expose /metrics via the shared axum router using handle.render().
**Warning signs:** "address already in use" error at startup, or metrics on a different port than the API.

### Pitfall 4: Feature Flag Compilation Errors
**What goes wrong:** Conditional compilation with `#[cfg(feature = "...")]` causes type mismatches or missing trait impls when features are disabled.
**Why it happens:** Feature-gated code doesn't compile when the feature is off, so errors aren't caught until someone builds with `--no-default-features`.
**How to avoid:** Keep all core types unconditional. Only gate adapter crate dependencies and registration calls. Use a stub/noop pattern for disabled adapters rather than conditional compilation of core structs.
**Warning signs:** CI only tests with default features, missing `--no-default-features` test job.

### Pitfall 5: axum 0.8 State API Changes
**What goes wrong:** axum 0.8 changed how state works compared to 0.7. Router<S> means "router missing state S", not "router with state S".
**Why it happens:** API redesign between versions.
**How to avoid:** Use `Router::new().with_state(AppState{})` to provide state. All handlers extract via `State<AppState>`. Nest routers with `with_state()` on each nested router.
**Warning signs:** Compilation error "the trait bound `Router<AppState>: MakeService<...>` is not satisfied".

## Code Examples

### axum HTTP Server with Shared State
```rust
// Source: Context7 axum docs
use axum::{Router, routing::{get, post}, extract::State};
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    // shared references to agent components
}

let app = Router::new()
    .route("/v1/health", get(health_handler))
    .route("/v1/messages", post(messages_handler))
    .route("/ws", get(ws_handler))
    .route("/metrics", get(metrics_handler))
    .with_state(app_state);

let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
axum::serve(listener, app).await.unwrap();
```

### axum WebSocket Handler
```rust
// Source: Context7 axum docs
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    while let Some(msg) = socket.next().await {
        if let Ok(Message::Text(text)) = msg {
            // Parse as JSON, create InboundMessage, send to agent
            // Wait for response via oneshot channel
            // Send response back through socket
        }
    }
}
```

### Prometheus Metrics with Manual Render (axum Integration)
```rust
// Source: Context7 metrics-rs docs
use metrics::{counter, gauge, histogram, describe_counter, describe_gauge, describe_histogram};
use metrics_exporter_prometheus::PrometheusBuilder;

// At startup:
let prometheus_handle = PrometheusBuilder::new()
    .install_recorder()
    .expect("failed to install Prometheus recorder");

// Register metric descriptions:
describe_counter!("blufio_messages_total", "Total messages processed");
describe_gauge!("blufio_active_sessions", "Currently active sessions");
describe_histogram!("blufio_response_latency_seconds", "LLM response latency");

// In axum handler:
async fn metrics_handler(
    State(handle): State<metrics_exporter_prometheus::PrometheusHandle>,
) -> String {
    handle.render()
}

// Recording metrics (anywhere in codebase):
counter!("blufio_messages_total", "channel" => "telegram").increment(1);
gauge!("blufio_active_sessions").set(42.0);
histogram!("blufio_response_latency_seconds").record(0.250);
```

### Ed25519 Keypair Generation
```rust
// Source: ed25519-dalek docs
use ed25519_dalek::{SigningKey, VerifyingKey, Signer, Verifier, Signature};
use rand::rngs::OsRng;

// Generate keypair:
let signing_key = SigningKey::generate(&mut OsRng);
let verifying_key: VerifyingKey = (&signing_key).into();

// Sign a message (bearer token validation):
let message = b"token-payload";
let signature: Signature = signing_key.sign(message);

// Verify:
assert!(verifying_key.verify(message, &signature).is_ok());

// Serialize for vault storage:
let private_bytes = signing_key.to_bytes(); // [u8; 32]
let public_bytes = verifying_key.to_bytes(); // [u8; 32]
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| axum 0.7 with `axum::Server` | axum 0.8 with `axum::serve()` | axum 0.8 (2024) | Server::bind replaced by TcpListener + serve() |
| hyper 0.14 | hyper 1.0 (via axum 0.8) | hyper 1.0 (2023) | axum 0.8 uses hyper 1.0 internally; no direct hyper API needed |
| prometheus crate | metrics-rs facade + exporter | 2023-2024 | Cleaner API, facade pattern allows swapping backends |
| custom WebSocket | axum built-in ws | axum 0.6+ | No need for separate tungstenite setup |

**Deprecated/outdated:**
- `axum::Server` -- replaced by `axum::serve()` in 0.8
- `hyper::Server` -- replaced by hyper 1.0 server patterns
- prometheus crate's custom HTTP listener -- use metrics-exporter-prometheus with manual render instead

## Open Questions

1. **Multi-channel graceful shutdown ordering**
   - What we know: CancellationToken handles single-channel shutdown cleanly
   - What's unclear: Order of shutdown for Telegram vs Gateway when both are active
   - Recommendation: Cancel the multiplexer, which cancels all child channels. Gateway server shuts down first (fast), Telegram long-poll finishes (may take up to poll timeout).

2. **SSE vs WebSocket for streaming responses**
   - What we know: Both are supported by axum. SSE is simpler for server-push. WebSocket is bidirectional.
   - What's unclear: Whether to support both or pick one for the initial implementation
   - Recommendation: Support both as specified in CONTEXT.md. POST /v1/messages with Accept: text/event-stream returns SSE. /ws returns WebSocket. They share the same agent loop underneath.

3. **Plugin manifest backward compatibility**
   - What we know: skill.toml uses a specific format. plugin.toml extends it.
   - What's unclear: Whether a single format can serve both skills and plugins
   - Recommendation: Use separate formats (skill.toml and plugin.toml). They share concepts but serve different purposes (WASM sandbox config vs adapter config).

## Sources

### Primary (HIGH confidence)
- /tokio-rs/axum (Context7) - WebSocket handlers, Router state, SSE streaming, serve() pattern
- /metrics-rs/metrics (Context7) - Counter/gauge/histogram macros, PrometheusBuilder, manual render, axum integration

### Secondary (MEDIUM confidence)
- axum 0.8 migration patterns verified via Context7 code examples
- metrics-exporter-prometheus manual render pattern verified via Context7 docs

### Tertiary (LOW confidence)
- ed25519-dalek 2.x API (based on training data; recommend verifying exact API when implementing)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - axum and metrics-rs verified via Context7 with current versions
- Architecture: HIGH - patterns derived from existing codebase analysis + standard Rust async patterns
- Pitfalls: HIGH - identified from actual code patterns (AgentLoop single-channel design, PrometheusBuilder default behavior)

**Research date:** 2026-03-01
**Valid until:** 2026-04-01 (stable ecosystem, 30-day validity)
