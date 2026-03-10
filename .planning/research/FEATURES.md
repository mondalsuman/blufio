# Feature Research

**Domain:** PRD Gap Closure for a multi-provider AI agent platform (Rust)
**Researched:** 2026-03-10
**Confidence:** HIGH

## Scope

This document covers the 15 NEW feature domains targeted for v1.5 PRD Gap Closure. All existing shipped features (FSM agent loop, 5 LLM providers, 8 channel adapters, three-zone context engine with single-pass compaction, WASM skills, SQLCipher, AES-256 vault, circuit breakers, degradation ladder, FormatPipeline, accurate token counting, Prometheus metrics, MCP client+server, hybrid memory search, event bus) are treated as foundation -- they are dependencies, not scope.

The v1.5 goal: close every remaining gap between the PRD vision and the shipped product.

---

## Feature Landscape

### Table Stakes (Users Expect These)

Features that any production AI agent platform must have. Missing these means the product feels incomplete for operators running Blufio for months on a VPS.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Multi-level context compaction (L0-L3) with quality scoring | Current compaction is single-pass, splits at history midpoint, and has no quality validation. Long-running sessions (days/weeks) accumulate compaction summaries that lose critical context. Any agent platform advertising long-term conversations must preserve key facts through compaction rounds. OpenClaw loses context entirely after ~50 turns. | HIGH | Extends existing `blufio-context/compaction.rs`. L0=raw messages, L1=turn-pair summaries, L2=session summary, L3=cross-session archive. Each level retains entity/decision graphs. Quality gates use probe-based validation: after compaction, test that key facts survive via LLM probe queries. ACON research shows 26-54% peak token reduction while preserving 95%+ accuracy with multi-step validation. Existing `DynamicZone` triggers compaction at 70% threshold -- extend with tiered triggers (L0 at 50%, L1 at 70%, L2 at 85%). |
| Prompt injection defense layers | OWASP Top 10 for LLM Apps 2025 ranks prompt injection #1. Attack success rates reach 84% in agentic systems. Blufio has MCP description sanitization and trust zone labeling but no systematic input/output defense. Any agent system executing tools (WASM skills, MCP) from user-influenced context is vulnerable to injection. | HIGH | 5-layer defense as specified in PRD: L1=regex pattern classifier (known attack signatures), L3=HMAC boundary tokens (cryptographic separation of system/user content), L4=output validator (LLM output screened before tool execution), L5=human-in-the-loop (configurable confirmation for high-risk operations). L2 was omitted in PRD -- reserved for future ML classifier. Existing `blufio-security` crate handles TLS/SSRF/redaction; extend with injection defense module. |
| PII detection and redaction | Existing `redact.rs` handles API keys and bearer tokens only. No detection of user PII (email, phone, SSN, credit cards). For a personal AI agent handling private conversations, PII appearing in logs, exports, or LLM context is a compliance liability. Operators in EU/California expect PII awareness. | MEDIUM | Extend `blufio-security/redact.rs` with regex patterns: email (`[\w.+-]+@[\w-]+\.[\w.]+`), phone (international formats), SSN (`\d{3}-\d{2}-\d{4}`), credit cards (Luhn-validated 13-19 digit patterns). Use regex-only approach for v1.5 -- ML-based NER is overkill for a single-binary agent. Apply redaction in log output (existing `RedactingWriter`), data exports, and optionally in LLM context injection. |
| Data export (JSON, CSV) | Any system storing user data must provide export capability. GDPR Article 20 (data portability) requires machine-readable export. Operators need export for backup verification, migration, and debugging. | LOW | Export sessions, messages, memories, cost records filtered by session/date/type. JSON for programmatic use, CSV for spreadsheet analysis. CLI command `blufio export --format json --session <id> --from <date> --to <date>`. Reads directly from SQLite via existing `blufio-storage` queries. |
| Retention policy enforcement | Long-running agents accumulate unbounded data. Without retention policies, SQLite database grows indefinitely. Cost records, old session messages, and superseded memories should be automatically cleaned. Operators expect configurable TTLs per data type. | MEDIUM | TOML config per data type: `[retention] messages_days = 90`, `sessions_days = 180`, `cost_records_days = 365`, `memories_days = 0` (0=forever). Background task runs on configurable interval (default: daily). Deletes records older than retention period. Must respect audit trail (hash chain entries are never deleted). Must respect GDPR erasure requests (immediate, not waiting for retention). |
| OpenAPI spec generation | Blufio ships an API gateway with 15+ endpoints. Any production API needs documentation. OpenAPI enables client SDK generation, Swagger UI, and API testing. Without it, integrators must read source code or guess at payloads. | MEDIUM | Use `utoipa` + `utoipa-axum` crates for compile-time OpenAPI 3.1 generation from existing axum handlers. Add `#[utoipa::path]` annotations to gateway handlers. Serve spec at `/openapi.json` and optional Swagger UI at `/docs`. Does not require restructuring existing handlers -- annotation-only changes. |
| Clippy unwrap enforcement | 80K LOC codebase with uncounted `.unwrap()` calls. Any `unwrap` in library code is a potential panic in production. For a system targeting months of uptime on a $4/month VPS, panics are unacceptable. | LOW | Add `#![deny(clippy::unwrap_used)]` to all library crate `lib.rs` files. Replace `.unwrap()` with `.expect("reason")`, `?`, or `.unwrap_or_default()` as appropriate. Binary crate (`blufio/main.rs`) can keep `unwrap()` in startup paths where panicking is acceptable. |

### Differentiators (Competitive Advantage)

Features that set Blufio apart from OpenClaw and other agent platforms. These are not table stakes but create significant competitive advantage.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Hash-chained tamper-evident audit trail | No open-source AI agent platform provides cryptographic audit trails. Every action (tool execution, memory modification, config change, provider call) gets a hash-chained log entry where altering any historical record breaks the chain. Enables enterprise trust and regulatory compliance. AuditableLLM research shows negligible overhead: 3.4ms/step, 5.7% slowdown. | MEDIUM | New `blufio-audit` crate or module in `blufio-storage`. Each entry: `{id, timestamp, actor, action, target, detail, prev_hash, hash}` where `hash = SHA-256(prev_hash || serialized_entry)`. Store in dedicated `audit_trail` SQLite table. Verification CLI command `blufio audit verify` walks chain and reports breaks. Integrate with EventBus -- subscribe to all event types and log audit entries. Audit entries are append-only; retention policies NEVER delete them (separate from message retention). |
| Data classification framework (4 levels) | No competitor classifies data sensitivity levels. With 4 levels (Public/Internal/Confidential/Restricted), Blufio can enforce per-level controls: Restricted data never leaves the system, Confidential data is encrypted at rest (already via SQLCipher), Internal data is access-controlled, Public data flows freely. This is table stakes for enterprise but a differentiator in the AI agent space. | MEDIUM | Enum `DataClassification { Public, Internal, Confidential, Restricted }` in `blufio-core`. Tag memories, messages, exports, and config values with classification level. Controls matrix: Restricted = never exported + never in LLM context + encrypted at rest. Confidential = encrypted at rest + redacted in logs. Internal = access-controlled + audit-logged. Public = no restrictions. Classification can be set explicitly or inferred (messages with PII auto-classify as Confidential). |
| Memory temporal decay with MMR diversity | Current retriever uses RRF fusion (vector + BM25) with confidence boost but no temporal decay and no diversity enforcement. Older memories rank equally to recent ones. Redundant memories (same topic, slight variations) dominate results. OpenClaw implements both decay and MMR. Blufio's memory retrieval must match or exceed. | MEDIUM | Temporal decay: `score *= 0.95^days_since_creation` (configurable decay factor). Half-life ~14 days. MMR re-ranking: after RRF fusion, apply `finalScore = lambda * relevance - (1-lambda) * max_similarity_to_already_selected` with lambda=0.7. This runs on the already-scored results, so zero additional API calls. Both apply in `HybridRetriever::retrieve()` after the existing RRF step. Add `importance` field to Memory struct for manual boost (explicit memories get importance=1.0, extracted get 0.6). |
| Lifecycle hook system (11 events) | No agent platform provides extensible lifecycle hooks. Hooks let operators run shell commands at specific lifecycle points: backup before shutdown, notify on degradation, sync memory on session close, restart services on config change. Kubernetes/Nomad/Docker all have lifecycle hooks because the pattern is proven. | MEDIUM | 11 lifecycle events: `pre_start`, `post_start`, `pre_shutdown`, `post_shutdown`, `session_created`, `session_closed`, `pre_compaction`, `post_compaction`, `degradation_changed`, `config_reloaded`, `memory_extracted`. Each hook defined in TOML with priority (BTreeMap ordering), command, timeout, and sandbox flag. Hooks run as shell subprocesses with configurable environment variables. Sandboxed hooks run with restricted PATH and no network. Subscribe to EventBus events to trigger hooks asynchronously. |
| Hot reload (config, TLS, plugins) | No competitor supports zero-downtime config reload. Blufio targets months of uptime; restarting for a config change breaks that promise. ArcSwap provides wait-free, lock-free atomic pointer swaps. Combined with `notify` crate for file watching, config changes apply within seconds without dropping connections. | HIGH | Three reload targets: (1) Config: watch `blufio.toml` with `notify` crate, parse new config, validate, swap via `ArcSwap<BlufioConfig>`. All components that hold config references must read through ArcSwap. (2) TLS certs: use `tls-hot-reload` crate or implement `rustls::server::ResolvesServerCert` with file watcher. Certificate rotation applies without connection drop. (3) Plugin state: re-scan skill directory, reload changed WASM modules, verify signatures. The config change is the hardest because every crate reads config at construction time -- must refactor to read through shared ArcSwap. |
| Cron/scheduler system | No agent platform provides built-in scheduled tasks. Operators want scheduled memory cleanup, backup triggers, health report generation, and custom skill execution on cron schedules. Currently requires external crontab or systemd timers. | MEDIUM | Use `tokio-cron-scheduler` crate (active, 0.15.x, tokio-native). Define schedules in TOML: `[[cron]] name = "daily-backup"`, `schedule = "0 3 * * *"`, `command = "blufio backup"`. Also generate systemd timer unit files via `blufio cron generate-timers`. Built-in tasks: memory cleanup, cost report, health check, retention enforcement. Custom tasks run as shell commands (same sandbox as hooks). Store last-run timestamps in SQLite to handle restarts gracefully. |
| GDPR erasure tooling | OpenClaw has no GDPR tooling. Blufio can be the first open-source agent platform with built-in right-to-erasure support. EDPB 2025 CEF report found that most controllers lack appropriate procedures. CLI command `blufio gdpr erase --user <id>` deletes all user data: messages, memories, session metadata, cost records. Transparency disclosure via `blufio gdpr report --user <id>` shows what data is held. | MEDIUM | Must handle: (1) message deletion from all sessions, (2) memory deletion and re-embedding of remaining memories, (3) cost record anonymization (keep aggregates, remove user association), (4) audit trail annotation (log erasure event without deleting audit entries -- GDPR allows keeping processing records). Also: retention policy enforcement as automated erasure. Export before erasure as safety net. Backup exclusion -- must document that backup files may contain pre-erasure data. |
| iMessage, Email, SMS channel adapters | Expanding from 8 to 11 channels. iMessage reaches iOS users (massive market). Email enables async agent interaction. SMS provides universal reach. These three channels cover the remaining major communication platforms not yet supported. | MEDIUM-HIGH | **iMessage:** BlueBubbles REST API + webhook adapter. Requires macOS server running BlueBubbles. Poll or webhook for incoming messages. REST API for sending. OpenClaw has a BlueBubbles plugin as reference. **Email:** IMAP polling for incoming + SMTP via `lettre` for outgoing. `async-imap` crate for async IMAP. Map email threads to sessions via In-Reply-To headers. **SMS:** Twilio Programmable Messaging API. Webhook for incoming (same pattern as WhatsApp Cloud API). REST API for outgoing. No official Twilio Rust SDK -- use reqwest directly (same as WhatsApp adapter). Each adapter implements existing `ChannelAdapter` trait. |
| OpenTelemetry distributed tracing | Blufio has Prometheus metrics but no distributed tracing. For operators running Blufio behind reverse proxies, load balancers, or with MCP servers, trace propagation is essential for debugging latency. OpenTelemetry is the industry standard. | MEDIUM | Use `opentelemetry` + `tracing-opentelemetry` crates. Bridge existing `tracing` spans to OpenTelemetry. Export via OTLP (gRPC or HTTP) to Jaeger, Tempo, or any OTLP-compatible backend. Optional and disabled by default (zero overhead when disabled). Config: `[opentelemetry] enabled = false`, `endpoint = "http://localhost:4317"`, `service_name = "blufio"`. Key spans: agent loop iteration, LLM provider call, tool execution, memory retrieval, context assembly. Trace context propagation through MCP calls. |
| Litestream WAL-based replication | SQLite is a single-file database. Disk failure = total data loss. Litestream provides continuous WAL-frame streaming to S3/GCS/Azure Blob with point-in-time recovery. For a $4/month VPS, this is the difference between "acceptable risk" and "production-grade." | LOW | Litestream runs as a sidecar process -- no code changes in Blufio. Ship `litestream.yml` config template. Document setup in operator guide. CLI command `blufio litestream init` generates config from `blufio.toml` storage path. `blufio litestream status` checks replication lag. Blufio's existing WAL mode is compatible (Litestream requires WAL). Only consideration: Litestream takes over checkpointing, so Blufio must not set `wal_autocheckpoint` to 0 conflictingly. |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| ML-based PII detection (NER models) | "Regex misses context-dependent PII like names and addresses" | Adds ONNX model loading overhead (already have one for embeddings). False positive rate on names is high (is "Paris" a city or a name?). Increases binary size. For a personal agent with one user, the user's own data is not "unknown PII" -- they chose to share it. | Regex patterns for structured PII (email, phone, SSN, CC). Users tag sensitive memories manually via data classification. ML-based NER deferred to v2+ if demand materializes. |
| Real-time PII scanning of all LLM context | "Scan every message sent to LLM providers for PII before transmission" | Adds latency to every LLM call (regex scan of full context window = 100K+ tokens). PII in conversation IS the conversation -- the user is talking about their life. Blocking PII from reaching the LLM defeats the purpose of a personal agent. | PII redaction in logs and exports only. Optional PII stripping for LLM context as opt-in config. Data classification framework handles what should/should not leave the system. |
| Blockchain-based audit trail | "Hash chains are not truly tamper-proof without distributed consensus" | Blockchain adds massive complexity, external dependencies (node, consensus, storage), and latency for zero practical benefit in a single-instance system. The threat model is "detect tampering," not "prevent tampering by a Byzantine adversary." | SHA-256 hash-chained log in SQLite. Operator can periodically snapshot chain head hash to external witness (cloud KMS, git repo, email) for independent verification. This is what Certificate Transparency uses (Trillian) and it works at internet scale. |
| Full GDPR consent management UI | "Need cookie banners, consent tracking, and preference centers" | Blufio is a CLI/API agent, not a web application. There is no web UI for users to interact with. Consent management requires a user-facing interface that does not exist. | Document that Blufio operators are data controllers responsible for obtaining consent. Provide GDPR tooling (erasure, export, transparency) but not consent UI. Operators integrate with their own consent management systems. |
| Cross-provider session migration on hot reload | "When config changes the default provider, migrate active sessions to the new provider" | Different providers have different tokenizers, context window sizes, tool calling formats, and prompt caching. Mid-conversation provider switch produces incoherent responses and wastes cached context. | Hot reload applies to new sessions only. Active sessions complete on their current provider. Operator can force-close sessions via CLI if immediate migration is needed. Document this as a design decision. |
| Embedded Litestream (compiled into binary) | "Ship Litestream as part of the Blufio binary for single-binary purity" | Litestream is a Go binary. Embedding it means CGO or subprocess management. Litestream's lifecycle (continuous WAL monitoring) is better managed as a sidecar. Single-binary constraint already has exceptions (signal-cli sidecar). | Ship config templates and CLI helpers. `blufio litestream init` generates config. Document sidecar deployment in operator guide. Docker compose includes Litestream container. |
| Automatic PII-based data classification | "Automatically classify all data based on PII content detection" | Auto-classification creates false confidence. A message mentioning "credit card" in casual conversation gets classified as Restricted. The user then cannot access their own conversation. Classification should be explicit or rule-based, not inference-based. | Rule-based classification: messages with detected PII patterns auto-tag as Confidential (not Restricted). Operators define classification rules in TOML. Users can manually classify memories. Restricted level requires explicit operator designation. |

---

## Feature Dependencies

```
Prompt Injection Defense
    |
    +--requires--> blufio-security crate (existing, extend)
    +--requires--> HMAC primitives (already have hmac via blufio-vault)
    |
    +--enhances--> Agent loop (pre/post LLM call validation)
    +--enhances--> WASM skill sandbox (tool call validation)
    +--enhances--> MCP client (external tool output validation)

Data Classification Framework
    |
    +--requires--> blufio-core types (new enum)
    |
    +--enables--> PII Redaction (auto-classify PII-containing data as Confidential)
    +--enables--> Retention Policies (per-classification retention rules)
    +--enables--> GDPR Tooling (classification drives export/erasure scope)
    +--enables--> Data Export (classification drives what is exportable)

PII Detection
    |
    +--requires--> blufio-security/redact.rs (extend existing patterns)
    |
    +--enhances--> Data Classification (auto-tag PII-detected content)
    +--enhances--> GDPR Tooling (find all user PII for erasure)
    +--enhances--> Data Export (redact PII from exports)
    +--enhances--> Audit Trail (redact PII from audit entries)

Hash-Chained Audit Trail
    |
    +--requires--> EventBus (subscribe to all events for audit logging)
    +--requires--> blufio-storage (new audit_trail table)
    +--requires--> SHA-256 (already have via sha2 crate in workspace)
    |
    +--enhances--> GDPR Tooling (erasure events logged in audit trail)
    +--enhances--> Data Classification (classification changes logged)

Retention Policies
    |
    +--requires--> Data Classification (per-classification retention rules)
    +--requires--> Cron/Scheduler (automated enforcement on schedule)
    |
    +--enhances--> GDPR Tooling (retention = automated erasure)
    +--conflicts--> Audit Trail (audit entries exempt from retention deletion)

GDPR Tooling
    |
    +--requires--> PII Detection (find user PII across all tables)
    +--requires--> Data Export (export before erasure as safety net)
    +--requires--> Audit Trail (log erasure events)
    +--requires--> Data Classification (scope erasure by classification)

Multi-Level Compaction
    |
    +--requires--> blufio-context (extend existing compaction.rs)
    +--requires--> Accurate token counting (already shipped in v1.4)
    |
    +--enhances--> Memory system (compacted summaries feed memory extraction)
    +--independent-of--> Security/compliance features

Memory Enhancements (temporal decay, MMR, LRU)
    |
    +--requires--> blufio-memory (extend existing retriever.rs, store.rs)
    |
    +--independent-of--> Security/compliance features
    +--independent-of--> Compaction (different subsystem)

Lifecycle Hooks
    |
    +--requires--> EventBus (hook triggers from bus events)
    +--requires--> Config system (TOML hook definitions)
    |
    +--enhances--> Hot Reload (hooks fire on config reload)
    +--enhances--> Cron/Scheduler (hooks can trigger scheduled tasks)

Hot Reload
    |
    +--requires--> ArcSwap (new dependency)
    +--requires--> notify crate (file watcher, new dependency)
    +--requires--> Config refactor (components read through ArcSwap)
    |
    +--enhances--> Lifecycle Hooks (config_reloaded hook fires)
    +--enhances--> TLS cert rotation (zero-downtime)

Cron/Scheduler
    |
    +--requires--> tokio-cron-scheduler (new dependency)
    +--requires--> blufio-storage (last-run timestamps)
    |
    +--enables--> Retention Policy enforcement (scheduled cleanup)
    +--enables--> Background memory validation
    +--enhances--> Lifecycle Hooks (cron tasks can trigger hooks)

Channel Adapters (iMessage, Email, SMS)
    |
    +--requires--> ChannelAdapter trait (already exists)
    +--requires--> blufio-config (new config sections)
    +--requires--> FormatPipeline integration (already wired in v1.4)
    |
    +--independent-of--> All other v1.5 features

OpenTelemetry
    |
    +--requires--> opentelemetry + tracing-opentelemetry crates (new deps)
    +--requires--> blufio-config (new otel config section)
    |
    +--independent-of--> All other v1.5 features

OpenAPI Spec
    |
    +--requires--> utoipa + utoipa-axum crates (new deps)
    +--requires--> blufio-gateway handlers (annotation changes)
    |
    +--independent-of--> All other v1.5 features

Litestream Replication
    |
    +--requires--> WAL mode (already enabled)
    +--requires--> Config templates (no code changes)
    |
    +--independent-of--> All other v1.5 features
```

### Dependency Notes

- **Data Classification should come before PII Detection:** Classification framework provides the enum and tagging infrastructure that PII detection populates. Building PII detection first means retrofitting classification tags later.
- **PII Detection should come before GDPR Tooling:** GDPR erasure needs to find all user PII across all tables. PII detection patterns provide the scanning capability.
- **Audit Trail should come before GDPR Tooling:** Erasure events must be logged in the audit trail. Building GDPR tooling without audit trail means erasure events are unauditable.
- **Cron/Scheduler should come before Retention Policies:** Retention enforcement runs on a schedule. Without the scheduler, retention requires external crontab (defeats single-binary value).
- **Compaction and Memory enhancements are independent of security/compliance features.** They can be developed on a parallel track.
- **Channel adapters are fully independent.** Each adapter is a separate crate implementing the existing ChannelAdapter trait. No dependency on any other v1.5 feature.
- **OpenTelemetry, OpenAPI, and Litestream are fully independent.** They can be built at any point in the milestone.
- **Hot Reload is the highest-risk feature.** It requires refactoring how every component reads config. This should be scoped carefully and potentially deferred to late in the milestone.

---

## MVP Definition

### Must Ship (v1.5 Core)

- [ ] **Multi-level compaction with quality scoring** -- Fixes the single biggest long-term usability problem: context degradation over extended conversations. Directly addresses core value of running for months.
- [ ] **Prompt injection defense (L1, L3, L4)** -- Security table stakes for any system executing tools from user-influenced context. L5 (human-in-loop) is stretch.
- [ ] **Cron/scheduler system** -- Unblocks retention policies, automated memory cleanup, and background tasks. Foundation for operational automation.
- [ ] **Memory temporal decay and MMR diversity** -- Fixes memory retrieval quality for long-running agents. Direct user-facing improvement.
- [ ] **Hash-chained audit trail** -- Differentiator. Enables compliance storytelling and operator trust.
- [ ] **Data classification framework** -- Foundation for PII, GDPR, retention, and export features.
- [ ] **Retention policy enforcement** -- Prevents unbounded database growth. Operational necessity for long-running agents.
- [ ] **PII detection patterns** -- Extends existing redaction system. Compliance foundation.
- [ ] **Data export (JSON, CSV)** -- GDPR Article 20 compliance. Simple to implement on existing queries.
- [ ] **GDPR erasure tooling** -- First open-source agent platform with built-in erasure. Competitive differentiator.
- [ ] **Clippy unwrap enforcement** -- Code quality. Prevents panics in production.

### Should Ship (v1.5 Extended)

- [ ] **OpenAPI spec generation** -- API documentation. High value, moderate effort.
- [ ] **Lifecycle hook system** -- Operator extensibility. Enables custom automation without code changes.
- [ ] **OpenTelemetry tracing** -- Observability upgrade. Optional, disabled by default.
- [ ] **Litestream replication setup** -- Config templates and CLI helpers. Low effort, high value for disaster recovery.
- [ ] **iMessage adapter** -- BlueBubbles REST API. Extends channel coverage.
- [ ] **Email adapter** -- IMAP/SMTP. Async agent interaction.
- [ ] **SMS adapter** -- Twilio API. Universal reach.

### Defer if Time-Constrained (v1.6+)

- [ ] **Hot reload (config, TLS, plugins)** -- High complexity, requires config access refactoring across all crates. High value but risky for a gap-closure milestone. Better as a focused phase.
- [ ] **Background memory validation + file watcher re-indexing** -- Nice to have but not blocking any other feature.
- [ ] **Context engine token budget enforcement verification** -- Testing/validation task, not a feature.

---

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Multi-level compaction | HIGH | HIGH | P1 -- Core value: long-term agent reliability |
| Prompt injection defense | HIGH | HIGH | P1 -- Security table stakes |
| Data classification | HIGH | LOW | P1 -- Foundation for compliance features |
| PII detection | HIGH | LOW | P1 -- Compliance foundation |
| Audit trail (hash-chained) | HIGH | MEDIUM | P1 -- Differentiator, compliance enabler |
| Cron/scheduler | HIGH | MEDIUM | P1 -- Unblocks retention and automation |
| Retention policies | HIGH | MEDIUM | P1 -- Operational necessity |
| Memory decay + MMR | MEDIUM | MEDIUM | P1 -- User-facing quality improvement |
| GDPR erasure + export | HIGH | MEDIUM | P1 -- Compliance differentiator |
| Data export | MEDIUM | LOW | P1 -- GDPR requirement, simple to build |
| Clippy unwrap enforcement | MEDIUM | LOW | P1 -- Code quality, prevents panics |
| OpenAPI spec | MEDIUM | MEDIUM | P2 -- API documentation |
| Hook system | MEDIUM | MEDIUM | P2 -- Operator extensibility |
| OpenTelemetry | MEDIUM | MEDIUM | P2 -- Observability upgrade |
| Litestream setup | MEDIUM | LOW | P2 -- Disaster recovery |
| iMessage adapter | LOW | MEDIUM | P2 -- Channel expansion |
| Email adapter | LOW | MEDIUM | P2 -- Channel expansion |
| SMS adapter | LOW | MEDIUM | P2 -- Channel expansion |
| Hot reload | HIGH | HIGH | P3 -- Defer to focused phase |
| Memory validation/file watcher | LOW | MEDIUM | P3 -- Nice to have |

**Priority key:**
- P1: Must have for v1.5 milestone
- P2: Should have, add within v1.5 if schedule allows
- P3: Nice to have, defer to v1.6 if time-constrained

---

## Competitor Feature Analysis

| Feature | OpenClaw | LangChain / LangGraph | Blufio v1.5 Approach |
|---------|----------|----------------------|----------------------|
| Context compaction | Single-pass recursive summary, loses facts after ~50 turns | Memory/summary buffer, no multi-level | L0-L3 tiered compaction with quality gates. Probe-based validation ensures key facts survive compression. Archive system for cold storage retrieval. |
| Prompt injection defense | None -- trusts all input | Experimental guardrails via LangChain-Guard, no structural defense | 5-layer defense: regex classifier, HMAC boundary tokens, output validator, human-in-loop. Structural separation of system/user content. |
| Scheduled tasks | None built-in, relies on external cron | Scheduled graphs via LangGraph Cloud | Built-in cron with TOML config, systemd timer generation, SQLite last-run tracking. Single-binary includes scheduler. |
| Memory temporal decay | 0.995/hour decay factor, 30-day half-life | Optional in vectorstores with metadata filtering | Configurable 0.95^days decay, importance boost, MMR diversity (lambda=0.7). Applied post-RRF-fusion in existing hybrid retriever. |
| Audit trail | None | None | SHA-256 hash-chained tamper-evident log in SQLite. EventBus integration for automatic capture. CLI verification. |
| Data classification | None | None | 4-level framework (Public/Internal/Confidential/Restricted) with per-level controls matrix. |
| Retention policies | None -- unbounded data growth | None built-in | Configurable per-type TTLs in TOML. Automated enforcement via cron scheduler. Audit trail exempt. |
| PII detection | None | Optional via Presidio integration | Built-in regex patterns for email, phone, SSN, CC. Extends existing secret redaction. Applied in logs, exports, optionally in LLM context. |
| GDPR tooling | None | None | CLI commands for erasure, export, transparency report. First open-source agent platform with built-in GDPR support. |
| Hook system | Lifecycle hooks for some events | Graph callbacks | 11 lifecycle events with BTreeMap priority, shell-based, sandboxed, configurable per-event. |
| Hot reload | Must restart for config changes | N/A (cloud service) | ArcSwap + notify for config/TLS/plugins. Zero-downtime updates. |
| Channel coverage | 15+ channels (Node.js) | Not a channel system | 8 existing + 3 new (iMessage via BlueBubbles, Email via IMAP/SMTP, SMS via Twilio) = 11 channels |
| OpenTelemetry | None | Built-in via LangSmith/LangFuse | Optional OTLP export via tracing-opentelemetry. Disabled by default. Zero overhead when off. |
| OpenAPI spec | None (no HTTP API) | LangServe has OpenAPI | utoipa + utoipa-axum for compile-time OpenAPI 3.1 from existing axum handlers. |
| Replication | None (JSONL files on disk) | Cloud-managed | Litestream sidecar for WAL-based continuous replication to S3/GCS. Config templates + CLI helpers. |

---

## Implementation Details: Key Features

### Multi-Level Compaction Design

The existing compaction in `blufio-context/compaction.rs` does a single LLM call to summarize the older half of conversation history. The v1.5 upgrade introduces four levels:

| Level | Content | Trigger | Retention |
|-------|---------|---------|-----------|
| L0 | Raw messages | Always (current behavior) | Until compacted to L1 |
| L1 | Turn-pair summaries (2-4 messages -> 1 summary) | Dynamic zone hits 50% of context budget | 24 hours after L2 creation |
| L2 | Session summary (all L1 summaries -> coherent narrative) | Dynamic zone hits 70% of context budget (current threshold) | Until session closes |
| L3 | Cross-session archive (L2 summaries from closed sessions) | Session close | Governed by retention policy |

**Quality scoring** uses probe-based validation:
1. Before compaction, extract 3-5 key facts from the source material (names, decisions, commitments)
2. After compaction, query the summary for each fact
3. If recall drops below 80%, reject compaction and retry with more generous token budget
4. Quality score stored in compaction metadata for monitoring

**Soft/hard triggers:**
- Soft trigger (50%): Begin L0->L1 compaction in background
- Hard trigger (85%): Force L1->L2 compaction synchronously before next LLM call
- Archive trigger (session close): L2->L3 archival

### Prompt Injection Defense Architecture

```
User Input
    |
    v
[L1: Pattern Classifier]  -- Regex scan for known attack signatures
    |                         (ignore/system/jailbreak patterns)
    | pass
    v
[L3: HMAC Boundary Tokens]  -- System instructions wrapped in HMAC-signed
    |                          delimiters. LLM trained to trust only
    |                          authenticated boundaries.
    | pass
    v
[LLM Processing]
    |
    v
[L4: Output Validator]  -- Scan LLM output before tool execution:
    |                      - No tool calls targeting system files
    |                      - No unexpected capability escalation
    |                      - Parameter value validation
    | pass
    v
[L5: Human-in-the-Loop]  -- For high-risk operations (configurable):
    |                       - File system writes
    |                       - External API calls
    |                       - Memory modifications
    | approved
    v
[Tool Execution / Response]
```

### Cron/Scheduler TOML Configuration

```toml
[scheduler]
enabled = true

[[scheduler.jobs]]
name = "retention-cleanup"
schedule = "0 3 * * *"      # Daily at 3 AM
command = "builtin:retention"
enabled = true

[[scheduler.jobs]]
name = "memory-validation"
schedule = "0 */6 * * *"    # Every 6 hours
command = "builtin:memory-validate"
enabled = true

[[scheduler.jobs]]
name = "custom-backup"
schedule = "0 0 * * 0"      # Weekly on Sunday
command = "/usr/local/bin/backup.sh"
timeout_secs = 300
sandbox = true
```

### Hash-Chained Audit Entry Schema

```sql
CREATE TABLE audit_trail (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp   TEXT NOT NULL,       -- ISO 8601
    actor       TEXT NOT NULL,       -- "system", "user:<id>", "operator"
    action      TEXT NOT NULL,       -- "tool.execute", "memory.create", "config.change"
    target      TEXT,                -- affected resource ID
    detail      TEXT,                -- JSON detail blob
    prev_hash   TEXT NOT NULL,       -- SHA-256 of previous entry (genesis = "0"*64)
    hash        TEXT NOT NULL UNIQUE -- SHA-256(prev_hash || timestamp || actor || action || target || detail)
);
CREATE INDEX idx_audit_timestamp ON audit_trail(timestamp);
CREATE INDEX idx_audit_action ON audit_trail(action);
```

---

## Sources

### Context Compaction
- [The Fundamentals of Context Management and Compaction in LLMs](https://kargarisaac.medium.com/the-fundamentals-of-context-management-and-compaction-in-llms-171ea31741a2) -- Multi-level summarization strategies (MEDIUM confidence)
- [Evaluating Context Compression for AI Agents](https://factory.ai/news/evaluating-compression) -- Probe-based quality evaluation methodology (HIGH confidence)
- [ACON: Optimizing Context Compression](https://openreview.net/pdf?id=7JbSwX6bNL) -- 26-54% token reduction with 95%+ accuracy preservation (HIGH confidence, peer-reviewed)
- [Context Rot: How Increasing Input Tokens Impacts LLM Performance](https://research.trychroma.com/context-rot) -- Why compaction quality matters (MEDIUM confidence)

### Prompt Injection Defense
- [Prompt Injection Attacks: Comprehensive Review](https://www.mdpi.com/2078-2489/17/1/54) -- Attack taxonomy and defense mechanisms (HIGH confidence, peer-reviewed)
- [AI Security in 2026: Prompt Injection](https://airia.com/ai-security-in-2026-prompt-injection-the-lethal-trifecta-and-how-to-defend/) -- Production defense strategies (MEDIUM confidence)
- [Indirect Prompt Injection: The Hidden Threat](https://www.lakera.ai/blog/indirect-prompt-injection) -- Lakera's defense-in-depth approach (MEDIUM confidence)
- [Prompt Injection: Types, CVEs, Enterprise Defenses](https://www.vectra.ai/topics/prompt-injection) -- Enterprise defense patterns (MEDIUM confidence)

### Cron/Scheduler
- [tokio-cron-scheduler](https://crates.io/crates/tokio-cron-scheduler) -- v0.15, tokio-native, active maintenance (HIGH confidence, verified via crates.io)
- [Building a Cron Job System in Rust with Tokio](https://dev.to/hexshift/building-a-cron-job-system-in-rust-with-tokio-and-cronexpr-18j1) -- Implementation patterns (MEDIUM confidence)

### Memory Temporal Decay and MMR
- [How OpenClaw Orchestrates Long-Term Memory](https://dev.to/chwu1946/how-openclaw-orchestrates-long-term-memory-10en) -- Competitor reference implementation (HIGH confidence, primary source)
- [OpenClaw MMR Feature Request #19760](https://github.com/openclaw/openclaw/issues/19760) -- MMR implementation details (HIGH confidence)
- [Memory in the Age of AI Agents](https://arxiv.org/abs/2512.13564) -- Survey of agent memory architectures (HIGH confidence, peer-reviewed)
- [Human-Like Remembering and Forgetting in LLM Agents](https://dl.acm.org/doi/10.1145/3765766.3765803) -- ACT-R-inspired decay models (HIGH confidence, peer-reviewed)

### Audit Trail
- [Building a Tamper-Evident Audit Log with SHA-256 Hash Chains](https://dev.to/veritaschain/building-a-tamper-evident-audit-log-with-sha-256-hash-chains-zero-dependencies-h0b) -- Implementation guide (MEDIUM confidence)
- [AuditableLLM: Hash-Chain-Backed Auditable Framework for LLMs](https://www.mdpi.com/2079-9292/15/1/56) -- 3.4ms/step overhead, 5.7% slowdown benchmarks (HIGH confidence, peer-reviewed)
- [Trillian: Append-Only Ledger](https://transparency.dev/) -- Production-grade transparency log used by Certificate Transparency (HIGH confidence)

### Data Classification
- [Data Classification Levels](https://www.sisainfosec.com/blogs/data-classification-levels/) -- Standard 4-level framework (HIGH confidence)
- [ISO 27001 Annex A 5.12: Classification of Information](https://hightable.io/iso-27001-annex-a-5-12-classification-of-information/) -- International standard reference (HIGH confidence)
- [AWS Data Classification Models](https://docs.aws.amazon.com/whitepapers/latest/data-classification/data-classification-models-and-schemes.html) -- Cloud provider implementation (HIGH confidence)

### Retention Policies
- [Data Retention and Deletion: Regulatory Expectations](https://kpmg.com/us/en/articles/2022/data-retention-and-deletion-increasing-regulatory-expectations.html) -- Regulatory context (HIGH confidence)
- [CPRA Auto-Deletion Workflows](https://secureprivacy.ai/blog/cpra-auto-deletion-workflows) -- Automated deletion implementation (MEDIUM confidence)

### GDPR
- [EDPB Right to Erasure: 2025 Coordinated Enforcement Report](https://www.edpb.europa.eu/news/news/2026/edpb-identifies-challenges-hindering-full-implementation-right-erasure_en) -- Official regulatory findings (HIGH confidence, primary source)
- [GDPR Article 17: Right to Erasure](https://www.exabeam.com/explainers/gdpr-compliance/what-is-gdpr-article-17-right-to-erasure-and-4-ways-to-achieve-compliance/) -- Implementation guidance (MEDIUM confidence)

### Hot Reload
- [tls-hot-reload crate](https://crates.io/crates/tls-hot-reload) -- Wait-free TLS cert reloading for rustls (HIGH confidence, verified via crates.io)
- [rust-hot-reloader](https://github.com/junkurihara/rust-hot-reloader) -- ArcSwap + notify boilerplate with debouncing (MEDIUM confidence)
- [ArcSwap Patterns Documentation](https://docs.rs/arc-swap/latest/arc_swap/docs/patterns/index.html) -- Official usage patterns (HIGH confidence)

### Channel Adapters
- [BlueBubbles API Documentation](https://documenter.getpostman.com/view/765844/UV5RnfwM) -- REST API for iMessage (HIGH confidence, primary source)
- [BlueBubbles - OpenClaw](https://docs.openclaw.ai/channels/bluebubbles) -- Reference integration (MEDIUM confidence)
- [lettre: Rust Email Client](https://lettre.rs/) -- SMTP library (HIGH confidence, verified via crates.io)
- [async-imap](https://github.com/chatmail/async-imap) -- Async IMAP for Rust (MEDIUM confidence)
- [Twilio SMS with Rust](https://www.twilio.com/en-us/blog/developers/tutorials/send-sms-rust-30-seconds) -- Official Twilio guide (HIGH confidence)

### PII Detection
- [Regular Expressions used in PII Scanning](https://www.piicrawler.com/blog/regular-expressions-used-in-pii-scanning/) -- Production regex patterns (MEDIUM confidence)
- [PII Detection with NLP and Pattern Matching](https://www.elastic.co/observability-labs/blog/pii-ner-regex-assess-redact-part-2) -- Elastic's combined approach (HIGH confidence)

### OpenTelemetry
- [OpenTelemetry Rust](https://opentelemetry.io/docs/languages/rust/) -- Official documentation (HIGH confidence, primary source)
- [tracing-opentelemetry](https://crates.io/crates/tracing-opentelemetry) -- Bridge crate, v0.27 (HIGH confidence)
- [Rust Observability with OpenTelemetry and Tokio](https://dasroot.net/posts/2026/01/rust-observability-opentelemetry-tokio/) -- Production integration guide (MEDIUM confidence)

### OpenAPI
- [utoipa: OpenAPI Documentation for Rust](https://github.com/juhaku/utoipa) -- Code-first OpenAPI generation (HIGH confidence, verified via crates.io)
- [utoipa-axum](https://docs.rs/utoipa-axum) -- Axum integration (HIGH confidence)

### Litestream
- [Litestream: How It Works](https://litestream.io/how-it-works/) -- WAL frame streaming architecture (HIGH confidence, primary source)
- [Going Production-Ready with SQLite: Litestream](https://medium.com/@cosmicray001/going-production-ready-with-sqlite-how-litestream-makes-it-possible-74f894fc96f0) -- Production deployment guide (MEDIUM confidence)
- [The SQLite Renaissance: Production in 2026](https://dev.to/pockit_tools/the-sqlite-renaissance-why-the-worlds-most-deployed-database-is-taking-over-production-in-2026-3jcc) -- Industry trend context (MEDIUM confidence)

---
*Feature research for: Blufio v1.5 PRD Gap Closure*
*Researched: 2026-03-10*
