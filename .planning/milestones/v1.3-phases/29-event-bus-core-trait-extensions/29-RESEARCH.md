# Phase 29: Event Bus & Core Trait Extensions - Research

**Researched:** 2026-03-05
**Domain:** Internal pub/sub event bus, provider-agnostic types, media provider traits
**Confidence:** HIGH

## Summary

Phase 29 establishes two foundational subsystems: (1) a typed internal event bus (`blufio-bus` crate) using tokio channels for pub/sub communication between components, and (2) core trait extensions in `blufio-core` including a provider-agnostic `ToolDefinition`, media provider traits (TTS, Transcription, Image), and custom provider TOML configuration.

The event bus uses `tokio::sync::broadcast` for fire-and-forget subscribers and `tokio::sync::mpsc` for critical/reliable subscribers. This dual-channel pattern is already established in the codebase -- the `ChannelMultiplexer` uses `mpsc` for inbound message aggregation. The bus should be a standalone `blufio-bus` crate to keep blufio-core dependency-light.

Core trait extensions follow the established pattern: all adapters extend `PluginAdapter` via `#[async_trait]`, types live in `blufio-core/src/types.rs`, and traits live in `blufio-core/src/traits/`. The provider-agnostic `ToolDefinition` replaces the current `Option<Vec<serde_json::Value>>` in `ProviderRequest.tools` with a strongly-typed representation.

**Primary recommendation:** Build blufio-bus as a thin crate wrapping tokio broadcast + mpsc channels with a typed event enum, then extend blufio-core with ToolDefinition, three media traits, and custom provider config parsing.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
None -- user delegated all decisions to Claude.

### Claude's Discretion
**Event Bus (blufio-bus):**
- Event type structure (flat enum vs category hierarchy)
- Event payload design (full data vs lightweight IDs)
- Serialization strategy (serde from day one vs in-memory only)
- Crate organization (standalone blufio-bus vs module in blufio-core)
- Which subscribers are "critical" (mpsc, guaranteed) vs "fire-and-forget" (broadcast, logged lag)
- Lag handling strategy (warn threshold, skip policy)
- Subscriber registration model (startup-only vs dynamic)
- Event filtering approach (topic-based vs all-to-all)

**Core Trait Extensions:**
- Provider-agnostic ToolDefinition field set (minimal 3-field vs extended with metadata)
- Media trait streaming support (request/response only vs streaming from day one)
- Media trait base (extend PluginAdapter vs standalone)
- ToolResult type scope (definitions only vs include tool result type)

**Custom Provider Config:**
- TOML structure ([providers.custom.<name>] vs [[custom_providers]])
- Wire protocol support (openai-compat only vs multiple protocols)
- API key resolution (env var only vs env + vault)
- Validation timing (startup fail-fast vs lazy on first use)

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| INFRA-01 | Internal event bus using tokio broadcast with lag handling | EventBus struct with broadcast channels, lag detection via `RecvError::Lagged`, logged warnings |
| INFRA-02 | Event bus publishes typed events (session, channel, skill, node, webhook, batch) | `BusEvent` enum with six domain-specific variants |
| INFRA-03 | Event bus uses mpsc for reliable subscribers (webhook delivery) | `subscribe_reliable()` returns mpsc::Receiver, EventBus internally fans out to mpsc senders |
| PROV-10 | Provider-agnostic ToolDefinition type in blufio-core | `ToolDefinition` struct with name, description, input_schema in blufio-core/src/types.rs |
| PROV-11 | TTS provider trait (AudioProvider) defined with reference interface | `TtsAdapter` trait extending PluginAdapter in blufio-core/src/traits/tts.rs |
| PROV-12 | Transcription provider trait defined with reference interface | `TranscriptionAdapter` trait extending PluginAdapter in blufio-core/src/traits/transcription.rs |
| PROV-13 | Image generation provider trait (ImageProvider) defined with reference interface | `ImageAdapter` trait extending PluginAdapter in blufio-core/src/traits/image.rs |
| PROV-14 | Custom provider via TOML config (base_url + wire_protocol + api_key_env) | `CustomProviderConfig` struct parsed from `[providers.custom.<name>]` TOML sections |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tokio::sync::broadcast | tokio 1.x | Fire-and-forget pub/sub channel | Already in workspace; native async, zero-copy Arc cloning |
| tokio::sync::mpsc | tokio 1.x | Reliable subscriber channels | Already used in ChannelMultiplexer; guaranteed delivery |
| serde + serde_json | 1.x | Event serialization | Already workspace dependency; needed for event payloads |
| async-trait | 0.1 | Async trait definitions | Already used for all adapter traits in blufio-core |
| tracing | 0.1 | Lag warnings and event logging | Already workspace dependency |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| chrono | 0.4 | Event timestamps | Already workspace dep; ISO 8601 timestamps on events |
| uuid | 1 | Event IDs | Already workspace dep; unique event correlation |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| tokio broadcast/mpsc | async-channel | Third-party dep; tokio channels are already proven in codebase |
| Custom event bus | eventador/event-listener | Project explicitly rejects external message brokers; in-process is correct |

## Architecture Patterns

### Recommended Project Structure

```
crates/
├── blufio-bus/              # NEW: Event bus crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs           # EventBus struct, public API
│       └── events.rs        # BusEvent enum and payload types
├── blufio-core/
│   └── src/
│       ├── types.rs         # ADD: ToolDefinition, media types
│       ├── traits/
│       │   ├── mod.rs       # ADD: tts, transcription, image modules
│       │   ├── tts.rs       # NEW: TtsAdapter trait
│       │   ├── transcription.rs  # NEW: TranscriptionAdapter trait
│       │   └── image.rs     # NEW: ImageAdapter trait
│       └── lib.rs           # ADD: re-exports for new types/traits
├── blufio-config/
│   └── src/
│       └── model.rs         # ADD: CustomProviderConfig, ProvidersConfig
```

### Pattern 1: Dual-Channel Event Bus
**What:** EventBus holds one broadcast::Sender for fire-and-forget and maintains a Vec of mpsc::Sender for reliable subscribers. On publish, sends to broadcast AND each mpsc sender.
**When to use:** When some subscribers can tolerate dropped events (metrics, logging) and others cannot (webhook delivery, audit).
**Example:**
```rust
pub struct EventBus {
    broadcast_tx: broadcast::Sender<BusEvent>,
    reliable_txs: RwLock<Vec<mpsc::Sender<BusEvent>>>,
}

impl EventBus {
    pub fn publish(&self, event: BusEvent) {
        // Fire-and-forget: broadcast (may lag)
        let _ = self.broadcast_tx.send(event.clone());
        // Reliable: each mpsc sender (blocks if full)
        let txs = self.reliable_txs.read();
        for tx in txs.iter() {
            if tx.try_send(event.clone()).is_err() {
                tracing::error!("reliable subscriber dropped event");
            }
        }
    }
}
```

### Pattern 2: Flat Event Enum with Domain Payloads
**What:** Single `BusEvent` enum with one variant per domain, each carrying a domain-specific payload struct.
**When to use:** When events need to cross subsystem boundaries and subscribers filter by variant.
**Example:**
```rust
#[derive(Debug, Clone)]
pub enum BusEvent {
    Session(SessionEvent),
    Channel(ChannelEvent),
    Skill(SkillEvent),
    Node(NodeEvent),
    Webhook(WebhookEvent),
    Batch(BatchEvent),
}
```

### Pattern 3: Trait Extension via PluginAdapter
**What:** New media traits extend PluginAdapter, following the exact same pattern as ProviderAdapter and ChannelAdapter.
**When to use:** For all new adapter trait definitions.
**Example:**
```rust
#[async_trait]
pub trait TtsAdapter: PluginAdapter {
    async fn synthesize(&self, request: TtsRequest) -> Result<TtsResponse, BlufioError>;
}
```

### Anti-Patterns to Avoid
- **External message broker:** Requirements explicitly exclude Redis/NATS -- contradicts single-binary model
- **Embedding bus in blufio-core:** Would add tokio dependency to core; keep blufio-core dependency-light
- **Generic/type-erased events:** Using `Box<dyn Any>` loses type safety; use a concrete enum
- **Blocking mpsc sends:** Use `try_send` with error logging, not `.await` which could block the publisher

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Async broadcast channel | Custom ring buffer | tokio::sync::broadcast | Battle-tested, handles lag detection automatically |
| Async mpsc channel | Custom queue | tokio::sync::mpsc | Already proven in ChannelMultiplexer |
| Event serialization | Custom binary format | serde_json | Already in use throughout codebase |
| UUID generation | Custom ID scheme | uuid::Uuid::new_v4() | Already used for session/message IDs |

**Key insight:** The event bus is intentionally thin -- a coordination layer wrapping tokio primitives, not a framework.

## Common Pitfalls

### Pitfall 1: Broadcast Lag Silently Dropping Events
**What goes wrong:** `broadcast::Receiver` returns `RecvError::Lagged(n)` when the receiver falls behind. If unhandled, the receiver skips `n` messages silently.
**Why it happens:** Slow subscribers (e.g., writing to disk) fall behind the broadcast buffer.
**How to avoid:** Log a warning with the lag count. Set broadcast capacity to a reasonable value (1024). For subscribers that cannot tolerate drops, use the reliable mpsc path.
**Warning signs:** Missing events in webhook delivery or audit logs.

### Pitfall 2: ToolDefinition Breaking Existing Anthropic Integration
**What goes wrong:** Changing `ProviderRequest.tools` from `Option<Vec<serde_json::Value>>` to `Option<Vec<ToolDefinition>>` breaks the Anthropic adapter's serialization.
**Why it happens:** The Anthropic adapter currently serializes tools as raw JSON values. A typed ToolDefinition needs a conversion path.
**How to avoid:** Add a `to_json_value()` method on ToolDefinition for backward compatibility. Update the Anthropic adapter to convert ToolDefinition -> its own wire format.
**Warning signs:** Tool calling stops working after the type change.

### Pitfall 3: Config deny_unknown_fields Rejecting New Sections
**What goes wrong:** Adding `[providers]` to BlufioConfig fails because `deny_unknown_fields` rejects the new section on existing configs.
**Why it happens:** The project uses strict TOML validation.
**How to avoid:** Add the new section with `#[serde(default)]` so it's optional. Existing configs without the section continue to work.
**Warning signs:** Startup crashes for existing users after upgrading.

### Pitfall 4: Circular Dependencies Between blufio-bus and blufio-core
**What goes wrong:** blufio-bus wants to use types from blufio-core (SessionId, etc.) and blufio-core wants to know about events.
**Why it happens:** Event payloads reference core domain types.
**How to avoid:** blufio-bus depends on blufio-core (one-way). EventBus is injected into components that publish, not the other way around. blufio-core never depends on blufio-bus.
**Warning signs:** Cargo complains about circular workspace dependencies.

## Code Examples

### EventBus Creation and Usage
```rust
use blufio_bus::{EventBus, BusEvent, SessionEvent};

// At startup (serve.rs)
let event_bus = Arc::new(EventBus::new(1024)); // broadcast capacity

// Subscribe (fire-and-forget)
let mut rx = event_bus.subscribe();
tokio::spawn(async move {
    loop {
        match rx.recv().await {
            Ok(event) => { /* handle */ },
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(skipped = n, "broadcast subscriber lagged");
            },
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
});

// Subscribe (reliable)
let mut reliable_rx = event_bus.subscribe_reliable(256);
tokio::spawn(async move {
    while let Some(event) = reliable_rx.recv().await {
        // Guaranteed delivery
    }
});

// Publish
event_bus.publish(BusEvent::Session(SessionEvent::Created {
    session_id: "sess-123".into(),
    channel: "telegram".into(),
}));
```

### Provider-Agnostic ToolDefinition
```rust
/// Provider-agnostic tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

impl ToolDefinition {
    /// Convert to raw JSON value for backward compatibility.
    pub fn to_json_value(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "description": self.description,
            "input_schema": self.input_schema,
        })
    }
}
```

### Custom Provider TOML Config
```toml
[providers.custom.my-provider]
base_url = "https://api.example.com/v1"
wire_protocol = "openai-compat"
api_key_env = "MY_PROVIDER_API_KEY"
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Raw JSON tools in ProviderRequest | Typed ToolDefinition | This phase | Type safety, multi-provider support |
| No internal event system | EventBus via tokio channels | This phase | Enables webhooks, bridging, nodes, batch |
| Anthropic-only ToolDefinition | Provider-agnostic in blufio-core | This phase | Foundation for OpenAI, Gemini, Ollama providers |

## Open Questions

1. **Event payload size strategy**
   - What we know: Full payloads (clone all data) are simpler but may be expensive for high-frequency events; lightweight IDs require storage lookups
   - What's unclear: Expected event volume in production
   - Recommendation: Start with full payloads (clone). The types are small (strings, enums). Optimize to Arc-wrapped payloads only if profiling shows a problem.

2. **ProviderRequest.tools migration path**
   - What we know: Currently `Option<Vec<serde_json::Value>>`, needs to become `Option<Vec<ToolDefinition>>`
   - What's unclear: Whether to change in-place or add alongside
   - Recommendation: Change in-place. The Anthropic adapter converts ToolDefinition to its wire format. ToolRegistry already returns tool definitions -- just change the return type.

## Sources

### Primary (HIGH confidence)
- Codebase analysis: blufio-core/src/traits/*.rs, blufio-core/src/types.rs, blufio-agent/src/channel_mux.rs
- Codebase analysis: blufio-config/src/model.rs (TOML config patterns)
- Codebase analysis: blufio-anthropic/src/types.rs (current ToolDefinition)
- tokio documentation: broadcast and mpsc channel semantics

### Secondary (MEDIUM confidence)
- Rust ecosystem patterns for internal event buses in single-binary applications

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - using existing workspace dependencies (tokio, serde, async-trait)
- Architecture: HIGH - follows established crate and trait patterns in the codebase
- Pitfalls: HIGH - identified from direct codebase analysis of deny_unknown_fields, broadcast lag semantics

**Research date:** 2026-03-05
**Valid until:** 2026-04-05 (stable patterns, no external dependency changes)
