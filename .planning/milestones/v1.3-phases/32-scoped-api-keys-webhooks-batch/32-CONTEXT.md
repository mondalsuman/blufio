# Phase 32: Scoped API Keys, Webhooks & Batch - Context

**Gathered:** 2026-03-05
**Status:** Ready for planning

<domain>
## Phase Boundary

Multi-user and multi-service deployments are secure with scoped API keys, async event delivery via webhooks with HMAC-SHA256 signing, and cost-efficient batch processing. Phase 31 endpoints must exist (they do). This phase adds access control, event delivery, and batch on top.

</domain>

<decisions>
## Implementation Decisions

### API key scope model
- Keys created via both CLI (`blufio api-keys create --scope chat.completions --rate-limit 60`) and API (POST /v1/api-keys for admin-scoped keys to create sub-keys)
- Endpoint-level scopes plus optional model restrictions: `chat.completions:openai/*` allows only OpenAI models on chat completions
- Keys stored in SQLite table: hashed (SHA-256), scopes as JSON column, created_at, expires_at, revoked_at fields
- Key format: `blf_sk_<random32chars>` — prefixed for identifiability (like Stripe sk_live_, OpenAI sk-)
- Revoked keys immediately rejected; expiration checked at auth time

### Webhook delivery
- Completion-focused event set: chat.completed, response.completed, tool.invoked, batch.completed. Extensible later.
- HMAC-SHA256 signature in `X-Webhook-Signature` header using per-webhook secret
- Exponential backoff retry (5 attempts: 1s, 5s, 25s, 2min, 10min)
- Dead letter queue in SQLite — failed events stored for operator replay. No data loss.
- Webhook registration via POST /v1/webhooks with url, events filter, secret auto-generated

### Batch processing
- POST /v1/batch accepts array of chat completion requests, returns batch_id
- Polling model: GET /v1/batch/{id} returns status + per-item results when complete
- Parallel execution with configurable concurrency limit (default 3)
- Batch items share the caller's API key scope — no privilege escalation
- Results include per-item success/error with original request index

### Rate limiting
- Sliding window counter per API key, enforced as axum middleware before handlers
- Counts stored in SQLite (key_id, window_start, count)
- Returns 429 with Retry-After and X-RateLimit-Remaining headers
- Default: 60 requests/minute, configurable per key at creation

### Claude's Discretion
- Exact retry backoff curve timing
- Batch max size limit
- Whether to add rate limit headers to all responses or only near-limit
- Dead letter replay API design (CLI command vs API endpoint)

</decisions>

<specifics>
## Specific Ideas

- Key prefix `blf_sk_` makes leaked keys easy to identify in logs and scanners (like GitHub secret scanning)
- Dead letter queue prevents webhook data loss during endpoint outages — critical for reliability
- Model-level scope restrictions enable multi-tenant setups (team A gets openai/*, team B gets ollama/*)

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `blufio-gateway/src/auth.rs`: AuthConfig with bearer token + Ed25519 keypair auth middleware — will need extension for scoped key lookup
- `blufio-bus::EventBus`: Reliable subscriber pattern for webhook event sourcing — subscribe to completion events, forward to webhook delivery
- `blufio-storage::StorageAdapter`: SQLite storage trait for new api_keys, webhooks, batches tables
- `blufio-gateway/src/server.rs`: GatewayState with existing route registration pattern

### Established Patterns
- Auth middleware checks bearer token first (fast path), then keypair (slow path) — scoped keys extend the bearer path
- SQLite WAL mode with single-writer thread via tokio-rusqlite
- Config structs use `#[serde(deny_unknown_fields)]` with defaults
- Gateway routes split into authenticated and unauthenticated groups

### Integration Points
- Auth middleware needs to resolve key -> scopes -> allow/deny per route
- EventBus subscriber for webhook delivery (chat.completed etc.)
- Batch processor needs access to ProviderRegistry from Phase 31's GatewayState
- New CLI subcommands: `blufio api-keys`, `blufio webhooks`

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 32-scoped-api-keys-webhooks-batch*
*Context gathered: 2026-03-05*
