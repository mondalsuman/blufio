# Architecture Research

**Domain:** Always-on AI agent platform (Rust, single-binary, plugin-composed)
**Researched:** 2026-02-28
**Confidence:** HIGH (core patterns), MEDIUM (WASM component model maturity)

## Standard Architecture

### System Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         INGEST LAYER                                    │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────────┐    │
│  │ Telegram │  │ HTTP API │  │WebSocket │  │  Cron / Heartbeat   │    │
│  │ Adapter  │  │ (axum)   │  │ (axum)   │  │  Scheduler          │    │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └──────────┬──────────┘    │
│       │              │             │                   │               │
├───────┴──────────────┴─────────────┴───────────────────┴───────────────┤
│                         GATEWAY / ROUTER                               │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  Message Bus (tokio::sync::mpsc bounded channels)               │   │
│  │  - Normalize inbound → Envelope { session_id, channel, payload }│   │
│  │  - Route to correct session / agent                             │   │
│  │  - Backpressure via bounded channel capacity                    │   │
│  └─────────────────────────────┬───────────────────────────────────┘   │
├────────────────────────────────┼──────────────────────────────────────-┤
│                         AGENT LAYER                                    │
│  ┌─────────────────────────────┴───────────────────────────────────┐   │
│  │                    Session Manager                              │   │
│  │  - Session registry (DashMap<SessionId, SessionHandle>)         │   │
│  │  - Spawn/resume/park sessions                                   │   │
│  │  - Idle timeout eviction (LRU)                                  │   │
│  └─────────────┬───────────────────────────────────┬───────────────┘   │
│                │                                   │                   │
│  ┌─────────────┴──────────┐         ┌──────────────┴──────────────┐   │
│  │    Agent Loop (FSM)    │         │    Agent Loop (FSM)         │   │
│  │  per-session tokio task│         │    per-session tokio task   │   │
│  │  States:               │         │                             │   │
│  │   Idle → Receiving →   │         │    (same structure,         │   │
│  │   Assembling Context → │         │     independent state)      │   │
│  │   Calling LLM →        │         │                             │   │
│  │   Executing Tools →    │         │                             │   │
│  │   Responding → Idle    │         │                             │   │
│  └─────────┬──────────────┘         └─────────────────────────────┘   │
│            │                                                           │
├────────────┼──────────────────────────────────────────────────────────-┤
│            │        INTELLIGENCE LAYER                                 │
│  ┌─────────┴──────────────────────────────────────────────────────┐   │
│  │                  Context Assembly Pipeline                     │   │
│  │  1. Static zone   (system prompt, persona — cache-aligned)     │   │
│  │  2. Conditional zone (memory, skills, knowledge — per-query)   │   │
│  │  3. Dynamic zone  (conversation history, current turn)         │   │
│  └─────────┬──────────────────────────────────────────────────────┘   │
│            │                                                           │
│  ┌─────────┴──────────┐  ┌────────────────┐  ┌───────────────────┐   │
│  │  Model Router      │  │  Cost Ledger   │  │  Embedding Engine │   │
│  │  Haiku/Sonnet/Opus │  │  Budget caps   │  │  ONNX via Candle  │   │
│  │  by complexity     │  │  Kill switches │  │  Semantic search   │   │
│  └────────────────────┘  └────────────────┘  └───────────────────┘   │
│                                                                       │
├──────────────────────────────────────────────────────────────────────-┤
│                         EXECUTION LAYER                                │
│  ┌────────────────────┐  ┌────────────────────────────────────────┐   │
│  │  WASM Skill        │  │  Built-in Tool Registry               │   │
│  │  Sandbox           │  │  (memory_store, memory_search,        │   │
│  │  (wasmtime)        │  │   schedule, cost_check, ...)          │   │
│  │  - WIT interfaces  │  │                                       │   │
│  │  - Capability gate │  │                                       │   │
│  │  - Fuel metering   │  │                                       │   │
│  └────────────────────┘  └────────────────────────────────────────┘   │
│                                                                       │
├──────────────────────────────────────────────────────────────────────-┤
│                         PERSISTENCE LAYER                              │
│  ┌──────────────────────────────────────────────────────────────┐     │
│  │                    SQLite (WAL mode)                          │     │
│  │  Tables: sessions, messages, memory, skills, queue, cron,    │     │
│  │          cost_ledger, config, embeddings                     │     │
│  │  Access: tokio-rusqlite (background thread per connection)   │     │
│  │  Encryption: SQLCipher for credential vault                  │     │
│  └──────────────────────────────────────────────────────────────┘     │
│                                                                       │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐    │
│  │  Credential  │  │  Config      │  │  Metrics / Observability │    │
│  │  Vault       │  │  (TOML)      │  │  (Prometheus)            │    │
│  │  AES-256-GCM │  │  deny_unkn.  │  │  metrics crate           │    │
│  └──────────────┘  └──────────────┘  └──────────────────────────┘    │
└──────────────────────────────────────────────────────────────────────-┘
```

### Component Responsibilities

| Component | Responsibility | Communicates With | Implementation |
|-----------|----------------|-------------------|----------------|
| **Channel Adapters** | Normalize platform-specific messages (Telegram, HTTP, WS) into canonical `Envelope` | Gateway message bus | Trait `ChannelAdapter` with `recv()` and `send()` async methods |
| **Gateway / Router** | Accept envelopes, route to correct session, enforce backpressure | Channel adapters, Session Manager | Bounded `tokio::sync::mpsc` channels, envelope normalization |
| **Session Manager** | Lifecycle of agent sessions: create, resume, park, evict | Gateway, Agent Loops, Persistence | `DashMap<SessionId, SessionHandle>` with LRU eviction |
| **Agent Loop (FSM)** | Core reasoning cycle per session: receive -> context -> LLM -> tools -> respond | Session Manager, Context Pipeline, Execution Layer, Persistence | Enum-based FSM in a `tokio::spawn` task per session |
| **Context Assembly Pipeline** | Build the prompt: static zone + conditional zone + dynamic zone | Agent Loop, Persistence, Embedding Engine | Three-zone assembler with cache-key alignment for Anthropic prompt caching |
| **Model Router** | Select Haiku/Sonnet/Opus based on query complexity and budget | Agent Loop, Cost Ledger, LLM Provider | Complexity classifier (token count, tool presence, conversation depth) |
| **Cost Ledger** | Track per-session and global token spend, enforce budget caps | Model Router, Agent Loop, Persistence | SQLite table with atomic increment, configurable thresholds |
| **Embedding Engine** | Generate vector embeddings for semantic memory search | Context Pipeline, Persistence | ONNX model via Candle, ~80MB, runs in dedicated thread |
| **WASM Skill Sandbox** | Execute third-party skills in memory-isolated sandbox | Agent Loop, Persistence (via host functions) | wasmtime with WIT interfaces, fuel metering, capability manifests |
| **Built-in Tools** | Core tool implementations (memory, scheduling, cost) | Agent Loop, Persistence | Direct Rust functions, no sandbox overhead |
| **SQLite Persistence** | All durable state: sessions, messages, memory, queue, cron, cost | All components that need state | rusqlite via tokio-rusqlite, WAL mode, SQLCipher for secrets |
| **Credential Vault** | Encrypted storage for API keys, tokens, secrets | Config, LLM Provider, Channel Adapters | AES-256-GCM encryption, separate SQLCipher database |
| **Config** | TOML-based configuration with strict validation | All components at startup | serde with `deny_unknown_fields`, layered (defaults -> file -> env -> CLI) |
| **Metrics / Observability** | Prometheus counters, histograms, gauges | All components | `metrics` crate with prometheus exporter |
| **Plugin Host** | Load/unload adapter implementations at runtime | All adapter-based components | Trait objects (`Box<dyn ChannelAdapter>`) registered at startup |

## Recommended Project Structure

```
blufio/
├── Cargo.toml                # Workspace root
├── crates/
│   ├── blufio-core/          # Core types, traits, error types
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── envelope.rs   # Canonical message envelope
│   │   │   ├── session.rs    # Session types, SessionId
│   │   │   ├── config.rs     # Config structs (TOML deserialization)
│   │   │   ├── error.rs      # Unified error types (thiserror)
│   │   │   └── traits/       # All adapter trait definitions
│   │   │       ├── mod.rs
│   │   │       ├── channel.rs    # ChannelAdapter trait
│   │   │       ├── provider.rs   # LlmProvider trait
│   │   │       ├── storage.rs    # StorageAdapter trait
│   │   │       ├── embedding.rs  # EmbeddingAdapter trait
│   │   │       ├── auth.rs       # AuthAdapter trait
│   │   │       └── skill.rs      # SkillRuntime trait
│   │   └── Cargo.toml
│   │
│   ├── blufio-gateway/       # HTTP/WS server, message routing
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── server.rs     # axum Router, WebSocket upgrade
│   │   │   ├── router.rs     # Envelope routing logic
│   │   │   └── middleware.rs # Auth, rate limiting, metrics
│   │   └── Cargo.toml
│   │
│   ├── blufio-agent/         # Agent loop, FSM, session manager
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── fsm.rs        # Agent state machine
│   │   │   ├── session.rs    # Session manager, lifecycle
│   │   │   ├── context.rs    # Context assembly pipeline
│   │   │   ├── router.rs     # Model routing logic
│   │   │   └── cost.rs       # Cost ledger, budget enforcement
│   │   └── Cargo.toml
│   │
│   ├── blufio-persist/       # SQLite persistence layer
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── db.rs         # Connection management (tokio-rusqlite)
│   │   │   ├── migrations.rs # Schema migrations
│   │   │   ├── models/       # Table-specific query modules
│   │   │   │   ├── sessions.rs
│   │   │   │   ├── messages.rs
│   │   │   │   ├── memory.rs
│   │   │   │   ├── queue.rs
│   │   │   │   ├── cron.rs
│   │   │   │   └── cost.rs
│   │   │   └── vault.rs      # Credential vault (SQLCipher)
│   │   └── Cargo.toml
│   │
│   ├── blufio-skills/        # WASM skill runtime
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── sandbox.rs    # wasmtime engine, store, linker
│   │   │   ├── manifest.rs   # Capability manifest parsing
│   │   │   ├── registry.rs   # Skill discovery and loading
│   │   │   └── host.rs       # Host functions exposed to WASM
│   │   ├── wit/              # WIT interface definitions
│   │   │   └── skill.wit     # Guest/host contract
│   │   └── Cargo.toml
│   │
│   ├── blufio-telegram/      # Telegram channel adapter
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   └── adapter.rs    # Implements ChannelAdapter trait
│   │   └── Cargo.toml
│   │
│   ├── blufio-anthropic/     # Anthropic LLM provider
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── client.rs     # API client (reqwest)
│   │   │   └── adapter.rs    # Implements LlmProvider trait
│   │   └── Cargo.toml
│   │
│   └── blufio-cli/           # CLI binary
│       ├── src/
│       │   ├── main.rs        # Entry point, clap argument parsing
│       │   ├── commands/      # serve, status, config, shell, plugin, skill, doctor
│       │   └── app.rs         # Application bootstrap, wiring
│       └── Cargo.toml
│
├── skills/                   # Example WASM skills (separate compile targets)
│   └── hello-world/
│       ├── src/lib.rs
│       └── Cargo.toml
│
├── scripts/                  # Shell automation (lifecycle, log rotation)
├── config/                   # Default config templates
└── tests/                    # Integration tests
    ├── gateway_tests.rs
    ├── agent_tests.rs
    └── skill_tests.rs
```

### Structure Rationale

- **crates/ workspace:** Each major component is a separate crate for independent compilation, clear dependency boundaries, and testability. The final binary links them all statically.
- **blufio-core/:** Contains zero dependencies on runtime components. Only types, traits, and errors. Every other crate depends on core. This prevents circular dependencies and enforces the plugin boundary.
- **blufio-gateway/ separate from blufio-agent/:** The gateway handles network I/O and protocol translation. The agent handles reasoning. They communicate through the message bus (channels), not direct function calls. This separation means you can test agent logic without a running HTTP server.
- **blufio-persist/ centralized:** All database access goes through one crate. No component talks to SQLite directly. This enforces schema consistency and makes migration/backup logic single-sourced.
- **blufio-skills/ isolated:** The WASM runtime has its own crate because wasmtime is a heavy dependency. It only gets compiled when skill support is needed. The WIT files live here because they define the host-guest contract.
- **Adapter crates (telegram, anthropic):** Each adapter is its own crate implementing a trait from core. Adding a new channel or provider means adding a new crate, not modifying existing ones. For v1.0 these compile into the main binary; post-v1.0 they could become WASM plugins.

## Architectural Patterns

### Pattern 1: Enum-Based FSM for Agent Loop

**What:** Model each agent session as an explicit finite state machine using Rust enums. Each state carries only the data relevant to that phase. Transitions are exhaustive match arms — the compiler ensures every state is handled.

**When to use:** Always. This is the core reasoning loop.

**Trade-offs:** More verbose than an implicit loop, but eliminates impossible states at compile time. Each session's FSM runs in its own tokio task, so sessions are naturally concurrent.

**Example:**
```rust
#[derive(Debug)]
enum AgentState {
    Idle,
    Receiving { envelope: Envelope },
    AssemblingContext { envelope: Envelope },
    CallingLlm { context: AssembledContext },
    ExecutingTools { response: LlmResponse, pending_calls: Vec<ToolCall> },
    Responding { final_response: String, session_id: SessionId },
    Error { error: AgentError, session_id: SessionId },
}

impl AgentState {
    async fn step(self, deps: &AgentDeps) -> AgentState {
        match self {
            AgentState::Idle => {
                // Wait on session's mpsc receiver
                match deps.inbox.recv().await {
                    Some(envelope) => AgentState::Receiving { envelope },
                    None => AgentState::Idle, // Channel closed, session ending
                }
            }
            AgentState::Receiving { envelope } => {
                // Persist inbound message, transition to context assembly
                deps.persist.save_message(&envelope).await;
                AgentState::AssemblingContext { envelope }
            }
            AgentState::AssemblingContext { envelope } => {
                let context = deps.context_pipeline
                    .assemble(&envelope, &deps.session)
                    .await;
                AgentState::CallingLlm { context }
            }
            AgentState::CallingLlm { context } => {
                let model = deps.model_router.select(&context);
                match deps.provider.complete(model, &context).await {
                    Ok(response) => {
                        let tool_calls = response.extract_tool_calls();
                        if tool_calls.is_empty() {
                            AgentState::Responding {
                                final_response: response.text,
                                session_id: context.session_id,
                            }
                        } else {
                            AgentState::ExecutingTools {
                                response,
                                pending_calls: tool_calls,
                            }
                        }
                    }
                    Err(e) => AgentState::Error {
                        error: e.into(),
                        session_id: context.session_id,
                    },
                }
            }
            // ... ExecutingTools, Responding, Error transitions
            _ => todo!(),
        }
    }
}

// Session task: just loop the FSM
async fn run_session(deps: AgentDeps) {
    let mut state = AgentState::Idle;
    loop {
        state = state.step(&deps).await;
    }
}
```

**Confidence:** HIGH. Enum-based state machines are a well-established Rust pattern. LangGraph (the dominant agent framework, ~400 companies in production) uses a graph-based state machine for exactly this reason. Rust's type system makes it even stronger because impossible transitions are compile-time errors.

### Pattern 2: Trait-Based Plugin System (Compile-Time Composition)

**What:** Define adapter interfaces as Rust traits in `blufio-core`. Implementations live in separate crates. At build time, the binary composes the selected implementations. At runtime, they're behind `Arc<dyn Trait + Send + Sync>` for dynamic dispatch.

**When to use:** For all extension points — channels, LLM providers, storage backends, embedding engines, auth, observability.

**Trade-offs:** ~2-5% overhead from dynamic dispatch via vtable (negligible for I/O-bound operations like HTTP calls and database queries). No runtime plugin loading for v1.0 (WASM skills fill that role). Adding a new adapter requires recompilation, but operators never need the Rust toolchain because official builds ship all adapters.

**Example:**
```rust
// In blufio-core/src/traits/channel.rs
#[async_trait]
pub trait ChannelAdapter: Send + Sync + 'static {
    /// Human-readable name for logging/config
    fn name(&self) -> &str;

    /// Start receiving messages. Returns a stream of Envelopes.
    async fn start(&self, tx: mpsc::Sender<Envelope>) -> Result<(), ChannelError>;

    /// Send a response back through this channel
    async fn send(&self, session_id: &SessionId, message: &str) -> Result<(), ChannelError>;

    /// Graceful shutdown
    async fn shutdown(&self) -> Result<(), ChannelError>;
}

// In blufio-core/src/traits/provider.rs
#[async_trait]
pub trait LlmProvider: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn supported_models(&self) -> &[ModelId];
    async fn complete(
        &self,
        model: &ModelId,
        context: &AssembledContext,
    ) -> Result<LlmResponse, ProviderError>;
    async fn count_tokens(&self, text: &str) -> Result<usize, ProviderError>;
}

// In blufio-cli/src/app.rs — wiring at startup
fn build_app(config: &Config) -> App {
    let channel: Arc<dyn ChannelAdapter> = match config.channel.kind.as_str() {
        "telegram" => Arc::new(TelegramAdapter::new(&config.channel.telegram)),
        _ => panic!("Unknown channel: {}", config.channel.kind),
    };
    let provider: Arc<dyn LlmProvider> = match config.provider.kind.as_str() {
        "anthropic" => Arc::new(AnthropicProvider::new(&config.provider.anthropic)),
        _ => panic!("Unknown provider: {}", config.provider.kind),
    };
    App { channel, provider, /* ... */ }
}
```

**Confidence:** HIGH. This is standard Rust architecture. The `async_trait` crate (or native async-in-traits in Rust 2024+) enables async methods in trait objects. The `tower` ecosystem (which axum is built on) uses this exact pattern for middleware composition.

### Pattern 3: Three-Zone Context Assembly with Cache Alignment

**What:** Split the LLM prompt into three zones ordered for maximum Anthropic prompt caching effectiveness:

1. **Static zone** (top of prompt): System prompt, persona, core instructions. Identical across turns for a session. Tagged with `cache_control: { type: "ephemeral" }` on the last block. This gets cached by Anthropic after the first call — subsequent calls pay only 10% of input token cost for this section.
2. **Conditional zone** (middle): Relevant memories retrieved by embedding similarity, loaded skill descriptions (progressive: names only until needed, then full SKILL.md), retrieved knowledge. Changes per-query but is assembled from stable fragments.
3. **Dynamic zone** (bottom): Conversation history for this session, the current user message. Changes every turn.

**When to use:** Every LLM call. This is the core token optimization strategy producing the 68-84% reduction.

**Trade-offs:** More complex prompt construction. Requires careful ordering (Anthropic caches from the prefix — content must be ordered stable-to-volatile). Conditional zone requires embedding search, adding ~5-15ms latency per query.

**Example:**
```rust
struct AssembledContext {
    /// Static zone — cached after first call
    system_prompt: Vec<ContentBlock>,
    /// Conditional zone — per-query relevant context
    conditional: Vec<ContentBlock>,
    /// Dynamic zone — conversation history + current turn
    messages: Vec<Message>,
    /// Estimated token count (for model routing)
    estimated_tokens: usize,
    /// Cache alignment metadata
    cache_breakpoints: Vec<usize>,
}

impl ContextPipeline {
    async fn assemble(
        &self,
        envelope: &Envelope,
        session: &Session,
    ) -> AssembledContext {
        // 1. Static zone (from config, same every call)
        let system_prompt = self.build_static_zone(session).await;

        // 2. Conditional zone (embedding search + skill discovery)
        let query_embedding = self.embedding.embed(&envelope.text).await;
        let relevant_memories = self.persist
            .search_memories(session.id, &query_embedding, 5)
            .await;
        let skill_descriptions = self.skill_registry
            .get_relevant_descriptions(&envelope.text)
            .await;
        let conditional = self.build_conditional_zone(
            &relevant_memories,
            &skill_descriptions,
        );

        // 3. Dynamic zone (conversation history, truncated to budget)
        let history = self.persist
            .get_recent_messages(session.id, self.config.max_history_tokens)
            .await;
        let messages = self.build_dynamic_zone(history, envelope);

        let estimated_tokens = self.estimate_tokens(&system_prompt, &conditional, &messages);

        AssembledContext {
            system_prompt,
            conditional,
            messages,
            estimated_tokens,
            cache_breakpoints: vec![system_prompt.len()], // Cache break after static zone
        }
    }
}
```

**Confidence:** HIGH. Anthropic's own engineering blog (February 2025) explicitly recommends this pattern: "the smallest possible set of high-signal tokens that maximize the likelihood of some desired outcome." Prompt caching from prefix is a documented Anthropic API feature. The three-zone structure maps directly to their caching semantics.

### Pattern 4: WASM Skill Sandbox via Component Model

**What:** Third-party skills are compiled to WASM components that run in wasmtime's sandbox. The host defines a WIT interface specifying what capabilities skills can use (read memory, write memory, make HTTP requests, etc.). Each skill declares a capability manifest; the host only exposes the capabilities declared. Fuel metering limits CPU consumption.

**When to use:** For all third-party/community skills. Built-in tools bypass the sandbox.

**Trade-offs:** ~10-50x slower than native Rust function calls for the boundary crossing itself. But skills are I/O-bound (calling APIs, reading memory), so the overhead is negligible in practice. Compilation of WASM modules takes ~50-200ms on first load; AOT compilation eliminates this on subsequent loads. Memory overhead is ~1-2MB per loaded skill instance.

**Example WIT interface:**
```wit
package blufio:skill@0.1.0;

interface types {
    record skill-request {
        session-id: string,
        arguments: string,       // JSON string
    }

    record skill-response {
        content: string,          // Response text
        metadata: option<string>, // Optional JSON metadata
    }

    record memory-entry {
        key: string,
        value: string,
        score: float64,
    }
}

interface host-capabilities {
    use types.{memory-entry};

    /// Read from session memory
    memory-read: func(session-id: string, query: string, limit: u32) -> list<memory-entry>;

    /// Write to session memory
    memory-write: func(session-id: string, key: string, value: string) -> result<_, string>;

    /// Make an HTTP request (subject to allowlist)
    http-request: func(method: string, url: string, body: option<string>) -> result<string, string>;

    /// Log a message
    log: func(level: string, message: string);
}

world skill {
    import host-capabilities;

    /// Skill metadata
    export name: func() -> string;
    export description: func() -> string;
    export version: func() -> string;

    /// Execute the skill
    export execute: func(request: types.skill-request) -> result<types.skill-response, string>;
}
```

**Confidence:** MEDIUM-HIGH. wasmtime is the most mature WASM runtime (Bytecode Alliance, full WASI 0.2 support since late 2024). The Component Model and WIT are stable enough for production use — Microsoft's Wassette (released 2025) uses exactly this pattern for AI agent tool execution. However, the WASI ecosystem is still evolving (WASI 0.3 preview is in progress), so the WIT interface will need versioning from day one.

### Pattern 5: tokio-rusqlite for Async Persistence

**What:** SQLite is inherently synchronous (file I/O with locks). `tokio-rusqlite` wraps a `rusqlite::Connection` in a dedicated OS thread and communicates via channels, making it async-compatible without blocking the tokio runtime.

**When to use:** All database access in the application.

**Trade-offs:** Adds one OS thread per connection. For a single-instance agent with 1-3 connections (read, write, vault), this is negligible. Not suitable for high-concurrency database access (hundreds of connections), but SQLite itself can't handle that anyway — this is by design for a single-instance platform.

**Example:**
```rust
use tokio_rusqlite::Connection;

pub struct Database {
    /// Primary read/write connection (WAL mode allows concurrent reads)
    write_conn: Connection,
    /// Read-only connection for queries that shouldn't block writes
    read_conn: Connection,
    /// Separate encrypted connection for credential vault
    vault_conn: Connection,
}

impl Database {
    pub async fn new(path: &Path) -> Result<Self> {
        let write_conn = Connection::open(path).await?;
        // Configure WAL mode, synchronous=NORMAL, foreign keys
        write_conn.call(|conn| {
            conn.execute_batch("
                PRAGMA journal_mode=WAL;
                PRAGMA synchronous=NORMAL;
                PRAGMA foreign_keys=ON;
                PRAGMA wal_autocheckpoint=1000;
                PRAGMA busy_timeout=5000;
            ")?;
            Ok(())
        }).await?;

        let read_conn = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ).await?;

        let vault_path = path.with_extension("vault.db");
        let vault_conn = Connection::open(&vault_path).await?;
        // SQLCipher key derivation happens here
        // vault_conn.call(|conn| { conn.execute("PRAGMA key = ?", [&key])?; Ok(()) }).await?;

        Ok(Self { write_conn, read_conn, vault_conn })
    }

    pub async fn save_message(&self, msg: &Message) -> Result<()> {
        self.write_conn.call(move |conn| {
            conn.execute(
                "INSERT INTO messages (session_id, role, content, tokens, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![msg.session_id, msg.role, msg.content, msg.tokens, msg.created_at],
            )?;
            Ok(())
        }).await
    }

    pub async fn search_memories(
        &self,
        session_id: &SessionId,
        embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        // Use read connection for search queries
        let sid = session_id.clone();
        let emb = embedding.to_vec();
        self.read_conn.call(move |conn| {
            // Cosine similarity search against stored embeddings
            // (Using a custom SQLite function or application-level computation)
            let mut stmt = conn.prepare(
                "SELECT key, value, embedding FROM memory WHERE session_id = ?1"
            )?;
            let entries: Vec<MemoryEntry> = stmt.query_map([&sid], |row| {
                Ok(MemoryEntry {
                    key: row.get(0)?,
                    value: row.get(1)?,
                    embedding: row.get::<_, Vec<u8>>(2)?,
                })
            })?.collect::<Result<Vec<_>, _>>()?;
            // Compute similarity in application code, sort, take top N
            Ok(rank_by_cosine_similarity(entries, &emb, limit))
        }).await
    }
}
```

**Confidence:** HIGH. tokio-rusqlite is the established pattern for async SQLite in Rust. WAL mode + synchronous=NORMAL is the recommended configuration per SQLite documentation for applications that need consistency with good write throughput (tens of thousands of inserts per second).

## Data Flow

### Primary Message Flow (Inbound)

```
[User sends message on Telegram]
    |
    v
[Telegram Adapter] — polls Telegram Bot API via long-polling
    |  Converts telegram_bot::Message → Envelope { session_id, channel: "telegram", payload, metadata }
    v
[Gateway Message Bus] — bounded mpsc channel (capacity: 1024)
    |  Deduplication check (idempotency key from channel message ID)
    |  Persist envelope to `queue` table (durable before routing)
    v
[Session Manager] — looks up session by (channel, user_id) composite key
    |  If session exists and active: route to session's inbox channel
    |  If session exists but parked: resume session task, then route
    |  If no session: create new session (persist to `sessions` table), spawn task, route
    v
[Agent Loop FSM] — per-session tokio task, receives from inbox mpsc
    |
    |-- [Context Assembly Pipeline]
    |     |  1. Load static zone (system prompt from config, cached in memory)
    |     |  2. Embed user query → search memory table → top-5 relevant memories
    |     |  3. Match query against skill descriptions → include relevant skill names
    |     |  4. Load conversation history (last N messages from `messages` table)
    |     |  5. Assemble into AssembledContext with cache breakpoints
    |     v
    |-- [Model Router]
    |     |  Classify complexity: simple (Haiku) / moderate (Sonnet) / complex (Opus)
    |     |  Check cost ledger: budget remaining? → if not, use cheapest model or reject
    |     v
    |-- [LLM Provider] — HTTP POST to Anthropic API (reqwest)
    |     |  Send AssembledContext with cache_control markers
    |     |  Receive LlmResponse: { text, tool_calls, usage: { input_tokens, output_tokens, cache_hit_tokens } }
    |     |  Record usage in cost ledger
    |     v
    |-- [Tool Execution] (if tool_calls present)
    |     |  For each tool call:
    |     |    Built-in tool? → execute directly
    |     |    WASM skill? → load into sandbox, execute with fuel limit, collect result
    |     |  Append tool results to context
    |     |  Loop back to LLM Provider for next turn
    |     v
    |-- [Response Assembly]
          |  Persist assistant message to `messages` table
          |  Extract any memory-worthy content → persist to `memory` table with embedding
          |  Update cost ledger with final totals
          |  Send response back through channel adapter
          v
[Telegram Adapter] — sends reply via Bot API
    |
    v
[User sees response]
```

### Heartbeat Flow (Proactive)

```
[Cron Scheduler] — tokio::time::interval, checks `cron` table
    |
    v
[Heartbeat Task] — runs every N minutes per configured schedule
    |  Build minimal context: system prompt + "check for pending tasks/reminders"
    |  Always use Haiku (cheapest model)
    |  Skip if nothing changed since last heartbeat (hash comparison)
    v
[Agent Loop FSM] — same flow as inbound, but source is "heartbeat" not "user"
    |  If LLM returns empty/no-action → discard, record skip in metrics
    |  If LLM returns action → execute and send proactive message to user
    v
[Cost Ledger] — heartbeats tracked separately for cost visibility
```

### Multi-Agent Routing Flow

```
[Agent A receives message requiring Agent B's expertise]
    |
    v
[Agent A's FSM] — tool call: route_to_agent(agent_b_id, message)
    |  Create inter-agent envelope, sign with Ed25519 session key
    v
[Gateway Message Bus] — route to Agent B's session
    |  Verify Ed25519 signature
    v
[Agent B's FSM] — processes as internal message (not user-facing)
    |  Responds back through same routing path
    v
[Agent A's FSM] — incorporates Agent B's response into its own context
    |
    v
[Final response to user]
```

### Key Data Flows

1. **Envelope normalization:** Every inbound message (Telegram, HTTP, WebSocket, heartbeat, inter-agent) gets converted to a canonical `Envelope` struct before entering the message bus. This is the single integration point — adding a new channel means implementing one trait, not modifying the agent loop.

2. **Persistence-first queuing:** Envelopes are persisted to the `queue` table before routing. If the process crashes between receiving a Telegram message and completing the response, the message is recoverable on restart. The queue table acts as a write-ahead log for messages.

3. **Context flows down, responses flow up:** The context pipeline pulls data from persistence (memories, history, skills) and assembles it downward into the LLM call. Responses flow upward through the same channel adapter that received the original message. The agent loop never reaches "up" to the gateway; it uses a response channel provided at session creation.

4. **Cost is a first-class data flow:** Every LLM call records input_tokens, output_tokens, cache_hit_tokens, model_used, and cost_usd in the cost_ledger table atomically. Budget checks happen before the LLM call (pre-flight) and after (reconciliation). Kill switches trigger on threshold breach.

## Build Order (Dependency-Driven)

The build order is dictated by component dependencies. Each phase produces a testable, runnable artifact.

### Phase 1: Foundation (Weeks 1-2)

**Build:** `blufio-core` + `blufio-persist` + `blufio-cli` (skeleton)

**Why first:** Everything depends on core types and persistence. You cannot build an agent loop without a place to store state, and you cannot build adapters without trait definitions to implement.

**Deliverable:** A CLI that can initialize a SQLite database, run migrations, and read/write sessions and messages via the persistence layer. No LLM, no channels — just the skeleton.

**Dependencies unlocked:** Agent loop, gateway, all adapters.

```
blufio-core (traits, types, config)
    |
    v
blufio-persist (SQLite, migrations, models)
    |
    v
blufio-cli (skeleton: init, doctor commands)
```

### Phase 2: Agent Loop (Weeks 2-3)

**Build:** `blufio-agent` (FSM + context pipeline + model router) + `blufio-anthropic`

**Why second:** The agent loop is the core value. Once you can receive a message, assemble context, call the LLM, and store the response, you have a working (headless) agent. The context pipeline starts simple (static system prompt + conversation history) and grows conditional/dynamic zones incrementally.

**Deliverable:** A headless agent callable via Rust test harness. Feed it a message, get a response. No Telegram, no HTTP — just the reasoning loop.

**Dependencies unlocked:** Channel adapters, skill execution, cost tracking.

```
blufio-anthropic (LLM provider)
    |
    v
blufio-agent (FSM, context pipeline, session manager)
    |
    depends on: blufio-core, blufio-persist, blufio-anthropic
```

### Phase 3: First Channel (Weeks 3-4)

**Build:** `blufio-telegram` + `blufio-gateway` (message routing)

**Why third:** With a working agent loop, adding Telegram makes it user-facing. The gateway/router wires the channel adapter to the session manager. This is the first "ship it" moment — a working Telegram bot backed by Claude.

**Deliverable:** `blufio serve` starts a Telegram bot that responds to messages using Claude. Conversations persist across restarts.

**Dependencies unlocked:** HTTP/WS gateway, additional channels, observability.

```
blufio-telegram (Telegram adapter)
    |
    v
blufio-gateway (message bus, routing)
    |
    depends on: blufio-core, blufio-agent
    |
    v
blufio-cli (serve command wires everything together)
```

### Phase 4: Intelligence Layer (Weeks 4-6)

**Build:** Embedding engine + semantic memory + model routing + cost ledger + cache-aligned context

**Why fourth:** The basic agent works. Now make it smart and affordable. Embedding-based memory search, three-zone context with Anthropic cache alignment, model routing (Haiku for simple queries, Opus for complex), and cost tracking.

**Deliverable:** Agent that remembers across sessions, optimizes token usage, routes to appropriate model, and tracks costs.

**Dependencies unlocked:** Skill system, advanced features.

### Phase 5: Skill Sandbox (Weeks 6-8)

**Build:** `blufio-skills` (wasmtime + WIT + registry + fuel metering)

**Why fifth:** Skills depend on a working agent loop, persistence (for skill state), and the context pipeline (for skill discovery). The WASM sandbox is complex but isolated — it doesn't change how the agent loop works, it adds a new execution path for tool calls.

**Deliverable:** Community skills can be loaded from `.wasm` files, discovered by the agent, executed in a sandbox with capability restrictions.

### Phase 6: Hardening (Weeks 8-10)

**Build:** HTTP/WS gateway, credential vault (SQLCipher), Prometheus metrics, heartbeats/cron, multi-agent routing, CLI completions, systemd integration, backup/restore.

**Why last:** These are quality-of-life and production-readiness features. They don't change the architecture — they harden it. Each is independently addable because the trait boundaries are already established.

**Deliverable:** Production-ready single binary suitable for deployment on a $4/month VPS.

## Scaling Considerations

| Scale | Architecture Adjustments |
|-------|--------------------------|
| 1-5 sessions | Single SQLite connection, no connection pooling. All sessions share one write connection. This is the expected v1.0 deployment. |
| 5-50 sessions | Read/write connection split (WAL mode enables concurrent reads). LRU session eviction for idle sessions. Embedding search may need indexing (sqlite-vss or application-level HNSW). |
| 50-200 sessions | Multiple read connections (2-4). Background compaction of conversation history. Skill pre-compilation (AOT) to avoid JIT compilation latency. Consider sharding sessions across multiple SQLite files. |
| 200+ sessions | Beyond single-instance scope. Would need a message queue (NATS/Redis), PostgreSQL, and process-per-agent architecture. Out of scope for v1.0 — the architecture supports migration because all persistence goes through traits. |

### Scaling Priorities

1. **First bottleneck: LLM API latency.** Each agent turn takes 1-10 seconds waiting for the LLM. Sessions are inherently concurrent (each in its own tokio task), so this is naturally parallelized. The bottleneck is API rate limits, not local compute. **Fix:** Model routing (use Haiku for simple queries) and prompt caching (reduce input tokens by 68-84%).

2. **Second bottleneck: Embedding computation.** ONNX model inference for memory search takes ~5-15ms per query on CPU. At 50+ concurrent sessions, this could queue up. **Fix:** Batch embedding requests, cache recent embeddings, or offload to a dedicated thread pool.

3. **Third bottleneck: SQLite write contention.** WAL mode allows one writer at a time. Under heavy load, writes may queue. **Fix:** Batch writes (aggregate messages before committing), use WAL2 mode (experimental, allows concurrent writers to different pages), or shard by session_id.

## Anti-Patterns

### Anti-Pattern 1: Shared Mutable State Across Sessions

**What people do:** Use a single `Arc<Mutex<HashMap<SessionId, SessionState>>>` and lock it for every operation, including during LLM calls.

**Why it's wrong:** LLM calls take 1-10 seconds. Holding a lock across an LLM call blocks all other sessions. This is the "async mutex holding across await" problem — it serializes what should be concurrent.

**Do this instead:** Each session owns its state in its own tokio task. The session manager holds `DashMap<SessionId, SessionHandle>` where `SessionHandle` contains only a `mpsc::Sender<Envelope>` to send messages into the session. No shared mutable state between sessions. The only shared resource is the database, which handles its own concurrency via WAL mode.

### Anti-Pattern 2: Unbounded Channels and Queues

**What people do:** Use unbounded channels (`tokio::sync::mpsc::unbounded_channel()`) between components, assuming messages will be consumed fast enough.

**Why it's wrong:** If the LLM is slow or a session hangs, unbounded channels grow without limit. On a VPS with 512MB RAM, this causes OOM within hours. This is exactly how OpenClaw leaks memory — in-memory queues with no eviction.

**Do this instead:** Bounded channels everywhere. `mpsc::channel(1024)` for the gateway bus. `mpsc::channel(16)` for per-session inboxes. When a channel is full, the sender gets backpressure (returns `Err(TrySendError::Full)`), and the gateway can respond with "server busy" instead of silently queuing. Pair with `persist-first queuing` — write to the SQLite queue table before trying to route, so messages survive backpressure.

### Anti-Pattern 3: Monolithic Context (Inject Everything)

**What people do:** Concatenate the entire system prompt, all memories, all skill definitions, all conversation history into every LLM call regardless of query complexity.

**Why it's wrong:** This is OpenClaw's ~35K tokens/turn problem. It wastes money ($769/month on Opus for heartbeats alone), hits context window limits faster, and degrades LLM quality through context rot (Chroma Research: accuracy decreases as context length increases beyond the relevant content).

**Do this instead:** Three-zone context assembly. Static zone is cached (10% cost after first call). Conditional zone loads only relevant content via embedding search. Dynamic zone is truncated to a token budget. Simple queries (greetings, status checks) might use 2-5K tokens total instead of 35K.

### Anti-Pattern 4: Trusting WASM Skills Without Capability Gating

**What people do:** Give WASM skills access to all host functions (network, filesystem, memory) because "the sandbox prevents real damage."

**Why it's wrong:** The WASM sandbox prevents memory corruption and arbitrary code execution, but it doesn't prevent a skill from reading all user memories, making unlimited HTTP requests, or consuming unlimited CPU. A malicious skill could exfiltrate data through the host functions you provide.

**Do this instead:** Capability manifests. Each skill declares what it needs (`["memory:read", "http:get:api.weather.com"]`). The host only links the declared capabilities into the wasmtime linker. Fuel metering limits CPU. Memory limits restrict allocation. The operator reviews the manifest before installing a skill.

### Anti-Pattern 5: Synchronous SQLite in Async Context

**What people do:** Call `rusqlite::Connection` methods directly from async functions, blocking the tokio runtime thread.

**Why it's wrong:** SQLite operations can take 1-50ms (or longer for complex queries or WAL checkpoints). Blocking the tokio runtime thread for that duration starves all other tasks on that thread. With the default 4-thread runtime, blocking one thread reduces throughput by 25%.

**Do this instead:** Use `tokio-rusqlite`, which runs SQLite operations on a dedicated OS thread and communicates via channels. The async caller awaits without blocking the runtime. For long operations (migrations, bulk imports), use `tokio::task::spawn_blocking()`.

## Integration Points

### External Services

| Service | Integration Pattern | Notes |
|---------|---------------------|-------|
| Anthropic API | HTTP POST via reqwest, streaming SSE for responses | Use `cache_control` markers for prompt caching. Handle 429 (rate limit) with exponential backoff. Connection pool via reqwest's built-in pool. |
| Telegram Bot API | Long-polling via teloxide or custom reqwest loop | Avoid webhooks (requires TLS termination, public IP). Long-polling is simpler for single-instance. Parse update_id for deduplication. |
| ONNX Runtime (Candle) | In-process inference, dedicated thread | Load model at startup (~80MB, ~2s). Run inference in `spawn_blocking` to avoid blocking tokio. Cache recent embeddings in LRU map. |

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| Gateway <-> Session Manager | `mpsc::Sender<Envelope>` (bounded) | Gateway sends envelopes. Session Manager resolves to session inbox. No direct function calls. |
| Session Manager <-> Agent Loop | `mpsc::Sender<Envelope>` per session (bounded) | Each session has its own inbox channel. Session Manager holds the sender half. Agent loop holds the receiver half. |
| Agent Loop <-> Context Pipeline | Direct function call (same crate) | Synchronous assembly within the agent loop's async task. No channel needed — context assembly is a pure function of (session state, persistence queries). |
| Agent Loop <-> LLM Provider | `Arc<dyn LlmProvider>` async method call | Dynamic dispatch through trait object. Provider handles HTTP, retries, token counting internally. |
| Agent Loop <-> Skill Sandbox | `Arc<dyn SkillRuntime>` async method call | Dynamic dispatch. Skill runtime manages wasmtime engine, store lifecycle, fuel metering. Returns structured `SkillResponse`. |
| All Components <-> Persistence | `Arc<Database>` with async methods | Single persistence crate. All SQL lives here. Components never construct raw SQL — they call typed methods like `save_message()`, `search_memories()`. |
| All Components <-> Metrics | `metrics` crate macros (global) | Fire-and-forget. `counter!("messages_received").increment(1)`. Prometheus exporter runs in gateway. No channel needed — the metrics crate uses atomic operations internally. |

## Sources

- [Anthropic: Effective Context Engineering for AI Agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) (HIGH confidence — primary source for context assembly patterns)
- [Chroma Research: Context Rot](https://research.trychroma.com/context-rot) (HIGH confidence — evidence for three-zone context approach)
- [Wasmtime Documentation](https://docs.rs/wasmtime/latest/wasmtime/) (HIGH confidence — Context7 verified, authoritative source)
- [Wasmtime Component Model](https://docs.rs/wasmtime/latest/wasmtime/component/index.html) (HIGH confidence — Context7 verified)
- [Axum Documentation](https://docs.rs/axum/latest/axum/) (HIGH confidence — Context7 verified)
- [tokio-rusqlite](https://docs.rs/tokio-rusqlite) (HIGH confidence — authoritative source)
- [Building Native Plugin Systems with WebAssembly Components](https://tartanllama.xyz/posts/wasm-plugins/) (MEDIUM confidence — practical implementation guide)
- [WASI and the WebAssembly Component Model: Current Status](https://eunomia.dev/blog/2025/02/16/wasi-and-the-webassembly-component-model-current-status/) (MEDIUM confidence — ecosystem overview)
- [Wassette: Microsoft's Rust-Powered Bridge Between Wasm and MCP](https://thenewstack.io/wassette-microsofts-rust-powered-bridge-between-wasm-and-mcp/) (MEDIUM confidence — validates WASM-for-agent-tools pattern)
- [Lindy AI: AI Agent Architecture Guide](https://www.lindy.ai/blog/ai-agent-architecture) (LOW confidence — general overview)
- [LangGraph State Machines for Agent Task Flows](https://dev.to/jamesli/langgraph-state-machines-managing-complex-agent-task-flows-in-production-36f4) (MEDIUM confidence — validates FSM pattern)
- [Plugin Based Architecture in Rust](https://dev.to/mineichen/plugin-based-architecture-in-rust-4om7) (MEDIUM confidence — community pattern)
- [Memory in the Age of AI Agents](https://arxiv.org/abs/2512.13564) (MEDIUM confidence — academic survey of memory architectures)

---
*Architecture research for: Blufio — Rust AI agent platform*
*Researched: 2026-02-28*
