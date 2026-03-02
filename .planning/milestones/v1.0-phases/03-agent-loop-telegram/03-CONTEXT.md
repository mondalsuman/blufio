# Phase 3: Agent Loop & Telegram - Context

**Gathered:** 2026-03-01
**Status:** Ready for planning

<domain>
## Phase Boundary

A working always-on Telegram bot backed by Claude. The minimum viable agent that receives messages via Telegram, assembles basic context (system prompt + recent history), calls Anthropic, and responds with streaming output. Conversations persist across restarts. Graceful shutdown on SIGTERM. A CLI REPL (`blufio shell`) provides local testing. No tool execution, no memory, no model routing, no cost tracking — those are later phases.

</domain>

<decisions>
## Implementation Decisions

### Agent personality & system prompt
- Default personality: concise assistant — brief, direct answers, minimal filler
- Agent identifies itself by its configured name (`agent.name`, default "blufio") in the system prompt
- System prompt loading: `agent.system_prompt` TOML field for short prompts, OR `agent.system_prompt_file` points to a markdown file — markdown file takes precedence if both exist
- Default system prompt baked in: "You are {agent.name}, a concise personal assistant." (overridable via config)
- Basic context assembly: include last 20 messages from the session by default (full context engine comes in Phase 4)

### Telegram interaction style
- Unauthorized users: silently ignored — no response, no error message, reduces attack surface
- Typing indicators: send Telegram "typing..." chat action while generating, refreshed every ~5 seconds until response is ready
- Media handling: images sent to Claude vision API, documents extracted as text where possible, voice messages saved with a transcription hook point (actual transcription deferred)
- Scope: DMs only for Phase 3 — group chat messages are ignored entirely
- Allowed-users enforcement via `telegram.allowed_users` config (existing field)

### Streaming & response delivery
- Edit-in-place streaming: send an initial message, then edit it as tokens arrive — throttle edits to ~every 1-2 seconds to avoid Telegram rate limits
- Long responses: split at natural paragraph boundaries when exceeding Telegram's 4096 character limit, sent as sequential messages
- Formatting: Telegram MarkdownV2 parse mode — code blocks, bold, italic, links render natively (requires escaping special characters)
- Error handling: retry Anthropic API call once on transient errors (429, 500, 503), then send brief user-facing error message ("Something went wrong. Try again in a moment.")

### CLI REPL experience
- Prompt: simple colored prompt (e.g., "blufio> ") — agent responses printed below without prefix
- Streaming: print tokens to terminal as they arrive, same streaming infrastructure as Telegram
- Session persistence: new session each `blufio shell` invocation — clean slate, previous sessions remain in DB
- History: readline-style input history within a session

### Claude's Discretion
- Multi-line input handling in the REPL (backslash continuation, key combo, or auto-detect)
- Exact streaming edit throttle interval tuning
- Loading skeleton / placeholder text during initial stream setup
- Default system prompt wording beyond the core "concise assistant" directive
- Reconnection backoff strategy for Telegram long-polling

</decisions>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches for the Telegram bot implementation and CLI REPL.

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ChannelAdapter` trait (`crates/blufio-core/src/traits/channel.rs`): connect/send/receive methods ready to implement for Telegram
- `ProviderAdapter` trait (`crates/blufio-core/src/traits/provider.rs`): complete/stream methods ready to implement for Anthropic
- `SqliteStorage` (`crates/blufio-storage/src/adapter.rs`): fully implemented with session CRUD, message insert/query, and queue operations
- Placeholder types: `InboundMessage`, `OutboundMessage`, `ProviderRequest`, `ProviderResponse`, `ProviderStreamChunk` — need to be fleshed out with real fields
- CLI stubs: `Serve` and `Shell` commands already defined in `crates/blufio/src/main.rs` via clap

### Established Patterns
- `async_trait` for all adapter traits — async-first design
- `PluginAdapter` as base trait: name(), version(), adapter_type(), health_check(), shutdown()
- `BlufioError` enum with `Channel`, `Provider`, `Timeout` variants for error handling
- `tokio::sync::OnceCell` for lazy initialization (see SqliteStorage)
- `serde(deny_unknown_fields)` on all config structs
- `tracing` crate for structured logging

### Integration Points
- Config: `TelegramConfig` (bot_token, allowed_users) and `AnthropicConfig` (api_key, default_model) already exist
- Storage: session/message persistence ready — create_session, insert_message, get_messages all working
- Queue: crash-safe message queue (enqueue/dequeue/ack/fail) available for inbound/outbound message routing
- Vault: credential storage for API keys and bot tokens already implemented
- Binary entry: `main.rs` loads config, parses CLI — `Serve` and `Shell` handlers need implementation

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 03-agent-loop-telegram*
*Context gathered: 2026-03-01*
