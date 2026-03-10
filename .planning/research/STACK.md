# Stack Research: v1.5 PRD Gap Closure

**Domain:** Rust AI agent platform -- infrastructure hardening, compliance, and channel expansion
**Researched:** 2026-03-10
**Confidence:** HIGH (versions verified against crates.io API, compatibility cross-checked)

## Existing Stack (DO NOT ADD -- Already in Workspace)

These are already in the workspace and cover their respective domains. Listed to prevent accidental duplication and to show which new features they serve.

| Already Have | Version | v1.5 Feature It Serves |
|---|---|---|
| `regex` | 1 | PII pattern expansion, prompt injection L1 classifier |
| `sha2` | 0.10 | Hash-chained audit trail |
| `hmac` | 0.12 | Prompt injection L3 HMAC boundary tokens |
| `chrono` | 0.4 | Temporal decay, retention policies, audit timestamps |
| `serde` + `serde_json` | 1 | JSON data export, data classification serialization |
| `tokio` | 1 | Cron scheduler timer loop, file watchers, hook subprocess execution |
| `tracing` | 0.1 | OpenTelemetry bridge layer (existing spans become OTel traces) |
| `tracing-subscriber` | 0.3 | OpenTelemetry layer registration |
| `rusqlite` | 0.37 | Audit trail tables, retention enforcement, compaction storage |
| `tokio-rusqlite` | 0.7 | Async SQLite for all new persistence |
| `dashmap` | 6 | Concurrent maps (memory index, hook registry) |
| `metrics` | 0.24 | Prometheus metrics for new subsystems |
| `uuid` | 1 | Audit entry IDs, export record IDs |
| `reqwest` | 0.13 | BlueBubbles REST API, Twilio SMS API, email API fallback |
| `axum` | 0.8 | OpenAPI route annotations |
| `ring` | 0.17 | HMAC boundary token computation |
| `ndarray` | 0.17 | MMR diversity reranking (cosine similarity) |
| `strum` | 0.26 | Data classification enum derives |
| `jsonschema` | 0.28 | Prompt injection L4 output validation |
| `proptest` | 1 | Property-based test expansion |

---

## New Dependencies Required

### 1. Hot Reload & File Watching

| Technology | Version | Purpose | Why Recommended |
|---|---|---|---|
| `arc-swap` | 1.8 | Lock-free atomic pointer swap for config/TLS/plugin hot reload | Read-mostly write-seldom pattern matches config hot reload exactly. `ArcSwap<Config>` lets all request handlers load current config via `arc_swap::Guard` with zero contention -- no RwLock needed. 143M+ downloads, battle-tested in tikv. MSRV 1.31 (no conflict with workspace 1.85). Zero transitive dependencies. |
| `notify` | 8.0 | Cross-platform filesystem event watcher for config/cert/plugin changes | Mature (62M+ downloads), used by rust-analyzer, zed, deno. kqueue on macOS, inotify on Linux. Triggers reload on TOML config change, TLS cert rotation, plugin directory modification. MSRV 1.85 matches workspace. Pin to 8.x stable -- 9.0 is still RC. |

**Integration pattern:** `notify::RecommendedWatcher` watches config files and cert paths. On file change event, debounce ~500ms, parse new config, validate, then `arc_swap::ArcSwap::store()` atomically swaps the new `Arc<Config>`. All consumers call `config.load()` on each request -- zero restart needed. Emit `BusEvent::ConfigReloaded` through existing `EventBus` for audit logging.

**Where these live:** `blufio-config` gets `arc-swap` for the swappable config holder. A new hot-reload module (in `blufio-config` or `blufio-agent`) uses `notify` for file watching. TLS cert reload lives in `blufio-gateway`.

### 2. Cron Scheduling

| Technology | Version | Purpose | Why Recommended |
|---|---|---|---|
| `cron` | 0.15 | Cron expression parser (6-field standard + schedule iteration) | Lightweight parser-only crate -- no runtime, no job store, no signal handling. Parses standard cron expressions into a `Schedule` that yields `DateTime<Utc>` iterators. 20M+ downloads, uses `chrono` (already in workspace). Blufio already has `tokio::time` for timer loops and `EventBus` for notifications -- only the parser is missing. |

**Why NOT `tokio-cron-scheduler` 0.15:** Pulls PostgreSQL/Nats metadata stores, its own `Job` abstraction, and signal handling. Blufio already has `tokio::time::sleep_until` for the timer loop, `EventBus` for notifications, and SQLite for persistence. Using `cron` parser + custom `tokio::spawn` loop is simpler, lighter, and integrates cleanly with the existing architecture.

**systemd timer generation:** Convert cron expressions to systemd OnCalendar syntax via a small conversion function (~50 lines). No external crate needed -- it's string template generation.

### 3. OpenTelemetry Distributed Tracing

| Technology | Version | Purpose | Why Recommended |
|---|---|---|---|
| `opentelemetry` | 0.31 | Core OpenTelemetry API (trace context, span creation) | Official OTel Rust SDK. Traces are Beta status (sufficient for optional/disabled-by-default). MSRV 1.75 (compatible with workspace 1.85). |
| `opentelemetry_sdk` | 0.31 | SDK implementation (BatchSpanProcessor, exporters) | Must match `opentelemetry` version exactly. Provides async span processing. |
| `opentelemetry-otlp` | 0.31 | OTLP exporter to Jaeger/Tempo/Grafana/etc. | Default features changed since 0.28: now HTTP + reqwest (no gRPC default). Use features `["http-proto", "reqwest-client"]` to reuse existing `reqwest` dep -- avoids pulling in `tonic`. |
| `tracing-opentelemetry` | 0.32 | Bridge existing `tracing` spans to OpenTelemetry | Version 0.32.1 (2026-01-12) requires `opentelemetry` 0.31. This is the key integration point -- Blufio already uses `tracing` everywhere, so adding an `OpenTelemetryLayer` to `tracing-subscriber` exports ALL existing instrumented spans as OTel traces with zero code changes to the 35 crates. |

**Version alignment (critical):**

| Crate | Version | Compatibility |
|---|---|---|
| `opentelemetry` | 0.31 | Base API |
| `opentelemetry_sdk` | 0.31 | Must match opentelemetry |
| `opentelemetry-otlp` | 0.31 | Must match opentelemetry |
| `tracing-opentelemetry` | 0.32 | Requires opentelemetry ^0.31 (offset-by-one versioning) |

**Integration:** New `blufio-telemetry` crate (optional, feature-gated). When `[telemetry.opentelemetry]` enabled in TOML config, register `tracing_opentelemetry::OpenTelemetryLayer` with subscriber. Endpoint via `OTEL_EXPORTER_OTLP_ENDPOINT` env var (OTel convention). Disabled by default -- adds ~15 transitive deps and runtime overhead.

### 4. OpenAPI Spec Generation

| Technology | Version | Purpose | Why Recommended |
|---|---|---|---|
| `utoipa` | 5.4 | Compile-time OpenAPI 3.1 spec generation from code annotations | Code-first: `#[utoipa::path]` on handlers, `#[derive(ToSchema)]` on types. Generates spec at compile time -- zero runtime overhead. Works with existing `serde` derives. 5M+ downloads, actively maintained. |
| `utoipa-axum` | 0.2 | Axum router integration | Requires axum ^0.8 (matches workspace exactly). `OpenApiRouter` registers handlers and generates spec simultaneously -- routes and docs stay in sync. |
| `utoipa-swagger-ui` | 9.0 | Optional Swagger UI serving (dev/enterprise only) | Serves interactive API docs at `/swagger-ui`. Behind feature flag -- bundles ~4MB of Swagger UI JS/CSS assets. |

**Integration:** Add `#[utoipa::path]` annotations to existing gateway handlers in `blufio-gateway`. Derive `ToSchema` on request/response types (most already have `serde::Serialize`). Serve spec at `GET /openapi.json`. Swagger UI optionally at `/swagger-ui` behind `swagger-ui` feature flag.

### 5. Email Channel Adapter

| Technology | Version | Purpose | Why Recommended |
|---|---|---|---|
| `lettre` | 0.11 | SMTP email sending (outbound) | The standard Rust email crate. 8M+ downloads. Supports SMTP with STARTTLS/TLS, async via `tokio1` feature. Use features `["tokio1-rustls-tls", "builder", "hostname"]` to align with workspace's rustls preference (no OpenSSL dependency). |
| `mail-parser` | 0.11 | Parse inbound emails (MIME, headers, body extraction) | Parses RFC 5322 messages, MIME multipart, attachments. 2M+ downloads, actively maintained (0.11.2 released 2026-02-14). Needed for processing incoming emails from IMAP polling or webhook inbound parse. |

**Integration:** New `blufio-email` crate implementing `ChannelAdapter`. Outbound: `lettre` SMTP transport to configured mail server. Inbound: IMAP polling loop (using raw IMAP commands over `tokio::net::TcpStream` + rustls) or webhook from email provider (SendGrid/Mailgun inbound parse -- same webhook pattern as WhatsApp). `mail-parser` extracts text from MIME messages. Config: `[channels.email]` TOML section.

### 6. Data Export

| Technology | Version | Purpose | Why Recommended |
|---|---|---|---|
| `csv` | 1.4 | CSV serialization for data export | BurntSushi's csv crate. 46M+ downloads. Zero-copy serde integration -- `#[derive(Serialize)]` on export structs, then `csv::Writer::serialize()`. Already have `serde` workspace-wide. |

**Integration:** Export module in `blufio-storage` or new `blufio-export` crate. Queries SQLite for messages/sessions/memories by date/session/type, serializes to CSV or JSON (JSON covered by existing `serde_json`). CLI: `blufio export --format csv --from 2026-01-01 --session abc`.

---

## Features That Need NO New Crates (Build with Existing Stack)

These are implementable entirely with crates already in the workspace. Each entry explains what existing crate(s) serve the purpose.

### Multi-Level Compaction (L0-L3) with Quality Scoring

**Build with:** `rusqlite` + `tokio-rusqlite` + `chrono` + existing `blufio-context/src/compaction.rs`

Current single-level compaction (`generate_compaction_summary`) becomes L1. Add:
- L0: Raw messages (already stored in SQLite)
- L2: Session-level summary merge (summarize summaries)
- L3: Cross-session archive (long-term knowledge extraction)

Quality scoring: token count ratio (`summary_tokens / original_tokens`), information density heuristic (named entities + facts preserved / total), ROUGE-like overlap check. Pure Rust math -- no external crate.

Soft/hard trigger thresholds: configurable in TOML (`[compaction] soft_threshold = 50, hard_threshold = 100` messages). Cold storage: move L3 archives to separate SQLite table with retrieval via SQL query.

### Prompt Injection Defense (L1-L5)

**Build with:** `regex` + `hmac` + `sha2` + `jsonschema` + `ring` (all already in workspace)

| Layer | Implementation | Existing Crate |
|---|---|---|
| L1: Pattern classifier | `RegexSet` of ~30 known injection patterns | `regex` 1 |
| L3: HMAC boundary tokens | Per-turn HMAC wrapping system instructions | `hmac` 0.12 + `sha2` 0.10 |
| L4: Output validator | Check LLM output for prompt leakage/tool injection | `regex` + `jsonschema` 0.28 |
| L5: Human-in-the-loop | Escalation via `EventBus` + channel notification | `blufio-bus` (existing) |

L1 patterns include: "ignore previous instructions", "you are now", "system prompt:", "reveal your instructions", "ADMIN OVERRIDE", etc. Store in a compiled `RegexSet` for O(n) multi-pattern matching. The `regex` crate's internal `aho-corasick` handles efficient multi-pattern search.

No ML classifier needed for L1 -- regex covers the practical attack surface. ML-based detection (L2) can be added later via the existing ONNX runtime if a suitable model becomes available.

### Memory Temporal Decay

**Build with:** `chrono` (already in workspace)

One line of math: `score * 0.95_f64.powf(days_since_access)`. Store `last_accessed: DateTime<Utc>` in SQLite memory table. Apply decay multiplier during recall scoring in `blufio-memory`. Importance boost: multiply by user-assigned importance weight (1.0-5.0).

### MMR (Maximal Marginal Relevance) Diversity

**Build with:** `ndarray` (already in workspace for ONNX embeddings)

MMR formula: `score = lambda * sim(query, doc) - (1 - lambda) * max_selected(sim(doc, selected_doc))`

Cosine similarity via `ndarray` dot products on existing 384-dim embedding vectors. ~20 lines of Rust. Applied as a reranking pass after initial retrieval in `blufio-memory`.

### LRU Eviction for Memory Index

**Build with:** `rusqlite` + `chrono` (already in workspace)

Bounded memory index (default 10K entries). Track `last_accessed` timestamp in SQLite. Background task runs periodically (via the new cron scheduler), queries `SELECT id FROM memories ORDER BY last_accessed ASC LIMIT (count - 10000)`, deletes excess entries. The eviction is SQL-based, not in-memory -- no `lru` crate needed.

### Hash-Chained Audit Trail

**Build with:** `sha2` + `rusqlite` + `chrono` + `serde_json` + `uuid` (all already in workspace)

Schema: `audit_trail(id TEXT PK, timestamp TEXT, actor TEXT, action TEXT, resource TEXT, details_json TEXT, prev_hash TEXT, hash TEXT)`

Each entry: `hash = SHA-256(prev_hash || timestamp || actor || action || resource || details_json)`. Tamper detection: walk chain, verify each hash. New `blufio-audit` crate with ~300 lines.

### Data Classification Framework

**Build with:** `serde` + `strum` (both already in workspace)

```rust
#[derive(Serialize, Deserialize, EnumString, Display, PartialOrd, Ord)]
pub enum DataClassification { Public, Internal, Confidential, Restricted }
```

Tag data in SQLite metadata columns. Policy enforcement: simple `>=` comparison. Classification rules configurable in TOML.

### Retention Policy Enforcement

**Build with:** `chrono` + `rusqlite` + new `cron` crate (for schedule)

Background task on cron schedule. Per-type retention config:
```toml
[retention.messages]
max_age_days = 90
[retention.sessions]
max_age_days = 365
[retention.audit]
max_age_days = 2555  # 7 years
```

Enforcement: `DELETE FROM messages WHERE created_at < datetime('now', '-90 days')`. Run in a transaction with audit logging.

### Lifecycle Hook System

**Build with:** `tokio::process::Command` + `serde` + `chrono` (all already in workspace)

11 lifecycle events: `pre_startup`, `post_startup`, `pre_shutdown`, `post_shutdown`, `pre_request`, `post_request`, `pre_response`, `post_response`, `on_error`, `on_session_create`, `on_session_destroy`.

Hooks in `BTreeMap<i32, Vec<HookConfig>>` ordered by priority. Shell hooks via `tokio::process::Command` with timeout (30s default) + environment variable injection. Subscribe to `EventBus` events.

### PII Regex Expansion

**Build with:** `regex` (already in workspace)

Extend existing `blufio-security/src/redact.rs` `REDACTION_PATTERNS` with:

| PII Type | Regex Pattern |
|---|---|
| Email | `[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}` |
| US Phone | `\+?1?\s*\(?[0-9]{3}\)?[-.\s]?[0-9]{3}[-.\s]?[0-9]{4}` |
| SSN | `\b\d{3}[-]?\d{2}[-]?\d{4}\b` |
| Credit Card | `\b(?:\d{4}[-\s]?){3}\d{4}\b` |
| IP Address | `\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b` |

Same `LazyLock<Vec<Regex>>` architecture as existing secret redaction. Add Luhn checksum validation for credit card false-positive reduction.

### GDPR Erasure Tooling

**Build with:** `rusqlite` + `chrono` + `serde_json` (all already in workspace)

CLI: `blufio gdpr erase --user <id>`. Cascading delete across: messages, memories, sessions, audit entries, cost records. Generate JSON erasure receipt: `{ user_id, tables_affected, rows_deleted, timestamp, operator }`. Retention policy integration: auto-erase after per-type retention periods.

### Clippy Unwrap Enforcement

**Build with:** Clippy (Rust toolchain, already available)

Add `#![deny(clippy::unwrap_used)]` to each library crate's `lib.rs`. Replace `.unwrap()` with `.expect("descriptive reason")` or proper `?` error propagation. CI enforcement: `cargo clippy --workspace -- -D clippy::unwrap_used`. No crate addition.

### Litestream WAL Replication

**Build with:** Config file generation (no Rust crate needed)

Litestream is a standalone Go binary (v0.5.8, released 2026-02-12). It runs as a separate process/systemd service watching Blufio's SQLite WAL file. Integration:

1. `blufio litestream init` CLI command generates `litestream.yml` from Blufio's TOML config
2. Prints systemd unit file for Litestream sidecar
3. Documents S3/GCS/Azure/SFTP replica configuration

No Rust crate exists for Litestream, and none is needed -- it's a sidecar model.

### iMessage (BlueBubbles) Adapter

**Build with:** `reqwest` + `serde` + `serde_json` (all already in workspace)

BlueBubbles exposes a REST API authenticated via `?guid=password` query param:
- Send message: `POST /api/v1/message/text`
- Get messages: `GET /api/v1/message`
- Get chats: `GET /api/v1/chat`
- Webhooks: POST to configured URL on new message, typing, read receipt (10 event types)

Same HTTP client pattern as WhatsApp Cloud API adapter. New `blufio-imessage` crate implementing `ChannelAdapter`. Requires BlueBubbles server running on macOS (same constraint as OpenClaw's BlueBubbles integration).

### SMS Adapter

**Build with:** `reqwest` + `serde` + `serde_json` (all already in workspace)

Twilio REST API: `POST https://api.twilio.com/2010-04-01/Accounts/{sid}/Messages.json` with HTTP Basic Auth (`account_sid:auth_token`). Inbound SMS via webhook (Twilio POSTs to configured URL). Same webhook pattern as other adapters.

No Twilio-specific crate needed -- community crates (`twilio`, `twilio-async`) are thin wrappers around `reqwest` with stale deps. Direct `reqwest` calls are cleaner and avoid dependency on unmaintained wrappers.

New `blufio-sms` crate implementing `ChannelAdapter`.

---

## Workspace Cargo.toml Additions

```toml
# Add to [workspace.dependencies]:

# Hot reload
arc-swap = "1.8"
notify = { version = "8", default-features = false, features = ["macos_kqueue"] }

# Scheduling
cron = "0.15"

# OpenTelemetry (all behind feature flag -- not in default build)
opentelemetry = { version = "0.31", default-features = false, features = ["trace"] }
opentelemetry_sdk = { version = "0.31", default-features = false, features = ["trace", "rt-tokio"] }
opentelemetry-otlp = { version = "0.31", default-features = false, features = ["http-proto", "reqwest-client"] }
tracing-opentelemetry = { version = "0.32", default-features = false }

# OpenAPI
utoipa = { version = "5.4", features = ["axum_extras"] }
utoipa-axum = "0.2"
utoipa-swagger-ui = { version = "9.0", features = ["axum"] }

# Email channel
lettre = { version = "0.11", default-features = false, features = ["tokio1-rustls-tls", "builder", "hostname"] }
mail-parser = "0.11"

# Data export
csv = "1.4"
```

## New Crates to Add to Workspace

| New Crate | Purpose | Key External Dependencies |
|---|---|---|
| `blufio-scheduler` | Cron parsing, tokio timer loop, systemd timer generation | `cron`, `tokio`, `blufio-config`, `blufio-bus` |
| `blufio-hooks` | Lifecycle hook registry, shell executor with timeout | `tokio` (process::Command), `blufio-bus`, `blufio-config` |
| `blufio-audit` | Hash-chained tamper-evident audit trail | `sha2`, `rusqlite`, `chrono`, `blufio-storage`, `blufio-bus` |
| `blufio-compliance` | Data classification, retention policies, GDPR erasure | `blufio-storage`, `blufio-security`, `chrono`, `cron` |
| `blufio-export` | JSON/CSV data export with date/session/type filtering | `csv`, `serde_json`, `blufio-storage` |
| `blufio-telemetry` | OpenTelemetry integration (optional, feature-gated) | `opentelemetry*`, `tracing-opentelemetry` |
| `blufio-imessage` | iMessage adapter via BlueBubbles REST API | `reqwest`, `serde`, `blufio-core` |
| `blufio-email` | Email adapter (SMTP out, IMAP/webhook in) | `lettre`, `mail-parser`, `blufio-core` |
| `blufio-sms` | SMS adapter via Twilio REST API | `reqwest`, `serde`, `blufio-core` |

**Existing crates extended (no new crate):**

| Existing Crate | New v1.5 Functionality |
|---|---|
| `blufio-context` | Multi-level compaction (L0-L3), quality scoring, quality gates |
| `blufio-memory` | Temporal decay, MMR reranking, LRU eviction, file watcher re-indexing |
| `blufio-security` | Prompt injection classifier (L1-L5), expanded PII regex patterns |
| `blufio-config` | Hot reload via `arc-swap`, `notify` file watchers |
| `blufio-gateway` | OpenAPI annotations (`utoipa`), `/openapi.json` + `/swagger-ui` endpoints |
| `blufio-storage` | Retention enforcement queries, GDPR cascading deletes, audit trail schema |
| `blufio` (binary) | New feature flags, CLI commands (export, gdpr, litestream) |

## Feature Flags (binary crate)

```toml
# Add to blufio/Cargo.toml [features]:
imessage = ["dep:blufio-imessage"]
email = ["dep:blufio-email"]
sms = ["dep:blufio-sms"]
opentelemetry = ["dep:blufio-telemetry"]
swagger-ui = ["dep:utoipa-swagger-ui"]

# Update default features to include new channels:
default = [
    # ... existing features ...
    "imessage", "email", "sms",
    # NOT opentelemetry (opt-in only)
    # NOT swagger-ui (opt-in only)
]
```

---

## Alternatives Considered

| Recommended | Alternative | Why Not |
|---|---|---|
| `cron` 0.15 (parser only) | `tokio-cron-scheduler` 0.15 | Pulls Postgres/Nats stores, own job abstraction, signal handling. Blufio has tokio timers + SQLite + EventBus already. |
| `cron` 0.15 | `croner` 3.0 | More features (L for last day, # for nth weekday) but heavier. Standard cron syntax sufficient for Blufio's needs. |
| `arc-swap` 1.8 | `RwLock<Arc<T>>` | RwLock blocks readers during write. ArcSwap is wait-free for readers -- critical for hot-path config access. |
| `notify` 8.x | `notify` 9.0-rc | RC not stable. 8.2 is production-ready. Upgrade when 9.0 hits stable release. |
| `opentelemetry-otlp` HTTP mode | `opentelemetry-otlp` gRPC (tonic) | Would pull in `tonic` + `prost` + protobuf codegen. HTTP/proto is lighter, sufficient, and reuses existing `reqwest`. |
| `lettre` (SMTP) | SendGrid/Mailgun API via `reqwest` | SMTP is protocol-level, works with any mail server. API approach creates vendor lock-in. Keep provider API as alternative config option. |
| `utoipa` 5.4 | `aide` | `aide` is newer, less mature (~500K downloads vs 5M). `utoipa` has proven axum 0.8 support and compile-time generation. |
| `csv` 1.4 | Manual string formatting | `csv` handles quoting, escaping, headers, serde integration. Reimplementing correctly is error-prone (RFC 4180 edge cases). |
| `mail-parser` 0.11 | `mailparse` | `mailparse` last release 2023 (unmaintained). `mail-parser` actively maintained (0.11.2 released 2026-02-14). |
| Direct Twilio `reqwest` calls | `twilio` crate | Community crate is thin wrapper with stale deps. Direct `reqwest` is cleaner, one fewer dependency, same amount of code. |
| Direct BlueBubbles `reqwest` calls | No Rust crate exists | BlueBubbles is a simple REST API with query-param auth. No SDK needed. |
| SQL-based LRU eviction | `lru` crate | Eviction is `DELETE FROM ... ORDER BY last_accessed` -- SQL query, not in-memory data structure. `lru` crate solves the wrong problem. |
| Regex prompt injection (L1) | ML classifier crate | No production-quality Rust prompt injection ML crate exists. Regex is the pragmatic L1; ML can be added later as L2 via existing ONNX runtime. |

## What NOT to Use

| Avoid | Why | Use Instead |
|---|---|---|
| `tokio-cron-scheduler` | Over-engineered for embedded use; unnecessary Postgres/Nats deps | `cron` parser + `tokio::time` loop |
| `tonic` (gRPC for OTel) | Heavy dep chain; unnecessary when HTTP OTLP works | `opentelemetry-otlp` with `http-proto` + `reqwest-client` |
| `twilio` crate | Thin wrapper with stale dependencies | Direct `reqwest` calls to Twilio REST API |
| `lru` crate | Memory eviction is SQL-based, not in-memory | `DELETE FROM ... ORDER BY last_accessed LIMIT n` |
| `notify` 9.0-rc | Release candidate, not stable | `notify` 8.x stable |
| `mailparse` | Unmaintained since 2023 | `mail-parser` 0.11 |
| `utoipa-swagger-ui` in default build | Bundles ~4MB of Swagger UI assets in binary | Feature-gated, dev/enterprise only |
| Any PII detection ML crate | No mature Rust option; regex covers 95%+ of cases | Extend existing regex patterns in `blufio-security` |
| `imap` crate for email inbound | Adds complexity; webhook-based inbound is simpler | IMAP polling with raw TcpStream or email provider webhook |
| `opentelemetry` in default features | +15 transitive deps, runtime overhead | Feature-gated `opentelemetry` flag, disabled by default |

---

## Version Compatibility Matrix

| Package A | Compatible With | Notes |
|---|---|---|
| `opentelemetry` 0.31 | `opentelemetry_sdk` 0.31, `opentelemetry-otlp` 0.31 | Must use same minor version across all OTel crates |
| `tracing-opentelemetry` 0.32 | `opentelemetry` 0.31 | Offset-by-one versioning (documented behavior) |
| `utoipa` 5.4 | `utoipa-axum` 0.2, `utoipa-swagger-ui` 9.0 | utoipa-axum requires utoipa ^5.0 |
| `utoipa-axum` 0.2 | `axum` 0.8 | Matches workspace axum version exactly |
| `notify` 8.2 | Rust 1.85+ | MSRV matches workspace rust-version |
| `arc-swap` 1.8 | Rust 1.31+ | No version conflict possible |
| `lettre` 0.11 | `rustls` via `tokio1-rustls-tls` | Aligns with workspace rustls preference |
| `cron` 0.15 | `chrono` 0.4 | Uses workspace chrono for schedule iteration |
| `csv` 1.4 | `serde` 1 | Uses workspace serde for record serialization |
| `mail-parser` 0.11 | No shared deps | Standalone parsing library |
| All OTel crates | MSRV 1.75 | Compatible with workspace 1.85 |

---

## Dependency Budget Impact

**Current:** ~75 direct crates (workspace Cargo.toml lists 52 workspace deps + per-crate local deps).
**Constraint:** <80 direct crates for tractable audit surface.

| Addition | Direct Deps Added | Transitive Impact | Notes |
|---|---|---|---|
| `arc-swap` | +1 | 0 | Zero transitive deps |
| `notify` | +1 | +2 (filetime, walkdir) | Lightweight |
| `cron` | +1 | +1 (nom for parsing) | Lightweight |
| `utoipa` + `utoipa-axum` | +2 | +1 (indexmap transitively present) | Compile-time only overhead |
| `csv` | +1 | +1 (csv-core) | Lightweight |
| `lettre` + `mail-parser` | +2 | +3 (email-encoding, idna, hostname) | Behind feature flag |
| `utoipa-swagger-ui` | +1 | +3 (behind feature flag) | Dev/enterprise only |
| OTel stack (4 crates) | +4 | +8-12 | Behind feature flag |
| **Total (default build)** | **+8** | **+8** | Within 80-crate budget |
| **Total (all features)** | **+13** | **+20-25** | Acceptable with feature flags |

The default build (without OTel and Swagger UI) adds 8 direct + 8 transitive deps, staying within the <80 crate constraint. OTel and Swagger UI are feature-gated and only compiled when explicitly enabled.

---

## Sources

### Versions Verified via crates.io API (HIGH confidence)
- [arc-swap](https://crates.io/crates/arc-swap) -- v1.8.2, released 2026-02-14
- [notify](https://crates.io/crates/notify) -- v8.2.0 stable, v9.0.0-rc.2 RC (pinning to 8.x)
- [cron](https://crates.io/crates/cron) -- v0.15.0, released 2025-01-14
- [opentelemetry](https://crates.io/crates/opentelemetry) -- v0.31.0, released 2025-09-25
- [opentelemetry_sdk](https://crates.io/crates/opentelemetry_sdk) -- v0.31.0
- [opentelemetry-otlp](https://crates.io/crates/opentelemetry-otlp) -- v0.31.0, HTTP default since 0.28
- [tracing-opentelemetry](https://crates.io/crates/tracing-opentelemetry) -- v0.32.1, released 2026-01-12, requires opentelemetry ^0.31
- [utoipa](https://crates.io/crates/utoipa) -- v5.4.0, released 2025-06-16
- [utoipa-axum](https://crates.io/crates/utoipa-axum) -- v0.2.0, requires axum ^0.8, utoipa ^5.0
- [utoipa-swagger-ui](https://crates.io/crates/utoipa-swagger-ui) -- v9.0.2
- [lettre](https://crates.io/crates/lettre) -- v0.11.19, released 2025-10-08
- [mail-parser](https://crates.io/crates/mail-parser) -- v0.11.2, released 2026-02-14
- [csv](https://crates.io/crates/csv) -- v1.4.0, released 2025-10-17

### Official Documentation (HIGH confidence)
- [OpenTelemetry Rust docs](https://opentelemetry.io/docs/languages/rust/) -- traces Beta, MSRV 1.75
- [tracing-opentelemetry GitHub](https://github.com/tokio-rs/tracing-opentelemetry) -- offset-by-one versioning, compatibility notes
- [BlueBubbles REST API](https://docs.bluebubbles.app/server/developer-guides/rest-api-and-webhooks) -- webhook + REST docs, 10 event types, guid auth
- [Litestream](https://litestream.io/) -- v0.5.8 (2026-02-12), Go binary, WAL replication sidecar
- [utoipa GitHub](https://github.com/juhaku/utoipa) -- axum 0.8 support, compile-time generation

### Community / WebSearch (MEDIUM confidence)
- [Twilio Rust SMS tutorial](https://www.twilio.com/en-us/blog/developers/tutorials/integrations/send-sms-with-twilio-rust-openapi) -- direct reqwest pattern
- [arc-swap patterns](https://docs.rs/arc-swap/latest/arc_swap/docs/patterns/index.html) -- config hot reload pattern

### Local Codebase Verification
- `Cargo.toml` -- confirmed all existing workspace deps and versions
- `blufio-security/src/redact.rs` -- confirmed existing regex-based redaction (4 patterns, extensible)
- `blufio-context/src/compaction.rs` -- confirmed single-level compaction (L1 base for multi-level)
- `blufio-bus/src/events.rs` -- confirmed EventBus event types (extensible for new subsystems)
- `blufio-memory/src/store.rs` -- confirmed BM25 search (base for temporal decay/MMR extension)
- `blufio-gateway/Cargo.toml` -- confirmed axum 0.8 (compatible with utoipa-axum 0.2)

---
*Stack research for: Blufio v1.5 PRD Gap Closure*
*Researched: 2026-03-10*
