# Phase 33: Discord & Slack Channel Adapters - Research

**Researched:** 2026-03-06
**Domain:** Chat platform integrations (Discord Gateway API, Slack Events/Socket Mode API)
**Confidence:** HIGH

## Summary

This phase adds two channel adapters -- Discord and Slack -- to the existing Blufio channel multiplexer architecture. Both adapters implement the `ChannelAdapter` trait already defined in `blufio-core`. The Telegram adapter (`blufio-telegram`) serves as a well-established reference implementation with proven patterns for message handling, streaming, markdown conversion, and authorization filtering.

The Discord adapter uses the `serenity` crate (0.12.5), which provides built-in rate limiting, Gateway WebSocket management, and slash command support. The Slack adapter uses `slack-morphism` (2.18.0), which provides Socket Mode WebSocket connections (no public URL needed), Block Kit message building, and slash command routing. Both crates are mature Rust-native libraries with active maintenance.

A centralized `FormatPipeline` must be extracted to `blufio-core` to convert rich content to channel-specific formats. The existing `StreamingEditor` in `blufio-telegram` must be generalized into a shared trait, with each adapter implementing platform-specific send/edit operations.

**Primary recommendation:** Follow the Telegram adapter's modular crate structure exactly (lib.rs, handler.rs, markdown.rs, streaming.rs) for both new adapters. Use serenity directly (not poise) for Discord to maintain control over the event handler loop and integrate cleanly with the existing `ChannelAdapter` trait pattern. Use slack-morphism with the `hyper` feature for Slack Socket Mode.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Discord bot responds in DMs and when @mentioned in server channels (not all channels)
- Discord uses embeds for structured content (status, help, errors); plain markdown text for chat responses
- MESSAGE_CONTENT privileged intent: warn at startup if missing, degrade gracefully (DMs and @mentions still work without it)
- Uses serenity crate (per ROADMAP.md requirement)
- Socket Mode as primary connection method for Slack (no public URL needed -- matches Blufio's self-hosted nature)
- Slack responds in DMs and when @Blufio in channels (mirrors Discord behavior)
- Rich Block Kit formatting for structured content (help, status, errors); mrkdwn text for chat responses
- Single /blufio slash command with subcommands (consistent with Discord branding)
- Centralized FormatPipeline in blufio-core -- takes rich content + target ChannelCapabilities, produces channel-specific output
- Best-effort markdown conversion per channel: standard markdown -> Discord markdown, Slack mrkdwn, Telegram MarkdownV2
- When channel doesn't support a content type: convert to readable text representation (embed -> formatted text block, image -> [image: caption])
- Extend ChannelCapabilities with supports_embeds, supports_reactions, supports_threads
- Extract shared StreamingEditor trait to blufio-core -- common buffering, throttle, paragraph-split logic
- Unified typing indicator pattern across adapters: background task sends typing every ~5s until response ready
- Long message splitting at paragraph boundaries: ~1800 chars for Discord (2000 limit), higher threshold for Slack
- Refactor existing Telegram StreamingEditor to use the shared trait
- Both adapters should feel consistent: same slash command structure (/blufio), same behavior model (DMs + mentions), same structured content approach

### Claude's Discretion
- Slash command design (single /blufio vs multiple top-level -- leaning single based on discussion)
- Slack Rust crate selection (slack-morphism recommended in roadmap, but evaluate alternatives)
- Whether each platform gets edit-in-place streaming or full-response-at-once (evaluate rate limits)
- Platform-specific throttle intervals for message editing
- Discord embed styling and color scheme

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| CHAN-01 | Discord adapter with Gateway WebSocket and REST via serenity | serenity 0.12.5 provides Gateway, REST, built-in rate limiting; follows TelegramChannel pattern |
| CHAN-02 | Discord slash commands and ephemeral responses | serenity `interaction_create` event + `CreateInteractionResponse::Message` with `.ephemeral(true)` |
| CHAN-03 | Discord MESSAGE_CONTENT privileged intent correctly handled | `GatewayIntents::MESSAGE_CONTENT` flag; detect absence at startup via `ready` event |
| CHAN-04 | Slack adapter with Events API and Socket Mode via slack-morphism | slack-morphism 2.18.0 with `hyper` feature; `SlackSocketModeClientListener` trait for events |
| CHAN-05 | Slack slash commands and Block Kit messages | slack-morphism Block Kit types (`SlackSectionBlockElement`, `SlackMessageContent`); slash command routing via Socket Mode events |
| CHAN-11 | All new adapters implement ChannelAdapter trait with capabilities manifest | Existing `ChannelAdapter` trait in `blufio-core/src/traits/channel.rs`; extend `ChannelCapabilities` with new fields |
| CHAN-12 | Format degradation pipeline works across all new channel capabilities | New `FormatPipeline` in blufio-core; markdown conversion modules per adapter |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| serenity | 0.12.5 | Discord Gateway WebSocket, REST API, slash commands, embeds | Project ROADMAP requirement; most mature Rust Discord library; 4.3M+ downloads; built-in rate limiting |
| slack-morphism | 2.18.0 | Slack Web API, Socket Mode, Block Kit, slash commands | Project ROADMAP recommendation; only actively maintained Rust Slack library with Socket Mode; typed Block Kit |
| tokio | 1.x | Async runtime, mpsc channels, spawn, timers | Already used throughout project |
| async-trait | 0.1 | Async trait support for ChannelAdapter | Already used throughout project |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tokio-util | 0.7 | CancellationToken for typing indicator tasks | Already used in blufio-telegram streaming |
| serde_json | 1 | Metadata JSON serialization | Already used throughout project |
| chrono | 0.4 | Timestamp formatting | Already used in blufio-telegram handler |
| tracing | 0.1 | Structured logging | Already used throughout project |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| serenity (direct) | poise (command framework on top of serenity) | Poise adds macro-driven slash commands and auto-parsing, but abstracts away the event handler loop making it harder to integrate with our ChannelAdapter trait pattern. Use serenity directly. |
| slack-morphism | slack-api (crate) | slack-api is unmaintained (last update 2020). slack-morphism is the only viable option. |

**Installation (workspace Cargo.toml additions):**
```toml
serenity = { version = "0.12", default-features = false, features = ["client", "gateway", "model", "rustls_backend", "cache"] }
slack-morphism = { version = "2.18", features = ["hyper"] }
```

## Architecture Patterns

### Recommended Project Structure
```
crates/
├── blufio-core/src/
│   ├── types.rs              # Extend ChannelCapabilities; add FormatPipeline types
│   ├── traits/channel.rs     # Existing ChannelAdapter trait (unchanged)
│   ├── format.rs             # NEW: FormatPipeline + markdown conversion
│   └── streaming.rs          # NEW: SharedStreamingEditor trait
├── blufio-discord/src/
│   ├── lib.rs                # DiscordChannel struct implementing ChannelAdapter
│   ├── handler.rs            # Message routing, @mention detection, authorization
│   ├── markdown.rs           # Standard markdown -> Discord markdown conversion
│   ├── commands.rs           # Slash command registration and handling
│   └── streaming.rs          # Discord-specific StreamingEditor implementation
├── blufio-slack/src/
│   ├── lib.rs                # SlackChannel struct implementing ChannelAdapter
│   ├── handler.rs            # Message routing, @mention detection, authorization
│   ├── markdown.rs           # Standard markdown -> Slack mrkdwn conversion
│   ├── commands.rs           # Slash command routing and response formatting
│   ├── blocks.rs             # Block Kit message builders for structured content
│   └── streaming.rs          # Slack-specific StreamingEditor implementation
├── blufio-telegram/src/
│   └── streaming.rs          # REFACTORED: Uses shared StreamingEditor trait
├── blufio-config/src/
│   └── model.rs              # Add DiscordConfig, SlackConfig structs
└── blufio/src/
    └── serve.rs              # Wire Discord/Slack channels into multiplexer
```

### Pattern 1: ChannelAdapter Implementation (follow Telegram exactly)
**What:** Each channel adapter is a standalone crate implementing `ChannelAdapter` trait with mpsc-based inbound message queuing, background receive task, and authorization filtering.
**When to use:** Every new channel adapter.
**Example:**
```rust
// Source: Existing pattern from crates/blufio-telegram/src/lib.rs
pub struct DiscordChannel {
    client: serenity::Client,        // Or Arc<serenity::Http> + gateway handle
    config: DiscordConfig,
    inbound_rx: tokio::sync::Mutex<mpsc::Receiver<InboundMessage>>,
    inbound_tx: mpsc::Sender<InboundMessage>,
    http: Arc<serenity::Http>,       // For send/edit operations
    gateway_handle: Option<tokio::task::JoinHandle<()>>,
}

#[async_trait]
impl ChannelAdapter for DiscordChannel {
    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_edit: true,
            supports_typing: true,
            supports_images: true,
            supports_documents: true,
            supports_voice: false,
            max_message_length: Some(2000),
            // New fields:
            supports_embeds: true,
            supports_reactions: true,
            supports_threads: true,
        }
    }

    async fn connect(&mut self) -> Result<(), BlufioError> {
        // Spawn serenity client in background task
        // EventHandler forwards messages to inbound_tx
    }

    async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> {
        // Use self.http to send message to channel
    }

    async fn receive(&self) -> Result<InboundMessage, BlufioError> {
        // Receive from self.inbound_rx (same as Telegram)
    }
}
```

### Pattern 2: Shared StreamingEditor Trait
**What:** Common trait defining buffering, throttle, and paragraph-split logic. Each adapter implements platform-specific send/edit.
**When to use:** Extracting from Telegram and sharing with Discord/Slack.
**Example:**
```rust
// In blufio-core/src/streaming.rs
#[async_trait]
pub trait StreamingEditorOps: Send {
    /// Send initial message, return platform-specific message ID
    async fn send_initial(&mut self, text: &str) -> Result<String, BlufioError>;

    /// Edit existing message with updated text
    async fn edit_message(&mut self, msg_id: &str, text: &str) -> Result<(), BlufioError>;

    /// Platform-specific max message length
    fn max_message_length(&self) -> usize;

    /// Platform-specific throttle interval
    fn throttle_interval(&self) -> Duration;
}

/// Shared streaming logic: buffering, throttle, paragraph splitting
pub struct StreamingBuffer {
    buffer: String,
    last_edit: Instant,
    messages_sent: Vec<String>,
    split_threshold: usize,
}

impl StreamingBuffer {
    pub async fn push_chunk<E: StreamingEditorOps>(
        &mut self, editor: &mut E, text: &str
    ) -> Result<(), BlufioError> {
        // Shared logic: append, check split threshold, check throttle
    }

    pub async fn finalize<E: StreamingEditorOps>(
        &mut self, editor: &mut E
    ) -> Result<(), BlufioError> {
        // Shared logic: send remaining buffer
    }
}
```

### Pattern 3: FormatPipeline for Cross-Channel Content
**What:** Centralized content formatter that takes rich content + target capabilities, produces channel-specific output.
**When to use:** Every outbound message that may contain structured content.
**Example:**
```rust
// In blufio-core/src/format.rs
pub enum RichContent {
    Text(String),
    Embed { title: String, description: String, fields: Vec<(String, String, bool)>, color: Option<u32> },
    Image { url: String, caption: Option<String> },
    CodeBlock { language: Option<String>, code: String },
}

pub struct FormatPipeline;

impl FormatPipeline {
    pub fn format(content: &RichContent, caps: &ChannelCapabilities) -> FormattedOutput {
        match content {
            RichContent::Embed { .. } if caps.supports_embeds => {
                // Pass through as-is for embed-capable channels
            }
            RichContent::Embed { title, description, fields, .. } => {
                // Degrade: convert embed to formatted text block
                // **Title**\nDescription\n\n**Field1:** Value1\n...
            }
            // ...
        }
    }
}
```

### Pattern 4: Discord @mention Detection
**What:** In server channels, only respond when the bot is @mentioned. In DMs, always respond.
**When to use:** Discord message handler.
**Example:**
```rust
// In blufio-discord/src/handler.rs
fn should_respond(msg: &serenity::model::channel::Message, bot_id: UserId) -> bool {
    // Always respond in DMs
    if msg.is_private() {
        return true;
    }
    // In servers, only when @mentioned
    msg.mentions.iter().any(|u| u.id == bot_id)
}
```

### Pattern 5: Slack Socket Mode Event Handling
**What:** Implement `SlackSocketModeClientListener` to receive events and route to adapter.
**When to use:** Slack adapter connection.
**Example:**
```rust
// In blufio-slack/src/handler.rs
struct SlackEventHandler {
    inbound_tx: mpsc::Sender<InboundMessage>,
    bot_user_id: String,
    allowed_users: Vec<String>,
}

impl SlackSocketModeClientListener for SlackEventHandler {
    fn on_message(&self, _client_id: &SlackSocketModeWssClientId, message_body: String)
        -> Pin<Box<dyn Future<Output = Option<String>> + Send>> {
        // Parse event JSON, filter by type (message, slash_command)
        // Check authorization, convert to InboundMessage, send to inbound_tx
        // Return acknowledgment JSON for slash commands
    }
}
```

### Anti-Patterns to Avoid
- **Using poise framework:** Poise takes over the event loop and command registration, conflicting with our ChannelAdapter trait pattern. Use serenity directly.
- **Hardcoding rate limits:** Both serenity (Discord) and slack-morphism (Slack) handle rate limiting internally. Do not implement custom rate limiting on top.
- **Global slash command registration in ready():** Registering commands on every bot restart creates API calls. Register once and cache, or use guild-scoped registration for development.
- **Requesting MESSAGE_CONTENT without needing it:** Discord reviewers deny the intent if slash commands can achieve the same goal. Design the bot to work without MESSAGE_CONTENT (slash commands + DMs + @mentions) and treat the intent as an optional enhancement.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Discord Gateway WebSocket | Custom WebSocket client | serenity's built-in gateway | Handles reconnection, heartbeat, session resume, sharding, compression |
| Discord rate limiting | Manual rate limit tracking | serenity's built-in HTTP client | Parses X-RateLimit headers, queues requests, handles 429 retries per-bucket |
| Slack WebSocket connection | Custom Socket Mode client | slack-morphism's Socket Mode manager | Handles reconnection, multi-connection, envelope acknowledgment |
| Slack Block Kit serialization | Manual JSON construction | slack-morphism's typed Block Kit models | Ensures valid block structure, handles all element types with compile-time safety |
| Paragraph-boundary text splitting | New implementation | Existing `split_at_paragraph_boundary()` in blufio-telegram | Already tested and handles double-newline > single-newline > space > hard-split priority |

**Key insight:** Both serenity and slack-morphism are full-featured clients that handle connection lifecycle, rate limiting, and reconnection internally. The adapter's job is to translate between the platform's event model and Blufio's `InboundMessage`/`OutboundMessage` types.

## Common Pitfalls

### Pitfall 1: MESSAGE_CONTENT Intent Missing Silently
**What goes wrong:** Bot connects to Discord but receives empty message content in guild messages, causing confusing behavior without any error.
**Why it happens:** MESSAGE_CONTENT is a privileged intent that must be enabled in Discord Developer Portal AND requested in `GatewayIntents`. Without it, `msg.content` is empty string in guild messages.
**How to avoid:** Check for the intent in the `ready` event handler. If `ready.guilds` contains guilds but the intent was not granted, log a prominent warning. DMs and @mentions still receive content without the intent.
**Warning signs:** Empty message content in guild channels while DMs work fine.

### Pitfall 2: Discord Message Length Limit (2000 chars)
**What goes wrong:** Bot sends a message exceeding 2000 characters and gets an API error.
**Why it happens:** Discord has a hard 2000-character limit for regular messages. Embeds have a separate 6000-character total limit.
**How to avoid:** Split at paragraph boundaries using ~1800 char threshold (leave margin for formatting overhead). Use the existing `split_at_paragraph_boundary()` function.
**Warning signs:** HTTP 400 errors on send_message calls with long content.

### Pitfall 3: Slack mrkdwn vs Standard Markdown
**What goes wrong:** Markdown formatting renders as raw text or produces unexpected output in Slack.
**Why it happens:** Slack's mrkdwn syntax differs significantly: `*bold*` (not `**bold**`), `_italic_` (not `*italic*`), links are `<url|text>` (not `[text](url)`), no header support (`#` has no effect).
**How to avoid:** Build a dedicated `markdown_to_mrkdwn()` converter that handles all syntax differences. Test with all common patterns.
**Warning signs:** Raw asterisks/brackets appearing in Slack messages.

### Pitfall 4: Slack Socket Mode Envelope Acknowledgment
**What goes wrong:** Slack disconnects the bot or events appear to be "lost."
**Why it happens:** Socket Mode requires acknowledging every envelope within 3 seconds by sending back the `envelope_id`. Unacknowledged envelopes cause Slack to retry and eventually disconnect.
**How to avoid:** Return the acknowledgment immediately in `on_message`, before processing the event. Process the event asynchronously after acknowledging.
**Warning signs:** Duplicate events, WebSocket disconnections, "Your app didn't respond" errors in Slack logs.

### Pitfall 5: Discord Embed vs Message Content Confusion
**What goes wrong:** Bot sends embeds for everything, making casual chat feel heavy and unnatural.
**Why it happens:** Developer treats all responses as "rich content" needing embeds.
**How to avoid:** Per the user decision: plain text for chat responses, embeds only for structured content (status, help, errors). The FormatPipeline should distinguish between response types.
**Warning signs:** Users complaining about overly formatted responses.

### Pitfall 6: Serenity Client Ownership
**What goes wrong:** Cannot extract `Http` handle from serenity `Client` for use in `send()` and `edit_message()`.
**Why it happens:** `Client::start()` consumes self, so you lose access to the client after starting the gateway.
**How to avoid:** Extract the `Http` handle (`client.http.clone()`) before calling `start()`, or use `client.cache_and_http.http.clone()`. Store the `Arc<Http>` in the adapter struct for send/edit operations.
**Warning signs:** Borrow checker errors when trying to use the client after starting gateway.

### Pitfall 7: Slack chat.update Rate Limits for Streaming
**What goes wrong:** Streaming edits hit Slack's Tier 3 rate limit (50+/min), causing 429 errors.
**Why it happens:** Edit-in-place streaming with aggressive throttle intervals exceeds the rate limit.
**How to avoid:** Use a conservative throttle interval of ~3000ms for Slack (compared to ~1500ms for Telegram). Consider full-response-at-once for Slack if rate limits are too restrictive.
**Warning signs:** HTTP 429 responses from chat.update.

## Code Examples

Verified patterns from official sources:

### Discord: Serenity EventHandler with Mention Detection
```rust
// Source: serenity 0.12.5 docs + Context7
use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::model::id::UserId;
use serenity::prelude::*;

struct Handler {
    inbound_tx: mpsc::Sender<InboundMessage>,
    allowed_users: Vec<String>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        // Skip bot messages
        if msg.author.bot { return; }

        let bot_id = ctx.cache.current_user().id;

        // DMs: always respond; Channels: only when @mentioned
        let should_respond = msg.is_private()
            || msg.mentions.iter().any(|u| u.id == bot_id);

        if !should_respond { return; }

        // Authorization check
        let sender_id = msg.author.id.to_string();
        if !self.allowed_users.is_empty()
            && !self.allowed_users.contains(&sender_id)
        {
            return;
        }

        // Strip @mention from content for clean processing
        let content = if !msg.is_private() {
            msg.content
                .replace(&format!("<@{}>", bot_id), "")
                .trim()
                .to_string()
        } else {
            msg.content.clone()
        };

        // Convert to InboundMessage and send
        let inbound = InboundMessage {
            id: msg.id.to_string(),
            session_id: None,
            channel: "discord".to_string(),
            sender_id,
            content: MessageContent::Text(content),
            timestamp: msg.timestamp.to_rfc3339(),
            metadata: Some(serde_json::json!({
                "channel_id": msg.channel_id.to_string(),
                "guild_id": msg.guild_id.map(|g| g.to_string()),
            }).to_string()),
        };

        let _ = self.inbound_tx.send(inbound).await;
    }

    async fn ready(&self, _ctx: Context, ready: Ready) {
        info!(bot_name = %ready.user.name, "Discord bot connected");

        // CHAN-03: Warn about MESSAGE_CONTENT intent
        // Note: serenity doesn't expose whether the intent was granted;
        // detect it by checking if guild message content is empty
    }
}
```

### Discord: Slash Command Registration and Handling
```rust
// Source: serenity 0.12.5 Context7 examples
use serenity::builder::{
    CreateCommand, CreateCommandOption, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};
use serenity::model::application::{CommandOptionType, Interaction};

// In EventHandler implementation:
async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
    if let Interaction::Command(command) = interaction {
        let (content, ephemeral) = match command.data.name.as_str() {
            "blufio" => {
                let subcommand = command.data.options.first()
                    .map(|o| o.name.as_str())
                    .unwrap_or("help");

                match subcommand {
                    "status" => (format_status_embed(), true),
                    "help" => (format_help_embed(), true),
                    _ => ("Unknown subcommand".into(), true),
                }
            }
            _ => ("Unknown command".into(), true),
        };

        let data = CreateInteractionResponseMessage::new()
            .content(content)
            .ephemeral(ephemeral);

        let builder = CreateInteractionResponse::Message(data);
        let _ = command.create_response(&ctx.http, builder).await;
    }
}

// Register in ready():
async fn ready(&self, ctx: Context, ready: Ready) {
    let commands = vec![
        CreateCommand::new("blufio")
            .description("Blufio AI assistant")
            .add_option(
                CreateCommandOption::new(CommandOptionType::SubCommand, "status", "Show bot status")
            )
            .add_option(
                CreateCommandOption::new(CommandOptionType::SubCommand, "help", "Show help")
            )
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::SubCommand, "chat", "Chat with Blufio"
                ).add_sub_option(
                    CreateCommandOption::new(CommandOptionType::String, "message", "Your message")
                        .required(true)
                )
            ),
    ];

    // Register globally
    if let Err(e) = serenity::model::application::Command::set_global_commands(&ctx.http, commands).await {
        error!(error = %e, "failed to register slash commands");
    }
}
```

### Discord: Embed Construction
```rust
// Source: serenity 0.12.5 Context7 examples
use serenity::builder::{CreateEmbed, CreateEmbedFooter, CreateMessage};
use serenity::model::Timestamp;

fn build_status_embed(status: &str, model: &str) -> CreateEmbed {
    CreateEmbed::new()
        .title("Blufio Status")
        .description(status)
        .color(0x5865F2) // Discord blurple
        .field("Model", model, true)
        .field("Uptime", "2h 15m", true)
        .footer(CreateEmbedFooter::new("Blufio AI Assistant"))
        .timestamp(Timestamp::now())
}
```

### Slack: Socket Mode Setup with slack-morphism
```rust
// Source: slack-morphism 2.18.0 docs.rs + Context7
use slack_morphism::prelude::*;

// Create client and connect via Socket Mode
let client = SlackClient::new(SlackClientHyperConnector::new()?);

let token_value: SlackApiTokenValue = config.bot_token.clone().into();
let token = SlackApiToken::new(token_value);

// Configure Socket Mode
let socket_mode_config = SlackClientSocketModeConfig::new();
let app_token_value: SlackApiTokenValue = config.app_token.clone().into();
let app_token = SlackApiToken::new(app_token_value);

// Create listener
let listener = Arc::new(SlackEventHandler {
    inbound_tx: tx.clone(),
    client: client.clone(),
    bot_token: token.clone(),
    allowed_users: config.allowed_users.clone(),
});

// Register and start
let sm_client = client.socket_mode_clients_manager();
sm_client.register_new_token(&socket_mode_config, app_token, listener).await?;
sm_client.start().await;
```

### Slack: Block Kit Message Construction
```rust
// Source: slack-morphism 2.18.0 Block Kit types
use slack_morphism::prelude::*;

fn build_status_blocks(status: &str, model: &str) -> Vec<SlackBlock> {
    vec![
        SlackBlock::Header(SlackHeaderBlock::new(
            SlackBlockPlainTextOnly::from("Blufio Status")
        )),
        SlackBlock::Section(SlackSectionBlock::new()
            .with_text(SlackBlockText::MarkDown(
                SlackBlockMarkDownText::new(format!("*Status:* {}", status))
            ))
        ),
        SlackBlock::Section(SlackSectionBlock::new()
            .with_fields(vec![
                SlackBlockText::MarkDown(SlackBlockMarkDownText::new(format!("*Model:*\n{}", model))),
                SlackBlockText::MarkDown(SlackBlockMarkDownText::new("*Uptime:*\n2h 15m".into())),
            ])
        ),
        SlackBlock::Divider(SlackDividerBlock::new()),
    ]
}
```

### Markdown-to-mrkdwn Conversion (Slack)
```rust
// Key differences: standard markdown -> Slack mrkdwn
pub fn markdown_to_mrkdwn(text: &str) -> String {
    let mut result = text.to_string();

    // Bold: **text** -> *text*
    // Must be done before italic conversion
    result = regex_replace(&result, r"\*\*(.+?)\*\*", "*$1*");

    // Italic: *text* -> _text_  (after bold conversion)
    // Be careful not to match already-converted bold markers
    result = regex_replace(&result, r"(?<!\*)\*([^*]+?)\*(?!\*)", "_$1_");

    // Strikethrough: ~~text~~ -> ~text~
    result = regex_replace(&result, r"~~(.+?)~~", "~$1~");

    // Links: [text](url) -> <url|text>
    result = regex_replace(&result, r"\[(.+?)\]\((.+?)\)", "<$2|$1>");

    // Headers: # text -> *text* (bold, since mrkdwn has no headers)
    result = regex_replace(&result, r"^#{1,6}\s+(.+)$", "*$1*");

    result
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Discord prefix commands (!command) | Slash commands (/command) | 2022 (Discord API v10) | Slash commands are the standard; prefix commands still work but slash is preferred |
| Discord message content freely available | MESSAGE_CONTENT privileged intent required | Sept 2022 | Bots in 100+ guilds must apply; content empty in guild messages without intent |
| Slack Events API (HTTP webhook) | Socket Mode (WebSocket, no public URL) | 2021 | Perfect for self-hosted; no ngrok/public URL needed |
| Slack legacy message formatting | Block Kit | 2019 | Richer formatting; Section, Header, Divider, Actions blocks |
| serenity 0.11 (deprecated) | serenity 0.12.5 | Dec 2025 | New builder pattern, improved gateway, Rust 1.74+ MSRV |
| slack-morphism 1.x | slack-morphism 2.18.0 | Feb 2026 | Redesigned API, axum/hyper features, improved Socket Mode |

**Deprecated/outdated:**
- serenity StandardFramework: Still works but slash commands via `interaction_create` is the modern approach
- Slack Events API HTTP mode: Still supported but Socket Mode is preferred for self-hosted apps (no public URL)
- Slack legacy attachments: Deprecated in favor of Block Kit

## Discretion Recommendations

### Slash command design: Single /blufio with subcommands
**Recommendation:** Use a single `/blufio` command with subcommands (`status`, `help`, `chat`) on both Discord and Slack. This is consistent across platforms and avoids namespace pollution. Discord supports this natively via `SubCommand` option type. Slack achieves this by parsing the text after `/blufio`.

### Slack Rust crate: slack-morphism
**Recommendation:** Use `slack-morphism` 2.18.0. It is the only actively maintained Rust Slack library with Socket Mode support. No viable alternatives exist. The `slack-api` crate was last updated in 2020 and lacks Socket Mode entirely.

### Streaming: Edit-in-place on Discord, evaluate for Slack
**Recommendation:**
- **Discord:** Edit-in-place streaming works well. Serenity handles rate limits internally. Use ~1000ms throttle interval. Discord's rate limits are per-channel and generous enough for streaming edits.
- **Slack:** Edit-in-place is viable but use a conservative ~3000ms throttle interval. Slack's `chat.update` is Tier 3 (50+/min). For typical single-user conversations this is fine. If issues arise, degrade to full-response-at-once.

### Platform-specific throttle intervals
**Recommendation:**
- Discord: 1000ms (generous per-channel rate limits, built-in handling in serenity)
- Slack: 3000ms (Tier 3 rate limit of 50+/min on chat.update, be conservative)
- Telegram: 1500ms (existing, keep as-is)

### Discord embed styling
**Recommendation:** Use Discord's blurple color (#5865F2 / 0x5865F2) as the primary embed color for brand consistency. Use green (#57F287) for success/healthy status, red (#ED4245) for errors, and yellow (#FEE75C) for warnings.

## Open Questions

1. **Discord Gateway shard management for ChannelAdapter trait**
   - What we know: serenity's `Client::start()` consumes self, which conflicts with storing the client in the adapter struct. The `Http` handle must be extracted before starting.
   - What's unclear: The exact pattern for cleanly separating the gateway lifecycle from the HTTP client in serenity 0.12.
   - Recommendation: Create the `Client`, clone `client.http`, spawn `client.start()` in a background task, store the `Http` arc and the JoinHandle in the adapter struct. This mirrors the Telegram pattern where the polling handle is stored.

2. **Slack bot_user_id discovery for @mention filtering**
   - What we know: Need to know the bot's user ID to detect @mentions in channel messages. Slack's `auth.test` API returns the bot user ID.
   - What's unclear: Whether slack-morphism exposes this directly or requires a manual API call.
   - Recommendation: Call `auth.test` during `connect()` to discover the bot user ID. Store it for mention filtering.

3. **ChannelCapabilities extension backward compatibility**
   - What we know: Adding `supports_embeds`, `supports_reactions`, `supports_threads` to `ChannelCapabilities` changes the struct layout.
   - What's unclear: Whether any external code constructs `ChannelCapabilities` directly.
   - Recommendation: Add new fields with `#[serde(default)]` and set sensible defaults (false). This is a non-breaking change within the project since all construction happens in adapter code.

## Sources

### Primary (HIGH confidence)
- Context7 `/serenity-rs/serenity` - Gateway intents, slash commands, embeds, EventHandler pattern
- Context7 `/websites/rs_serenity_serenity` - Client setup, intents, MESSAGE_CONTENT privileged intent
- Context7 `/serenity-rs/poise` - Evaluated and rejected for this use case (conflicts with ChannelAdapter pattern)
- Context7 `/websites/rs_slack-morphism_slack_morphism` - Socket Mode listener, Web API, Block Kit types
- crates.io API - serenity 0.12.5 (Dec 2025), slack-morphism 2.18.0 (Feb 2026)
- Existing codebase: `blufio-telegram/src/` - Reference implementation for all patterns

### Secondary (MEDIUM confidence)
- [Discord Developer Docs - Rate Limits](https://discord.com/developers/docs/topics/rate-limits) - 50 req/s global, per-bucket limiting by channel_id
- [Discord Developer Docs - MESSAGE_CONTENT FAQ](https://support-dev.discord.com/hc/en-us/articles/4404772028055) - Privileged intent requirements
- [Slack Rate Limits](https://docs.slack.dev/apis/web-api/rate-limits/) - Tier 1-4 system; chat.update is Tier 3 (50+/min)
- [Slack chat.update method](https://docs.slack.dev/reference/methods/chat.update) - Tier 3, arguments, response format
- [Slack mrkdwn formatting](https://docs.slack.dev/messaging/formatting-message-text/) - Syntax differences from standard markdown
- [Discord embed limits](https://discord.com/developers/docs/resources/message#embed-object-embed-limits) - 6000 total chars, 4096 description

### Tertiary (LOW confidence)
- None -- all findings verified with primary or secondary sources.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - serenity and slack-morphism are the definitive Rust libraries for their respective platforms; versions verified via crates.io
- Architecture: HIGH - follows established Telegram adapter pattern exactly; all integration points documented in existing codebase
- Pitfalls: HIGH - MESSAGE_CONTENT intent, rate limits, mrkdwn syntax all verified with official documentation
- Format pipeline: MEDIUM - design is sound but implementation details (rich content enum variants, degradation rules) will need iteration during implementation

**Research date:** 2026-03-06
**Valid until:** 2026-04-06 (stable domain; serenity 0.12.x and slack-morphism 2.x are mature)
