# Phase 29: Event Bus & Core Trait Extensions - Context

**Gathered:** 2026-03-05
**Status:** Ready for planning

<domain>
## Phase Boundary

Establish the internal pub/sub backbone (blufio-bus crate) that unblocks webhooks, bridging, nodes, and batch processing. Extend blufio-core with provider-agnostic ToolDefinition type, media provider traits (TTS, Transcription, Image), and custom provider TOML configuration. No implementations of media providers or custom providers — only trait definitions and config parsing.

</domain>

<decisions>
## Implementation Decisions

### Claude's Discretion

The user delegated all implementation decisions to Claude. The following areas should be resolved during research and planning based on what best fits the existing codebase patterns and downstream consumer needs:

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

</decisions>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches. User trusts Claude to make all architectural decisions based on codebase conventions and downstream needs (Phase 30-39).

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `PluginAdapter` trait (blufio-core/src/traits/adapter.rs): Base trait for all adapters — name(), version(), health_check(). Media traits should extend this for consistency.
- `ProviderAdapter` trait (blufio-core/src/traits/provider.rs): Existing LLM provider interface with complete() and stream() methods. Pattern to follow for media traits.
- `ChannelAdapter` trait (blufio-core/src/traits/channel.rs): Well-established pattern with multiple implementations (Telegram, Gateway, MockChannel). Shows how adapters are wired.
- `ToolDefinition` in blufio-anthropic/src/types.rs: Current Anthropic-specific version with {name, description, input_schema}. Must be replaced/superseded by provider-agnostic version in blufio-core.
- `ProviderRequest.tools` field (blufio-core/src/types.rs:179): Currently `Option<Vec<serde_json::Value>>` — needs updating to use the new provider-agnostic ToolDefinition type.

### Established Patterns
- Trait hierarchy: All adapters extend `PluginAdapter` (adapter.rs → channel.rs, provider.rs, storage.rs, etc.)
- Async traits: `#[async_trait]` used consistently across all adapter traits
- Types live in `blufio-core/src/types.rs` with re-exports from `blufio-core/src/lib.rs`
- Crate-per-adapter pattern: blufio-anthropic, blufio-telegram, blufio-gateway each implement one adapter
- Channel multiplexer (blufio-agent/src/channel_mux.rs): Shows how multiple adapters are aggregated — EventBus may need similar multiplexing pattern

### Integration Points
- `blufio-agent/src/lib.rs`: Main agent loop — will need Arc<EventBus> injected to publish events
- `blufio/src/serve.rs`: Server startup — where EventBus initialization and subscriber wiring happens
- `blufio-config`: TOML config parsing — where custom provider config declarations will be parsed
- `blufio-core/src/traits/mod.rs`: Where new media trait modules need to be added and re-exported
- `Cargo.toml` workspace: New blufio-bus crate needs to be added to workspace members

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 29-event-bus-core-trait-extensions*
*Context gathered: 2026-03-05*
