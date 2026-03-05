# Pitfalls Research: v1.3 Ecosystem Expansion

**Domain:** Adding OpenAI-compatible API, multi-provider LLM, multi-channel adapters, Docker, event bus, skill registry, node system, and migration tooling to existing Rust AI agent platform (39K LOC, 21 crates)
**Researched:** 2026-03-05
**Confidence:** HIGH for Rust/axum/SQLite specifics (verified against existing codebase); MEDIUM for provider format differences (official docs + community); MEDIUM for channel platform specifics (official rate limit docs + known issues)

---

## Critical Pitfalls

Mistakes that cause rewrites, data loss, broken integrations, or security incidents.

---

### Pitfall 1: OpenAI stop_reason vs finish_reason Field Name Mismatch Breaks Every Client

**What goes wrong:**
The existing Blufio `ProviderResponse` type uses `stop_reason: Option<String>` (Anthropic's field name). The OpenAI `/v1/chat/completions` specification uses `finish_reason` (not `stop_reason`) in the response body. When implementing the OpenAI-compatible endpoint, developers copy-paste the existing response struct and rename it, but forget to change the JSON field name — so clients receive `stop_reason` where they expect `finish_reason`. Every OpenAI-compatible client that uses the stop condition (LangChain, LlamaIndex, OpenWebUI, most SDKs) silently treats responses as incomplete or loops forever.

Additionally, the OpenAI spec uses `"tool_calls"` as the stop reason string value while Anthropic uses `"tool_use"`. An OpenAI-compatible client sending a request expecting `finish_reason: "tool_calls"` will receive `finish_reason: "tool_use"` from a naively proxied Anthropic response, causing infinite retries or silent tool-call drops.

**Why it happens:**
The existing `ProviderResponse` and `ProviderStreamChunk` types in `blufio-core/src/types.rs` are designed around Anthropic's format. When adding the OpenAI shim layer in `blufio-gateway`, developers reuse these types rather than defining a separate OpenAI wire format, leaking Anthropic field names into the external API surface.

**How to avoid:**
Define completely separate wire types for the OpenAI-compatible response in the gateway crate. Never expose internal `ProviderResponse` directly — always translate through an explicit `OpenAiChatResponse` struct with `#[serde(rename = "finish_reason")]` and value mapping: `"tool_use"` → `"tool_calls"`, `"end_turn"` → `"stop"`, `"max_tokens"` → `"length"`. Write a test that sends an OpenAI-format request and asserts the response contains `finish_reason` (not `stop_reason`).

**Warning signs:**
- Client SDKs loop or hang after receiving a response
- Tool invocations never trigger even when LLM requests them
- `grep "stop_reason"` appears in OpenAI response JSON in integration tests

**Phase to address:**
OpenAI-compatible API layer phase (first phase of v1.3). Must be defined before any provider is wired up.

---

### Pitfall 2: Ollama Streaming + Tool Calls Silently Drops Tool Invocations

**What goes wrong:**
Ollama's OpenAI compatibility layer (`/v1/chat/completions` with `stream: true`) does not correctly forward tool call data when streaming is enabled. The model decides to call a tool, but the streaming response returns an empty content block with `finish_reason: "stop"` — the tool call is silently lost. This was confirmed as an active issue in Ollama's GitHub tracker (issue #12557). Blufio always streams by default for latency reasons. When the Ollama provider is wired with stream enabled, tool-augmented conversations stop working without any error surfaced to the user.

**Why it happens:**
Ollama's OpenAI compatibility layer is partial. The native Ollama API (`/api/chat`) supports streaming + tool calling since May 2025, but the OpenAI shim does not. Developers assume "OpenAI-compatible" means fully compatible. It does not.

**How to avoid:**
For the Ollama provider crate (`blufio-ollama`), use Ollama's native API (`/api/chat`) rather than the OpenAI shim. Implement a separate response parser for Ollama's native format. Add a specific integration test that verifies tool calls are received when `stream: true` — this will fail immediately if you accidentally use the shim endpoint. Document clearly in code comments that the native endpoint must be used.

**Warning signs:**
- Agent responds as if it received no tool results when Ollama is active provider
- No `ToolUse` events appear in `ProviderStreamChunk` stream with Ollama
- Provider health check passes but tool invocations never execute

**Phase to address:**
Multi-provider LLM phase. Ollama crate must explicitly test tool calling with streaming before being considered complete.

---

### Pitfall 3: Provider Trait Tool Format Leaks Anthropic-Specific Structure

**What goes wrong:**
The existing `ProviderRequest.tools` field is `Option<Vec<serde_json::Value>>` with the comment "Anthropic format." When implementing OpenAI, Ollama, or Gemini providers, developers pass these Anthropic-format tool definitions through unchanged, or attempt to serialize them and hit provider-specific schema rejections. Gemini's native API requires `FunctionDeclaration` objects with a different schema shape. OpenAI requires `{"type": "function", "function": {...}}` wrapping. Anthropic uses bare `{"name": ..., "description": ..., "input_schema": ...}` without the `"function"` key wrapper.

**Why it happens:**
The `ProviderRequest` type was designed for Anthropic. The `tools` field as `serde_json::Value` appears flexible, but each provider requires a completely different JSON schema for tool definitions. Developers assume "it's just JSON, it'll work" and test only with Anthropic-style definitions.

**How to avoid:**
Define a provider-agnostic `ToolDefinition` type in `blufio-core` with fields: `name: String`, `description: String`, `parameters: serde_json::Value` (JSON Schema). Each provider adapter crate implements its own serialization to the provider's wire format. The Anthropic adapter wraps it in `{input_schema: ...}`, the OpenAI adapter in `{type: "function", function: {name, description, parameters}}`, the Gemini adapter in `{functionDeclarations: [{name, description, parameters}]}`. Add a test in each provider crate asserting the wire format matches the provider's documented spec.

**Warning signs:**
- Provider returns 400 Bad Request when tools are passed
- Tool calls work with Anthropic but not OpenAI or Gemini provider
- JSON deserialization errors on provider responses mentioning unknown fields

**Phase to address:**
Multi-provider LLM phase. Must fix the trait before implementing any non-Anthropic provider.

---

### Pitfall 4: SQLite Single-Writer Becomes Latency Bottleneck Under Concurrent API Load

**What goes wrong:**
The existing system uses `tokio-rusqlite` with a dedicated single-writer thread — correct for the current single-channel, sequential conversation model. When the OpenAI-compatible API is exposed, callers can send concurrent requests (multiple clients, batch operations, webhook triggers). Each API request needs to write session/message data. All writes queue behind the single writer. At 5-10 concurrent requests, P99 latency climbs above the 120-second gateway timeout. The system appears to work in testing (sequential requests) but fails under real concurrent load.

**Why it happens:**
The single-writer pattern is correct for SQLite WAL mode and was a deliberate decision. The failure is not the pattern itself, but the assumption that it scales to concurrent write loads introduced by an API layer. The gateway currently has a 120s response timeout; if writes queue, LLM calls succeed but response routing times out, leaving orphaned oneshot channels in `response_map`.

**How to avoid:**
Add a write queue depth Prometheus gauge metric. Add a write latency histogram. Set an alert threshold at 50% queue depth. Audit all database write paths triggered by API requests and identify which can be deferred (async queue rather than synchronous write). For example: cost tracking writes can be batched and written every N seconds rather than per-request. Session creation writes are on the critical path and cannot be deferred — keep them fast by minimizing schema joins.

**Warning signs:**
- P99 latency grows with concurrent client count
- `response_map` accumulates entries that never get consumed (memory leak symptom)
- Gateway timeout errors appear under load but not sequential testing

**Phase to address:**
OpenAI-compatible API layer phase (concurrent request testing must be in acceptance criteria) and event bus phase (event bus should drain writes asynchronously where possible).

---

### Pitfall 5: Event Bus Broadcast Receiver Lag Causes Silent Message Loss

**What goes wrong:**
`tokio::sync::broadcast` is the natural choice for an internal event bus — it supports multiple receivers and is already in the dependency tree. However, if any receiver is slow (e.g., a metrics aggregator blocked on a Prometheus scrape, or a webhook dispatcher waiting for an HTTP round-trip), the broadcast channel's ring buffer fills. Tokio's broadcast channel returns `RecvError::Lagged` — not an error that panics or logs visibly by default. The receiver that lagged misses events silently. If the webhook dispatcher misses 50 events and never logs, operators have no idea until they notice missing webhooks.

**Why it happens:**
`tokio::sync::broadcast` trades reliability for low latency. It does not block senders when receivers lag — it drops the oldest messages and records a lag count. New Rust developers familiar with mpsc channels (which apply backpressure) assume broadcast does the same. It does not.

**How to avoid:**
Treat `RecvError::Lagged(n)` as an observable error — log it as `tracing::warn!` with the receiver name and lag count, and emit a Prometheus counter. Set broadcast channel capacity generously (1024+ for event bus). For receivers that must not miss events (webhook dispatcher, audit log), use mpsc instead: the event bus sends to a dedicated mpsc channel per critical subscriber, maintaining per-subscriber backpressure. Use broadcast only for fire-and-forget subscribers (metrics, debug logging).

**Warning signs:**
- Webhook delivery counts don't match published event counts
- No errors logged but downstream systems report gaps
- `RecvError::Lagged` appears in traces only when searching explicitly

**Phase to address:**
Event bus phase. Must be designed with lag handling from day one — retrofitting is harder than preventing.

---

### Pitfall 6: WhatsApp Unofficial Library Results in Account Ban

**What goes wrong:**
Implementing a WhatsApp adapter using an unofficial Rust library (reverse-engineered protocol) appears to work in development. In production, Meta's anti-abuse systems detect the non-standard client within days to weeks and permanently ban the phone number. The number loses WhatsApp access entirely. There is no appeal process for numbers banned for TOS violations. Operators who invested in a WhatsApp-first deployment lose their business number.

**Why it happens:**
Meta aggressively monitors for unofficial clients in 2025. The WhatsApp Business API requires going through a BSP (Business Solution Provider) or official Meta Tech Partner. Unofficial libraries like `whatsapp-web.rs` or `baileys` ports mimic the WhatsApp Web protocol and are detectable. The risk is categorically different from Telegram (open Bot API) or Discord (open developer API).

**How to avoid:**
Use only the official WhatsApp Business API (`graph.facebook.com/v22.0/{phone-number-id}/messages`). This requires Meta Business account, WhatsApp Business App setup, and BSP approval. Accept that unverified business portfolios start at 250 messages/24h. Do not ship a WhatsApp adapter using unofficial libraries even for local testing — the ban risk applies to any number used. Document the official API requirement prominently in the adapter's README and in the TOML config validation.

**Warning signs:**
- Library uses WebSocket to `web.whatsapp.com` rather than `graph.facebook.com`
- Library documentation mentions "multi-device" session scanning
- No mention of official Meta developer approval in the library's documentation

**Phase to address:**
Multi-channel adapter phase (Discord/WhatsApp/Slack). WhatsApp must use official API or be deferred to a future milestone when official API integration is resourced.

---

### Pitfall 7: Discord Message Content Intent Not Requested Silently Breaks Text Reception

**What goes wrong:**
Discord bots created after April 2022 must explicitly request the `MESSAGE_CONTENT` privileged intent to read message content in servers. Without this intent, `message.content` is always an empty string for messages not directed at the bot via mention or DM. A Blufio Discord adapter that does not request this intent appears to work in DMs (bots always see DM content) but silently receives empty strings for all server messages. Operators report "bot not responding" without any error in logs.

**Why it happens:**
The intent is not enabled by default in the Discord Developer Portal, and many Discord library wrappers don't fail loudly when content is empty — they simply process an empty string as if it were a valid message. Developers test in DMs, find it working, and only discover the problem in production server deployments.

**How to avoid:**
Enable `MESSAGE_CONTENT` in the Discord Developer Portal at bot creation time. In the adapter code, assert the intent is present in the gateway connection, and add a startup check: if the bot is in any guild and receives a non-DM message with empty content, emit a `tracing::error!` with explicit guidance to enable the intent. Add a Discord-specific integration test that verifies content is received from guild messages.

**Warning signs:**
- Bot responds to DMs but not server messages
- Incoming messages always have empty content in logs
- No errors — just empty string processing

**Phase to address:**
Multi-channel adapter phase. This check must be in the adapter's `connect()` implementation.

---

### Pitfall 8: Slack Rate Limit Tier Mismatch Causes Invisible Throttling

**What goes wrong:**
Slack's Web API has tiered rate limits per method. As of May 2025, `conversations.history` and `conversations.replies` are limited to 1 request/minute for non-Marketplace apps. A Blufio Slack adapter that polls these endpoints for conversation context hits the limit within seconds of startup, causing subsequent calls to return HTTP 429. If the adapter treats 429 as a transient error and retries without exponential backoff + jitter, it enters a retry storm that worsens rate limiting. Messages are delayed or dropped. Operators see the bot responding but slowly and incompletely.

**Why it happens:**
Most developers test Slack adapters with lightweight usage that doesn't trigger rate limits. The 1 req/min limit for conversation history feels impossibly low until you have multiple active sessions all trying to load context simultaneously.

**How to avoid:**
Design the Slack adapter to use push-based event delivery (Slack Events API or Socket Mode) rather than polling. Cache conversation history locally in SQLite and only fetch from Slack on cache miss. Implement per-method rate limit tracking using the `Retry-After` header Slack returns on 429. Add a `slack_api_calls` Prometheus counter with method label so operators can see approach to limits.

**Warning signs:**
- HTTP 429 responses in Slack adapter logs
- Message processing latency grows over the session lifetime
- Bot falls behind in responding to busy channels

**Phase to address:**
Multi-channel adapter phase (Slack specifically). Rate limit strategy must be in the adapter design, not retrofitted.

---

### Pitfall 9: Signal Adapter Has No Stable Rust Library

**What goes wrong:**
Signal has no official bot API. All Rust Signal integrations rely on reverse-engineered protocols or brittle bridges to `signal-cli` (a Java process). Signal protocol updates can break all unofficial clients simultaneously with no warning. A `signal-cli` subprocess dependency violates the single-binary constraint and introduces a Java runtime dependency. Matrix bridges exist but require a running Matrix homeserver.

**Why it happens:**
Signal is not a developer-friendly platform. The organization's position is that bots and automation are not supported use cases, and they actively resist unofficial API access.

**How to avoid:**
Do not implement a direct Signal adapter for v1.3. The only viable path is through `signal-cli` as a subprocess (Java dependency, violates single-binary) or a Matrix bridge (homeserver dependency). Defer Signal to a future milestone when/if an official API exists, or implement as a plugin that accepts the subprocess dependency explicitly. Document this as an ecosystem gap in the v1.3 release notes.

**Warning signs:**
- Any Rust Signal crate that hasn't been updated in 3+ months
- Library documentation mentions running `signal-cli` or registering a phone number via a third-party relay
- No official Signal developer documentation referenced

**Phase to address:**
Multi-channel adapter phase. Signal should be removed from v1.3 scope or explicitly scoped as `signal-cli` bridge with documented Java dependency.

---

### Pitfall 10: Skill Registry Code Signing Does Not Prevent Capability Escalation

**What goes wrong:**
Ed25519 signatures verify that a skill package was signed by a known key — they do not verify that the skill's manifest accurately describes its actual capabilities. A malicious skill author signs a WASM module that calls host functions not declared in its capability manifest. If the WASM sandbox only checks declared capabilities at install time rather than at each host function call, the skill bypasses the sandbox. The existing `wasmtime` sandbox with fuel/memory/epoch limits is correct, but the host function dispatch must validate the calling skill's declared capabilities on every call.

**Why it happens:**
Signature verification is conflated with capability verification. Developers implement signing as a separate pre-install step and assume that a signed skill is trusted to run with its declared capabilities unchecked. The attack vector is: publish v1.0 with minimal capabilities and earn user trust, then publish v1.1 signed by the same key but with a manifest that declares no new capabilities while the WASM binary actually calls `network_fetch` — if dispatch doesn't check per-call, the call succeeds.

**How to avoid:**
In `blufio-skill`'s WASM host function dispatch, check the calling skill's capability manifest on every host function invocation (not just at install time). The manifest is loaded once and cached in the sandbox context. Host functions that require network capability must verify `manifest.capabilities.network.is_some()` before executing. This is enforcement, not just declaration. Add a test that installs a skill without network capability and verifies that attempting a `network_fetch` host call returns a sandbox error rather than succeeding.

**Warning signs:**
- Host function dispatch uses a single global "is skill trusted?" boolean rather than per-function capability check
- Skills can make network calls regardless of their declared capabilities
- Capability checks only run at `blufio skill install` time

**Phase to address:**
Skill registry phase. Must be verified before code signing is advertised as a security feature.

---

### Pitfall 11: Docker Image for SQLCipher Binary Requires Non-Scratch Base

**What goes wrong:**
The standard advice for static Rust binaries is to use `FROM scratch` — the most minimal base. However, Blufio uses SQLCipher which requires the `libsqlcipher` shared library unless the crate is built with the `bundled` feature. More critically, the binary makes HTTPS connections to LLM APIs and needs CA certificates. A `FROM scratch` image has no `/etc/ssl/certs/ca-certificates.crt`, causing all outbound TLS connections to fail with certificate validation errors. The binary starts, reports healthy, but every LLM call fails.

**Why it happens:**
The "use scratch for static Rust binaries" advice is correct for pure-Rust binaries with no system library dependencies and no TLS. Blufio has both: SQLCipher (via rusqlite's SQLCipher feature) links to libsqlcipher unless bundled, and reqwest's TLS stack needs CA certs regardless of static/dynamic linking.

**How to avoid:**
Use `gcr.io/distroless/static-debian12:nonroot` as the final stage base. It provides CA certificates, `/etc/passwd` for the non-root user, and timezone data — nothing else. Alternatively, use `FROM scratch` and explicitly COPY the CA bundle: `COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/`. Build SQLCipher with the `bundled` feature (`RUSQLITE_BUNDLED_SQLCIPHER=1`) to eliminate the shared library dependency. Verify the image works by running `docker run blufio blufio doctor` in CI before tagging a release.

**Warning signs:**
- `docker run blufio blufio doctor` reports LLM provider unhealthy
- reqwest errors mentioning certificate validation failure
- Binary works on host (has system certs) but not in container

**Phase to address:**
Docker image phase. The Dockerfile must be validated with an actual LLM call, not just a health endpoint check.

---

### Pitfall 12: Node Pairing Token Has No Expiry — Leaked Token = Permanent Access

**What goes wrong:**
A node pairing system typically involves generating a one-time or short-lived pairing token. If the pairing token does not expire, an operator who accidentally logs or displays the token during setup provides an attacker with permanent ability to pair a malicious node. Paired nodes receive the gateway's Ed25519 public key and can issue signed inter-agent messages. A malicious node could flood the agent with crafted messages, extract session data, or trigger tool invocations with operator-level trust.

**Why it happens:**
Pairing tokens are often implemented as a UUID stored in SQLite with no `expires_at` column. Adding expiry is easy to defer ("we'll add it later") and the system works without it during development.

**How to avoid:**
All pairing tokens must have an `expires_at` timestamp, default 15 minutes. Store tokens in a `node_pairing_tokens` table with columns: `token TEXT PRIMARY KEY`, `created_at TEXT`, `expires_at TEXT`, `used BOOLEAN`. Mark token as used after successful pairing (one-time use). Add a Prometheus counter for expired tokens and failed pairings. Reject any token older than its `expires_at` at the cryptographic verification layer — not just at the API layer.

**Warning signs:**
- Pairing token has no time component in its data
- Token table has no `expires_at` column
- Same token can be used multiple times

**Phase to address:**
Node system phase. Expiry and single-use enforcement must be in the initial implementation.

---

### Pitfall 13: OpenClaw Migration Loses Daily Memory and Long-Term Context Files

**What goes wrong:**
OpenClaw stores agent personality and memory in `~/.openclaw/` as flat files: `config.json`, `MEMORY.md` (long-term memory), `LESSONS.md` (learned behaviors), `CONTEXT.md` (active discussion), and per-day `workspace/YYYY-MM-DD.md` files. A migration tool that only exports `config.json` and database records misses the MEMORY.md, LESSONS.md, and workspace files entirely. After migration, the agent in Blufio has no memory of anything that happened in OpenClaw. Users who valued their agent's personalization lose months of accumulated context.

**Why it happens:**
The migration tool developer focuses on structured data (sessions, messages, credentials) and overlooks the unstructured flat files that represent the "personality" of an OpenClaw agent. The tool passes its own integration tests (which don't include memory file migration) but fails the operator's actual use case.

**How to avoid:**
The OpenClaw migration tool must explicitly enumerate and export: `config.yaml`, `credentials.json` (if present, with encryption), `MEMORY.md`, `LESSONS.md`, `CONTEXT.md`, all `workspace/YYYY-MM-DD.md` files, and session/message data. Import these into Blufio's memory system by injecting MEMORY.md and LESSONS.md as static context zone entries (read-only, high-priority). Add a migration dry-run mode that lists all files that will be migrated before executing. Add a post-migration verification that confirms the count of migrated items against the source.

**Warning signs:**
- Migration tool documentation mentions only "config and sessions"
- No memory file enumeration in the export step
- Post-migration agent has no knowledge of past conversations

**Phase to address:**
OpenClaw migration phase (last phase of v1.3). Must include a detailed checklist of OpenClaw artifacts to migrate.

---

### Pitfall 14: OpenAI-Compatible API Exposes Internal Session IDs to External Callers

**What goes wrong:**
The OpenAI chat completions spec includes an optional `user` field and returns a `system_fingerprint` and response `id`. Developers mapping these to Blufio internals often expose the internal SQLite session UUID as the `id` or attach it to the OpenAI response. This leaks the internal session identifier to API callers, which then use it to directly query `GET /v1/sessions/{id}`, bypassing any access control that would normally scope session access to the originating channel's user. A caller who receives another session's ID can read that session's messages.

**Why it happens:**
It's convenient to return the internal session ID as the OpenAI response ID. The IDs are UUIDs — they look random and non-guessable — so developers assume they're safe to expose. But any caller who receives their own session ID might use it to guess neighboring session IDs.

**How to avoid:**
Return opaque, externally-scoped IDs for all OpenAI-compatible responses. Never expose the internal SQLite `session_id` directly. Use a separate `external_id` column in the sessions table that is a different UUID, generated fresh at session creation, distinct from the internal primary key. The `/v1/sessions` list endpoint should be scoped by the authenticated API key's scope — a scoped key for caller X should only see sessions created by caller X.

**Warning signs:**
- OpenAI response `id` matches the internal SQLite `session_id` in the database
- Any caller can list all sessions via GET /v1/sessions without filtering
- No per-API-key session scoping in the sessions query

**Phase to address:**
OpenAI-compatible API layer phase and scoped API key phase. Both must be designed together.

---

## Technical Debt Patterns

Shortcuts that seem reasonable during v1.3 but create long-term problems.

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Reuse `ProviderRequest` for all providers | Avoids defining new types | Anthropic-specific fields leak into all providers; every new provider requires hacks | Never — define a provider-agnostic request type now |
| Use `CorsLayer::permissive()` on OpenAI endpoints | Fastest to ship | Any website can call your API endpoints with user's credentials via browser | Only acceptable on endpoints that require API key auth (bearer token) |
| Blocking write to SQLite per API request | Simplest implementation | Write queue depth grows with concurrent callers; P99 latency spikes | Acceptable for writes that are on the critical path; defer all others |
| Single Ed25519 signing key for skill registry | One key to manage | Key compromise invalidates all skills; no rotation path | Acceptable if key rotation is planned within 2 phases |
| `DashMap` for event bus subscriber state | Simple, fast | Subscribers that die leave stale entries; memory grows indefinitely | Acceptable if subscribers have explicit deregistration and health checks |
| Skip WhatsApp official API and use unofficial | Faster development | Account ban within weeks; no recovery | Never — TOS violation with irreversible consequences |
| Hardcode provider base URLs | Avoids config complexity | No air-gap support, no proxy support, no testing against local mocks | Acceptable only if URLs are in TOML config with documented override mechanism |
| Broadcast channel for all event types | Single simple API | Slow subscribers cause lag for all; hard to add per-event backpressure | Acceptable for pure fire-and-forget events; not for reliable delivery |

---

## Integration Gotchas

Common mistakes when connecting to external services or wiring internal components.

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| Ollama provider | Using `/v1/chat/completions` shim with stream+tools | Use native `/api/chat` endpoint for tool support |
| OpenRouter | Not forwarding `X-Title` and `X-HTTP-Referer` headers | Include these headers for OpenRouter leaderboard and rate limiting |
| Gemini API | Sending OpenAI-format `{type: "function", function: {...}}` tool definitions | Translate to Gemini native `{functionDeclarations: [...]}` format |
| Discord adapter | Testing only in DMs | Test in a guild server with MESSAGE_CONTENT intent explicitly enabled |
| Slack adapter | Polling `conversations.history` per session | Use Slack Events API webhooks or Socket Mode for push delivery |
| WhatsApp | Using unofficial library | Use only official Meta Graph API; document BSP requirement |
| Matrix SDK | Calling `/sync` in a busy loop | Use `matrix-sdk`'s built-in sync loop; manual sync bypasses sync token management |
| IRC adapter | Not handling server PING/PONG | The `irc` crate handles this automatically; do not implement manually |
| axum OpenAI routes | Applying permissive CORS globally | Apply permissive CORS only to non-authenticated routes; API key routes need restrictive CORS |
| Event bus | Using `tokio::sync::broadcast` for all subscribers | Use mpsc for reliable delivery, broadcast only for fire-and-forget |
| Scoped API keys | Using global rate limiter instance | Each API key needs an independent rate limit bucket (use `tower-governor` or `DashMap<key_id, bucket>`) |
| Node pairing | Storing pairing token as plain UUID | Store with `expires_at` + `used` flag; reject expired/used tokens at verification |

---

## Performance Traps

Patterns that work at small scale but degrade as usage grows.

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Synchronous SQLite write per API request | P99 latency grows with concurrent callers | Defer non-critical writes (cost tracking, audit log) to async queue | 5+ concurrent API callers |
| No rate limiting on `/v1/chat/completions` | Runaway LLM cost; provider rate limit bans | Implement per-key token bucket rate limiting before exposing API | First external caller who scripts the endpoint |
| DashMap `response_map` with no TTL cleanup | Memory grows on timeout (orphaned oneshot senders) | Add a background task that sweeps entries older than 130s | 50+ concurrent requests with frequent timeouts |
| Loading all sessions for `GET /v1/sessions` | Full table scan on large session table | Add pagination (`?limit=50&cursor=...`) from day one | 1000+ sessions |
| Event bus with all subscribers on same tokio thread pool | Slow subscriber (HTTP webhook) blocks all events | Run webhook dispatch on a dedicated thread pool or spawn per-event | First slow external webhook receiver |
| Compiling all channel adapters into single binary | 50MB+ binary; 10+ second cold start | Use feature flags per adapter (`cargo build --features discord,slack`) | When adding 6 channel adapters simultaneously |
| Broadcasting every internal event to all subscribers | Broadcast channel fills when any subscriber is slow | Use mpsc per subscriber for reliable delivery | First subscriber that does IO (webhook, metrics flush) |
| No connection pooling for LLM provider HTTP clients | New TCP connection per LLM request | Use `reqwest::Client` as a long-lived singleton (already done for Anthropic; do same for all providers) | >1 req/sec per provider |

---

## Security Mistakes

Domain-specific security issues beyond general web security.

| Mistake | Risk | Prevention |
|---------|------|------------|
| Exposing internal session IDs via OpenAI response `id` | Session enumeration — callers can probe neighboring sessions | Use separate external UUID distinct from internal primary key |
| Scoped API key stored as plaintext in SQLite | Key theft from DB compromise grants full API access | Store SHA-256 hash of key in DB; verify hash on each request |
| Skill registry key embedded in binary without rotation mechanism | Single key compromise invalidates all skills permanently | Implement key ID + version in signatures; support multiple active public keys |
| Node pairing tokens without expiry | Leaked token = permanent node compromise | Enforce 15-minute expiry and single-use; use time-based HMAC |
| Permissive CORS on OpenAI API endpoints | Browser-based CSRF attacks using operator's API key | Apply restrictive CORS on key-authenticated routes; use `CorsLayer::new()` with explicit allowed origins |
| Docker container running as root | Container escape gives host root access | Use `distroless/static-debian12:nonroot`; add `USER nonroot` in Dockerfile |
| Provider API keys in container environment without secrets management | Plaintext keys visible in `docker inspect` | Use Docker secrets or environment injection from vault; never bake keys into image layers |
| Skill WASM capability check only at install time | Malicious v1.1 update bypasses sandbox | Check capabilities per host function call, not once at install |
| WhatsApp unofficial library | Account ban + potential TOS legal risk | Official Meta Graph API only |
| No webhook signature verification | Fake webhooks trigger agent actions | Verify webhook signatures (HMAC-SHA256 for Slack/Discord, Meta signature for WhatsApp) |

---

## "Looks Done But Isn't" Checklist

Things that appear complete but are missing critical pieces.

- [ ] **OpenAI-compatible endpoint:** Test it with an actual OpenAI SDK client (not just curl) — SDK clients parse response fields differently than raw HTTP callers. Verify `finish_reason` appears in streaming chunks and in final response.

- [ ] **Ollama provider with tools:** Test tool calling with `stream: true` explicitly — silent failure mode means manual tests without streaming look fine while the real usage pattern is broken.

- [ ] **Discord adapter:** Deploy to a test guild server (not just DMs) and verify `message.content` is non-empty. Check `MESSAGE_CONTENT` intent is enabled in Developer Portal.

- [ ] **Scoped API keys:** Verify that Key A cannot list sessions created by Key B. Verify rate limiting applies per-key independently, not globally.

- [ ] **Skill code signing:** Verify that a skill with no network capability cannot make a `network_fetch` call (test the enforcement, not just the signing ceremony).

- [ ] **Docker image:** Run `docker run blufio blufio doctor` and verify the Anthropic provider reports healthy (actual TLS certificate validation with CA bundle).

- [ ] **Node pairing:** Attempt to reuse an already-used pairing token — verify it is rejected. Attempt to use a token after 15 minutes — verify it is rejected.

- [ ] **Event bus:** Subscribe a slow consumer (sleep 5s in handler) and verify the fast consumer on the same broadcast does not lag. Verify `RecvError::Lagged` is logged as a warning metric.

- [ ] **OpenClaw migration:** Run migration on an OpenClaw instance with 90 days of `workspace/` files and verify all files appear in Blufio's static context zone.

- [ ] **WhatsApp adapter:** Confirm it uses `graph.facebook.com` (official API), not `web.whatsapp.com` or `ws.web.whatsapp.net` (unofficial). Document BSP setup requirement in config validation error.

- [ ] **Multi-provider routing:** Send a request that triggers tool use with each provider independently (Anthropic, OpenAI, Ollama, Gemini). Verify tool result appears in conversation history.

- [ ] **Rate limiting on `/v1/chat/completions`:** Verify that a burst of 100 requests from one API key is throttled, and a burst from a different API key is independently throttled without affecting the first key.

---

## Recovery Strategies

When pitfalls occur despite prevention, how to recover.

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| OpenAI field name mismatch in production | MEDIUM | Deploy corrected wire types, no data migration needed; bump API version if clients cached schema |
| Ollama tool call silent drops | LOW | Switch provider crate to native `/api/chat` endpoint; redeploy without data changes |
| WhatsApp account banned | HIGH | Phone number permanently lost; obtain new number + new Meta Business verification; no automated recovery |
| SQLite write queue saturation under load | MEDIUM | Enable async write deferral for non-critical paths; restart to clear queue backlog |
| Skill capability bypass discovered | HIGH | Emergency revoke of signing key; re-audit all installed skills; push patched runtime; notify all users |
| Docker image TLS failure in prod | LOW | Rebuild with correct base image (distroless or with CA bundle); 5-minute fix once identified |
| Leaked pairing token exploited | HIGH | Revoke all pairing tokens; audit paired nodes for unauthorized entries; rotate gateway keypair |
| Event bus subscriber lag causing dropped webhooks | MEDIUM | Switch affected subscriber from broadcast to dedicated mpsc channel; replay missed events from SQLite audit log |
| OpenClaw migration missing memory files | MEDIUM | Re-run migration with updated tool that includes memory file enumeration; inject into context zone |
| Node pairing token no-expiry exploited | HIGH | Force re-pairing all nodes; rotate signing key; audit all signed messages from the pairing window |

---

## Pitfall-to-Phase Mapping

How roadmap phases should address these pitfalls.

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| OpenAI field name mismatch (stop_reason vs finish_reason) | Phase 1: OpenAI-compatible API | Integration test with actual OpenAI SDK client |
| Provider tool format leaks Anthropic structure | Phase 1: Before any non-Anthropic provider | Compile-time type check + per-provider wire format test |
| SQLite single-writer bottleneck under concurrent API load | Phase 1 + concurrent load test | P99 latency stays <2s under 10 concurrent callers |
| Ollama streaming + tools silent drop | Phase 2: Multi-provider LLM | Tool-call integration test with stream: true for Ollama |
| Gemini native tool format differences | Phase 2: Multi-provider LLM | Wire format unit test for Gemini tool schema translation |
| Discord MESSAGE_CONTENT intent | Phase 3: Multi-channel adapters | Guild message reception test (not DM-only test) |
| WhatsApp unofficial library ban risk | Phase 3: Multi-channel adapters | Verify `graph.facebook.com` in adapter; block unofficial library in cargo-deny |
| Slack rate limit tier throttling | Phase 3: Multi-channel adapters | Verify Events API or Socket Mode (not polling) in adapter design |
| Signal no stable Rust library | Phase 3: Multi-channel adapters | Document as deferred in phase acceptance criteria |
| Docker CA bundle missing | Phase 4: Docker image | CI runs `docker run blufio blufio doctor` with LLM health check |
| Docker root user | Phase 4: Docker image | CI verifies `whoami` returns `nonroot` inside container |
| Event bus broadcast receiver lag | Phase 5: Event bus | Slow subscriber test verifies `RecvError::Lagged` is logged |
| Skill capability bypass (signing != sandbox enforcement) | Phase 6: Skill registry | Unit test: skill without network capability cannot call network host function |
| Code signing key without rotation mechanism | Phase 6: Skill registry | Key ID versioning in signature format from day one |
| Node pairing token no-expiry | Phase 7: Node system | Token expiry test: token rejected after 15 minutes |
| Node pairing token reuse | Phase 7: Node system | Token single-use test: second use of valid token rejected |
| Internal session ID exposure via OpenAI response | Phase 1: OpenAI-compatible API | Verify `response.id` != internal SQLite `session_id` |
| Scoped API key session cross-access | Phase 1: Scoped API keys | Key A cannot list Key B's sessions (authorization test) |
| OpenClaw missing memory file migration | Phase 8: Migration tooling | Migration dry-run output includes MEMORY.md, LESSONS.md, workspace/ files |
| Cargo workspace feature unification surprises | Every phase with new crates | Run `cargo build --features X` per adapter; check for unexpected feature unification |

---

## Sources

- Ollama tool calling + streaming issue: https://github.com/ollama/ollama/issues/12557
- Ollama OpenAI compatibility documentation: https://docs.ollama.com/api/openai-compatibility
- OpenAI Chat Completions `finish_reason` spec: https://platform.openai.com/docs/api-reference/chat/create
- OpenAI Responses API vs Chat Completions: https://platform.openai.com/docs/guides/responses-vs-chat-completions
- OpenAI Responses API tool schema differences: https://medium.com/@laurentkubaski/openai-tool-schema-differences-between-the-response-api-and-the-chat-completion-api-8f99ce8a9371
- WhatsApp Business API unofficial vs official risk: https://wisemelon.ai/blog/whatsapp-business-api-vs-unofficial-whatsapp-tools
- WhatsApp messaging limits (October 2025 portfolio-level): https://chatarmin.com/en/blog/whats-app-messaging-limits
- Discord rate limits: https://docs.discord.com/developers/topics/rate-limits
- Slack rate limit changes (May 2025, 1 req/min for non-Marketplace): https://docs.slack.dev/changelog/2025/05/29/rate-limit-changes-for-non-marketplace-apps/
- Tokio broadcast channel lag behavior: https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html
- Event bus implementation in Tokio (lag handling): https://blog.digital-horror.com/blog/event-bus-in-tokio/
- Docker static Rust binary with CA certs: https://dev.to/abhishekpareek/build-statically-linked-rust-binary-with-musl-and-avoid-a-common-pitfall-ahc
- SQLite single-writer bottleneck under concurrent load: https://www.bugsink.com/blog/database-transactions/
- Gemini OpenAI compatibility and function calling: https://ai.google.dev/gemini-api/docs/openai
- Gemini function calling schema format: https://ai.google.dev/gemini-api/docs/function-calling
- Cargo workspace feature unification pitfall: https://nickb.dev/blog/cargo-workspace-and-the-feature-unification-pitfall/
- OpenClaw migration broken state (issue #5103): https://github.com/openclaw/openclaw/issues/5103
- Axum rate limiting with tower-governor: https://github.com/benwis/tower-governor
- IRC Rust crate (active): https://crates.io/crates/irc
- Matrix SDK Rust crate: https://crates.io/crates/matrix-sdk
- Blufio codebase: existing `ProviderRequest.tools` in `blufio-core/src/types.rs` (Anthropic-format comment)
- Blufio codebase: `GatewayState.response_map` DashMap with no TTL cleanup in `blufio-gateway/src/server.rs`
- Blufio codebase: `ChannelCapabilities` in `blufio-core/src/types.rs` (format degradation surface)

---
*Pitfalls research for: v1.3 Ecosystem Expansion — adding OpenAI-compatible API, multi-provider LLM, multi-channel adapters, Docker, event bus, skill registry, node system, migration tooling to existing 39K LOC Rust AI agent platform*
*Researched: 2026-03-05*
