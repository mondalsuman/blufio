# Phase 3: Agent Loop & Telegram - Research

**Researched:** 2026-03-01
**Domain:** Async agent architecture, Telegram Bot API, Anthropic Messages API streaming
**Confidence:** HIGH

## Summary

Phase 3 delivers the minimum viable agent: an FSM-per-session loop that receives Telegram messages, assembles basic context (system prompt + recent history), calls the Anthropic Messages API with streaming, and delivers responses with edit-in-place partial output. The architecture builds directly on the existing trait system (`ChannelAdapter`, `ProviderAdapter`, `StorageAdapter`) and storage layer from Phases 1-2.

The Rust ecosystem has mature, well-maintained libraries for all components: `teloxide 0.13+` for Telegram (long polling, media handling, MarkdownV2), `reqwest 0.12` (already in workspace) for direct HTTP streaming to Anthropic's SSE endpoint, and `tokio` (already in workspace) for async orchestration with signal handling. The Anthropic API does not have an official Rust SDK, so we build a thin client directly against the Messages API using reqwest + SSE parsing -- this is the standard approach in the Rust ecosystem and gives us full control over streaming behavior.

**Primary recommendation:** Build the Anthropic client directly using reqwest with SSE parsing (no third-party SDK wrapper), implement Telegram via teloxide with long polling, and wire them together through a session-based agent loop using tokio channels for message routing.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Default personality: concise assistant -- brief, direct answers, minimal filler
- Agent identifies itself by its configured name (`agent.name`, default "blufio") in the system prompt
- System prompt loading: `agent.system_prompt` TOML field for short prompts, OR `agent.system_prompt_file` points to a markdown file -- markdown file takes precedence if both exist
- Default system prompt baked in: "You are {agent.name}, a concise personal assistant." (overridable via config)
- Basic context assembly: include last 20 messages from the session by default (full context engine comes in Phase 4)
- Unauthorized users: silently ignored -- no response, no error message
- Typing indicators: send Telegram "typing..." chat action while generating, refreshed every ~5 seconds until response is ready
- Media handling: images sent to Claude vision API, documents extracted as text where possible, voice messages saved with a transcription hook point (actual transcription deferred)
- Scope: DMs only for Phase 3 -- group chat messages are ignored entirely
- Allowed-users enforcement via `telegram.allowed_users` config (existing field)
- Edit-in-place streaming: send an initial message, then edit it as tokens arrive -- throttle edits to ~every 1-2 seconds to avoid Telegram rate limits
- Long responses: split at natural paragraph boundaries when exceeding Telegram's 4096 character limit, sent as sequential messages
- Formatting: Telegram MarkdownV2 parse mode -- code blocks, bold, italic, links render natively (requires escaping special characters)
- Error handling: retry Anthropic API call once on transient errors (429, 500, 503), then send brief user-facing error message
- CLI REPL prompt: simple colored prompt (e.g., "blufio> ") -- agent responses printed below without prefix
- CLI streaming: print tokens to terminal as they arrive, same streaming infrastructure as Telegram
- CLI session persistence: new session each `blufio shell` invocation -- clean slate, previous sessions remain in DB
- CLI readline-style input history within a session

### Claude's Discretion
- Multi-line input handling in the REPL (backslash continuation, key combo, or auto-detect)
- Exact streaming edit throttle interval tuning
- Loading skeleton / placeholder text during initial stream setup
- Default system prompt wording beyond the core "concise assistant" directive
- Reconnection backoff strategy for Telegram long-polling

### Deferred Ideas (OUT OF SCOPE)
- None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| CORE-01 | Agent executes FSM-per-session loop: receive -> assemble context -> call LLM -> execute tools -> respond | Session state machine pattern, tokio::select! for concurrent session handling |
| CORE-02 | Agent handles streaming responses from LLM providers with partial output delivery | Anthropic SSE streaming protocol, reqwest byte streaming, tokio channels for chunk routing |
| CORE-03 | Agent gracefully shuts down on SIGTERM, draining active sessions before exit | tokio::signal for SIGTERM, CancellationToken pattern, shutdown coordinator |
| LLM-01 | Provider trait abstracts LLM interaction behind pluggable interface | Existing ProviderAdapter trait needs stream return type fix (Iterator -> Stream) |
| LLM-02 | Anthropic provider adapter supports Claude models with streaming and tool calling | Direct reqwest client against api.anthropic.com/v1/messages with SSE parsing |
| LLM-08 | System prompt and agent personality are configurable via TOML + optional markdown files | Config model extension for system_prompt/system_prompt_file fields |
| CHAN-01 | Telegram channel adapter receives and sends messages via Telegram Bot API | teloxide with long polling, send_message/edit_message_text for streaming |
| CHAN-02 | Channel adapter trait enables future channel plugins without core changes | Existing ChannelAdapter trait, extend with edit/typing/media capabilities |
| CHAN-03 | Telegram adapter handles message types: text, images, documents, voice | teloxide message type matching, file download for media, vision API for images |
| CHAN-04 | Telegram adapter implements reliable long-polling with automatic reconnection | teloxide Polling builder with configurable timeout and automatic retry |
| CLI-01 | `blufio serve` starts the agent with zero-config defaults | Wiring serve command to agent loop + Telegram + Anthropic + SQLite startup |
| CLI-05 | `blufio shell` provides interactive REPL for testing | rustyline for readline, same streaming infrastructure as Telegram |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| teloxide | 0.13 | Telegram Bot API framework | Most mature Rust Telegram framework, active development, 3.5k+ GitHub stars, full Bot API coverage including long polling, media, chat actions |
| reqwest | 0.12 | HTTP client for Anthropic API | Already in workspace, async with tokio, streaming response body, rustls-tls |
| tokio | 1.x | Async runtime | Already in workspace, provides channels, signals, select!, task spawning |
| serde_json | 1.x | JSON serialization for API payloads | Standard Rust JSON library, needed for Anthropic request/response types |
| rustyline | 14 | CLI REPL with readline | Standard Rust readline library, history, completion, customizable prompt |
| futures | 0.3 | Stream utilities | StreamExt for async stream processing of SSE events |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| eventsource-stream | 0.2 | SSE parser for reqwest byte stream | Parsing Anthropic's SSE streaming responses into typed events |
| tracing | 0.1 | Structured logging | Already in workspace, use for all debug/info/error logging |
| tokio-util | 0.7 | CancellationToken | Graceful shutdown coordination across tasks |
| base64 | 0.22 | Base64 encoding for image data | Encoding downloaded Telegram images for Claude vision API |
| colored | 2 | Terminal colors for REPL prompt | Colored "blufio> " prompt in shell mode |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| teloxide | grammers / telegram-bot-api | teloxide has best documentation and highest adoption in Rust ecosystem |
| Direct reqwest SSE | anthropic-sdk-rust / misanthropy | Third-party SDKs add dependency risk, less control over streaming; direct is simpler and more maintainable |
| rustyline | reedline | rustyline is more mature and lightweight; reedline (nushell) is heavier |
| eventsource-stream | Manual SSE parsing | eventsource-stream handles edge cases (multi-line data, retry fields); not worth hand-rolling |

**Installation (Cargo.toml additions):**
```toml
teloxide = { version = "0.13", features = ["macros"] }
serde_json = "1"
rustyline = "14"
futures = "0.3"
eventsource-stream = "0.2"
tokio-util = "0.7"
base64 = "0.22"
colored = "2"
```

## Architecture Patterns

### Recommended Project Structure
```
crates/
├── blufio-core/src/
│   ├── types.rs           # Extend placeholder types with real fields
│   └── traits/
│       ├── channel.rs     # Extend ChannelAdapter with edit/typing capabilities
│       └── provider.rs    # Fix stream() return type: Iterator -> Pin<Box<dyn Stream>>
├── blufio-anthropic/src/  # NEW CRATE
│   ├── lib.rs             # AnthropicProvider implementing ProviderAdapter
│   ├── client.rs          # HTTP client, request building, auth headers
│   ├── types.rs           # Anthropic API types (MessageRequest, StreamEvent, etc.)
│   └── sse.rs             # SSE stream parser and event deserialization
├── blufio-telegram/src/   # NEW CRATE
│   ├── lib.rs             # TelegramChannel implementing ChannelAdapter
│   ├── handler.rs         # Message routing, auth filtering, media handling
│   ├── streaming.rs       # Edit-in-place message streaming with throttle
│   └── markdown.rs        # MarkdownV2 escaping and formatting
├── blufio-agent/src/      # NEW CRATE
│   ├── lib.rs             # AgentLoop: the FSM coordinator
│   ├── session.rs         # Per-session state machine
│   ├── context.rs         # Basic context assembly (system prompt + last N messages)
│   └── shutdown.rs        # Graceful shutdown coordinator
└── blufio/src/
    └── main.rs            # Wire serve + shell commands
```

### Pattern 1: Session FSM (Finite State Machine)
**What:** Each conversation session runs as an independent state machine: Idle -> Receiving -> Processing -> Responding -> Idle
**When to use:** Always -- every message from any channel goes through this loop
**Example:**
```rust
enum SessionState {
    Idle,
    Receiving,  // Incoming message being processed
    Processing, // Calling LLM provider
    Responding, // Streaming response back to channel
    Draining,   // Shutdown requested, finishing current response
}

struct SessionActor {
    session_id: String,
    state: SessionState,
    storage: Arc<dyn StorageAdapter>,
    provider: Arc<dyn ProviderAdapter>,
    // Channel-agnostic response sender
    response_tx: mpsc::Sender<ResponseChunk>,
}
```

### Pattern 2: Channel-Agnostic Message Bus
**What:** Inbound messages from any channel (Telegram, CLI) are normalized into `InboundMessage` and routed to the agent loop. Responses flow back as `OutboundMessage` chunks.
**When to use:** Core routing pattern -- decouples channels from processing
**Example:**
```rust
// Inbound: channel -> agent
let (inbound_tx, inbound_rx) = mpsc::channel::<InboundMessage>(100);

// Outbound: agent -> channel (per-session)
let (outbound_tx, outbound_rx) = mpsc::channel::<ResponseChunk>(100);
```

### Pattern 3: Edit-in-Place Streaming
**What:** For Telegram, send an initial placeholder message, then edit it as tokens accumulate. Throttle edits to avoid rate limits.
**When to use:** Telegram response delivery
**Example:**
```rust
struct StreamingEditor {
    bot: Bot,
    chat_id: ChatId,
    message_id: Option<MessageId>,
    buffer: String,
    last_edit: Instant,
    throttle: Duration, // ~1.5 seconds
}

impl StreamingEditor {
    async fn push_chunk(&mut self, text: &str) -> Result<(), BlufioError> {
        self.buffer.push_str(text);
        if self.last_edit.elapsed() >= self.throttle {
            if let Some(msg_id) = self.message_id {
                self.bot.edit_message_text(self.chat_id, msg_id, &self.buffer)
                    .parse_mode(ParseMode::MarkdownV2)
                    .await?;
            } else {
                let sent = self.bot.send_message(self.chat_id, &self.buffer)
                    .parse_mode(ParseMode::MarkdownV2)
                    .await?;
                self.message_id = Some(sent.id);
            }
            self.last_edit = Instant::now();
        }
        Ok(())
    }
}
```

### Pattern 4: Graceful Shutdown
**What:** Listen for SIGTERM/SIGINT, set a cancellation token, drain active sessions, then exit
**When to use:** `blufio serve` process lifecycle
**Example:**
```rust
let cancel = CancellationToken::new();
let cancel_clone = cancel.clone();

tokio::spawn(async move {
    tokio::signal::ctrl_c().await.ok();
    cancel_clone.cancel();
});

// In agent loop
tokio::select! {
    msg = inbound_rx.recv() => { /* process message */ }
    _ = cancel.cancelled() => {
        // Stop accepting new messages
        // Wait for active sessions to finish
        // Shutdown storage
        break;
    }
}
```

### Anti-Patterns to Avoid
- **Spawning unbounded tasks per message:** Use bounded channels and backpressure instead
- **Blocking the Telegram polling loop:** Always spawn message processing into separate tasks
- **Storing API keys in config plaintext:** Use the vault from Phase 2 for bot_token and api_key
- **Parsing SSE manually with string splitting:** Use eventsource-stream which handles edge cases
- **Editing Telegram messages on every token:** Throttle to avoid 429 rate limits (Telegram allows ~30 messages/second per chat)

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| SSE stream parsing | Custom line parser | eventsource-stream | Multi-line data fields, retry/id fields, reconnection logic |
| Telegram Bot API | Raw HTTP calls | teloxide | Message type variants, file uploads, rate limit handling, long poll retry |
| Readline REPL | Custom stdin reader | rustyline | History, line editing, Ctrl+C handling, UTF-8, multi-line |
| MarkdownV2 escaping | Manual character escaping | Dedicated escape function | 18 special characters to escape, context-dependent (inside code blocks vs outside) |
| Graceful shutdown | Manual signal handling + flags | tokio-util CancellationToken | Race-condition-free shutdown coordination across tasks |

**Key insight:** The Telegram Bot API has many edge cases (message length limits, rate limits, media file handling, MarkdownV2 escaping rules) that teloxide handles. Building a raw HTTP client would take weeks of debugging.

## Common Pitfalls

### Pitfall 1: Telegram MarkdownV2 Escaping
**What goes wrong:** LLM output contains characters that break MarkdownV2 parsing (underscores, brackets, dots, etc.)
**Why it happens:** MarkdownV2 requires escaping 18 special characters: `_ * [ ] ( ) ~ ` > # + - = | { } . !`
**How to avoid:** Apply escaping AFTER receiving the full text chunk, but BEFORE sending to Telegram. Pre-code-block content needs different escaping than inside code blocks.
**Warning signs:** Telegram returns "Bad Request: can't parse entities" errors

### Pitfall 2: Telegram Edit Rate Limits
**What goes wrong:** Editing a message too frequently causes 429 Too Many Requests
**Why it happens:** Telegram rate limits are ~30 API calls/second per bot, but edit_message is more restricted per chat
**How to avoid:** Throttle edit_message_text calls to every 1-2 seconds. Buffer accumulated text and batch updates.
**Warning signs:** Intermittent 429 errors during streaming

### Pitfall 3: Anthropic SSE Stream Interruption
**What goes wrong:** Network interruption mid-stream leaves a partial response
**Why it happens:** SSE connections can drop due to timeouts, network issues, or server overload (529)
**How to avoid:** Implement retry with the accumulated partial response. On error, save what we have and notify the user. For Claude 4.5 and earlier, can resume from partial; for Claude 4.6, need to prompt continuation.
**Warning signs:** Incomplete messages in chat, missing `message_stop` event

### Pitfall 4: Session Leak on Ungraceful Shutdown
**What goes wrong:** Active sessions remain in "active" state in the database after crash
**Why it happens:** Process killed without draining sessions
**How to avoid:** On startup, scan for stale "active" sessions and mark them as "interrupted". Use the crash-safe queue for in-flight messages.
**Warning signs:** Session counts growing without bound in the database

### Pitfall 5: Blocking the Tokio Runtime
**What goes wrong:** File I/O or heavy computation blocks the async runtime
**Why it happens:** Reading system prompt files, image processing, or base64 encoding on the main runtime
**How to avoid:** Use `tokio::task::spawn_blocking` for file reads and CPU-intensive work
**Warning signs:** Latency spikes, unresponsive bot

### Pitfall 6: Telegram Message Length Overflow
**What goes wrong:** Sending a message over 4096 characters causes API error
**Why it happens:** LLM responses can be arbitrarily long
**How to avoid:** Split at paragraph boundaries. Track accumulated length during streaming. When approaching 4096, finalize current message and start a new one.
**Warning signs:** "Bad Request: message is too long" errors

## Code Examples

### Anthropic Messages API Streaming Request
```rust
// Direct HTTP request to Anthropic Messages API
let response = client
    .post("https://api.anthropic.com/v1/messages")
    .header("x-api-key", &api_key)
    .header("anthropic-version", "2023-06-01")
    .header("content-type", "application/json")
    .json(&serde_json::json!({
        "model": model,
        "messages": messages,
        "system": system_prompt,
        "max_tokens": 4096,
        "stream": true
    }))
    .send()
    .await?;

// Parse SSE stream
use eventsource_stream::Eventsource;
use futures::StreamExt;

let mut stream = response.bytes_stream().eventsource();
while let Some(event) = stream.next().await {
    match event {
        Ok(ev) => {
            match ev.event.as_str() {
                "content_block_delta" => {
                    let data: serde_json::Value = serde_json::from_str(&ev.data)?;
                    if let Some(text) = data["delta"]["text"].as_str() {
                        // Forward text chunk to channel
                        tx.send(ResponseChunk::Text(text.to_string())).await?;
                    }
                }
                "message_stop" => break,
                "message_delta" => {
                    // Extract usage info for cost tracking
                    let data: serde_json::Value = serde_json::from_str(&ev.data)?;
                    if let Some(usage) = data.get("usage") {
                        tx.send(ResponseChunk::Usage(parse_usage(usage))).await?;
                    }
                }
                "ping" => {} // Ignore keepalive
                "error" => {
                    let data: serde_json::Value = serde_json::from_str(&ev.data)?;
                    return Err(parse_api_error(&data));
                }
                _ => {} // Ignore unknown events per versioning policy
            }
        }
        Err(e) => return Err(BlufioError::Provider {
            message: format!("SSE stream error: {e}"),
            source: Some(Box::new(e)),
        }),
    }
}
```

### Anthropic Messages API Types
```rust
// SSE event types from Anthropic streaming
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum StreamEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: MessageInfo },
    #[serde(rename = "content_block_start")]
    ContentBlockStart { index: usize, content_block: ContentBlock },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { index: usize, delta: Delta },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: usize },
    #[serde(rename = "message_delta")]
    MessageDelta { delta: MessageDeltaInfo, usage: Option<Usage> },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "error")]
    Error { error: ApiError },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum Delta {
    #[serde(rename = "text_delta")]
    Text { text: String },
    #[serde(rename = "input_json_delta")]
    InputJson { partial_json: String },
}
```

### Telegram Long Polling with teloxide
```rust
use teloxide::prelude::*;

let bot = Bot::new(bot_token);
let handler = dptree::entry()
    .branch(Update::filter_message().endpoint(handle_message));

Dispatcher::builder(bot, handler)
    .default_handler(|_| async {}) // Silently ignore non-message updates
    .build()
    .dispatch()
    .await;

async fn handle_message(bot: Bot, msg: Message) -> ResponseResult<()> {
    // 1. Check allowed users
    // 2. Ignore group messages (DMs only)
    // 3. Extract content (text/photo/document/voice)
    // 4. Route to agent loop
    // 5. Stream response back
    Ok(())
}
```

### Graceful Shutdown with SIGTERM
```rust
use tokio_util::sync::CancellationToken;
use tokio::signal::unix::{signal, SignalKind};

let cancel = CancellationToken::new();

// Listen for both SIGTERM and SIGINT
let cancel_sig = cancel.clone();
tokio::spawn(async move {
    let mut sigterm = signal(SignalKind::terminate()).unwrap();
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = sigterm.recv() => {}
    }
    tracing::info!("shutdown signal received, draining active sessions...");
    cancel_sig.cancel();
});
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| telegram-bot crate | teloxide 0.13 | 2024 | teloxide is the de facto standard, telegram-bot unmaintained |
| hyper for HTTP | reqwest 0.12 with rustls | 2024 | reqwest is higher-level, handles connection pooling, TLS automatically |
| Manual SSE parsing | eventsource-stream 0.2 | 2023 | Proper SSE spec compliance, handles edge cases |
| Anthropic SDK wrappers | Direct reqwest + SSE | Ongoing | No official Rust SDK; direct approach avoids dependency on unmaintained third-party crates |
| readline crate | rustyline 14 | 2024 | readline is deprecated, rustyline is actively maintained |

**Deprecated/outdated:**
- `telegram-bot` crate: unmaintained since 2021, use teloxide instead
- `readline` crate: deprecated, use rustyline
- Anthropic API version `2023-01-01`: superseded by `2023-06-01` which adds streaming and tool use

## Open Questions

1. **teloxide version compatibility with tokio 1.x**
   - What we know: teloxide 0.13 targets tokio 1.x, compatible with our workspace
   - What's unclear: Exact minimum tokio version requirement
   - Recommendation: Pin teloxide 0.13 and verify compilation

2. **Anthropic vision API for Telegram images**
   - What we know: Claude supports image input via base64-encoded content blocks
   - What's unclear: Maximum image size, supported formats via Telegram download
   - Recommendation: Download image via teloxide, resize if > 5MB, base64 encode, send as image content block

3. **Voice message transcription hook**
   - What we know: Phase 3 requires a "transcription hook point" but not actual transcription
   - What's unclear: How the hook interface should look for future phases
   - Recommendation: Define a `TranscriptionHook` trait with a no-op default implementation; voice messages stored as files with metadata for future processing

## Sources

### Primary (HIGH confidence)
- Anthropic Messages API Streaming: https://platform.claude.com/docs/en/api/messages-streaming -- SSE event types, streaming protocol, request/response format
- teloxide docs: https://docs.rs/teloxide/0.13/teloxide -- Bot API, Polling, Dispatcher, message handling
- tokio signal handling: https://docs.rs/tokio/latest/tokio/signal -- ctrl_c, Unix signals
- reqwest: https://docs.rs/reqwest/0.12 -- HTTP client, streaming body

### Secondary (MEDIUM confidence)
- eventsource-stream: https://crates.io/crates/eventsource-stream -- SSE parser for reqwest
- rustyline: https://crates.io/crates/rustyline -- readline implementation
- Telegram Bot API limits: https://core.telegram.org/bots/api -- 4096 char limit, rate limits

### Tertiary (LOW confidence)
- Telegram edit rate limits: Community reports suggest ~20-30 edits/minute/chat is safe (no official docs on exact limits)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - all libraries well-established, some already in workspace
- Architecture: HIGH - patterns are standard for Rust async services
- Pitfalls: HIGH - documented from Telegram API docs and Anthropic streaming docs

**Research date:** 2026-03-01
**Valid until:** 2026-04-01 (30 days -- stable ecosystem)
