# Architecture Research: v1.3 Ecosystem Expansion

**Domain:** Rust AI Agent Platform — adding OpenAI-compatible API, multi-provider LLM, multi-channel adapters, event bus, skill registry, node system, Docker, and migration tooling to a 21-crate workspace
**Researched:** 2026-03-05
**Confidence:** HIGH (direct codebase analysis + verified library docs + official API specs)

---

## Standard Architecture

### System Overview: Current State (v1.2)

```
┌──────────────────────────────────────────────────────────────────────┐
│                         blufio process (single binary)                │
│                                                                        │
│  ┌──────────────────────────────────────────────────────────────────┐ │
│  │                     INBOUND CHANNELS LAYER                        │ │
│  │  ┌──────────────┐     ┌───────────────────────────────────────┐  │ │
│  │  │blufio-telegram│     │     blufio-gateway (axum HTTP/WS)     │  │ │
│  │  │  (teloxide)  │     │  POST /v1/messages  GET /ws           │  │ │
│  │  └──────┬───────┘     └──────────────────┬────────────────────┘  │ │
│  │         │                                 │                        │ │
│  │         └────────────────┬────────────────┘                       │ │
│  │                          ▼                                         │ │
│  │              ┌────────────────────────┐                            │ │
│  │              │  ChannelMultiplexer    │  (blufio-agent)            │ │
│  │              │  aggregates N channels │                            │ │
│  │              └──────────┬─────────────┘                           │ │
│  └─────────────────────────┼───────────────────────────────────────┘ │
│                             │                                          │
│  ┌──────────────────────────▼───────────────────────────────────────┐ │
│  │                      AGENT LOOP LAYER                              │ │
│  │  blufio-agent: FSM per session, tool loop, budget enforcement      │ │
│  │  blufio-router: Haiku/Sonnet/Opus model selection                  │ │
│  │  blufio-context: 3-zone context engine with cache alignment        │ │
│  │  blufio-memory: ONNX embedding, hybrid search, fact extraction     │ │
│  └────────┬──────────────────────────────┬────────────────────────────┘ │
│           │                              │                              │
│  ┌────────▼──────────┐     ┌────────────▼──────────────────────────┐  │
│  │  PROVIDER LAYER   │     │           SKILL / TOOL LAYER          │  │
│  │  blufio-anthropic  │     │  blufio-skill: WASM sandbox (wasmtime) │  │
│  │  ProviderAdapter  │     │  blufio-mcp-client: external tools    │  │
│  └────────┬──────────┘     │  blufio-mcp-server: expose tools      │  │
│           │                └──────────────┬────────────────────────┘  │
│           │                               │                            │
│  ┌────────▼───────────────────────────────▼──────────────────────────┐ │
│  │                    PERSISTENCE LAYER                                │ │
│  │  blufio-storage: SQLCipher SQLite (WAL, single-writer)             │ │
│  │  blufio-vault: AES-256-GCM credential store                        │ │
│  │  blufio-cost: token usage + budget tracking ledger                 │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  CROSS-CUTTING: blufio-core (traits), blufio-config, blufio-security,  │
│  blufio-plugin, blufio-prometheus, blufio-auth-keypair, blufio-verify  │
└──────────────────────────────────────────────────────────────────────────┘
```

### System Overview: Target State (v1.3)

```
┌──────────────────────────────────────────────────────────────────────────┐
│                          blufio process (single binary)                   │
│                                                                            │
│  ┌──────────────────────────────────────────────────────────────────────┐ │
│  │                     INBOUND CHANNELS LAYER                             │ │
│  │  ┌────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌───────────┐   │ │
│  │  │Telegram│ │ Discord  │ │  Slack   │ │ WhatsApp │ │Matrix/IRC │   │ │
│  │  │(built-in│ │(NEW crate│ │(NEW crate│ │(NEW crate│ │(NEW crates│   │ │
│  │  └───┬────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘ └─────┬─────┘   │ │
│  │      └──────────────────────────────────────────────────┐          │ │
│  │                                                           ▼          │ │
│  │  ┌──────────────────────────────────────────────────────────────┐  │ │
│  │  │             blufio-gateway (MODIFIED — axum)                  │  │ │
│  │  │  Existing:  POST /v1/messages  GET /ws                        │  │ │
│  │  │  NEW:       POST /v1/chat/completions (OpenAI compat)         │  │ │
│  │  │             POST /v1/responses      (OpenResponses)           │  │ │
│  │  │             POST /v1/tools/invoke   (Tools API)               │  │ │
│  │  │             POST /v1/webhooks       (Webhook mgmt)            │  │ │
│  │  │             POST /v1/batch          (Batch ops)               │  │ │
│  │  │  Auth:      Scoped API keys with per-key rate limits          │  │ │
│  │  └──────────────────────────────────┬───────────────────────────┘  │ │
│  └─────────────────────────────────────┼──────────────────────────────┘ │
│                                         │                                 │
│  ┌──────────────────────────────────────▼─────────────────────────────┐  │
│  │                  EVENT BUS (NEW — blufio-bus)                        │  │
│  │  tokio broadcast channel for cross-component pub/sub events          │  │
│  │  Events: MessageReceived, MessageSent, ToolInvoked, SessionStarted   │  │
│  └──────────────┬────────────────────────────────────────────────────┘  │
│                  │                                                         │
│  ┌───────────────▼─────────────────────────────────────────────────────┐  │
│  │                       AGENT LOOP LAYER                                │  │
│  │  blufio-agent (MODIFIED: event bus integration, multi-provider)       │  │
│  └──────┬───────────────────────────────────┬────────────────────────┘  │
│         │                                    │                             │
│  ┌──────▼──────────────┐        ┌───────────▼────────────────────────┐   │
│  │  PROVIDER LAYER      │        │       SKILL / TOOL LAYER           │   │
│  │  blufio-anthropic    │        │  blufio-skill: WASM + registry     │   │
│  │  blufio-openai (NEW) │        │  blufio-registry (NEW): marketplace│   │
│  │  blufio-ollama (NEW) │        │  Ed25519 skill signing             │   │
│  │  blufio-openrouter   │        │  blufio-mcp-client, mcp-server     │   │
│  │    (NEW)             │        └────────────────────────────────────┘   │
│  │  blufio-gemini (NEW) │                                                  │
│  │  (TTS/image traits   │                                                  │
│  │   in blufio-core)    │                                                  │
│  └──────────────────────┘                                                  │
│                                                                             │
│  NODE SYSTEM (NEW — blufio-node): paired-device mesh via Ed25519 + HTTP   │
│                                                                             │
│  MIGRATION (NEW — blufio binary): openclaw import, session bridge          │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Integration Analysis: New Features vs Existing Architecture

### Feature 1: OpenAI-Compatible API Layer

**Integration surface:** `blufio-gateway` (modification) + `blufio-config` (modification)
**New crate needed:** No — gateway modification only
**Approach:** Add new axum route group `/v1/` alongside existing `/v1/messages`

The gateway currently handles Blufio's native protocol (`POST /v1/messages`). The OpenAI-compatible API must translate between OpenAI request format and Blufio's `ProviderRequest`/`InboundMessage` types, then route through the existing agent loop.

**New routes in `blufio-gateway/src/server.rs`:**

```
POST /v1/chat/completions   → OpenAI Chat Completions API
POST /v1/responses          → OpenResponses API (stateful, multi-turn)
POST /v1/tools/invoke       → Direct tool invocation without agent loop
GET  /v1/api-keys           → List scoped API keys
POST /v1/api-keys           → Create scoped API key
DELETE /v1/api-keys/{id}    → Revoke scoped API key
GET  /v1/webhooks           → List webhook registrations
POST /v1/webhooks           → Register webhook endpoint
DELETE /v1/webhooks/{id}    → Remove webhook
POST /v1/batch              → Submit batch of completions
GET  /v1/batch/{id}         → Poll batch status
```

**New module structure in `blufio-gateway/src/`:**

```
blufio-gateway/src/
├── openai/
│   ├── mod.rs          # Route group assembly
│   ├── chat.rs         # /v1/chat/completions handler
│   ├── responses.rs    # /v1/responses handler (OpenResponses)
│   ├── tools.rs        # /v1/tools/invoke handler
│   ├── batch.rs        # /v1/batch handlers
│   ├── types.rs        # OpenAI request/response types (serde)
│   └── translate.rs    # OpenAI types <-> Blufio types conversion
├── apikey/
│   ├── mod.rs          # Scoped API key management
│   ├── store.rs        # Key storage in SQLite
│   └── middleware.rs   # Per-key auth + rate limit middleware
├── webhook/
│   ├── mod.rs          # Webhook registration + delivery
│   └── store.rs        # Webhook DB persistence
├── auth.rs             # Existing auth (bearer + keypair)
├── handlers.rs         # Existing handlers
├── server.rs           # MODIFIED: mount openai routes
├── sse.rs
└── ws.rs
```

**Key translation invariants:**

The `chat/completions` handler converts OpenAI `messages[]` → Blufio `ProviderRequest`, routes through the existing `AgentLoop` or directly to a `ProviderAdapter`, then translates `ProviderResponse` → OpenAI response format. Streaming uses SSE (`text/event-stream`) with the same SSE infrastructure used by the MCP server.

**Scoped API keys:** A new `ApiKeyStore` backed by a new table in the existing SQLite DB (via `blufio-storage`). Keys carry a scope bitmask (read, write, tools) and a per-key rate limit config. The `tower_governor` crate (or `governor`) provides token-bucket rate limiting per key using a DashMap keyed by hashed API key.

**Confidence:** HIGH — axum route nesting is a first-class pattern. OpenAI API format is publicly documented. tower_governor verified on docs.rs.

---

### Feature 2: Multi-Provider LLM Support

**Integration surface:** New provider crates, `blufio-core` (minor extension), `blufio-config` (new sections)
**New crates needed:** `blufio-openai`, `blufio-ollama`, `blufio-openrouter`, `blufio-gemini`

All four new providers implement the existing `ProviderAdapter` trait from `blufio-core`. No trait changes are required for text completion. The trait already supports streaming via `Pin<Box<dyn Stream<...>>>`.

**Provider architecture pattern (same as `blufio-anthropic`):**

```
blufio-{provider}/src/
├── lib.rs        # ProviderAdapter impl struct
├── client.rs     # reqwest HTTP client wrapper
├── types.rs      # API request/response structs (serde)
└── sse.rs        # SSE/streaming parser (for streaming providers)
```

**Provider-specific notes:**

| Provider | API Format | Key Difference from Anthropic |
|----------|------------|-------------------------------|
| `blufio-openai` | OpenAI Chat Completions | SSE format with `data: [DONE]` terminator, `choices[0].delta.content` chunks |
| `blufio-ollama` | OpenAI-compatible `/api/chat` | Local HTTP (no TLS required), pull model detection, no API key |
| `blufio-openrouter` | OpenAI-compatible + `HTTP-Referer` header | Model selection via `model` field (maps to OpenRouter's unified namespace) |
| `blufio-gemini` | Google Generative Language API | Different JSON structure, `candidates[0].content.parts[0].text`, SSE via `alt=sse` |

**TTS/Transcription/Image traits:** Add to `blufio-core/src/traits/`:
- `TtsAdapter`: `async fn synthesize(&self, text: &str, voice: &str) -> Result<Bytes, BlufioError>`
- `TranscriptionAdapter`: `async fn transcribe(&self, audio: Bytes, format: &str) -> Result<String, BlufioError>`
- `ImageAdapter`: `async fn generate(&self, prompt: &str, size: &str) -> Result<String, BlufioError>` (returns URL)

**Config additions for multi-provider:**

```toml
# blufio-config/src/model.rs — new sections
[providers]
default = "anthropic"    # Which provider to use for main agent loop

[providers.openai]
api_key = "sk-..."
model = "gpt-4o"

[providers.ollama]
base_url = "http://localhost:11434"
model = "llama3.2"

[providers.openrouter]
api_key = "sk-or-..."
model = "anthropic/claude-3.5-sonnet"

[providers.gemini]
api_key = "..."
model = "gemini-2.0-flash"
```

The `AgentLoop` selects the active provider at startup from `config.providers.default`. The `ModelRouter` continues to route within the active provider's model tiers (or the provider exposes Haiku/Sonnet/Opus equivalents).

**Confidence:** HIGH — all four providers have verified Rust client libraries. ProviderAdapter trait boundary is clean.

---

### Feature 3: Multi-Channel Adapters

**Integration surface:** New channel crates, `blufio-agent/src/channel_mux.rs` (no change needed — multiplexer already supports N channels), `blufio-config` (new sections)
**New crates needed:** `blufio-discord`, `blufio-slack`, `blufio-whatsapp`, `blufio-signal`, `blufio-irc`, `blufio-matrix`

All implement `ChannelAdapter` from `blufio-core`. The `ChannelMultiplexer` already handles N channels with per-channel background receive tasks — zero changes needed there.

**Channel crate structure (same pattern as `blufio-telegram`):**

```
blufio-{channel}/src/
├── lib.rs       # ChannelAdapter impl — connect(), send(), receive()
├── client.rs    # Platform SDK wrapper
└── types.rs     # Platform-specific message types
```

**Platform SDK recommendations:**

| Channel | Recommended Crate | Protocol | AGPL Risk |
|---------|-------------------|----------|-----------|
| Discord | `serenity` (0.12) or `twilight` | Gateway WebSocket + REST | No |
| Slack | `slack-morphism` | Events API (HTTP webhook) or Socket Mode (WebSocket) | No |
| WhatsApp | `reqwest` + unofficial API or official Meta Cloud API | HTTP | No |
| Signal | `presage` | Signal protocol | AGPL — isolated to crate boundary |
| IRC | `irc` crate | TCP text protocol | No |
| Matrix | `matrix-sdk` | HTTP + Matrix protocol | No |

**Signal AGPL isolation:** Signal requires the `presage` crate which is AGPL-3.0. The `blufio-signal` crate must be compiled as a separate binary or kept in a distinct workspace member with clear license documentation. The plugin boundary prevents AGPL contamination of the core. Alternatively, Signal is implemented as a separate `blufio-signal-bridge` process communicating via the gateway HTTP API.

**Cross-channel bridging:** Messages on one channel forwarded to another. Implementation: the agent loop receives a message on channel A, processes it, and the response is sent to channel B. The `ChannelMultiplexer.send()` already routes by channel name — bridging is a routing policy in the agent, not a structural change. A `BridgeRule` type in `blufio-config` controls source→destination channel mapping.

**Config additions:**

```toml
[channels.discord]
token = "Bot ..."
guild_id = "..."

[channels.slack]
bot_token = "xoxb-..."
app_token = "xapp-..."

[channels.matrix]
homeserver = "https://matrix.org"
access_token = "..."
```

**Confidence:** HIGH for Discord (serenity well-maintained), MEDIUM for WhatsApp (Meta API stability), LOW for Signal (presage maintenance uncertain).

---

### Feature 4: Internal Event Bus

**Integration surface:** New `blufio-bus` crate, `blufio-agent` (integration), `blufio-gateway` (webhook delivery), `blufio-core` (event type definitions)
**New crates needed:** `blufio-bus`

The event bus is a typed pub/sub system over `tokio::sync::broadcast`. It replaces the current pattern where metrics and logging are the only cross-cutting signals. The bus enables webhook delivery, cross-channel bridging, and future observability extensions.

**`blufio-bus` crate structure:**

```
blufio-bus/src/
├── lib.rs          # EventBus struct, subscribe(), publish()
├── events.rs       # BlufioEvent enum — all platform events
└── subscription.rs # Subscriber handle with typed filtering
```

**Event type design:**

```rust
// blufio-core/src/types.rs (add event types)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum BlufioEvent {
    MessageReceived { session_id: String, channel: String, content: String },
    MessageSent     { session_id: String, channel: String, content: String },
    ToolInvoked     { session_id: String, tool_name: String, input: serde_json::Value },
    ToolCompleted   { session_id: String, tool_name: String, output: String, error: bool },
    SessionStarted  { session_id: String, channel: String, user_id: String },
    SessionEnded    { session_id: String },
    ProviderCall    { session_id: String, model: String, input_tokens: u32 },
    ProviderResponse{ session_id: String, model: String, output_tokens: u32 },
    CostExceeded    { budget_type: String, limit_usd: f64, actual_usd: f64 },
    SkillLoaded     { skill_name: String, version: String },
    NodeConnected   { node_id: String, peer_addr: String },
}
```

**EventBus implementation:**

```rust
// blufio-bus/src/lib.rs
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<BlufioEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn publish(&self, event: BlufioEvent) {
        // Drop lagged messages silently (never block the agent loop)
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<BlufioEvent> {
        self.tx.subscribe()
    }
}
```

**Integration points for the event bus:**
- `AgentLoop` holds `Arc<EventBus>`, publishes `MessageReceived`, `ToolInvoked`, `ProviderCall`, etc.
- The webhook delivery system subscribes and filters events matching registered webhook patterns.
- Cross-channel bridge rules subscribe and forward matching messages.

**Backpressure:** The broadcast channel has a fixed capacity (default 1024). Slow webhook consumers get `RecvError::Lagged` and drop events — this is correct behavior. The agent loop is never blocked by slow subscribers.

**Confidence:** HIGH — tokio broadcast channel is the canonical Rust fan-out pattern. Zero external dependencies needed.

---

### Feature 5: Skill Registry / Marketplace

**Integration surface:** New `blufio-registry` crate, `blufio-skill` (modification), `blufio-config` (new section), binary crate (CLI)
**New crates needed:** `blufio-registry`

The skill registry is a local + optionally remote index of WASM skill packages. Each skill package is:
1. A WASM binary (`skill.wasm`)
2. A manifest (`skill.toml` — name, version, description, capabilities, signature)
3. An Ed25519 signature file (`skill.wasm.sig`) using the existing minisign-verify infrastructure

**Registry architecture:**

```
blufio-registry/src/
├── lib.rs          # SkillRegistry: discover, install, verify, list
├── index.rs        # Registry index: local file + optional remote JSON feed
├── installer.rs    # Download, verify signature, extract to skills_dir
├── signing.rs      # Ed25519 sign/verify for skill packages
└── store.rs        # SQLite-backed installed skill metadata
```

**Registry index format (JSON):**

```json
{
  "version": 1,
  "skills": [
    {
      "name": "weather",
      "version": "1.2.0",
      "description": "Fetch weather for any location",
      "author": "blufio-community",
      "download_url": "https://registry.blufio.dev/skills/weather-1.2.0.wasm",
      "signature_url": "https://registry.blufio.dev/skills/weather-1.2.0.wasm.sig",
      "sha256": "abc123...",
      "public_key": "RWSomeBase64..."
    }
  ]
}
```

**CLI commands (added to binary crate):**

```
blufio skill search [query]     # Search local + remote index
blufio skill install [name]     # Download, verify, install
blufio skill uninstall [name]   # Remove from skills_dir
blufio skill list               # List installed skills + status
blufio skill verify [name]      # Re-verify signature of installed skill
blufio skill publish [path]     # Package + sign a skill for publishing
```

**Code signing:** Uses existing `ed25519-dalek` workspace dependency. The same Ed25519 key pair used for multi-agent delegation can be used for skill signing, or a separate signing key pair. The `minisign-verify` crate already in workspace handles verification.

**Integration with existing `blufio-skill`:** The registry discovers skills installed in `config.skill.skills_dir`. The existing `SkillStore` and `WasmSkillRuntime` load them. Registry only adds install/verify/search — runtime is unchanged.

**Confidence:** HIGH — skill signing uses existing ed25519-dalek. Registry index is a static JSON file. No new complex dependencies.

---

### Feature 6: Node System (Paired Devices)

**Integration surface:** New `blufio-node` crate, binary crate (CLI pairing), `blufio-config` (new section), `blufio-gateway` (node peer routes)
**New crates needed:** `blufio-node`

The node system creates a trusted mesh of Blufio instances that can share sessions, delegate tasks, and relay channel messages. It is **not** P2P networking — nodes communicate via authenticated HTTPS using the existing gateway pattern.

**Architecture:**

```
Node A (gateway :3000)          Node B (gateway :3000)
┌─────────────────────┐         ┌─────────────────────┐
│  blufio-node        │         │  blufio-node        │
│  NodeRegistry       │◄──────►│  NodeRegistry       │
│  peers: {B: pubkey} │  HTTPS  │  peers: {A: pubkey} │
└─────────────────────┘         └─────────────────────┘
```

**Trust model:** Nodes authenticate using Ed25519 device keypairs (already implemented in `blufio-auth-keypair`). Pairing is done out-of-band (QR code / token exchange) and stored in the vault.

**Node communication:** Reuses the existing gateway HTTP/WS infrastructure. Nodes exchange messages via a new authenticated endpoint:

```
POST /v1/nodes/relay     # Relay message to/from peer node
GET  /v1/nodes           # List paired nodes + status
POST /v1/nodes/pair      # Initiate pairing (challenge-response)
DELETE /v1/nodes/{id}    # Unpair node
```

**`blufio-node` crate structure:**

```
blufio-node/src/
├── lib.rs          # NodeSystem: pair, relay, health
├── peer.rs         # NodePeer: Ed25519 identity, HTTP client
├── registry.rs     # NodeRegistry: stored peers with pubkeys
└── relay.rs        # Message relay protocol
```

**Config:**

```toml
[node]
enabled = false
# Pairing managed via CLI — not static config
```

**Avoid libp2p:** Full P2P networking (libp2p, hole punching, DHT) is overkill. The "node system" in the PRD describes paired trusted devices — this is a curated mesh, not an open network. The existing HTTPS + Ed25519 auth pattern is sufficient and keeps the dependency surface small.

**Confidence:** MEDIUM — architecture is simple (HTTPS + Ed25519 auth already exists), but exact protocol design needs implementation decisions.

---

### Feature 7: Docker Image

**Integration surface:** New `Dockerfile` + `docker-compose.yml` in `deploy/`, no crate changes
**New crates needed:** None

**Multi-stage build pattern:**

```dockerfile
# Stage 1: Build the static musl binary
FROM clux/muslrust:stable AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
# Build with release-musl profile (LTO=true, opt-level=s, strip=symbols)
RUN cargo build --profile release-musl --target x86_64-unknown-linux-musl \
    --bin blufio

# Stage 2: Minimal runtime image
FROM scratch
# SSL certificates for HTTPS outbound (reqwest uses rustls, but needs root certs)
COPY --from=alpine:3 /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release-musl/blufio /blufio
# Data directory mount point
VOLUME ["/data"]
ENV BLUFIO_DATA_DIR=/data
ENTRYPOINT ["/blufio"]
CMD ["serve"]
```

**Docker Compose for development/deployment:**

```yaml
# deploy/docker-compose.yml
services:
  blufio:
    image: blufio:latest
    volumes:
      - blufio_data:/data
      - ./blufio.toml:/etc/blufio/blufio.toml:ro
    environment:
      - ANTHROPIC_API_KEY
      - BLUFIO_DB_KEY
      - BLUFIO_VAULT_KEY
    ports:
      - "3000:3000"
    restart: unless-stopped
    healthcheck:
      test: ["/blufio", "doctor", "--json"]
      interval: 30s
      timeout: 10s
      retries: 3

volumes:
  blufio_data:
```

**Key Docker considerations:**
- `scratch` base requires static binary — already handled by musl profile
- `clux/muslrust` image is the canonical Rust musl build environment
- SQLCipher vendored OpenSSL compiles into the binary — no runtime SSL library needed
- `BLUFIO_DATA_DIR` env var overrides default XDG paths for containerized paths
- sd-notify is a no-op in Docker (no NOTIFY_SOCKET) — silent, no change needed

**Multi-instance systemd template:** A `blufio@.service` template with `%i` interpolation for instance name/port, allowing `systemctl start blufio@home blufio@work` for separate instances.

**Confidence:** HIGH — musl static builds are well-established for Rust. clux/muslrust is the standard solution. Docker Compose format stable.

---

### Feature 8: OpenClaw Migration Tool

**Integration surface:** Binary crate (new CLI subcommand `blufio migrate openclaw`), `blufio-storage` (write path, already exists)
**New crates needed:** None

**OpenClaw data format:**

OpenClaw stores data as:
- `~/.openclaw/sessions/*.jsonl` — Session transcripts (one file per session)
- `~/.openclaw/memory/memories.json` — Memory store
- `~/.openclaw/skills/` — Skill directories (Node.js files)
- Credentials in environment files or plain TOML

**Migration flow:**

```
blufio migrate openclaw [--source ~/.openclaw] [--dry-run]
  |
  v
1. Discover OpenClaw data directory
2. Parse session JSONL files -> normalize to Blufio Message structs
3. Create sessions in blufio-storage (create_session, insert_message)
4. Parse memory JSON -> embed and store in blufio-memory (best-effort)
5. Report skill migration skipped (WASM recompile required)
6. Report credential migration skipped (manual — security boundary)
```

**Session JSONL normalization:**

```rust
// OpenClaw session JSONL entry
{ "role": "user", "content": "...", "timestamp": 1234567890 }
{ "role": "assistant", "content": "...", "timestamp": 1234567891, "usage": {...} }

// Blufio Message
Message { id, session_id, role, content, token_count, metadata, created_at }
```

**CLI module location:** `crates/blufio/src/migrate.rs`. Same pattern as `update.rs` from v1.2.

**Confidence:** HIGH — OpenClaw JSONL format is public (GitHub repos). Migration is a read-translate-write operation using existing storage API.

---

### Feature 9: CLI Utilities

**Integration surface:** Binary crate only
**New crates needed:** None

| Command | Location | Implementation |
|---------|----------|----------------|
| `blufio bench` | `crates/blufio/src/bench.rs` | Synthetic throughput test: POST to gateway, measure latency percentiles |
| `blufio privacy evidence-report` | `crates/blufio/src/privacy.rs` | Enumerate: data stored, providers connected, channels active, retention |
| `blufio config recipe` | `crates/blufio/src/config_cmd.rs` | Generate annotated blufio.toml templates for common scenarios |
| `blufio uninstall` | `crates/blufio/src/uninstall.rs` | Remove binary, systemd unit, data dir (with confirmation) |
| `blufio bundle` | `crates/blufio/src/bundle.rs` | Create self-contained air-gapped tarball: binary + DB + skills |

---

## Component Dependency Graph

```
blufio (binary)
├── blufio-agent          [MODIFIED: event bus, multi-provider selection]
│   ├── blufio-core       [MODIFIED: new provider/TTS/image traits, event types]
│   ├── blufio-config     [MODIFIED: providers[], channels[], node, registry sections]
│   ├── blufio-context
│   ├── blufio-cost
│   ├── blufio-memory
│   ├── blufio-router
│   └── blufio-skill      [MODIFIED: registry integration]
│
├── blufio-gateway        [MODIFIED: OpenAI routes, scoped keys, webhook delivery]
│   ├── blufio-core
│   ├── blufio-bus        [NEW: event bus subscription for webhooks]
│   └── blufio-storage    [NEW dep: API key + webhook store]
│
├── blufio-bus            [NEW: tokio broadcast event bus]
│   └── blufio-core       [event type definitions]
│
├── blufio-openai         [NEW: ProviderAdapter for OpenAI]
│   └── blufio-core
│
├── blufio-ollama         [NEW: ProviderAdapter for Ollama]
│   └── blufio-core
│
├── blufio-openrouter     [NEW: ProviderAdapter for OpenRouter]
│   └── blufio-core
│
├── blufio-gemini         [NEW: ProviderAdapter for Google Gemini]
│   └── blufio-core
│
├── blufio-discord        [NEW: ChannelAdapter for Discord]
│   └── blufio-core
│
├── blufio-slack          [NEW: ChannelAdapter for Slack]
│   └── blufio-core
│
├── blufio-whatsapp       [NEW: ChannelAdapter for WhatsApp]
│   └── blufio-core
│
├── blufio-matrix         [NEW: ChannelAdapter for Matrix]
│   └── blufio-core
│
├── blufio-irc            [NEW: ChannelAdapter for IRC]
│   └── blufio-core
│
├── blufio-registry       [NEW: skill registry + signing]
│   ├── blufio-core
│   ├── blufio-skill
│   └── blufio-storage    [NEW dep: registry metadata table]
│
├── blufio-node           [NEW: paired-device node system]
│   ├── blufio-core
│   ├── blufio-auth-keypair [EXISTING: Ed25519 device identity]
│   └── blufio-gateway    [EXISTING: HTTP endpoint reuse]
│
└── (existing unchanged crates)
    blufio-anthropic, blufio-auth-keypair, blufio-mcp-client,
    blufio-mcp-server, blufio-prometheus, blufio-security,
    blufio-storage, blufio-vault, blufio-verify, blufio-test-utils
```

---

## Crate Change Classification

### New Crates (9)

| Crate | Purpose | Key Dependencies |
|-------|---------|-----------------|
| `blufio-bus` | Internal pub/sub event bus | blufio-core, tokio |
| `blufio-openai` | OpenAI provider adapter | blufio-core, reqwest |
| `blufio-ollama` | Ollama local LLM adapter | blufio-core, reqwest |
| `blufio-openrouter` | OpenRouter multi-model adapter | blufio-core, reqwest |
| `blufio-gemini` | Google Gemini provider adapter | blufio-core, reqwest |
| `blufio-discord` | Discord channel adapter | blufio-core, serenity or twilight |
| `blufio-slack` | Slack channel adapter | blufio-core, slack-morphism |
| `blufio-matrix` | Matrix channel adapter | blufio-core, matrix-sdk |
| `blufio-irc` | IRC channel adapter | blufio-core, irc crate |
| `blufio-registry` | Skill registry + marketplace | blufio-core, blufio-skill, blufio-storage |
| `blufio-node` | Paired-device node mesh | blufio-core, blufio-auth-keypair |
| `blufio-whatsapp` | WhatsApp channel adapter | blufio-core, reqwest (Meta API) |

Note: WhatsApp may be implemented as a reqwest client against Meta Cloud API rather than a separate crate if the dependency footprint is small enough to fold into `blufio-gateway`.

### Modified Crates (6)

| Crate | Change | Why |
|-------|--------|-----|
| `blufio-core` | Add TTS/Transcription/Image trait definitions; add `BlufioEvent` type | New provider and bus capabilities |
| `blufio-config` | Add `[providers]`, `[channels.*]`, `[node]`, `[registry]`, `[api_keys]` sections | New feature configuration |
| `blufio-gateway` | Add OpenAI route group, scoped API key middleware, webhook delivery, node endpoints | Core API layer expansion |
| `blufio-agent` | Publish events to `EventBus`, multi-provider selection from config | Event bus integration, provider flexibility |
| `blufio-skill` | Integrate `blufio-registry` for install/verify; no runtime changes | Registry-backed skill discovery |
| `blufio` (binary) | Add CLI subcommands: migrate, bench, bundle, privacy-report, config-recipe, uninstall | New CLI utilities |

### Unchanged Crates (15)

`blufio-anthropic`, `blufio-auth-keypair`, `blufio-context`, `blufio-cost`, `blufio-mcp-client`, `blufio-mcp-server`, `blufio-memory`, `blufio-prometheus`, `blufio-router`, `blufio-security`, `blufio-storage`, `blufio-telegram`, `blufio-test-utils`, `blufio-vault`, `blufio-verify`

These crates have clean boundaries and no v1.3 feature crosses into them.

---

## Data Flow Changes

### New Flow: OpenAI-Compatible Chat Completions

```
Client (curl / SDK)
    │ POST /v1/chat/completions
    │ Authorization: Bearer sk-blufio-...
    ▼
blufio-gateway (axum)
    │ ApiKeyMiddleware: validate key, check scope, rate-limit
    ▼
openai/chat.rs handler
    │ translate OpenAI messages[] → ProviderRequest
    │ OR route through AgentLoop (session-aware mode)
    ▼
ProviderAdapter::stream()   (or complete())
    │ provider selected from config / request model field
    ▼
blufio-anthropic / blufio-openai / blufio-ollama / etc.
    │ SSE stream back
    ▼
openai/translate.rs
    │ Blufio ProviderStreamChunk → OpenAI data: {"choices":[{"delta":{...}}]}
    ▼
SSE response to client
```

### New Flow: Event Bus → Webhook Delivery

```
AgentLoop::handle_inbound()
    │ EventBus::publish(BlufioEvent::MessageSent { ... })
    ▼
blufio-bus broadcast channel
    │ subscriber: WebhookDeliveryTask (running in background)
    ▼
WebhookDeliveryTask
    │ filter events matching webhook pattern
    │ POST webhook_url with JSON payload
    │ retry on failure (3x with exponential backoff)
    ▼
Operator's webhook receiver
```

### New Flow: Cross-Channel Bridge

```
ChannelMultiplexer receives InboundMessage from Discord
    │ channel = "discord", sender_id = "user123"
    ▼
EventBus::publish(MessageReceived { channel: "discord", ... })
    ▼
BridgeRule subscriber: "discord" → "telegram"
    │ applies if bridge rule matches
    │ creates synthetic InboundMessage with channel = "telegram"
    ▼
ChannelMultiplexer::send() routes to Telegram channel
```

### New Flow: Skill Registry Install

```
blufio skill install weather
    ▼
blufio-registry
    │ fetch index: ~/.blufio/registry/index.json (+ remote if configured)
    │ find "weather" entry with download_url + signature_url
    ▼
reqwest download skill.wasm to temp file
    ▼
ed25519-dalek: verify signature with publisher public key
    │ abort if verification fails
    ▼
copy to config.skill.skills_dir/weather/
write registry metadata to SQLite (name, version, hash, installed_at)
    ▼
blufio skill list shows "weather" as active
AgentLoop picks up new tool on next restart (or hot-reload)
```

---

## Suggested Build Order

Build order is driven by: (a) crate dependencies, (b) feature dependencies, (c) risk profile (high-risk features last within a group).

### Phase 1: Event Bus + Core Trait Extensions
**Rationale:** Everything else that is new publishes to the event bus or uses new traits. Build the foundation first.
- `blufio-bus`: Zero external deps beyond tokio. Establishes the pub/sub contract.
- `blufio-core` trait additions: TTS/Image/Transcription traits (no implementation, just trait defs). `BlufioEvent` enum.
- **Duration estimate:** 1-2 days

### Phase 2: New Provider Crates
**Rationale:** Providers are self-contained and don't depend on new channel or node features. Establishes multi-provider before the gateway API is built on top.
- `blufio-openai` (OpenAI-compatible format — also used by Ollama/OpenRouter)
- `blufio-ollama` (subset of OpenAI API, local endpoint)
- `blufio-openrouter` (OpenAI API + extra headers)
- `blufio-gemini` (different API format — most complex of the four)
- `blufio-config` + `blufio-agent` modifications for multi-provider selection
- **Duration estimate:** 3-4 days

### Phase 3: OpenAI-Compatible Gateway API
**Rationale:** Depends on Phase 2 (providers must exist to back the API). This is the highest-value external surface.
- `blufio-gateway` modifications: OpenAI routes, scoped API keys, webhook infrastructure
- Scoped API key DB table in `blufio-storage` (schema migration via refinery)
- Rate limiting via `tower_governor`
- Webhook delivery task using `blufio-bus`
- **Duration estimate:** 4-5 days

### Phase 4: Multi-Channel Adapters
**Rationale:** Each channel is independent. Build in order of priority: Discord > Slack > Matrix > IRC > WhatsApp > Signal (if included).
- `blufio-discord` (serenity/twilight)
- `blufio-slack` (slack-morphism)
- `blufio-matrix` (matrix-sdk)
- `blufio-irc` (irc crate — simplest protocol)
- `blufio-whatsapp` (Meta API — depends on business account access)
- Cross-channel bridging config + routing in `blufio-agent`
- **Duration estimate:** 5-7 days

### Phase 5: Skill Registry + Code Signing
**Rationale:** Depends on existing `blufio-skill` infrastructure. Registry is standalone.
- `blufio-registry`: index parsing, download, Ed25519 verify, SQLite metadata
- CLI commands: `blufio skill search/install/uninstall/verify/publish`
- **Duration estimate:** 2-3 days

### Phase 6: Node System
**Rationale:** Depends on existing gateway (HTTP) and auth-keypair (Ed25519). Build after core API is stable.
- `blufio-node`: pairing protocol, peer registry, relay endpoint
- Gateway node routes
- Pairing CLI commands
- **Duration estimate:** 3-4 days

### Phase 7: Docker + Deployment Infrastructure
**Rationale:** Depends on binary being feature-complete. No crate changes — pure infrastructure.
- `Dockerfile` (multi-stage musl build)
- `docker-compose.yml`
- `blufio@.service` systemd template for multi-instance
- **Duration estimate:** 1-2 days

### Phase 8: Migration Tool + CLI Utilities
**Rationale:** Uses existing storage API. No new dependencies. Batch of independent CLI subcommands.
- `blufio migrate openclaw`
- `blufio bench`
- `blufio privacy evidence-report`
- `blufio config recipe`
- `blufio uninstall`
- `blufio bundle`
- **Duration estimate:** 3-4 days

### Build Order Rationale Summary

```
Phase 1 (bus + traits)       ─┐
Phase 2 (providers)           ├─> Phase 3 (gateway API) ─> Phase 4 (channels)
                              ┘                          ─> Phase 5 (registry)
                                                         ─> Phase 6 (node)

Phase 7 (Docker) — after binary is feature-complete
Phase 8 (migration + CLI) — parallel to Phase 4-6, no deps
```

Provider crates before gateway because the gateway API needs working providers to back it. Channels before registry because the ChannelMultiplexer is the integration point for testing channel combinations. Node after gateway because it reuses gateway infrastructure. Docker last because it's packaging, not functionality.

---

## Architectural Patterns to Follow

### Pattern 1: Provider Crate Template

**What:** Every new provider crate follows the same 4-file structure as `blufio-anthropic`.
**When to use:** All four new provider crates.
**Trade-offs:** Consistent, auditable, easy to add future providers. Slight code duplication in client.rs HTTP boilerplate (acceptable given each provider has different auth/error handling).

```rust
// blufio-{provider}/src/lib.rs — template
pub struct {Provider}Provider {
    client: {Provider}Client,
    system_prompt: String,
}

#[async_trait]
impl PluginAdapter for {Provider}Provider { ... }

#[async_trait]
impl ProviderAdapter for {Provider}Provider {
    async fn complete(&self, request: ProviderRequest) -> Result<ProviderResponse, BlufioError> { ... }
    async fn stream(...) -> Result<Pin<Box<dyn Stream<...>>>, BlufioError> { ... }
}
```

### Pattern 2: Channel Crate Template

**What:** Every new channel crate follows the `blufio-telegram` pattern.
**When to use:** All six new channel crates.
**Trade-offs:** Platform SDKs have different async models (polling vs webhook vs WebSocket). Each needs careful adaptation to the `receive()` pull model. The channel must normalize platform messages to `InboundMessage`.

```rust
pub struct {Platform}Channel {
    config: {Platform}Config,
    inbound_tx: mpsc::Sender<InboundMessage>,
    inbound_rx: Mutex<mpsc::Receiver<InboundMessage>>,
    // platform-specific client
}

impl ChannelAdapter for {Platform}Channel {
    async fn connect(&mut self) -> Result<(), BlufioError> {
        // Start background task: platform SDK -> normalize -> inbound_tx.send()
    }
    async fn receive(&self) -> Result<InboundMessage, BlufioError> {
        // Pull from inbound_rx
    }
    async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> {
        // Platform SDK send
    }
}
```

### Pattern 3: Event Bus as Side Channel

**What:** The event bus is never in the critical path. All `EventBus::publish()` calls are fire-and-forget.
**When to use:** Every event emission in the agent loop.
**Trade-offs:** Events may be dropped if subscribers are slow (broadcast lag). This is acceptable — events are for observability and webhooks, not reliability-critical paths. The agent loop must never `await` on event delivery.

```rust
// CORRECT: non-blocking publish
self.event_bus.publish(BlufioEvent::MessageSent { ... });

// WRONG: awaiting event delivery would block the agent loop
self.event_bus.publish_and_wait(event).await; // Never do this
```

### Pattern 4: OpenAI Translation Layer

**What:** A dedicated `translate.rs` module in `blufio-gateway/src/openai/` converts between OpenAI and Blufio types.
**When to use:** All OpenAI-compatible API handlers.
**Trade-offs:** Centralizes all format conversion. Makes it easy to handle OpenAI API version changes. Some semantic information loss is unavoidable (OpenAI `logprobs` doesn't map to anything in Blufio).

```rust
// blufio-gateway/src/openai/translate.rs
pub fn openai_messages_to_provider_request(
    messages: Vec<OpenAiMessage>,
    model: &str,
    max_tokens: Option<u32>,
    stream: bool,
) -> ProviderRequest { ... }

pub fn provider_response_to_openai(
    response: ProviderResponse,
    model: &str,
) -> OpenAiChatCompletionResponse { ... }

pub fn stream_chunk_to_openai_delta(
    chunk: ProviderStreamChunk,
    model: &str,
) -> Option<String> { ... } // Returns SSE line or None (skip)
```

---

## Anti-Patterns to Avoid

### Anti-Pattern 1: Embedding Provider Logic in the Gateway

**What:** Putting OpenAI API translation logic directly in gateway handlers rather than a translation module.
**Why bad:** Gateway handlers grow unmanageable. Provider format differences (Anthropic vs OpenAI vs Gemini) cannot be mixed into a single handler. Testing becomes impossible.
**Instead:** Gateway handlers are thin orchestrators. All format translation goes in `translate.rs`. Actual provider calls go through the `ProviderAdapter` trait.

### Anti-Pattern 2: New Provider Traits Instead of Configuration

**What:** Creating `OpenAIProviderAdapter`, `OllamaProviderAdapter` traits instead of using the existing `ProviderAdapter` trait.
**Why bad:** Breaks the plugin boundary. The agent loop would need to know about specific providers. Defeats the purpose of adapter traits.
**Instead:** All providers implement `ProviderAdapter`. Provider selection is a config decision, not a type-system decision.

### Anti-Pattern 3: Synchronous Event Bus

**What:** Using a synchronous channel (std::sync::mpsc) for the event bus.
**Why bad:** Blocking the agent loop on event delivery under backpressure. The event bus must be non-blocking.
**Instead:** `tokio::sync::broadcast` with fixed capacity. Slow consumers lag and drop events.

### Anti-Pattern 4: Global Mutable Registry State

**What:** Using a global `Lazy<Mutex<SkillRegistry>>` or similar for the skill registry.
**Why bad:** Hidden coupling, hard to test, lock contention.
**Instead:** Registry is an `Arc<SkillRegistry>` passed to components that need it. Same pattern as `Arc<ToolRegistry>` already in the codebase.

### Anti-Pattern 5: Per-Channel Blocking Receive in Multiplexer

**What:** The `ChannelMultiplexer` polling all channels sequentially in a single loop.
**Why bad:** One slow channel (e.g., IRC reconnect delay) blocks messages from all other channels.
**Instead:** The existing `ChannelMultiplexer` already spawns a separate `tokio::spawn` background task per channel — this pattern MUST be preserved for all new channel adapters.

### Anti-Pattern 6: Signal Channel in Core Binary

**What:** Adding the Signal (presage, AGPL) channel directly to the main workspace.
**Why bad:** AGPL license would contaminate the entire workspace. Cannot dual-license as MIT+Apache-2.0.
**Instead:** `blufio-signal` is either (a) a separate workspace with its own `Cargo.toml` and AGPL license, or (b) implemented as a separate bridge binary that communicates with Blufio via the gateway HTTP API.

### Anti-Pattern 7: OpenAI API Passthrough Without Session Context

**What:** The `/v1/chat/completions` handler bypasses the agent loop entirely and calls providers directly.
**Why bad:** Loses session management, tool execution, context engine, cost tracking, and all v1.x features. The gateway becomes a dumb LLM proxy.
**Instead:** Two modes controlled by a query parameter or config: `?session=true` routes through the full agent loop (session-aware, tool-enabled), default mode calls provider directly (stateless, fast).

---

## Scalability Considerations

| Concern | Current (v1.2) | With v1.3 |
|---------|----------------|-----------|
| Channel count | 2 (Telegram + Gateway) | Up to 8 (6 new channels) — multiplexer handles N channels already |
| Provider selection | Compile-time (Anthropic only) | Runtime (config-driven, hot-swappable via restart) |
| Event bus throughput | N/A | Broadcast channel saturates at ~1M events/sec (never the bottleneck) |
| Webhook delivery | N/A | Async background task — bounded queue, non-blocking |
| API key lookups | N/A | DashMap in-memory cache with SQLite as source of truth |
| Skill registry size | Local files only | Index JSON + SQLite metadata — O(1) lookup |
| Node mesh size | N/A | Designed for 2-10 paired devices, not hundreds |
| Memory impact | 50-80MB idle | ~10-20MB additional per active channel (SDK connection pools) |
| Binary size | ~50MB with all official plugins | ~60-70MB with all new crates |

The architecture remains single-process, single-writer SQLite. This is the correct choice for a personal agent platform targeting $4/month VPS deployments. Multi-node sharding remains out of scope per PROJECT.md.

---

## New Dependencies Summary

| Crate | Version | Purpose | Confidence |
|-------|---------|---------|------------|
| `serenity` | 0.12.x | Discord gateway + REST | MEDIUM — actively maintained |
| `slack-morphism` | latest | Slack Web/Events API | MEDIUM — niche but active |
| `matrix-sdk` | 0.7.x | Matrix protocol client | MEDIUM — stable but large |
| `irc` | 0.15.x | IRC client (text protocol) | HIGH — simple, stable |
| `tower_governor` | 0.4.x | Rate limiting middleware | HIGH — production-proven |
| No new provider deps | — | All providers use reqwest (already in workspace) | HIGH |
| No new bus deps | — | Event bus uses tokio broadcast (already in workspace) | HIGH |
| `cargo-chef` | build tool | Layer-cached Docker builds | HIGH — standard Rust Docker pattern |

**Key observation:** Most new features require NO new dependencies. The existing workspace already has:
- `reqwest` for all new provider HTTP clients
- `tokio::sync::broadcast` for the event bus
- `ed25519-dalek` for skill signing and node auth
- `axum` for all new gateway routes
- `tower` for new middleware
- `rusqlite` for API key and registry stores

---

## Integration Points Summary

### External Services

| Service | Integration Pattern | Notes |
|---------|---------------------|-------|
| OpenAI API | reqwest HTTP client, SSE streaming | Same pattern as Anthropic |
| Ollama (local) | reqwest HTTP, no TLS required, localhost:11434 | SSRF bypass needed for localhost |
| OpenRouter | reqwest HTTP + `HTTP-Referer` header | OpenAI-compatible API |
| Google Gemini | reqwest HTTP, different JSON schema, `alt=sse` for streaming | Most complex new provider |
| Discord | serenity WebSocket gateway + REST | Long-lived connection |
| Slack | slack-morphism: Socket Mode WebSocket OR Events API HTTP webhook | HTTP preferred (simpler) |
| WhatsApp | Meta Cloud API HTTP webhook + outbound messages | Requires business account |
| Matrix | matrix-sdk HTTP polling / sync API | Large SDK dependency |
| IRC | TCP text socket | Simplest protocol |

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| AgentLoop ↔ EventBus | `Arc<EventBus>`, publish() fire-and-forget | Never block agent loop |
| Gateway ↔ EventBus | broadcast::subscribe() | Webhook delivery subscriber |
| Gateway ↔ ApiKeyStore | `Arc<ApiKeyStore>` (DashMap + SQLite) | Per-request key validation |
| Registry ↔ SkillStore | Direct fn calls within same request | Install = write to SkillStore |
| NodeSystem ↔ Gateway | Gateway mounts node endpoints | Reuses auth infrastructure |
| NewProviders ↔ AgentLoop | `Arc<dyn ProviderAdapter>` selected at startup | Clean trait boundary |

---

## Sources

- Blufio codebase: direct analysis of 21 crates (HIGH confidence)
- [OpenAI Chat Completions API Reference](https://platform.openai.com/docs/api-reference/chat) — HIGH confidence
- [OpenAI Responses API](https://platform.openai.com/docs/api-reference/responses) — HIGH confidence
- [Ollama OpenAI Compatibility](https://docs.ollama.com/api/openai-compatibility) — HIGH confidence
- [tokio broadcast channel docs](https://docs.rs/tokio/latest/tokio/sync/struct.broadcast.Sender.html) — HIGH confidence
- [serenity Discord library](https://github.com/serenity-rs/serenity) — MEDIUM confidence
- [slack-morphism-rust](https://github.com/abdolence/slack-morphism-rust) — MEDIUM confidence
- [tower_governor rate limiting](https://github.com/benwis/tower-governor) — HIGH confidence
- [clux/muslrust Docker image](https://github.com/clux/muslrust) — HIGH confidence
- [wasmsign2 WASM signing](https://github.com/wasm-signatures/wasmsign2) — MEDIUM confidence
- [openrouter-rs SDK](https://docs.rs/openrouter_api) — MEDIUM confidence
- [gemini-rust crate](https://crates.io/crates/gemini-rust) — MEDIUM confidence (alternatives may be better)

---

*Architecture research for: v1.3 Ecosystem Expansion — Rust AI agent platform*
*Researched: 2026-03-05*
