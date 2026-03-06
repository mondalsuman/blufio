# Phase 34: WhatsApp, Signal, IRC & Matrix Adapters - Research

**Researched:** 2026-03-06
**Domain:** Messaging platform adapters (WhatsApp Cloud API, WhatsApp Web, Signal via signal-cli, IRC, Matrix) and cross-channel bridging
**Confidence:** MEDIUM-HIGH

## Summary

Phase 34 adds four new channel adapters (WhatsApp, Signal, IRC, Matrix) and a cross-channel bridge subsystem. The project already has a well-established adapter pattern from Phase 33: each adapter is its own crate (`blufio-{name}/`), implements `ChannelAdapter` trait, uses mpsc channels for inbound message forwarding, and is gated behind a feature flag in `serve.rs`. The Slack adapter (`blufio-slack/`) is the most complete reference implementation.

The four adapters span very different integration patterns: WhatsApp Cloud API uses webhook HTTP endpoints (reusing the existing gateway), WhatsApp Web uses an unofficial Rust library (`whatsapp-rust`) for direct protocol communication, Signal connects to an external `signal-cli` JSON-RPC daemon over TCP or Unix socket, IRC uses the `irc` crate for direct protocol handling with built-in TLS, and Matrix uses the official `matrix-sdk` 0.11 with async event handlers. The bridge subsystem subscribes to `ChannelEvent::MessageReceived` on the event bus and forwards messages to configured target channels.

The primary risk areas are: (1) the `irc` crate lacks built-in SASL PLAIN authentication so it must be implemented manually via raw IRC commands, (2) the `whatsapp-rust` crate is unofficial and may break, (3) the bridge subsystem must avoid infinite loops when two bridged channels both emit events, and (4) the `ChannelEvent::MessageReceived` currently lacks message content -- only `sender_id` and `channel` -- so it needs extension for bridging.

**Primary recommendation:** Build each adapter as a separate crate following the Slack adapter pattern. Implement the bridge subsystem in `blufio-bus` or a new `blufio-bridge` crate. Extend `ChannelEvent::MessageReceived` to include message text and sender display name for bridging. Use the `irc` crate v1.0+ for IRC, `matrix-sdk` 0.11.0 pinned for Matrix, `whatsapp-rust` for WhatsApp Web (behind feature flag), and raw HTTP for WhatsApp Cloud API and signal-cli JSON-RPC.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **Signal adapter**: External process only -- Blufio connects to a running signal-cli JSON-RPC instance, does NOT manage its lifecycle. Retry with exponential backoff on connection loss (1s, 2s, 4s... up to 60s). Report `Degraded` health status until reconnected. Transport: auto-detect between Unix domain socket and TCP. Support both 1:1 direct messages and group messages with @mention detection.
- **Cross-channel bridging**: Named bridge groups in TOML: `[bridge.group-name]` with `channels = ["telegram", "discord", "slack"]`. All channels in a group see each other's messages (implicit bidirectional). Bridge content: text messages with sender attribution (e.g., "[Telegram/Alice] Hello!"). Media forwarded as links when target channel doesn't support the format natively. Pass-through only -- bridged messages do NOT trigger Blufio's AI, do NOT create sessions, do NOT cost tokens. Basic filtering per bridge group: `exclude_bots = true` (default) and optional `include_users = [...]`.
- **WhatsApp adapter**: Primary: WhatsApp Cloud API (Meta Business API) using blufio-gateway's existing HTTP server with a `/webhooks/whatsapp` endpoint. Experimental: WhatsApp Web adapter behind `whatsapp-web` feature flag. Reactive only -- no template messages, no proactive outreach. Shared `[whatsapp]` config section with `variant = "cloud"` or `variant = "web"`.
- **IRC adapter**: Multi-channel per instance: single TCP connection, config lists channels to join. Respond on @mention or DM (PRIVMSG to bot nick). Built-in flood protection: message queue with configurable rate limit (default: 1 message per 2 seconds). Long messages split at word boundaries. Authentication: SASL PLAIN for modern servers, NickServ IDENTIFY as fallback. TLS required by default.
- **Matrix adapter**: Pinned to matrix-sdk 0.11. Room join and messaging via ChannelAdapter trait.

### Claude's Discretion
- Matrix adapter detailed behavior (room discovery, invite handling, encryption support level)
- IRC reconnection strategy details (ping timeout, reconnect delay)
- Signal JSON-RPC message format parsing implementation
- WhatsApp Web library choice (whatsapp-web.rs or alternative)
- Bridge message formatting details (exact attribution format, emoji usage)
- Error handling and logging patterns (follow existing adapter conventions)

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| CHAN-06 | WhatsApp Cloud API adapter (official Meta Business API) | Gateway webhook endpoint at `/webhooks/whatsapp`, HMAC-SHA256 verification, GET challenge verification, POST message parsing |
| CHAN-07 | WhatsApp Web adapter (experimental, behind feature flag, labeled unstable) | `whatsapp-rust` crate provides async Rust WhatsApp Web client; must be feature-gated with clear instability warning |
| CHAN-08 | Signal adapter via signal-cli JSON-RPC sidecar bridge | signal-cli daemon exposes JSON-RPC 2.0 over TCP/Unix socket; `receive` method notifications deliver messages; `send` method for outbound |
| CHAN-09 | IRC adapter with TLS and NickServ authentication via irc crate | `irc` crate v1.0+ provides async client with TLS via rustls, NickServ via `nick_password`/`should_ghost`; SASL PLAIN needs manual implementation via raw AUTHENTICATE commands |
| CHAN-10 | Matrix adapter with room join and messaging via matrix-sdk 0.11 | `matrix-sdk` 0.11.0 provides `Client::builder()`, `matrix_auth().login_username()`, event handlers for `OriginalSyncRoomMessageEvent`, room join/send APIs |
| INFRA-06 | Cross-channel bridging with configurable bridge rules in TOML | Bridge subscriber on event bus, `ChannelEvent::MessageReceived` needs content extension, bridge groups parsed from TOML config, loop prevention via origin tagging |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `irc` | 1.0+ | IRC client with TLS and NickServ | The standard Rust IRC library; async-friendly, RFC 2812/IRCv3 compliant, rustls TLS support, built-in NickServ ghost/identify |
| `matrix-sdk` | =0.11.0 (pinned) | Matrix client for room join, messaging, event handling | Official Matrix Rust SDK from matrix.org; requires Rust 1.85 (matches project MSRV) |
| `whatsapp-rust` | latest | WhatsApp Web protocol client (experimental) | Pure Rust WhatsApp Web client based on whatsmeow/Baileys patterns; only viable Rust option for WhatsApp Web |
| `reqwest` | 0.13 (workspace) | HTTP client for WhatsApp Cloud API and signal-cli JSON-RPC | Already in workspace; used for outbound API calls to Meta Graph API |
| `tokio` | 1.x (workspace) | Async runtime, TCP/Unix socket connections to signal-cli | Already in workspace; provides TcpStream, UnixStream, mpsc channels |
| `serde` / `serde_json` | 1.x (workspace) | JSON-RPC message parsing for signal-cli, webhook payload parsing | Already in workspace |
| `blufio-bus` | workspace | Event bus for bridge subscription | Existing crate; bridge subscribes to `BusEvent::Channel(ChannelEvent::MessageReceived)` |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tokio::io` | workspace | BufReader/BufWriter for signal-cli newline-delimited JSON-RPC over TCP/Unix socket | Signal adapter JSON-RPC stream parsing |
| `hmac` + `sha2` | workspace | HMAC-SHA256 for WhatsApp webhook signature verification | WhatsApp Cloud API webhook POST verification |
| `axum` | workspace | Route handler for `/webhooks/whatsapp` GET/POST | WhatsApp Cloud API webhook endpoint in gateway |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `irc` crate | Raw TCP + IRC protocol parser | Far more work, reinventing the wheel; `irc` crate handles TLS, flood control, reconnection |
| `whatsapp-rust` | `whatsappweb` (wiomoc) | `whatsappweb` is older (0.0.1), fewer features; `whatsapp-rust` is more actively maintained |
| Direct signal-cli JSON-RPC | signal-cli REST API wrapper | REST wrapper adds another dependency; JSON-RPC is the native daemon interface |
| `whatsapp-cloud-api` Rust crate | Direct HTTP calls to Meta API | The crate has 0% documentation; direct reqwest calls with typed structs give more control |

**Installation (Cargo.toml dependencies per adapter crate):**

```toml
# blufio-irc/Cargo.toml
irc = { version = "1", default-features = false, features = ["tls-rust"] }

# blufio-matrix/Cargo.toml
matrix-sdk = { version = "=0.11.0", default-features = false, features = ["rustls-tls"] }

# blufio-whatsapp/Cargo.toml (Cloud API - no extra deps beyond workspace)
# Uses reqwest (workspace), hmac (workspace), sha2 (workspace)

# blufio-whatsapp-web/Cargo.toml (experimental)
whatsapp-rust = { version = "*" }  # Pin to specific version after testing

# blufio-signal/Cargo.toml (no extra deps beyond workspace)
# Uses tokio (workspace), serde_json (workspace)
```

## Architecture Patterns

### Recommended Project Structure
```
crates/
├── blufio-whatsapp/        # WhatsApp Cloud API adapter (CHAN-06)
│   └── src/
│       ├── lib.rs          # WhatsAppCloudChannel: ChannelAdapter impl
│       ├── webhook.rs      # Axum handlers for /webhooks/whatsapp GET+POST
│       ├── api.rs          # Meta Graph API send message client
│       └── types.rs        # Webhook payload structs, API request/response types
├── blufio-whatsapp-web/    # WhatsApp Web adapter (CHAN-07, experimental)
│   └── src/
│       ├── lib.rs          # WhatsAppWebChannel: ChannelAdapter impl
│       └── session.rs      # Session persistence and QR code handling
├── blufio-signal/          # Signal adapter via signal-cli (CHAN-08)
│   └── src/
│       ├── lib.rs          # SignalChannel: ChannelAdapter impl
│       ├── jsonrpc.rs      # JSON-RPC 2.0 client (send/receive over stream)
│       └── types.rs        # signal-cli envelope, message types
├── blufio-irc/             # IRC adapter (CHAN-09)
│   └── src/
│       ├── lib.rs          # IrcChannel: ChannelAdapter impl
│       ├── flood.rs        # Message queue with rate limiting
│       ├── sasl.rs         # SASL PLAIN authentication (manual impl)
│       └── splitter.rs     # Word-boundary message splitting for PRIVMSG
├── blufio-matrix/          # Matrix adapter (CHAN-10)
│   └── src/
│       ├── lib.rs          # MatrixChannel: ChannelAdapter impl
│       └── handler.rs      # Event handler callbacks
└── blufio-bridge/          # Cross-channel bridge (INFRA-06)
    └── src/
        ├── lib.rs          # BridgeManager: config parsing, lifecycle
        ├── config.rs       # BridgeGroup, BridgeConfig TOML structs
        ├── router.rs       # Message routing between bridge groups
        └── formatter.rs    # Attribution formatting: "[Channel/User] text"
```

### Pattern 1: Adapter Crate Structure (follow Slack adapter)
**What:** Each adapter is a separate crate with feature flag gating.
**When to use:** Every new adapter.
**Example:**
```rust
// blufio-irc/src/lib.rs -- follows blufio-slack/src/lib.rs pattern
pub struct IrcChannel {
    config: IrcConfig,
    inbound_rx: tokio::sync::Mutex<mpsc::Receiver<InboundMessage>>,
    inbound_tx: mpsc::Sender<InboundMessage>,
    client: Option<irc::client::Client>,
}

impl IrcChannel {
    pub fn new(config: IrcConfig) -> Result<Self, BlufioError> {
        // Validate config, create mpsc channel
        let (inbound_tx, inbound_rx) = mpsc::channel(100);
        Ok(Self { config, inbound_rx: Mutex::new(inbound_rx), inbound_tx, client: None })
    }
}

#[async_trait]
impl PluginAdapter for IrcChannel {
    fn name(&self) -> &str { "irc" }
    fn version(&self) -> semver::Version { semver::Version::new(0, 1, 0) }
    fn adapter_type(&self) -> AdapterType { AdapterType::Channel }
    // health_check, shutdown...
}

#[async_trait]
impl ChannelAdapter for IrcChannel {
    fn capabilities(&self) -> ChannelCapabilities { /* ... */ }
    async fn connect(&mut self) -> Result<(), BlufioError> { /* ... */ }
    async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> { /* ... */ }
    async fn receive(&self) -> Result<InboundMessage, BlufioError> { /* ... */ }
}
```

### Pattern 2: serve.rs Feature Flag Wiring
**What:** Conditional compilation block in serve.rs to wire adapter into ChannelMultiplexer.
**When to use:** Every new adapter registration.
**Example:**
```rust
// In serve.rs, following the existing Slack/Discord/Telegram pattern:
#[cfg(feature = "irc")]
use blufio_irc::IrcChannel;

// Inside run_serve():
#[cfg(feature = "irc")]
{
    if config.irc.server.is_some() {
        let irc = IrcChannel::new(config.irc.clone()).map_err(|e| {
            error!(error = %e, "failed to initialize IRC channel");
            e
        })?;
        mux.add_channel("irc".to_string(), Box::new(irc));
        info!("irc channel added to multiplexer");
    } else {
        info!("irc channel skipped (no server configured)");
    }
}
```

### Pattern 3: Config Section (follow existing pattern)
**What:** Config struct in `blufio-config/src/model.rs` with `#[serde(deny_unknown_fields)]`.
**When to use:** Each adapter needs its config section in BlufioConfig.
**Example:**
```rust
// In BlufioConfig struct:
#[serde(default)]
pub whatsapp: WhatsAppConfig,
#[serde(default)]
pub signal: SignalConfig,
#[serde(default)]
pub irc: IrcConfig,
#[serde(default)]
pub matrix: MatrixConfig,
#[serde(default)]
pub bridge: BridgeConfig,

// Example: IrcConfig
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IrcConfig {
    pub server: Option<String>,
    #[serde(default = "default_irc_port")]
    pub port: u16,
    pub nickname: Option<String>,
    #[serde(default)]
    pub channels: Vec<String>,
    #[serde(default = "default_true")]
    pub tls: bool,
    #[serde(default)]
    pub auth_method: Option<String>,  // "sasl" or "nickserv"
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default = "default_irc_rate_limit")]
    pub rate_limit_ms: u64,           // default: 2000
    #[serde(default)]
    pub allowed_users: Vec<String>,
}
```

### Pattern 4: Signal JSON-RPC Client
**What:** Newline-delimited JSON-RPC 2.0 over TCP or Unix socket.
**When to use:** Signal adapter communication with signal-cli daemon.
**Example:**
```rust
// signal-cli sends notifications as newline-delimited JSON:
// {"jsonrpc":"2.0","method":"receive","params":{"envelope":{"source":"+123","dataMessage":{"message":"hello"}}}}
//
// Blufio sends requests:
// {"jsonrpc":"2.0","method":"send","params":{"recipient":"+123","message":"response"},"id":"req-1"}

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

enum Transport {
    Tcp(tokio::net::TcpStream),
    Unix(tokio::net::UnixStream),
}

// Auto-detect: if socket_path is set, use Unix; otherwise TCP
async fn connect(config: &SignalConfig) -> Result<Transport, BlufioError> {
    if let Some(ref path) = config.socket_path {
        let stream = tokio::net::UnixStream::connect(path).await?;
        Ok(Transport::Unix(stream))
    } else {
        let addr = format!("{}:{}", config.host.as_deref().unwrap_or("127.0.0.1"),
                           config.port.unwrap_or(7583));
        let stream = tokio::net::TcpStream::connect(&addr).await?;
        Ok(Transport::Tcp(stream))
    }
}
```

### Pattern 5: WhatsApp Cloud API Webhook
**What:** GET handler for verification challenge, POST handler for incoming messages.
**When to use:** WhatsApp Cloud API integration.
**Example:**
```rust
// GET /webhooks/whatsapp -- verification
async fn whatsapp_verify(Query(params): Query<WhatsAppVerifyParams>) -> impl IntoResponse {
    if params.hub_mode == "subscribe" && params.hub_verify_token == expected_token {
        (StatusCode::OK, params.hub_challenge)
    } else {
        (StatusCode::FORBIDDEN, String::new())
    }
}

// POST /webhooks/whatsapp -- incoming messages
async fn whatsapp_webhook(
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Verify HMAC-SHA256 signature from X-Hub-Signature-256 header
    // Parse JSON payload: entry[].changes[].value.messages[]
    // Forward to inbound_tx as InboundMessage
    StatusCode::OK
}
```

### Pattern 6: Bridge Subsystem Event Flow
**What:** Subscribe to event bus, filter by bridge groups, forward to target channels.
**When to use:** Cross-channel bridging.
**Example:**
```rust
// Bridge subscribes to event bus and routes messages
async fn bridge_loop(
    bus: Arc<EventBus>,
    bridge_config: BridgeConfig,
    channels: Arc<Vec<(String, Arc<dyn ChannelAdapter + Send + Sync>)>>,
) {
    let mut rx = bus.subscribe();
    loop {
        match rx.recv().await {
            Ok(BusEvent::Channel(ChannelEvent::MessageReceived {
                channel, sender_id, sender_name, content, is_bridged, ..
            })) => {
                // Skip if already bridged (loop prevention)
                if is_bridged { continue; }
                // Find bridge groups containing this channel
                for group in &bridge_config.groups {
                    if group.channels.contains(&channel) {
                        let formatted = format!("[{}/{}] {}", channel, sender_name, content);
                        for target in &group.channels {
                            if target != &channel {
                                // Send to target channel via ChannelMultiplexer
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
```

### Anti-Patterns to Avoid
- **Bridge infinite loops:** If channel A bridges to channel B and vice versa, the bridged message from A arriving in B would trigger another bridge event. MUST tag bridged messages with `is_bridged = true` metadata and skip them in the bridge subscriber.
- **Blocking the event bus:** The bridge subscriber must process events quickly. If sending to a target channel is slow (e.g., rate-limited IRC), queue the outbound message rather than blocking the bus receiver.
- **Hardcoded protocol details:** IRC PRIVMSG line length is 512 bytes including the protocol overhead (`:nick!user@host PRIVMSG #channel :`). The splitter must account for this overhead, not just the 512 limit.
- **Ignoring signal-cli connection loss:** signal-cli may restart or crash. The adapter MUST handle EOF/connection-reset gracefully and reconnect with exponential backoff.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| IRC protocol parsing | Custom IRC message parser | `irc` crate's `Client` + `Stream` | IRC has edge cases (CTCP, modes, extended tags); the crate handles them |
| IRC TLS connections | Manual rustls setup for IRC | `irc` crate `tls-rust` feature | The crate handles TLS negotiation, certificate verification |
| IRC NickServ ghost/identify | Manual PRIVMSG to NickServ | `irc` crate `nick_password` + `should_ghost` config | Built-in, handles timing and sequence |
| Matrix event loop | Custom sync loop with /sync API | `matrix-sdk` event handler system | The SDK handles sync pagination, token management, reconnection |
| Matrix room state | Manual room state tracking | `matrix-sdk` `Room` type with `state()` method | SDK tracks join/invite/leave state automatically |
| JSON-RPC 2.0 framing | Full JSON-RPC library | Simple newline-delimited JSON parse | signal-cli uses trivial JSON-RPC (no batching, no complex error codes); a full library adds unnecessary weight |
| WhatsApp webhook HMAC | Custom HMAC implementation | `hmac` + `sha2` workspace crates | Cryptographic primitives must not be hand-rolled |
| Message rate limiting | Manual timer-based throttle | `tokio::time::interval` with message queue | Interval-based dequeuing is simpler and more reliable than timestamp tracking |

**Key insight:** The `irc` crate and `matrix-sdk` handle enormous amounts of protocol complexity (IRC flood control, Matrix /sync pagination, encryption state). Using them saves hundreds of lines of fragile protocol code.

## Common Pitfalls

### Pitfall 1: IRC SASL PLAIN Not Built Into the Crate
**What goes wrong:** The `irc` crate Config has `nick_password` for NickServ but no dedicated SASL field. Developers assume SASL is handled automatically.
**Why it happens:** The `irc` crate predates widespread SASL PLAIN adoption on IRC networks.
**How to avoid:** Implement SASL PLAIN manually: send `CAP REQ :sasl`, respond to `CAP ACK` with `AUTHENTICATE PLAIN`, then send base64-encoded `\0nick\0password`. Fall back to NickServ if SASL fails. The `irc` crate allows sending raw commands via `client.send()`.
**Warning signs:** Authentication works on some servers but not others (NickServ vs SASL-only).

### Pitfall 2: IRC PRIVMSG Line Length Limit
**What goes wrong:** Bot sends a message that exceeds 512 bytes total (including protocol prefix) and the server silently truncates it.
**Why it happens:** RFC 2812 limits total line length to 512 bytes including `\r\n`. The prefix (`:nick!user@host PRIVMSG #channel :`) consumes ~80-100 bytes depending on hostmask.
**How to avoid:** Calculate available payload length as `512 - prefix_length - 2` (for `\r\n`). Split at word boundaries. Send multiple PRIVMSG lines, respecting rate limit.
**Warning signs:** Messages appear cut off mid-word in IRC channels.

### Pitfall 3: signal-cli Connection Lifecycle
**What goes wrong:** signal-cli daemon restarts or network drops, and the adapter hangs waiting for data on a dead connection.
**Why it happens:** TCP connections don't always notify of remote close immediately.
**How to avoid:** Use a read timeout (e.g., 60s). If no data arrives within timeout and no keepalive response, attempt reconnection. Log degraded health status during reconnection.
**Warning signs:** Bot stops responding to Signal messages but shows no errors.

### Pitfall 4: Bridge Infinite Loop
**What goes wrong:** Channel A message bridges to Channel B, which emits a ChannelEvent, which bridges back to Channel A, creating an infinite loop.
**Why it happens:** The bridge subscriber sees all ChannelEvent::MessageReceived events, including ones it generated.
**How to avoid:** Tag bridged messages with `is_bridged: true` in metadata. The bridge subscriber skips events where `is_bridged` is true. Additionally, set a `bridge_origin` field to prevent re-bridging to the source.
**Warning signs:** Exponentially growing message volume, rate limits hit, CPU spike.

### Pitfall 5: WhatsApp Webhook Signature Verification Bypass
**What goes wrong:** Developer skips HMAC-SHA256 verification for speed, leaving the endpoint open to spoofed webhook payloads.
**Why it happens:** Verification adds complexity; "it works without it."
**How to avoid:** Always verify the `X-Hub-Signature-256` header. Compare with `sha256=<hex>` where hex is HMAC-SHA256 of raw body using the app secret. Reject unverified payloads with 401.
**Warning signs:** No signature check in the POST handler code path.

### Pitfall 6: Matrix SDK Version Mismatch
**What goes wrong:** `matrix-sdk` 0.12+ requires Rust 1.88, breaking the project's MSRV of 1.85.
**Why it happens:** Developer uses `matrix-sdk = "0.11"` without `=` prefix, allowing patch upgrades that may pull in breaking changes.
**How to avoid:** Pin exactly: `matrix-sdk = "=0.11.0"`. The project decision explicitly requires this pin. Note: E2E encryption is deferred to EXT-06 (future requirements).
**Warning signs:** CI fails with Rust version errors after dependency update.

### Pitfall 7: WhatsApp Web Account Ban
**What goes wrong:** Meta detects unofficial API usage and permanently bans the phone number.
**Why it happens:** WhatsApp Web protocol is reverse-engineered; Meta actively detects and bans unofficial clients.
**How to avoid:** Feature-flag the WhatsApp Web adapter (`whatsapp-web`), document it as unstable/experimental in all user-facing docs, never enable it by default.
**Warning signs:** Sudden disconnections, "Your phone number has been banned" errors.

## Code Examples

### WhatsApp Cloud API Send Message
```rust
// Source: Meta WhatsApp Cloud API documentation
// POST https://graph.facebook.com/v21.0/{phone_number_id}/messages
async fn send_whatsapp_message(
    client: &reqwest::Client,
    phone_number_id: &str,
    access_token: &str,
    to: &str,
    text: &str,
) -> Result<String, BlufioError> {
    let url = format!(
        "https://graph.facebook.com/v21.0/{}/messages",
        phone_number_id
    );
    let body = serde_json::json!({
        "messaging_product": "whatsapp",
        "to": to,
        "type": "text",
        "text": { "body": text }
    });
    let resp = client
        .post(&url)
        .bearer_auth(access_token)
        .json(&body)
        .send()
        .await
        .map_err(|e| BlufioError::Channel {
            message: format!("WhatsApp API error: {e}"),
            source: None,
        })?;
    let json: serde_json::Value = resp.json().await.map_err(|e| BlufioError::Channel {
        message: format!("WhatsApp response parse error: {e}"),
        source: None,
    })?;
    Ok(json["messages"][0]["id"].as_str().unwrap_or("unknown").to_string())
}
```

### WhatsApp Webhook Payload Parsing
```rust
// Source: Meta WhatsApp Cloud API webhook documentation
#[derive(Debug, Deserialize)]
pub struct WhatsAppWebhookPayload {
    pub object: String,
    pub entry: Vec<WhatsAppEntry>,
}

#[derive(Debug, Deserialize)]
pub struct WhatsAppEntry {
    pub id: String,
    pub changes: Vec<WhatsAppChange>,
}

#[derive(Debug, Deserialize)]
pub struct WhatsAppChange {
    pub field: String,
    pub value: WhatsAppChangeValue,
}

#[derive(Debug, Deserialize)]
pub struct WhatsAppChangeValue {
    pub messaging_product: Option<String>,
    pub metadata: Option<WhatsAppMetadata>,
    pub contacts: Option<Vec<WhatsAppContact>>,
    pub messages: Option<Vec<WhatsAppMessage>>,
    pub statuses: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
pub struct WhatsAppMetadata {
    pub display_phone_number: String,
    pub phone_number_id: String,
}

#[derive(Debug, Deserialize)]
pub struct WhatsAppContact {
    pub profile: WhatsAppProfile,
    pub wa_id: String,
}

#[derive(Debug, Deserialize)]
pub struct WhatsAppProfile {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct WhatsAppMessage {
    pub from: String,
    pub id: String,
    pub timestamp: String,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub text: Option<WhatsAppTextBody>,
}

#[derive(Debug, Deserialize)]
pub struct WhatsAppTextBody {
    pub body: String,
}
```

### signal-cli JSON-RPC Receive Notification
```rust
// Source: signal-cli man page (signal-cli-jsonrpc.5.adoc)
// Incoming message notification (newline-delimited JSON):
// {"jsonrpc":"2.0","method":"receive","params":{"envelope":{
//   "source":"+33123456789",
//   "sourceNumber":"+33123456789",
//   "sourceName":"Alice",
//   "timestamp":1631458508784,
//   "dataMessage":{"message":"Hello!","timestamp":1631458508784}
// }}}
//
// Group message:
// {"jsonrpc":"2.0","method":"receive","params":{"envelope":{
//   "source":"+33123456789",
//   "dataMessage":{"message":"Hello group!","groupInfo":{"groupId":"base64..."}}
// }}}

#[derive(Debug, Deserialize)]
pub struct SignalNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: SignalParams,
}

#[derive(Debug, Deserialize)]
pub struct SignalParams {
    pub envelope: SignalEnvelope,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalEnvelope {
    pub source: Option<String>,
    pub source_number: Option<String>,
    pub source_name: Option<String>,
    pub timestamp: Option<u64>,
    pub data_message: Option<SignalDataMessage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalDataMessage {
    pub message: Option<String>,
    pub timestamp: Option<u64>,
    pub group_info: Option<SignalGroupInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalGroupInfo {
    pub group_id: String,
}
```

### Matrix Event Handler with Invite Auto-Join
```rust
// Source: Context7 /matrix-org/matrix-rust-sdk
use matrix_sdk::{
    Client, Room, RoomState,
    config::SyncSettings,
    ruma::events::room::{
        message::{MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent},
        member::StrippedRoomMemberEvent,
    },
};

async fn on_room_message(event: OriginalSyncRoomMessageEvent, room: Room) {
    if room.state() != RoomState::Joined { return; }
    let MessageType::Text(text_content) = event.content.msgtype else { return };
    // Process text_content.body, check for @mention, forward to inbound_tx
}

async fn on_room_invite(event: StrippedRoomMemberEvent, client: Client, room: Room) {
    if event.state_key != client.user_id().unwrap() { return; }
    tokio::spawn(async move {
        // Retry join with backoff (room may not be ready immediately)
        for _ in 0..3 {
            if room.join().await.is_ok() { break; }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    });
}

// In connect():
client.add_event_handler(on_room_message);
client.add_event_handler(on_room_invite);
client.sync(SyncSettings::default()).await?;
```

### IRC SASL PLAIN Manual Implementation
```rust
// Source: IRC RFC / IRCv3 SASL specification
// SASL PLAIN is not built into the `irc` crate; must be sent as raw commands.
use irc::client::Client;
use base64::Engine;

async fn authenticate_sasl(client: &Client, nickname: &str, password: &str) -> Result<(), BlufioError> {
    // Request SASL capability
    client.send(irc::proto::Command::CAP(None, "REQ".into(), None, Some("sasl".into())))
        .map_err(|e| BlufioError::Channel { message: format!("CAP REQ failed: {e}"), source: None })?;

    // Wait for CAP ACK, then:
    client.send(irc::proto::Command::Raw("AUTHENTICATE".into(), vec!["PLAIN".into()]))
        .map_err(|e| BlufioError::Channel { message: format!("AUTHENTICATE failed: {e}"), source: None })?;

    // Encode credentials: \0nickname\0password
    let credentials = format!("\0{}\0{}", nickname, password);
    let encoded = base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes());

    client.send(irc::proto::Command::Raw("AUTHENTICATE".into(), vec![encoded]))
        .map_err(|e| BlufioError::Channel { message: format!("SASL auth failed: {e}"), source: None })?;

    // Wait for RPL_SASLSUCCESS (903) or ERR_SASLFAIL (904)
    Ok(())
}
```

### IRC Flood-Protected Message Queue
```rust
// Rate-limited message sending for IRC
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};

pub struct FloodProtectedSender {
    queue_tx: mpsc::Sender<(String, String)>,  // (target, message)
}

impl FloodProtectedSender {
    pub fn new(client: Arc<irc::client::Client>, rate_limit_ms: u64) -> Self {
        let (queue_tx, mut queue_rx) = mpsc::channel::<(String, String)>(256);
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_millis(rate_limit_ms));
            while let Some((target, message)) = queue_rx.recv().await {
                ticker.tick().await;  // Wait for rate limit window
                let _ = client.send_privmsg(&target, &message);
            }
        });
        Self { queue_tx }
    }

    pub async fn send(&self, target: &str, message: &str) -> Result<(), BlufioError> {
        self.queue_tx.send((target.to_string(), message.to_string())).await
            .map_err(|_| BlufioError::Channel {
                message: "IRC send queue closed".into(),
                source: None,
            })
    }
}
```

### Bridge Config TOML
```toml
# Cross-channel bridge configuration
[bridge.team-chat]
channels = ["telegram", "discord", "slack"]
exclude_bots = true

[bridge.support]
channels = ["whatsapp", "matrix"]
include_users = ["user123", "user456"]
exclude_bots = true
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| WhatsApp On-Premises API | WhatsApp Cloud API only | October 2025 (deprecated) | Must use Cloud API; on-prem no longer supported |
| matrix-sdk 0.7-0.9 | matrix-sdk 0.11.0 | 2025 | OAuth replaces OIDC, new event handler patterns, Rust 1.85 MSRV |
| signal-cli REST API wrapper | signal-cli native JSON-RPC daemon | v0.11+ | JSON-RPC is the first-class daemon interface; REST is a community wrapper |
| IRC SASL via NickServ | SASL PLAIN during connection registration | IRCv3 standard | SASL authenticates before visible on network; NickServ is post-connect fallback |

**Deprecated/outdated:**
- WhatsApp On-Premises API: Deprecated October 2025, Cloud API is the only path
- `matrix-sdk` < 0.11: Older versions use different auth APIs (OIDC vs OAuth rename)
- `whatsappweb` crate (wiomoc): At 0.0.1, appears abandoned; `whatsapp-rust` is the active alternative

## Open Questions

1. **`whatsapp-rust` crate stability and API surface**
   - What we know: It exists, is based on whatsmeow/Baileys patterns, has ~356 stars
   - What's unclear: The exact current version, API stability, QR code login flow, message receive callback pattern
   - Recommendation: Pin to a specific tested version; treat as experimental per user decision; document that breakage is expected

2. **ChannelEvent needs content for bridging**
   - What we know: Current `ChannelEvent::MessageReceived` only has `event_id`, `timestamp`, `channel`, `sender_id`
   - What's unclear: Whether adding `content: String` and `sender_name: String` fields to the event breaks existing subscribers
   - Recommendation: Add optional fields (`content: Option<String>`, `sender_name: Option<String>`, `is_bridged: bool`) to `ChannelEvent::MessageReceived`. Existing code ignores them. Bridge subscriber requires them.

3. **IRC SASL PLAIN: response handling**
   - What we know: Need to send raw CAP/AUTHENTICATE commands; the `irc` crate supports raw command sending
   - What's unclear: Exact flow for intercepting numeric responses (903/904) in the `irc` crate's message stream
   - Recommendation: Process SASL responses in the main message stream loop before entering the normal PRIVMSG handling; if SASL fails, fall back to NickServ automatically

4. **Matrix E2E encryption scope**
   - What we know: EXT-06 (future requirement) explicitly defers E2E encryption support
   - What's unclear: Whether `matrix-sdk` 0.11 enables encryption by default and whether it needs to be explicitly disabled
   - Recommendation: Use `default-features = false` and only enable `rustls-tls`; skip `e2e-encryption` feature. Document that encrypted rooms are not supported in this phase.

5. **Bridge access to ChannelMultiplexer for outbound sending**
   - What we know: The bridge needs to send messages to target channels; the `ChannelMultiplexer` owns the connected channel references
   - What's unclear: How to share the connected channel references with the bridge without circular dependencies
   - Recommendation: The bridge receives a clone of `Arc<Vec<(String, Arc<dyn ChannelAdapter>)>>` from the multiplexer after connect(). Or the bridge sends `OutboundMessage` via a shared mpsc channel that the multiplexer consumes.

## Sources

### Primary (HIGH confidence)
- Context7 `/matrix-org/matrix-rust-sdk` - Room management, message sending, event handlers, login/sync patterns
- signal-cli official man page `signal-cli-jsonrpc.5.adoc` - JSON-RPC 2.0 API specification, methods, notification format, socket/TCP transport
- Meta WhatsApp Cloud API webhook documentation - Payload format, verification flow, HMAC-SHA256 signing

### Secondary (MEDIUM confidence)
- `irc` crate docs.rs documentation - Config struct fields (TLS, NickServ, port defaults), API surface
- matrix-sdk 0.11.0 release notes - MSRV 1.85, OAuth changes, breaking changes from 0.10
- `whatsapp-rust` crate on crates.io/GitHub - Existence verified, based on whatsmeow/Baileys

### Tertiary (LOW confidence)
- IRC SASL PLAIN implementation details - Based on IRCv3 spec and blog posts; needs validation against `irc` crate's raw command API
- `whatsapp-rust` API surface and stability - No Context7 data, minimal docs.rs documentation; needs hands-on testing

## Metadata

**Confidence breakdown:**
- Standard stack: MEDIUM-HIGH - Core libraries verified via Context7/docs; `whatsapp-rust` is LOW confidence (unofficial, minimal docs)
- Architecture: HIGH - Follows established adapter pattern from Phase 33; Slack adapter is a complete reference
- Pitfalls: MEDIUM - IRC SASL and bridge loop prevention are well-understood problems; signal-cli reconnection specifics need validation

**Research date:** 2026-03-06
**Valid until:** 2026-04-06 (30 days; stable domain, libraries pinned)
