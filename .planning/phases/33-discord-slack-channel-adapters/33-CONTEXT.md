# Phase 33: Discord & Slack Channel Adapters - Context

**Gathered:** 2026-03-06
**Status:** Ready for planning

<domain>
## Phase Boundary

Two highest-value channel integrations: Discord bot adapter and Slack app adapter. Both implement the existing ChannelAdapter trait with capabilities manifest. Format degradation pipeline works across channel capabilities. Creating additional adapters (WhatsApp, Signal, IRC, Matrix) and cross-channel bridging belong to Phase 34.

</domain>

<decisions>
## Implementation Decisions

### Discord bot scope
- Responds in DMs and when @mentioned in server channels (not all channels)
- Embeds for structured content (status, help, errors); plain markdown text for chat responses
- MESSAGE_CONTENT privileged intent: warn at startup if missing, degrade gracefully (DMs and @mentions still work without it)
- Uses serenity crate (per ROADMAP.md requirement)

### Slack connection mode
- Socket Mode as primary connection method (no public URL needed — matches Blufio's self-hosted nature)
- Responds in DMs and when @Blufio in channels (mirrors Discord behavior)
- Rich Block Kit formatting for structured content (help, status, errors); mrkdwn text for chat responses
- Single /blufio slash command with subcommands (consistent with Discord branding)

### Format degradation pipeline
- Centralized FormatPipeline in blufio-core — takes rich content + target ChannelCapabilities, produces channel-specific output
- Best-effort markdown conversion per channel: standard markdown → Discord markdown, Slack mrkdwn, Telegram MarkdownV2
- When channel doesn't support a content type: convert to readable text representation (embed → formatted text block, image → [image: caption])
- Extend ChannelCapabilities with supports_embeds, supports_reactions, supports_threads so the pipeline knows what's available

### Streaming responses
- Extract shared StreamingEditor trait to blufio-core — common buffering, throttle, paragraph-split logic; each adapter implements channel-specific send/edit
- Unified typing indicator pattern across adapters: background task sends typing every ~5s until response ready
- Long message splitting at paragraph boundaries: ~1800 chars for Discord (2000 limit), higher threshold for Slack
- Refactor existing Telegram StreamingEditor to use the shared trait

### Claude's Discretion
- Slash command design (single /blufio vs multiple top-level — leaning single based on discussion)
- Slack Rust crate selection (slack-morphism recommended in roadmap, but evaluate alternatives)
- Whether each platform gets edit-in-place streaming or full-response-at-once (evaluate rate limits)
- Platform-specific throttle intervals for message editing
- Discord embed styling and color scheme

</decisions>

<specifics>
## Specific Ideas

- Both adapters should feel consistent: same slash command structure (/blufio), same behavior model (DMs + mentions), same structured content approach (embeds/blocks for status, plain text for chat)
- Follow Telegram adapter's architecture as reference: handler module, markdown module, media module, streaming module per adapter crate
- Discord bot should handle the MESSAGE_CONTENT intent correctly — this is a common pain point for Discord bot developers

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ChannelAdapter` trait (`blufio-core/src/traits/channel.rs`): Interface both adapters must implement — connect, send, receive, edit_message, send_typing
- `PluginAdapter` trait: Base trait providing name, version, adapter_type, health_check, shutdown
- `ChannelMultiplexer` (`blufio-agent/src/channel_mux.rs`): Aggregates adapters, routes messages by channel name — new adapters plug in via `add_channel()`
- `StreamingEditor` (`blufio-telegram/src/streaming.rs`): Edit-in-place streaming with throttle and paragraph splitting — to be generalized
- Core types: `InboundMessage`, `OutboundMessage`, `ChannelCapabilities`, `MessageId`, `MessageContent`
- `TelegramConfig` pattern in blufio-config: token + allowed_users — template for Discord/Slack configs

### Established Patterns
- Channel adapters use mpsc channels for inbound message queuing (100-buffer in Telegram, 512 in multiplexer)
- `connect()` spawns background tokio tasks for receiving messages
- Markdown formatting with MarkdownV2 → plain text fallback on parse failure
- Authorization filtering via allowed_users list in config
- Health check via API call (Telegram uses getMe)
- Modular crate structure: `blufio-{channel}/src/{lib, handler, markdown, media, streaming}.rs`

### Integration Points
- `blufio/src/serve.rs`: Where channels are wired up at startup — add Discord/Slack initialization
- `blufio-config`: Where config structs live — add DiscordConfig and SlackConfig
- `ChannelMultiplexer.add_channel()`: Registration point for new adapters
- `blufio-core/src/types.rs`: Where ChannelCapabilities lives — extend with new fields

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 33-discord-slack-channel-adapters*
*Context gathered: 2026-03-06*
