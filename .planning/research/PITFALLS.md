# Pitfalls Research

**Domain:** Rust AI agent platform (multi-channel, WASM-sandboxed, SQLite-persisted, LLM-abstracted)
**Researched:** 2026-02-28
**Confidence:** HIGH (Context7 + official docs + community post-mortems)

## Critical Pitfalls

### Pitfall 1: Blocking the Tokio Runtime with Sync Operations

**What goes wrong:**
SQLite calls (via rusqlite), cryptographic operations (AES-256-GCM, Ed25519), ONNX embedding inference, and WASM compilation are all synchronous/CPU-bound. Calling them directly from async task contexts starves the tokio worker threads. With the default multi-threaded runtime (typically 4-8 worker threads), even 2-3 concurrent blocking calls can freeze the entire agent loop -- no messages processed, no heartbeats sent, no HTTP requests served.

**Why it happens:**
rusqlite's `Connection` is `!Sync` and all its methods are blocking. Developers wrap it in a `Mutex<Connection>` and call `.lock().unwrap()` from async contexts. This blocks a tokio worker while waiting for the mutex AND while executing the SQL query. Crypto operations and WASM module instantiation have the same profile -- they look like "fast" calls but can take 1-50ms each, which is an eternity when you're occupying one of 4 worker threads.

**How to avoid:**
- Use `tokio-rusqlite` or `deadpool-sqlite` which run SQLite operations on dedicated background threads and return results via channels. Every database call goes through `connection.call(|conn| { ... }).await`.
- Use `tokio::task::spawn_blocking` for all crypto operations, WASM compilation, and embedding inference.
- Set up a dedicated thread pool (not tokio's blocking pool) for embedding inference since it's CPU-heavy and long-running.
- Never hold a `Mutex` across an `.await` point. Use `tokio::sync::Mutex` only when you must hold across await; prefer message-passing (mpsc channels) for database access patterns.
- Enable tokio-console during development to detect tasks that haven't yielded.

**Warning signs:**
- Agent stops responding to Telegram messages during database-heavy operations (skill installs, memory writes).
- Heartbeat timers fire late or skip entirely.
- tokio-console shows tasks stuck in "busy" state for >10ms.
- HTTP health check endpoint times out intermittently.

**Phase to address:**
Phase 1 (Core Foundation). The database access pattern and crypto wrapper must be async-safe from day one. Refactoring a synchronous database layer into an async one later requires touching every callsite.

---

### Pitfall 2: SQLite Single-Writer Bottleneck Under Concurrent Sessions

**What goes wrong:**
SQLite WAL mode allows concurrent readers but only one writer at a time. When multiple agent sessions try to write simultaneously (session state updates, message queue inserts, cost ledger updates, memory writes), writers queue behind the busy_timeout. With the default 5-second busy_timeout, writes start failing with SQLITE_BUSY under as few as 10 concurrent sessions. The queue grows, latency spikes, and the agent loop stalls waiting for database writes.

**Why it happens:**
Developers treat SQLite like Postgres and open multiple read-write connections in a pool. Each connection tries to BEGIN IMMEDIATE independently. Under load, they contend on the WAL write lock. Without careful transaction scoping, a single long-running read transaction (e.g., searching memory vectors) can block WAL checkpointing, causing the WAL file to grow unboundedly (observed: 500MB+ WAL files in production).

**How to avoid:**
- **Single writer pattern**: Exactly one connection (or one dedicated writer thread) handles ALL writes. All other connections are read-only. Writes are submitted via an mpsc channel to the writer thread, which batches them in transactions.
- **`BEGIN IMMEDIATE`** for all write transactions -- announces intent to write upfront, preventing deadlocks from read-then-write upgrade patterns.
- **PRAGMA configuration at connection open**:
  ```sql
  PRAGMA journal_mode = WAL;
  PRAGMA synchronous = NORMAL;
  PRAGMA busy_timeout = 5000;
  PRAGMA wal_autocheckpoint = 1000;
  PRAGMA foreign_keys = ON;
  PRAGMA cache_size = -8000;  -- 8MB
  ```
- **Batch writes**: Group multiple writes (session state + cost update + queue insert) into a single transaction on the writer thread. Target <5ms per transaction.
- **Monitor WAL file size**: Alert if WAL exceeds 10MB. Run `PRAGMA wal_checkpoint(TRUNCATE)` periodically from the writer thread when no readers are active.
- **Read connection pool**: 2-4 read-only connections for concurrent reads (session lookups, memory searches, cost queries).

**Warning signs:**
- SQLITE_BUSY errors in logs under moderate load (>5 concurrent users).
- WAL file growing continuously (check with `ls -la` on the `-wal` file).
- Write latency increasing over time (indicates WAL growth or checkpoint contention).
- Cost ledger entries missing or duplicated (lost writes or retry logic bugs).

**Phase to address:**
Phase 1 (Core Foundation). The single-writer pattern must be the architectural foundation for all database access. Retrofitting it requires rewriting every database interaction.

---

### Pitfall 3: WASM Sandbox Escape via Uncapped Resource Consumption

**What goes wrong:**
A malicious or buggy WASM skill allocates unbounded memory, enters an infinite loop, or exploits WASI host function resource allocation to crash the host process. This is not theoretical -- CVE-2025-53901 (host panic via `fd_renumber`) and CVE-2026-27572 (sandbox crash via excessive HTTP headers) demonstrate that WASI host implementations are attack surfaces. Without fuel limits and memory caps, a single skill can OOM the entire agent.

**Why it happens:**
Developers configure wasmtime with defaults (no fuel, no memory limits, no resource limiter) and focus on the "happy path" of cooperative skills. They assume WASM's sandbox prevents all harm, but the sandbox only prevents memory safety violations -- it does NOT prevent resource exhaustion. Wasmtime's `StoreLimits` and fuel consumption must be explicitly configured.

**How to avoid:**
- **Enable fuel consumption** via `Config::consume_fuel(true)`. Set fuel budgets per-skill-invocation (e.g., 1M fuel units for simple queries, 10M for complex tools). When fuel runs out, execution traps.
- **Configure `StoreLimitsBuilder`** with explicit caps:
  ```rust
  StoreLimitsBuilder::new()
      .memory_size(16 * 1024 * 1024)  // 16MB per skill instance
      .instances(2)                     // Max 2 instances
      .tables(4)                        // Max 4 tables
      .memories(1)                      // Max 1 memory
      .build()
  ```
- **Use `-Smax-resources` and `-Shostcall-fuel`** (wasmtime 42.0+) to limit component-model resource table entries and per-hostcall data copies.
- **Timeout enforcement**: Wrap skill execution in `tokio::time::timeout()`. Kill the Store (drop it) if the skill exceeds its time budget. Wasmtime supports `Config::epoch_interruption` for cooperative interruption from another thread.
- **Pin wasmtime to audited versions**. Subscribe to Bytecode Alliance security advisories. Update within 48 hours of any CVE.
- **Capability manifests**: Each skill declares what WASI capabilities it needs (filesystem: none, network: specific hosts, etc.). The host only provides requested capabilities. No blanket access.

**Warning signs:**
- Memory usage spikes when running third-party skills.
- Skills that "hang" without clear timeout behavior.
- Wasmtime version is more than 2 minor versions behind latest.
- Skills requesting capabilities they shouldn't need (network access for a formatting tool).

**Phase to address:**
Phase 3 (WASM Skill Sandbox). Must be fully implemented before accepting any third-party skills. The resource limiter, fuel, and capability manifest are the security boundary.

---

### Pitfall 4: Context Window Overflow Causing Silent Quality Degradation

**What goes wrong:**
The agent injects the full context (system prompt + skill descriptions + conversation history + memory retrievals + tool results) without tracking token counts. At first, everything fits in 200K tokens. But after 20+ turns with tool use, the context exceeds the window. Some frameworks silently truncate from the middle, dropping critical context. Modern Claude models return validation errors, but the agent has no recovery strategy. Worse: even when content fits, the "lost-in-the-middle" effect means LLMs underweight information placed in the middle of long contexts, producing unreliable responses.

**Why it happens:**
Developers build the context assembly as string concatenation without a token budget. They test with short conversations (3-5 turns) and never hit limits. The three-zone architecture (static/conditional/dynamic) is designed correctly but implemented without enforcement -- each zone grows without checking if total exceeds budget. Tool outputs (especially from web searches, file reads, or database queries) can be arbitrarily large.

**How to avoid:**
- **Token budget enforcer**: Before every LLM call, calculate total tokens (use tiktoken-rs or cl100k_base tokenizer). Allocate budget: system prompt (fixed, ~2K), skill descriptions (conditional, ~1-4K), conversation history (sliding window, max 8-15K), memory/RAG (conditional, ~2-4K), current turn (dynamic, remainder).
- **Truncation hierarchy**: When over budget, truncate in order: (1) older conversation turns, (2) verbose tool outputs (summarize or truncate to first 2K tokens), (3) low-relevance memory items, (4) non-essential skill descriptions. NEVER truncate system prompt or current user message.
- **Tool output caps**: Hard-limit tool outputs to 4K tokens. If a tool returns more, summarize with a fast model (Haiku) before injecting into context.
- **Context window monitoring**: Log actual token usage per LLM call. Alert when >80% of window is used. Track token usage trends per session.
- **Lost-in-the-middle mitigation**: Place the most important context (system prompt, current user message, most relevant memory) at the beginning and end. Place less critical history in the middle.

**Warning signs:**
- LLM API returns "context length exceeded" errors.
- Agent responses start ignoring earlier conversation context after 15+ turns.
- Agent "forgets" its instructions (system prompt truncated).
- Token costs spike unexpectedly (context growing without bounds).
- Agent hallucinates facts that contradict information in its own context.

**Phase to address:**
Phase 2 (Context Engine). The three-zone architecture with budget enforcement must be the hard boundary. Every piece of context enters through this gate.

---

### Pitfall 5: Prompt Cache Invalidation Destroying Cost Savings

**What goes wrong:**
Anthropic's prompt caching provides 90% read cost reduction ($0.30/M vs $3.00/M tokens) and up to 85% latency reduction. But cache invalidation is prefix-based -- ANY change to content before the cache breakpoint invalidates the entire cache. Developers insert dynamic content (timestamps, user names, session IDs) into the system prompt, breaking cache on every request. At 35K+ tokens per turn, this turns a $50/month agent into a $500/month agent.

**Why it happens:**
The cache works by exact prefix matching: Tools -> System Message -> Messages, in that order. Developers don't understand that the system prompt is part of the prefix, so they embed per-user or per-session content in it. They also don't realize that tool definitions are processed BEFORE the system prompt -- changing tool definitions invalidates the system prompt cache too. Additionally, JSON key ordering is not guaranteed in some serialization paths, breaking cache even when content hasn't changed.

**How to avoid:**
- **Static system prompt**: Zero dynamic content in the system prompt. User-specific context goes in the first user message or a dedicated context message AFTER the cache breakpoint.
- **Stable tool definitions**: Tool definitions are part of the cache prefix. Sort them deterministically. Never add/remove tools per-request.
- **Cache breakpoint placement**: Place `cache_control` on the system message (after tools). This caches both tools AND system prompt. Dynamic content goes in messages AFTER this breakpoint.
- **Deterministic JSON serialization**: Ensure tool schemas use ordered maps (BTreeMap in Rust, not HashMap). Verify with `serde_json::to_string` that output is byte-identical across requests.
- **Monitor cache hit rates**: Track `cache_read_input_tokens` vs `cache_creation_input_tokens` in API responses. Cache hit rate should be >80% for returning users. Alert if it drops below 50%.
- **Minimum token requirements**: Cached prefix must be >= 1,024 tokens (Sonnet/Opus) or >= 2,048 tokens (Haiku). Design the system prompt to meet this threshold.

**Warning signs:**
- `cache_creation_input_tokens` consistently high, `cache_read_input_tokens` consistently zero.
- Cost per turn is 5-10x higher than projected.
- First-turn latency is fine but subsequent turns don't improve (no cache hits).
- System prompt includes anything that changes per-request (time, user ID, session state).

**Phase to address:**
Phase 2 (Context Engine). Cache alignment must be designed into the context assembly pipeline from the start. The three-zone architecture directly supports this: static zone = cached prefix, conditional/dynamic zones = after breakpoint.

---

### Pitfall 6: Telegram Bot Reliability Failures in Always-On Operation

**What goes wrong:**
The bot stops receiving messages after running for 12-48 hours. No errors in logs, no crash -- the long polling connection silently dies and the HTTP client doesn't reconnect. Or: the bot receives duplicate messages after a restart because it didn't properly track the `update_id` offset. Or: Telegram rate limits the bot (30 messages/second to different chats, 1 message/second to same chat) and the bot drops messages without retry.

**Why it happens:**
Teloxide's default HTTP client (reqwest) has connection timeouts, but they may not be configured correctly for multi-day operation (documented in teloxide issue #223). The polling timeout must be LESS than the HTTP client timeout, or the client closes the connection before Telegram responds. On restart, if the last `update_id` offset wasn't persisted, all pending updates replay. Rate limiting requires backpressure-aware message sending, not fire-and-forget.

**How to avoid:**
- **HTTP client configuration**: Use teloxide's `default_reqwest_settings()` as a base. Verify polling timeout (default 10s) is well below HTTP client timeout (default 17s). For long-running bots, explicitly set:
  ```rust
  reqwest::Client::builder()
      .timeout(Duration::from_secs(35))  // > polling timeout + margin
      .connect_timeout(Duration::from_secs(5))
      .tcp_keepalive(Duration::from_secs(60))
      .pool_idle_timeout(Duration::from_secs(90))
  ```
- **Persist update offset**: Store the last processed `update_id` in SQLite. On restart, resume from `last_update_id + 1` to avoid reprocessing.
- **Exponential backoff**: Teloxide has a configurable `backoff_strategy` on `PollingBuilder`. Use exponential backoff for network errors (default), but cap at 30 seconds.
- **Rate limit awareness**: Queue outgoing messages. Respect Telegram's per-chat (1/sec) and global (30/sec) rate limits. Use a token bucket or leaky bucket algorithm.
- **Health monitoring**: If no updates received for >5 minutes AND the bot should be active, force-reconnect. Log connection state transitions.
- **Webhook conflicts**: Ensure no webhook is set when using long polling. Call `delete_webhook()` on startup.

**Warning signs:**
- Bot stops responding but process is alive (check with health endpoint).
- Duplicate message processing after restarts.
- Messages silently dropped during high-activity periods.
- `429 Too Many Requests` errors from Telegram API.
- TCP connection count to api.telegram.org grows over time (connection leak).

**Phase to address:**
Phase 1 (Core Foundation). The Telegram adapter is the primary channel at launch. Its reliability IS the product reliability. Connection handling, offset persistence, and rate limiting must be rock-solid from day one.

---

### Pitfall 7: Async Cancellation Safety Violations Corrupting State

**What goes wrong:**
A tokio `select!` branch cancels a future that was mid-way through a multi-step operation (e.g., writing to database then updating in-memory state). The database write succeeds but the in-memory state update never happens, leaving the system in an inconsistent state. This is especially dangerous in the agent loop where `select!` is used to race timeout, shutdown signal, incoming message, and heartbeat timer.

**Why it happens:**
In Rust async, dropping a future cancels it at the last `.await` point. Unlike Go's context cancellation, there's no cleanup callback -- the future just stops. If the future was between two `.await` points that must both complete (database write + cache update), the second never executes. Most tokio APIs document cancellation safety, but application code rarely considers it.

**How to avoid:**
- **Make critical operations atomic**: Wrap multi-step operations in `tokio::task::spawn` (which is NOT cancelled by `select!` dropping the JoinHandle -- the task runs to completion). Use `JoinHandle::abort()` only when you truly want cancellation.
- **Use `tokio::select!` carefully**: Only race futures that are cancellation-safe. Annotate `select!` arms with comments explaining cancellation behavior.
- **Transactional state updates**: All state changes go through the single-writer database thread. If the database write succeeds, the state is committed. In-memory caches rebuild from database state on startup.
- **`CancellationToken` for cooperative shutdown**: Instead of dropping futures, signal cancellation via `tokio_util::sync::CancellationToken`. Each component checks the token at safe points.
- **`TaskGroup`/`JoinSet` for structured concurrency**: Group related tasks. If one fails, cancel the rest. When the group is dropped, all tasks are cancelled.

**Warning signs:**
- In-memory state diverges from database state after running for hours.
- "Ghost" sessions that exist in memory but not in database (or vice versa).
- Intermittent test failures that only happen under load (race conditions).
- Agent loop occasionally skips heartbeats or double-processes messages.

**Phase to address:**
Phase 1 (Core Foundation). The agent loop's `select!` pattern and shutdown flow must be designed for cancellation safety from the start. This is not something you can retrofit.

---

### Pitfall 8: LLM Provider Abstraction That Leaks or Over-Abstracts

**What goes wrong:**
The provider abstraction trait either: (a) is so generic it can't express provider-specific features (Anthropic's cache_control, tool_use with thinking, extended thinking blocks, model routing), or (b) exposes a lowest-common-denominator API that makes every provider equally bad. The abstraction becomes a maintenance burden that slows down adopting new features from the primary provider (Anthropic).

**Why it happens:**
Developers design the provider trait for Day 100 (supporting 5 providers) instead of Day 1 (Anthropic only). They abstract too early, before understanding the actual variance between providers. OpenAI and Anthropic have materially different streaming formats (SSE with different event types), tool calling conventions, cache mechanics, and error handling. A trait that papers over these differences either leaks abstractions or loses capabilities.

**How to avoid:**
- **Anthropic-first, abstract-later**: Build the Anthropic client directly. Make it excellent -- streaming, tool use, cache_control, extended thinking, model routing (Haiku/Sonnet/Opus). Only then extract a trait from what's actually common.
- **Two-tier trait design**: Core trait covers the universal: `send_message(prompt, tools) -> Stream<Response>`. Provider-specific features use extension traits or type-erased metadata bags.
- **Streaming as the primitive**: The trait must return a `Stream<Item = StreamEvent>` (or async iterator), not a completed response. Every consumer works with streaming. Non-streaming is just collecting the stream.
- **Error types per provider**: Don't flatten all errors into a generic enum. Use `anyhow::Error` or a trait-object error type that preserves provider-specific error details (rate limits, overloaded, context too long).
- **Model routing is application logic, not provider logic**: The router selects which provider+model to call based on query complexity. It lives above the provider trait, not inside it.

**Warning signs:**
- New Anthropic feature (e.g., extended thinking) can't be exposed without trait changes.
- Streaming implementation has provider-specific branches in the "generic" code.
- Error handling loses information (can't distinguish rate limit from context overflow).
- Every new provider requires changes to the core trait.

**Phase to address:**
Phase 1 (Core Foundation). Build Anthropic client. Phase 2 (Context Engine) extracts the trait when building model routing. Phase 4+ adds other providers against the now-validated abstraction.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| `unwrap()` on database operations | Faster prototyping | Silent panics in production crash the agent | Never in production paths. Use `expect()` with context during Phase 1 prototyping only, replace with proper error handling before Phase 1 exits. |
| HashMap for tool definitions in API requests | Convenient serde | Non-deterministic JSON key ordering breaks prompt cache, costing 10x more per request | Never. Use BTreeMap from day one for any data that becomes part of an LLM API request. |
| In-memory session state without DB backing | Faster iteration | All sessions lost on restart/crash. State divergence if both exist. | Phase 1 only, if sessions are explicitly ephemeral. Must have DB backing before Phase 2. |
| String-typed configuration | Avoid config parsing complexity | Runtime errors from typos, no validation, no documentation | Never. TOML + serde with `deny_unknown_fields` from day one (already planned). |
| Single SQLite connection (no writer separation) | Simpler initial code | SQLITE_BUSY errors at >5 concurrent sessions, requiring full DB layer rewrite | Phase 1 MVP only if testing with 1-2 sessions. Must implement single-writer before multi-session testing. |
| Unbounded channels between components | No backpressure design needed | Memory grows without bound under load. OOM after hours of high traffic. | Never. Every channel must be bounded. Capacity = expected burst size (e.g., 256 for message queue, 64 for DB write queue). |
| `Box<dyn Error>` everywhere | Avoid designing error types | Impossible to match on specific errors (rate limit vs auth failure). Can't retry appropriately. | Phase 1 only. Define domain error types (AgentError, ProviderError, StorageError) in Phase 2. |
| Skipping WAL checkpoint management | "SQLite handles it" | WAL file grows to 500MB+, degrading read performance and consuming disk. | Never. Periodic TRUNCATE checkpoint from writer thread, with monitoring. |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| Anthropic API (streaming) | Collecting entire response before processing, losing streaming latency benefit. Not handling `overloaded` (529) errors with backoff. | Process `StreamEvent`s as they arrive. Implement exponential backoff (start 1s, max 60s) for 429/529 errors. Set `max_tokens` appropriately -- too high wastes budget, too low truncates responses. |
| Anthropic API (cache_control) | Placing `cache_control` breakpoints on dynamic content. Using >4 breakpoints (API limit). Not checking minimum token threshold (1024/2048). | Place breakpoints on stable content only (tools block end, system message end). Monitor `cache_read_input_tokens` in response `usage`. Ensure cached prefix meets minimum token count for the model. |
| Telegram Bot API | Running multiple bot instances with the same token (only one can poll). Not deleting webhook before starting long polling. Sending messages without respecting rate limits. | Single instance per token. Call `deleteWebhook` on startup. Implement outgoing message queue with rate limiting (30 msg/s global, 1 msg/s per chat). Persist `update_id` offset in database. |
| SQLite + WAL mode | Opening database without setting PRAGMA configuration. Using default busy_timeout (0ms). Not handling SQLITE_BUSY errors. | Run all PRAGMAs immediately after connection open, in a single transaction. Set busy_timeout >= 5000ms. Handle SQLITE_BUSY with application-level retry. |
| wasmtime (skill execution) | Running WASM without fuel or memory limits. Not handling panics from WASI host functions. Reusing Store across skill invocations (state leak). | Create fresh Store per skill invocation. Configure fuel, memory limits, and epoch interruption. Catch all Results from WASM calls -- never unwrap. Drop Store after execution to release resources. |
| reqwest (HTTP client) | Creating a new Client per request (expensive: TLS session setup, connection pool reset). Not setting timeouts on all operations. | Create ONE reqwest::Client at startup, share via Arc. Set connect_timeout, timeout, pool_idle_timeout, pool_max_idle_per_host. Reuse for all HTTP calls (Anthropic API, Telegram API, webhook delivery). |
| jemalloc (allocator) | Using system allocator, then investigating OOM issues without profiling capability. Not enabling profiling in production builds. | Use tikv-jemallocator from day one. Enable `profiling` feature behind a cargo feature flag. In production, set `MALLOC_CONF=prof:true,lg_prof_sample:19` to profile with <1% overhead. |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Tokenizer allocation per LLM call | 10-50ms overhead per context assembly as tiktoken re-initializes | Initialize tokenizer ONCE at startup, share via Arc. Tokenize incrementally (only new content). | Noticeable at >10 messages/second; adds up to seconds of latency under burst load. |
| Unbounded conversation history in context | Token costs grow linearly with conversation length. 50-turn conversation = 50K+ tokens per turn. | Sliding window (keep last N turns). Summarize older turns with Haiku when window shifts. Hard cap at 15K tokens for history zone. | After ~20 turns, each response costs 3-5x what it should. After ~50 turns, hits context window limit. |
| WASM module compilation on every invocation | 50-200ms per skill call for module compilation | Pre-compile modules to native code at install time (`Module::serialize`). Cache compiled modules in memory (LRU, max 50). Load from disk on cache miss. | First skill call is slow (acceptable). Every skill call is slow (not acceptable). Matters at >5 skill calls per minute. |
| Full table scan on session/memory lookups | Database queries slow as data grows. 100ms+ for memory search across 10K entries. | Create indexes on all frequently queried columns (session_id, created_at, embedding vector). Use EXPLAIN QUERY PLAN during development. | At ~1K sessions or ~10K memory entries. Degrades linearly without indexes. |
| Synchronous embedding inference blocking agent loop | Agent hangs for 50-200ms during embedding generation (memory search, skill matching) | Run ONNX inference on dedicated thread(s) via spawn_blocking or a custom thread pool. Batch embedding requests when possible. | Immediately noticeable when embedding is in the hot path (every message that triggers memory search). |
| WAL file growth from unchecked read transactions | Readers hold WAL pages open, preventing checkpoint. WAL grows to 100MB+, degrading read performance. | Bound read transaction duration (<100ms). Run periodic TRUNCATE checkpoints. Monitor WAL file size. Close idle read connections. | At sustained write load of >100 writes/minute with concurrent long-running reads. |
| Clone-heavy context assembly | Cloning large strings (system prompt, tool definitions) on every LLM call | Use `Arc<str>` for static content. Build context via references (`&str`) and only allocate the final assembled string once. | At >50 messages/second. Memory allocation overhead visible in jemalloc profiles. |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| WASM skills with unrestricted WASI capabilities | A skill meant to format text gets filesystem and network access. Malicious skill exfiltrates database, credentials, or sends data to external servers. | Capability manifest per skill (declare what you need). Host grants only requested capabilities. No filesystem access by default. Network access only to declared hosts via allowlist. Review capabilities during `blufio skill install`. |
| Credential vault key derived from weak source | AES-256-GCM is useless if the key derivation uses a predictable seed (hostname, MAC address, hardcoded salt). Attacker who gets the database can derive the key. | Use Argon2id KDF with high-memory parameters (64MB, 3 iterations). Key derived from user-provided passphrase set during `blufio init`. Salt stored alongside encrypted data but is random (32 bytes from CSPRNG). |
| Plaintext API keys in TOML config | Config file checked into git, visible to any process on the machine, shows up in error logs and core dumps. | All secrets go into the encrypted credential vault, not TOML. Config file references vault entries by name. Vault is encrypted at rest (AES-256-GCM). CLI command `blufio config set --secret` writes to vault. |
| LLM prompt injection via user input | User crafts message that overrides system prompt instructions, changes agent behavior, or extracts system prompt content. | System prompt in a separate API parameter (not concatenated in user message). Input sanitization for known injection patterns. Rate limiting on per-user basis. Monitor for anomalous tool invocations. Use Anthropic's system prompt isolation features. |
| Ed25519 key generation with insufficient entropy | Inter-agent signed messages can be forged if keypairs are generated from a weak RNG. | Use `ring` or `ed25519-dalek` with OS-provided CSPRNG (`OsRng`). Never use `thread_rng()` for cryptographic key generation. Verify key generation in integration tests by checking key uniqueness across runs. |
| SQLCipher with default KDF iterations | Default KDF iterations (256K for SQLCipher 4) may be reduced for "performance" during development and never restored. Attacker with database file can brute-force the key. | Set KDF iterations explicitly in code, not in a config that can be accidentally changed. Use `PRAGMA kdf_iter = 256000` minimum. Log the configured value at startup for audit. |
| Unbounded skill output injected into LLM context | A skill returns 100KB of output that is injected verbatim into the next LLM call, either overflowing the context window or displacing critical context (system prompt, instructions). | Hard cap skill output at 4096 tokens. Truncate or summarize (via Haiku) anything exceeding the cap BEFORE injection into context. Log truncation events. |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| No feedback during LLM processing | User sends message, waits 5-30 seconds with no indication the bot is working. Resends message, creating duplicates. | Send Telegram "typing..." indicator immediately on message receipt. If LLM takes >3s, send a brief "thinking..." message. Stream partial responses for long answers. |
| Cryptic error messages on LLM failures | "Error: request failed" when the real issue is rate limiting, context overflow, or invalid API key. User can't help diagnose. | Map provider errors to user-friendly messages: "I'm getting a lot of requests right now, I'll try again in a moment" (rate limit), "This conversation is getting long, I might miss some earlier details" (context near limit). Log detailed error internally. |
| Agent forgets context mid-conversation | After 20+ turns, agent contradicts earlier statements or forgets user preferences. User loses trust. | Implement explicit memory: summarize and store key facts from conversation. Display memory state on request (`/memory`). Warn user when conversation is approaching context limits. |
| Skill installation without progress feedback | `blufio skill install` downloads, compiles, and validates WASM with no output for 10+ seconds. User thinks it's hung. | Progress indicators for each step: downloading -> verifying signature -> compiling -> validating capabilities -> installed. Show estimated time for compilation step. |
| Configuration errors discovered at runtime | User edits TOML config, restarts agent, gets a panic 30 seconds later because a field is wrong. | `blufio doctor` validates configuration before starting. `blufio config validate` checks config file without starting the agent. Show exact line and field that's wrong, with suggestion. |

## "Looks Done But Isn't" Checklist

- [ ] **Agent loop**: Often missing graceful shutdown on SIGTERM -- verify agent completes in-flight LLM call and persists session state before exit
- [ ] **Telegram adapter**: Often missing update_id offset persistence -- verify bot resumes correctly after restart without replaying old messages
- [ ] **SQLite persistence**: Often missing WAL checkpoint management -- verify WAL file size stays bounded under sustained write load over 24+ hours
- [ ] **WASM sandbox**: Often missing resource limits -- verify a `loop {}` skill gets killed (fuel exhaustion) instead of hanging the agent
- [ ] **Prompt caching**: Often missing cache hit monitoring -- verify `cache_read_input_tokens > 0` in API responses for repeat interactions
- [ ] **Cost tracking**: Often missing edge cases -- verify cost ledger accounts for cache writes (2x base cost), retries, and failed requests
- [ ] **Context engine**: Often missing token budget enforcement -- verify context assembly respects budget even with large tool outputs
- [ ] **Error handling**: Often missing retry logic -- verify transient API errors (429, 529, 5xx) trigger exponential backoff, not immediate failure
- [ ] **Health checks**: Often missing liveness vs readiness distinction -- verify health endpoint distinguishes "process alive" from "agent loop functioning"
- [ ] **Credential vault**: Often missing key rotation -- verify vault supports re-encryption with a new passphrase without data loss
- [ ] **Multi-session**: Often missing session cleanup -- verify completed/abandoned sessions are cleaned up and don't leak memory or database entries
- [ ] **Metrics export**: Often missing histogram buckets -- verify Prometheus metrics have appropriate bucket boundaries for LLM latency (100ms to 60s range)

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Blocking async runtime (Pitfall 1) | MEDIUM | Migrate to tokio-rusqlite/deadpool-sqlite for DB. Wrap all CPU-bound ops in spawn_blocking. Requires touching every DB callsite but patterns are mechanical. |
| SQLite writer contention (Pitfall 2) | HIGH | Rewrite DB layer to single-writer channel pattern. All callers must switch from direct connection to channel-based API. May require schema changes for write batching. |
| WASM resource exhaustion (Pitfall 3) | LOW | Add StoreLimits and fuel configuration to existing WASM host. No API changes needed -- just configuration at Store creation. |
| Context window overflow (Pitfall 4) | MEDIUM | Add token counting and budget enforcement to context assembly pipeline. Requires refactoring context builder but doesn't change external API. |
| Prompt cache invalidation (Pitfall 5) | LOW-MEDIUM | Restructure system prompt to be static. Move dynamic content after cache breakpoint. Ensure deterministic serialization. Mostly configuration, some refactoring. |
| Telegram reliability (Pitfall 6) | LOW | Add HTTP client configuration, offset persistence, and rate limiting. Additive changes to existing adapter code. |
| Cancellation safety (Pitfall 7) | HIGH | Audit every select! and spawn. Restructure multi-step operations to be atomic. May require architectural changes to the agent loop. Very hard to retrofit. |
| Provider over-abstraction (Pitfall 8) | HIGH | If abstracted too early, must either break the abstraction or duplicate code. Better to delay abstraction. If already locked in, add extension trait escape hatches. |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Blocking async runtime | Phase 1: Core Foundation | tokio-console shows no tasks blocked >5ms. Load test with 10 concurrent sessions shows no latency spikes. |
| SQLite writer contention | Phase 1: Core Foundation | Stress test: 50 concurrent write operations complete without SQLITE_BUSY. WAL file stays <10MB. |
| WASM resource exhaustion | Phase 3: Skill Sandbox | Test skill with `loop {}` terminates within 100ms. Test skill allocating 1GB memory gets OOM error. Test skill with excessive host calls gets fuel error. |
| Context window overflow | Phase 2: Context Engine | 100-turn conversation stays within token budget. Tool output >4K tokens is truncated. Token usage logged and stays <80% of model limit. |
| Prompt cache invalidation | Phase 2: Context Engine | Cache hit rate >80% for returning users (measured from API response). Cost per turn matches projected rate. |
| Telegram reliability | Phase 1: Core Foundation | Bot runs 72+ hours without missing messages. Restart correctly resumes from last offset. Rate-limited burst of 100 messages is queued and delivered. |
| Cancellation safety | Phase 1: Core Foundation | Agent shutdown completes in-flight request. Database state consistent after forced shutdown. No "ghost" sessions after restart. |
| Provider over-abstraction | Phase 1-2: Foundation + Context | All Anthropic features (streaming, cache_control, tools, thinking) accessible. Adding a second provider doesn't change core trait. |

## Sources

- [Tokio async pitfalls and cancellation safety](https://blog.jetbrains.com/rust/2026/02/17/the-evolution-of-async-rust-from-tokio-to-high-level-applications/) -- JetBrains Rust blog (2026)
- [Async Rust cancellation safety deep dive](https://rfd.shared.oxide.computer/rfd/400) -- Oxide Computer RFD 400
- [Async Rust backpressure and concurrency](https://biriukov.dev/docs/async-rust-tokio-io/1-async-rust-with-tokio-io-streams-backpressure-concurrency-and-ergonomics/) -- Viacheslav Biriukov
- [Wasmtime security documentation](https://docs.wasmtime.dev/security.html) -- Bytecode Alliance (official)
- [Wasmtime security and correctness](https://bytecodealliance.org/articles/security-and-correctness-in-wasmtime) -- Bytecode Alliance (official)
- [WASM sandbox escape vectors](https://instatunnel.my/blog/the-wasm-breach-escaping-backend-webassembly-sandboxes) -- InstaTunnel (2026)
- [CVE-2025-53901: wasmtime fd_renumber panic](https://cve.imfht.com/detail/CVE-2025-53901)
- [CVE-2026-27572: wasmtime header overflow](https://cvereports.com/reports/CVE-2026-27572)
- [Wasmtime resource limiter API](https://docs.rs/wasmtime/latest/wasmtime/struct.Store.html) -- Context7 / docs.rs (HIGH confidence)
- [SQLite WAL concurrency and locking](https://sqlite.org/lockingv3.html) -- SQLite official
- [tokio-rusqlite: async SQLite wrapper](https://docs.rs/tokio-rusqlite) -- docs.rs (HIGH confidence)
- [rusqlite transactions with async/await issue #697](https://github.com/rusqlite/rusqlite/issues/697) -- GitHub
- [Rust plugin system: WASM vs dynamic loading](https://nullderef.com/blog/plugin-dynload/) -- NullDeref
- [WASM plugin architecture lessons learned](https://blog.anirudha.dev/wasmrun-plugin-architecture/) -- Anirudha (2025)
- [Anthropic prompt caching documentation](https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching) -- Anthropic (official, HIGH confidence)
- [Prompt caching cost optimization guide](https://promptbuilder.cc/blog/prompt-caching-token-economics-2025) -- PromptBuilder (2025)
- [Context engineering best practices from Anthropic](https://01.me/en/2025/12/context-engineering-from-claude/) -- Bojie Li (2025)
- [Context window overflow solutions](https://arxiv.org/html/2511.22729v1) -- arXiv (2025)
- [Context window management strategies](https://www.getmaxim.ai/articles/context-window-management-strategies-for-long-context-ai-agents-and-chatbots/) -- Maxim AI
- [Why AI agents fail: architecture not models](https://refreshagent.com/engineering/building-ai-agents-in-rust) -- Refresh Agent
- [AI agent state management failures](https://sderosiaux.medium.com/why-your-ai-agents-keep-failing-not-the-models-fault-dfa4de38a2b0) -- Medium (2025)
- [Teloxide polling configuration](https://docs.rs/teloxide/0.17.0/teloxide/update_listeners/struct.PollingBuilder.html) -- Context7 / docs.rs (HIGH confidence)
- [Teloxide default HTTP client settings](https://docs.rs/teloxide/0.17.0/teloxide/net/fn.default_reqwest_settings.html) -- Context7 / docs.rs (HIGH confidence)
- [Telegram bot long polling best practices](https://grammy.dev/guide/deployment-types) -- grammY docs
- [Telegram bot API FAQ](https://core.telegram.org/bots/faq) -- Telegram (official)
- [Rust jemalloc heap profiling](https://magiroux.com/rust-jemalloc-profiling) -- XuoriG
- [GreptimeDB Rust memory leak diagnosis](https://greptime.com/blogs/2023-06-15-rust-memory-leaks) -- Greptime
- [Tokio spawn_blocking documentation](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html) -- Context7 / docs.rs (HIGH confidence)
- [Tokio graceful shutdown guide](https://tokio.rs/tokio/topics/shutdown) -- Tokio (official)
- [Rust musl cross-compilation gotchas](https://john-millikin.com/notes-on-cross-compiling-rust) -- John Millikin

---
*Pitfalls research for: Rust AI agent platform (Blufio)*
*Researched: 2026-02-28*
