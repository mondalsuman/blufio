# Phase 34: WhatsApp, Signal, IRC & Matrix Adapters - Context

**Gathered:** 2026-03-06
**Status:** Ready for planning

<domain>
## Phase Boundary

Users can interact with Blufio through WhatsApp, Signal, IRC, and Matrix, and messages can bridge across any combination of channels. Each adapter implements the existing `ChannelAdapter` trait and integrates via feature flags and `ChannelMultiplexer`. Cross-channel bridging is a new subsystem driven by the event bus.

</domain>

<decisions>
## Implementation Decisions

### Signal adapter (signal-cli sidecar)
- External process only — Blufio connects to a running signal-cli JSON-RPC instance, does NOT manage its lifecycle
- Retry with exponential backoff on connection loss (1s, 2s, 4s... up to 60s). Report `Degraded` health status until reconnected
- Transport: auto-detect between Unix domain socket and TCP. If `socket_path` is set in config, use Unix socket; otherwise fall back to TCP `host:port`
- Support both 1:1 direct messages and group messages with @mention detection

### Cross-channel bridging
- Named bridge groups in TOML: `[bridge.group-name]` with `channels = ["telegram", "discord", "slack"]`. All channels in a group see each other's messages (implicit bidirectional)
- Bridge content: text messages with sender attribution (e.g., "[Telegram/Alice] Hello!"). Media forwarded as links when target channel doesn't support the format natively
- Pass-through only — bridged messages do NOT trigger Blufio's AI, do NOT create sessions, do NOT cost tokens. Blufio only responds when directly addressed via @mention or DM
- Basic filtering per bridge group: `exclude_bots = true` (default) and optional `include_users = [...]` to restrict which users' messages get bridged

### WhatsApp adapter
- Primary: WhatsApp Cloud API (Meta Business API) using blufio-gateway's existing HTTP server with a `/webhooks/whatsapp` endpoint for incoming webhooks
- Experimental: WhatsApp Web adapter behind `whatsapp-web` feature flag. Full send/receive implementation, documented as unstable (unofficial API, may break)
- Reactive only — no template messages, no proactive outreach. Only respond to incoming messages
- Shared `[whatsapp]` config section with `variant = "cloud"` or `variant = "web"`. Common fields (phone_number_id, allowed_users) at top level; variant-specific fields nested (cloud: verify_token, access_token; web: session_data_path)

### IRC adapter
- Multi-channel per instance: single TCP connection, config lists channels to join (`channels = ["#blufio", "#support"]`). Respond on @mention or DM (PRIVMSG to bot nick)
- Built-in flood protection: message queue with configurable rate limit (default: 1 message per 2 seconds)
- Long messages split at word boundaries into multiple PRIVMSG lines, respecting rate limit
- Authentication: SASL PLAIN for modern servers, NickServ IDENTIFY as fallback. Config chooses auth method (`auth_method = "sasl"` or `auth_method = "nickserv"`)
- TLS required by default (`tls = true`), with option to disable for local/testing

### Matrix adapter
- Pinned to matrix-sdk 0.11 per success criteria
- Room join and messaging via ChannelAdapter trait

### Claude's Discretion
- Matrix adapter detailed behavior (room discovery, invite handling, encryption support level)
- IRC reconnection strategy details (ping timeout, reconnect delay)
- Signal JSON-RPC message format parsing implementation
- WhatsApp Web library choice (whatsapp-web.rs or alternative)
- Bridge message formatting details (exact attribution format, emoji usage)
- Error handling and logging patterns (follow existing adapter conventions)

</decisions>

<specifics>
## Specific Ideas

- Signal adapter should follow the "external sidecar" pattern common in Docker Compose deployments — signal-cli runs as its own container/process
- Bridge groups should feel like IRC channel linking — simple config, predictable behavior
- WhatsApp Cloud API webhook verification should reuse gateway patterns already established
- IRC flood protection is essential — bots that flood get banned

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ChannelAdapter` trait (`blufio-core/src/traits/channel.rs`): Well-defined with connect, send, receive, edit_message, send_typing — all new adapters implement this
- `ChannelMultiplexer` (`blufio-agent/src/channel_mux.rs`): Aggregates channels, routes by name, spawns per-channel receive tasks — new adapters plug in automatically
- `PluginAdapter` trait: Base trait providing name, version, adapter_type, health_check, shutdown — consistent across all adapters
- `BusEvent::Channel` (`blufio-bus/src/events.rs`): MessageReceived/MessageSent events — bridge subsystem can subscribe to these
- Slack adapter (`blufio-slack/`): Most complete reference implementation with Socket Mode, mpsc channels, handler separation, streaming, markdown conversion
- Feature flag pattern in `serve.rs`: `#[cfg(feature = "slack")]` for conditional compilation — new adapters follow same pattern

### Established Patterns
- Each adapter is its own crate (`blufio-{name}/`) with feature flag gating
- Config struct per adapter in `blufio-config/src/model.rs` with `#[serde(deny_unknown_fields)]`
- mpsc channel (tx/rx) pattern for inbound message forwarding
- `InboundMessage` / `OutboundMessage` types with metadata JSON for channel-specific routing
- `ChannelCapabilities` struct declares what each adapter supports

### Integration Points
- `serve.rs`: Wire new adapters with feature flags, add to ChannelMultiplexer
- `BlufioConfig`: Add config sections for WhatsApp, Signal, IRC, Matrix
- `blufio-gateway`: Add `/webhooks/whatsapp` endpoint for Cloud API
- Event bus: New bridge subscriber listens to ChannelEvent::MessageReceived and forwards to target channels

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 34-whatsapp-signal-irc-matrix-adapters*
*Context gathered: 2026-03-06*
