# Phase 32: Scoped API Keys, Webhooks & Batch - Research

**Researched:** 2026-03-06
**Domain:** API access control, webhook delivery, batch processing (Rust/axum/SQLite)
**Confidence:** HIGH

## Summary

Phase 32 adds three capabilities to the existing gateway: scoped API key management with per-key rate limiting, webhook event delivery with HMAC-SHA256 signing, and batch request processing. All three build on established patterns in the codebase -- the existing auth middleware (`blufio-gateway/src/auth.rs`), the EventBus (`blufio-bus`), SQLite storage (`blufio-storage`), and the gateway route/state pattern (`blufio-gateway/src/server.rs`).

The implementation is straightforward because all infrastructure exists. The auth middleware currently supports a single bearer token -- this needs to evolve to look up scoped API keys from SQLite. The EventBus already defines `WebhookEvent` and `BatchEvent` variants. SQLite migrations use refinery with embedded SQL files. The gateway state struct (`GatewayState`) needs new fields for the API key store, webhook delivery engine, and batch processor.

**Primary recommendation:** Implement in three waves -- (1) API key storage + auth middleware extension + rate limiting, (2) webhook registration + delivery engine with EventBus integration, (3) batch processing API. Each is independently testable.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Keys created via both CLI (`blufio api-keys create --scope chat.completions --rate-limit 60`) and API (POST /v1/api-keys for admin-scoped keys to create sub-keys)
- Endpoint-level scopes plus optional model restrictions: `chat.completions:openai/*` allows only OpenAI models on chat completions
- Keys stored in SQLite table: hashed (SHA-256), scopes as JSON column, created_at, expires_at, revoked_at fields
- Key format: `blf_sk_<random32chars>` -- prefixed for identifiability (like Stripe sk_live_, OpenAI sk-)
- Revoked keys immediately rejected; expiration checked at auth time
- Completion-focused event set: chat.completed, response.completed, tool.invoked, batch.completed. Extensible later.
- HMAC-SHA256 signature in `X-Webhook-Signature` header using per-webhook secret
- Exponential backoff retry (5 attempts: 1s, 5s, 25s, 2min, 10min)
- Dead letter queue in SQLite -- failed events stored for operator replay. No data loss.
- Webhook registration via POST /v1/webhooks with url, events filter, secret auto-generated
- POST /v1/batch accepts array of chat completion requests, returns batch_id
- Polling model: GET /v1/batch/{id} returns status + per-item results when complete
- Parallel execution with configurable concurrency limit (default 3)
- Batch items share the caller's API key scope -- no privilege escalation
- Results include per-item success/error with original request index
- Sliding window counter per API key, enforced as axum middleware before handlers
- Counts stored in SQLite (key_id, window_start, count)
- Returns 429 with Retry-After and X-RateLimit-Remaining headers
- Default: 60 requests/minute, configurable per key at creation

### Claude's Discretion
- Exact retry backoff curve timing
- Batch max size limit
- Whether to add rate limit headers to all responses or only near-limit
- Dead letter replay API design (CLI command vs API endpoint)

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| API-11 | User can create scoped API keys via POST /v1/api-keys | SQLite table + handler + key generation; existing auth module pattern |
| API-12 | API keys support scope restrictions (chat.completions, tools.invoke, admin) | Scope model stored as JSON; middleware resolves key -> scopes -> allow/deny |
| API-13 | API keys support per-key rate limiting (requests per minute) | Sliding window counter in SQLite; axum middleware layer |
| API-14 | API keys support expiration and revocation | expires_at/revoked_at fields checked in auth middleware |
| API-15 | User can register webhooks via POST /v1/webhooks | SQLite webhooks table + handler + auto-generated HMAC secret |
| API-16 | Webhooks deliver events with HMAC-SHA256 signing and exponential backoff retry | EventBus reliable subscriber + delivery engine with reqwest + retry logic |
| API-17 | User can submit batch requests via POST /v1/batch | Handler accepts array, spawns parallel processing, returns batch_id |
| API-18 | Batch results available with per-item success/error status | GET /v1/batch/{id} polls SQLite for batch status + items |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| axum | workspace | HTTP framework, middleware, routing | Already used in gateway |
| rusqlite | workspace | SQLite access | Already used in blufio-storage |
| tokio-rusqlite | workspace | Async SQLite wrapper | Already used, single-writer pattern |
| refinery | workspace | Embedded SQL migrations | Already used for schema evolution |
| sha2 | 0.10 | SHA-256 hashing for API keys | Standard Rust crypto crate, RustCrypto family |
| hmac | 0.12 | HMAC-SHA256 for webhook signatures | Standard Rust crypto crate, RustCrypto family |
| reqwest | 0.12 | HTTP client for webhook delivery | De facto Rust HTTP client |
| rand | workspace or 0.8 | Secure random key generation | Standard for cryptographic random |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| hex | workspace | Hex encoding for hashed keys | Already a dependency |
| serde_json | workspace | JSON serialization for scopes, batch payloads | Already everywhere |
| tokio | workspace | Async runtime, semaphore for batch concurrency | Already everywhere |
| chrono | workspace | Timestamps for key expiry, rate limiting windows | Already a dependency |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| SQLite rate limit counters | In-memory (DashMap) | Memory counters reset on restart, SQLite persists; SQLite is simpler for single-instance |
| reqwest for webhook delivery | hyper directly | reqwest wraps hyper with ergonomic API; no need for low-level control |
| SHA-256 key hashing | bcrypt/argon2 | API keys are high-entropy random, not passwords; SHA-256 is appropriate and fast |

## Architecture Patterns

### Recommended Project Structure
```
crates/blufio-gateway/src/
├── auth.rs              # Extended: scoped key lookup + rate limiting
├── api_keys/            # NEW: API key management
│   ├── mod.rs           # Types: ApiKey, ApiKeyScope, CreateKeyRequest/Response
│   ├── handlers.rs      # POST /v1/api-keys, GET /v1/api-keys, DELETE /v1/api-keys/{id}
│   └── store.rs         # SQLite operations for api_keys table
├── webhooks/            # NEW: Webhook management and delivery
│   ├── mod.rs           # Types: Webhook, WebhookRegistration, DeliveryAttempt
│   ├── handlers.rs      # POST /v1/webhooks, GET /v1/webhooks, DELETE /v1/webhooks/{id}
│   ├── store.rs         # SQLite operations for webhooks + dead_letter tables
│   └── delivery.rs      # Delivery engine: HMAC signing, retry, EventBus subscriber
├── batch/               # NEW: Batch processing
│   ├── mod.rs           # Types: BatchRequest, BatchResponse, BatchItem
│   ├── handlers.rs      # POST /v1/batch, GET /v1/batch/{id}
│   └── processor.rs     # Parallel execution with Semaphore, result collection
├── rate_limit.rs        # NEW: Sliding window rate limiter middleware
├── server.rs            # Extended: new routes, new state fields
└── ...existing files...

crates/blufio-storage/migrations/
└── V7__api_keys_webhooks_batch.sql  # NEW: tables for all three features

crates/blufio/src/
└── main.rs              # Extended: new CLI subcommands (api-keys, webhooks)
```

### Pattern 1: Auth Middleware Extension
**What:** The existing `auth_middleware` checks a single bearer token. Extend it to also look up `blf_sk_*` tokens from the API key store, resolve scopes, and attach scope info to the request extension.
**When to use:** Every authenticated request.
**Example:**
```rust
// In auth.rs - extended bearer path
if let Some(token) = auth_header {
    // Fast path: check master bearer token first (existing behavior)
    if let Some(ref expected) = auth.bearer_token {
        if token == expected {
            // Master token has all scopes
            request.extensions_mut().insert(AuthContext::master());
            return Ok(next.run(request).await);
        }
    }
    // Scoped key path: look up hashed token in SQLite
    if token.starts_with("blf_sk_") {
        let hash = sha256_hex(token);
        if let Some(key) = auth.key_store.lookup(&hash).await? {
            if key.is_valid() {
                request.extensions_mut().insert(AuthContext::scoped(key.scopes));
                return Ok(next.run(request).await);
            }
        }
    }
}
```

### Pattern 2: Scope Enforcement per Route
**What:** After auth middleware attaches `AuthContext` to the request, route handlers check if the key's scopes allow the operation.
**When to use:** Each handler that needs scope checking.
**Example:**
```rust
// AuthContext extracted in handler
fn require_scope(ctx: &AuthContext, scope: &str) -> Result<(), StatusCode> {
    if ctx.has_scope(scope) { Ok(()) } else { Err(StatusCode::FORBIDDEN) }
}

// In handler:
async fn post_chat_completions(
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<GatewayState>,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Response, StatusCode> {
    require_scope(&auth_ctx, "chat.completions")?;
    // ... existing handler logic
}
```

### Pattern 3: Sliding Window Rate Limiter
**What:** Middleware that checks per-key request counts in a sliding window.
**When to use:** All authenticated routes, after auth middleware resolves the key.
**Example:**
```rust
// rate_limit.rs
pub async fn rate_limit_middleware(
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<GatewayState>,
    request: Request,
    next: Next,
) -> Result<Response, (StatusCode, HeaderMap)> {
    if let Some(key_id) = auth_ctx.key_id() {
        let limit = auth_ctx.rate_limit();
        let window_start = current_minute_start();
        let count = state.key_store.increment_count(key_id, window_start).await?;
        if count > limit {
            let mut headers = HeaderMap::new();
            headers.insert("Retry-After", /* seconds until window reset */);
            headers.insert("X-RateLimit-Remaining", HeaderValue::from(0));
            return Err((StatusCode::TOO_MANY_REQUESTS, headers));
        }
    }
    Ok(next.run(request).await)
}
```

### Pattern 4: EventBus Webhook Delivery
**What:** A reliable subscriber on the EventBus listens for completion events and dispatches webhook deliveries.
**When to use:** Background task started with the server.
**Example:**
```rust
// delivery.rs
pub async fn run_webhook_delivery(
    bus: Arc<EventBus>,
    store: Arc<WebhookStore>,
    client: reqwest::Client,
) {
    let mut rx = bus.subscribe_reliable(256).await;
    while let Some(event) = rx.recv().await {
        let event_type = match &event {
            BusEvent::Channel(ChannelEvent::MessageSent { .. }) => Some("chat.completed"),
            BusEvent::Skill(SkillEvent::Completed { .. }) => Some("tool.invoked"),
            BusEvent::Batch(BatchEvent::Completed { .. }) => Some("batch.completed"),
            _ => None,
        };
        if let Some(event_type) = event_type {
            let webhooks = store.list_for_event(event_type).await;
            for webhook in webhooks {
                deliver_with_retry(&client, &webhook, &event, &store).await;
            }
        }
    }
}
```

### Anti-Patterns to Avoid
- **Storing raw API keys:** Always store SHA-256 hash, never the plaintext key. The key is only shown once at creation time.
- **In-memory-only rate limiting:** Won't survive restarts; SQLite is the single source of truth for this single-instance deployment.
- **Blocking webhook delivery:** Never block the request/response cycle waiting for webhook delivery. EventBus decouples this.
- **Shared secrets in responses:** Webhook secrets should only be returned at creation time, not in GET /v1/webhooks list responses.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| HMAC-SHA256 | Custom HMAC implementation | `hmac` + `sha2` crates | Cryptographic correctness; audited implementation |
| HTTP client with retries | Raw hyper with manual retry | reqwest + custom retry loop | reqwest handles TLS, redirects, timeouts |
| Secure random generation | std::collections::HashMap random | `rand::rngs::OsRng` with `rand::distributions::Alphanumeric` | Cryptographically secure randomness |
| API key hashing | Custom hash function | `sha2::Sha256` | Standard, well-tested |

**Key insight:** All crypto operations (hashing, HMAC, random generation) must use audited crates from the RustCrypto project. Never hand-roll cryptographic primitives.

## Common Pitfalls

### Pitfall 1: Timing Attacks on API Key Lookup
**What goes wrong:** Comparing API key hashes with `==` leaks timing information about correct prefix bytes.
**Why it happens:** String equality short-circuits on first mismatch.
**How to avoid:** Use `subtle::ConstantTimeEq` for hash comparison, or since we're doing SQLite lookup by hash (exact match), the timing is dominated by the SQLite query and is not exploitable.
**Warning signs:** Direct byte/string comparison of security-sensitive values.

### Pitfall 2: Race Condition in Rate Limiting
**What goes wrong:** Two concurrent requests both read count=59 (under limit of 60), both increment, resulting in count=61.
**Why it happens:** Read-then-write without atomicity.
**How to avoid:** Use SQLite's `INSERT ... ON CONFLICT UPDATE count = count + 1` (atomic upsert). Since all writes go through tokio-rusqlite's single background thread, this is inherently serialized.
**Warning signs:** Separate SELECT + UPDATE queries for rate limiting.

### Pitfall 3: Webhook Delivery Blocking Event Processing
**What goes wrong:** Slow webhook endpoints block the delivery loop, causing EventBus backpressure.
**Why it happens:** Sequential delivery with long timeouts.
**How to avoid:** Spawn each delivery as a separate tokio task with a short timeout (10 seconds). Use `tokio::spawn` so failures don't block the main delivery loop.
**Warning signs:** `await` on HTTP delivery in the main subscriber loop.

### Pitfall 4: Batch Size Explosion
**What goes wrong:** A user submits 10,000 items in a single batch, exhausting server resources.
**Why it happens:** No size validation on batch input.
**How to avoid:** Enforce a maximum batch size (recommend 100 items). Return 400 if exceeded.
**Warning signs:** Unbounded Vec deserialization without size check.

### Pitfall 5: API Key Prefix Collision
**What goes wrong:** The `blf_sk_` prefix is checked case-sensitively, but some HTTP clients or proxies may lowercase headers.
**Why it happens:** Authorization header value is case-sensitive by spec, but real-world behavior varies.
**How to avoid:** The prefix check is fine as-is since OAuth bearer tokens are case-sensitive per RFC 6750. Document that keys are case-sensitive.
**Warning signs:** Case-insensitive matching on the prefix.

## Code Examples

### API Key Generation
```rust
use rand::Rng;
use sha2::{Sha256, Digest};

fn generate_api_key() -> (String, String) {
    let random_bytes: Vec<u8> = (0..32).map(|_| rand::rng().random::<u8>()).collect();
    let random_hex = hex::encode(&random_bytes);
    let raw_key = format!("blf_sk_{random_hex}");

    let mut hasher = Sha256::new();
    hasher.update(raw_key.as_bytes());
    let hash = hex::encode(hasher.finalize());

    (raw_key, hash) // raw_key shown to user once; hash stored in DB
}
```

### HMAC-SHA256 Webhook Signature
```rust
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

fn sign_webhook_payload(secret: &[u8], payload: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret)
        .expect("HMAC can take key of any size");
    mac.update(payload);
    let result = mac.finalize();
    hex::encode(result.into_bytes())
}
```

### SQLite Migration (V7)
```sql
-- API Keys
CREATE TABLE api_keys (
    id TEXT PRIMARY KEY,
    key_hash TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    scopes TEXT NOT NULL DEFAULT '[]',  -- JSON array of scope strings
    rate_limit INTEGER NOT NULL DEFAULT 60,
    created_at TEXT NOT NULL,
    expires_at TEXT,
    revoked_at TEXT
);
CREATE INDEX idx_api_keys_hash ON api_keys(key_hash);

-- Webhooks
CREATE TABLE webhooks (
    id TEXT PRIMARY KEY,
    url TEXT NOT NULL,
    secret TEXT NOT NULL,  -- HMAC secret (stored encrypted via SQLCipher)
    events TEXT NOT NULL DEFAULT '[]',  -- JSON array of event types
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Webhook Dead Letter Queue
CREATE TABLE webhook_dead_letter (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    webhook_id TEXT NOT NULL REFERENCES webhooks(id),
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    last_attempt_at TEXT NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_dead_letter_webhook ON webhook_dead_letter(webhook_id);

-- Batches
CREATE TABLE batches (
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL DEFAULT 'processing',  -- processing, completed, failed
    total_items INTEGER NOT NULL,
    completed_items INTEGER NOT NULL DEFAULT 0,
    failed_items INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    completed_at TEXT,
    api_key_id TEXT  -- track which key submitted the batch
);

-- Batch Items
CREATE TABLE batch_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    batch_id TEXT NOT NULL REFERENCES batches(id),
    item_index INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',  -- pending, processing, completed, failed
    request TEXT NOT NULL,  -- JSON: original chat completion request
    response TEXT,  -- JSON: chat completion response or error
    created_at TEXT NOT NULL,
    completed_at TEXT
);
CREATE INDEX idx_batch_items_batch ON batch_items(batch_id);

-- Rate Limit Counters
CREATE TABLE rate_limit_counters (
    key_id TEXT NOT NULL,
    window_start TEXT NOT NULL,
    count INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (key_id, window_start)
);
```

### Sliding Window Rate Limit (Atomic Upsert)
```rust
// Atomic increment using INSERT ON CONFLICT
let count: i64 = conn.call(move |conn| {
    conn.execute(
        "INSERT INTO rate_limit_counters (key_id, window_start, count)
         VALUES (?1, ?2, 1)
         ON CONFLICT(key_id, window_start) DO UPDATE SET count = count + 1",
        rusqlite::params![key_id, window_start],
    )?;
    let count: i64 = conn.query_row(
        "SELECT count FROM rate_limit_counters WHERE key_id = ?1 AND window_start = ?2",
        rusqlite::params![key_id, window_start],
        |row| row.get(0),
    )?;
    Ok(count)
}).await?;
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Single shared bearer token | Scoped API keys with per-key limits | This phase | Multi-tenant deployments become viable |
| Fire-and-forget event bus only | Reliable subscriber for webhook delivery | Phase 29 (bus exists) | Guaranteed event delivery |
| One-at-a-time API requests | Batch processing with concurrency control | This phase | Cost-efficient bulk operations |

## Open Questions

1. **Batch max size limit**
   - What we know: Needs a limit to prevent resource exhaustion
   - Recommendation: 100 items max. This balances utility with server resource management. Configurable in GatewayConfig.

2. **Rate limit headers on all responses**
   - What we know: Context says "whether to add rate limit headers to all responses or only near-limit"
   - Recommendation: Add `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset` headers to ALL responses for scoped API keys. This follows the standard pattern used by GitHub, Stripe, etc. Master bearer token responses omit these headers.

3. **Dead letter replay mechanism**
   - What we know: Context defers design choice (CLI vs API)
   - Recommendation: CLI command `blufio webhooks replay --webhook-id <id>` for v1. API endpoint can be added later. CLI is simpler and safer for operator use.

## Sources

### Primary (HIGH confidence)
- Codebase analysis: `blufio-gateway/src/auth.rs`, `blufio-gateway/src/server.rs`, `blufio-bus/src/events.rs`, `blufio-storage/src/adapter.rs`
- Codebase analysis: Existing migration pattern in `blufio-storage/migrations/`
- Codebase analysis: Config pattern in `blufio-config/src/model.rs`
- RustCrypto crates: `sha2`, `hmac` -- well-established Rust cryptographic primitives

### Secondary (MEDIUM confidence)
- Industry patterns: Stripe API key format, GitHub webhook signing, OpenAI batch API design

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - all libraries already in workspace or well-established RustCrypto
- Architecture: HIGH - extends existing patterns directly (auth middleware, EventBus, SQLite storage)
- Pitfalls: HIGH - well-known patterns for API key management, webhook delivery, rate limiting

**Research date:** 2026-03-06
**Valid until:** 2026-04-06 (stable domain, well-known patterns)
