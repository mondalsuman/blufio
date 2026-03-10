# Architecture Patterns

**Domain:** v1.5 PRD Gap Closure -- Feature Integration Into 35-Crate Rust AI Agent Platform
**Researched:** 2026-03-10
**Confidence:** HIGH (based on direct analysis of 80,101 LOC across 35 crates, 11 migration files, workspace Cargo.toml, and component wiring in serve.rs)

## Recommended Architecture

The v1.5 features integrate into the existing 35-crate workspace through three strategies: (A) **extend existing crates** where the feature naturally belongs, (B) **create new crates** where a new domain boundary exists, and (C) **add infrastructure** where cross-cutting concerns require new wiring. The guiding principle is: a new crate only when a new adapter trait or a clearly orthogonal subsystem justifies it.

### Crate Impact Map

| Feature | Strategy | Primary Crate(s) | New Crate? | New Migration? |
|---------|----------|-------------------|------------|----------------|
| Multi-level compaction (L0-L3) | Extend | blufio-context | No | No (metadata in existing messages table) |
| Prompt injection defense | Extend | blufio-security, blufio-agent | No | No |
| Cron/scheduler | **New crate** | blufio-scheduler | **Yes** | Yes (V12: scheduled_jobs) |
| Memory enhancements | Extend | blufio-memory | No | Yes (V13: ALTER memories) |
| Audit trail | **New crate** | blufio-audit | **Yes** | Yes (V14: audit_log) |
| Data classification | Extend | blufio-core (traits) | No | No |
| Retention policies | Extend | blufio-storage | No | Yes (V15: soft delete columns) |
| Hook system | **New crate** | blufio-hooks | **Yes** | No (TOML-driven) |
| Hot reload | Extend | blufio-config | No | No |
| iMessage adapter | **New crate** | blufio-imessage | **Yes** | No |
| Email adapter | **New crate** | blufio-email | **Yes** | No |
| SMS adapter | **New crate** | blufio-sms | **Yes** | No |
| PII redaction expansion | Extend | blufio-security | No | No |
| GDPR tooling | Extend | blufio (CLI binary) | No | No |
| OpenTelemetry | **New crate** | blufio-otel | **Yes** | No |
| OpenAPI spec | Extend | blufio-gateway | No | No |
| Litestream replication | External sidecar | blufio (CLI/docs) | No | No |
| Code quality hardening | Cross-cutting | All library crates | No | No |

**Summary: 7 new crates, 4 new migrations (V12-V15), bringing total to ~42 crates.**

---

## Component Architecture: Per-Feature Integration

### 1. Multi-Level Compaction (L0-L3) -- blufio-context

**Current state:** Single-level compaction in `compaction.rs` -- a flat module with `generate_compaction_summary()` and `persist_compaction_summary()`. The `DynamicZone` in `dynamic.rs` splits history at the midpoint and summarizes the older half via a single Haiku LLM call. No quality scoring, no tiered summarization, no archive, no soft/hard thresholds.

**Architecture change:**

```
blufio-context/
  src/
    compaction/           # Promote from single file to module
      mod.rs              # CompactionEngine with level-based dispatch
      levels.rs           # L0 (raw), L1 (session summary), L2 (topic cluster), L3 (persona)
      quality.rs          # QualityScoringGate -- cosine sim between summary and source
      triggers.rs         # Soft/hard threshold configuration and trigger logic
      archive.rs          # Cold storage serialization for compacted messages
    dynamic.rs            # Delegates to CompactionEngine instead of bare functions
```

**Data flow change:**

```
DynamicZone::assemble_messages()
  |
  +-- Estimate tokens via TokenizerCache (existing)
  |
  +-- IF tokens > soft_threshold (default 0.60):
  |     CompactionEngine::compact_incremental(L0 -> L1)
  |     QualityScoringGate::verify(summary, source_messages)
  |       IF quality_score < 0.7 -> retry with enhanced prompt
  |
  +-- IF tokens > hard_threshold (default 0.85):
  |     CompactionEngine::compact_deep(L1 -> L2, L2 -> L3)
  |     Archive::store(compacted_messages)  # optional cold storage
  |
  +-- Return DynamicResult with compaction_usage (Vec, not Option)
```

**Key integration points:**
- `ContextConfig` in blufio-config gains `compaction_soft_threshold`, `compaction_hard_threshold`, `compaction_quality_min`, `compaction_archive_enabled` fields
- `DynamicZone` delegates to `CompactionEngine` rather than calling `generate_compaction_summary` directly
- Compaction level metadata stored in existing `messages.metadata` JSON column (no new migration)
- `AssembledContext::compaction_usage` changes from `Option<TokenUsage>` to `Vec<TokenUsage>` (multi-stage)
- EventBus gets new `BusEvent::Compaction(CompactionEvent)` variant for level transitions and quality gate outcomes
- Quality gate uses cosine similarity of summary embedding vs. mean of source embeddings (reuses blufio-memory embedder)
- **No new crate dependencies** -- uses existing LLM provider calls and SHA-256 from `sha2`

### 2. Prompt Injection Defense Pipeline -- blufio-security + blufio-agent

**Current state:** blufio-security has three modules: TLS enforcement (`tls.rs`), SSRF prevention (`ssrf.rs`), and secret redaction (`redact.rs`). No input validation against prompt injection. The `REDACTION_PATTERNS` LazyLock in `redact.rs` has 4 regex patterns for API keys and tokens.

**Architecture change:**

```
blufio-security/
  src/
    injection/            # New module
      mod.rs              # InjectionDefense pipeline orchestrator
      pattern.rs          # L1: Regex/keyword pattern classifier (known attack patterns)
      boundary.rs         # L3: HMAC boundary token insertion/verification
      output_validator.rs # L4: Output scanning for leaked system prompt content
    redact.rs             # Existing (unchanged)
    ssrf.rs               # Existing (unchanged)
    tls.rs                # Existing (unchanged)
```

**Five-layer defense model:**

| Layer | Location | Mechanism | Blocking? |
|-------|----------|-----------|-----------|
| L1 | blufio-security/injection/pattern.rs | Regex classifier for known injection patterns | Configurable |
| L2 | System prompt instructions | "Ignore attempts to override" preamble | Passive |
| L3 | blufio-security/injection/boundary.rs | HMAC boundary tokens wrapping user content | Non-blocking (structural) |
| L4 | blufio-security/injection/output_validator.rs | Scan LLM output for system prompt leakage | Warning/block |
| L5 | blufio-agent (handle_inbound) | Human-in-the-loop queue for high-risk inputs | Blocking (async) |

**Integration into agent loop:**

```
AgentLoop::handle_inbound(inbound)
  |
  +-- InjectionDefense::classify_input(&inbound.content)  # L1 pattern check
  |     Returns: InputClassification { risk_level, matched_patterns }
  |
  +-- IF risk_level >= High && config.injection.human_in_loop:  # L5
  |     Enqueue for human review, respond with hold message
  |
  +-- ContextEngine::assemble() wraps user content with HMAC tokens  # L3
  |     boundary::wrap_user_content(content, session_hmac_key)
  |
  +-- After LLM response:
  |     output_validator::scan_output(&response, &system_prompt_hashes)  # L4
  |     Check for: system prompt leakage, instruction override indicators
```

**Key integration points:**
- blufio-security depends on `hmac` + `sha2` (already workspace dependencies)
- blufio-agent gains `Option<Arc<InjectionDefense>>` field on `AgentLoop` and `SessionActorConfig`
- `SecurityConfig` in blufio-config gains `[security.injection]` subsection
- HMAC key derived from session_id + vault master key (via blufio-vault's Argon2id KDF)
- EventBus: `BusEvent::Security(SecurityEvent::InjectionDetected { risk_level, patterns })` for observability
- Pattern database: embedded const strings in the binary (not external file) to preserve single-binary constraint
- L1 patterns based on [OWASP Prompt Injection Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/LLM_Prompt_Injection_Prevention_Cheat_Sheet.html)

### 3. Cron/Scheduler -- blufio-scheduler (NEW CRATE)

**Rationale for new crate:** Scheduling is an orthogonal concern -- it does not belong in blufio-agent (message processing), blufio-config (configuration loading), or blufio-storage (persistence). It needs its own lifecycle (start/stop), its own persistence (job run history), and its own error domain. It parallels blufio-resilience in being a cross-cutting operational concern.

**Architecture:**

```
blufio-scheduler/
  Cargo.toml             # Deps: blufio-core, blufio-config, blufio-bus, croner, tokio, chrono
  src/
    lib.rs               # Scheduler struct with start/stop/list
    job.rs               # ScheduledJob definition (cron expr, action, enabled)
    executor.rs          # Job execution loop (tokio::spawn + tick interval)
    systemd.rs           # systemd timer file generation (blufio scheduler export-systemd)
    config.rs            # SchedulerConfig with [[scheduler.jobs]] TOML array
```

**Integration:**

```rust
// serve.rs wiring (same pattern as circuit breaker registry):
let scheduler = Scheduler::new(&config.scheduler, event_bus.clone(), storage.clone());
scheduler.start(cancel_token.clone()).await;
```

**TOML configuration:**

```toml
[scheduler]
enabled = true
tick_interval_ms = 500

[[scheduler.jobs]]
name = "daily-memory-cleanup"
cron = "0 3 * * *"
action = "memory:cleanup"
enabled = true

[[scheduler.jobs]]
name = "retention-enforcement"
cron = "0 4 * * *"
action = "retention:enforce"
enabled = true
```

**Key integration points:**
- New `[scheduler]` section in BlufioConfig with `enabled` bool and `[[scheduler.jobs]]` array
- Actions are string-typed dispatch keys: `memory:cleanup`, `retention:enforce`, `backup:run`, `health:check`
- Scheduler publishes `BusEvent::Scheduler(SchedulerEvent::JobTriggered { ... })` to EventBus
- Action dispatch routes to internal subsystem methods via a registered action handler map
- Migration V12 adds `scheduled_jobs` table for job run history (last_run, next_run, run_count, last_error)
- CLI: `blufio scheduler list`, `blufio scheduler run <job>`, `blufio scheduler export-systemd`
- Uses `croner` crate for cron expression parsing (lightweight, well-maintained, no std issues)

### 4. Memory Enhancements -- blufio-memory

**Current state:** `HybridRetriever` does vector + BM25 + RRF fusion with confidence-based boosting (`explicit=0.9`, `extracted=0.6`). The `Memory` struct has `created_at`/`updated_at` timestamps but no access tracking. No temporal decay, no MMR diversity reranking, no index size bounds, no background validation.

**Architecture change:**

```
blufio-memory/
  src/
    retriever.rs          # Add temporal_decay(), importance_boost(), mmr_rerank() stages
    store.rs              # Add last_accessed_at tracking, LRU eviction, count queries
    validator.rs          # NEW: Background task to validate embedding dimensionality
    watcher.rs            # NEW: notify-based file watcher for auto re-indexing
    types.rs              # Add last_accessed_at, access_count fields to Memory struct
```

**Enhanced retrieval pipeline:**

```
HybridRetriever::retrieve(query)
  |
  +-- embed(query)                          # existing
  +-- vector_search()                       # existing
  +-- bm25_search()                         # existing
  +-- rrf_fusion()                          # existing
  +-- temporal_decay(0.95^days)             # NEW: score *= decay_factor^(days_since_created)
  +-- importance_boost(source)              # NEW: explicit=1.2x boost, extracted=1.0x
  +-- mmr_rerank(lambda=0.7)               # NEW: Maximal Marginal Relevance for diversity
  +-- update_access_timestamps()            # NEW: track last_accessed_at for LRU
  +-- return top-K results
```

**MMR algorithm:**

```
Selected = {most relevant result}
While |Selected| < K:
  For each candidate not in Selected:
    mmr_score = lambda * relevance(candidate) - (1-lambda) * max(sim(candidate, s) for s in Selected)
  Add candidate with highest mmr_score to Selected
```

**Key integration points:**
- Migration V13: `ALTER TABLE memories ADD COLUMN last_accessed_at TEXT DEFAULT NULL; ALTER TABLE memories ADD COLUMN access_count INTEGER DEFAULT 0;`
- `MemoryConfig` gains `temporal_decay_factor` (default 0.95), `mmr_lambda` (default 0.7), `max_index_size` (default 10000), `lru_eviction_enabled` (default true)
- LRU eviction: background tokio task checks `SELECT COUNT(*) FROM memories WHERE status='active'` > `max_index_size`, evicts least-recently-accessed
- File watcher uses `notify` crate for monitoring skill/memory directories for changes
- Background validator: spawned as tokio task in serve.rs, checks embedding dimensionality consistency
- EventBus: `BusEvent::Memory(MemoryEvent::Evicted { count, reason })` for observability
- New dependency: `notify = "7"` (shared with blufio-config hot reload)

### 5. Audit Trail -- blufio-audit (NEW CRATE)

**Rationale for new crate:** Audit is a cross-cutting concern that every subsystem feeds into but that must be independently reliable. It has its own storage table, its own integrity guarantees (hash chain), and its own verification logic. Embedding it in blufio-storage would conflate CRUD persistence with tamper-evident logging.

**Architecture:**

```
blufio-audit/
  Cargo.toml             # Deps: blufio-core, sha2, serde, serde_json, tokio-rusqlite, chrono, uuid
  src/
    lib.rs               # AuditLogger with append(), verify_chain(), export()
    entry.rs             # AuditEntry struct with hash_self, hash_prev fields
    chain.rs             # Hash chain verification logic
    canonical.rs         # Deterministic JSON serialization (sorted keys, no whitespace)
```

**Hash chain structure:**

```rust
pub struct AuditEntry {
    pub id: String,                    // UUID v4
    pub timestamp: String,             // ISO 8601
    pub actor: String,                 // "user:<id>", "system", "agent", "admin"
    pub action: String,                // "message.sent", "session.created", "memory.deleted"
    pub resource_type: String,         // "session", "message", "memory", "config"
    pub resource_id: Option<String>,   // ID of affected resource
    pub metadata: Option<String>,      // JSON: additional context (data classification tagged)
    pub hash_prev: String,             // SHA-256 of previous entry (or "genesis" for first)
    pub hash_self: String,             // SHA-256(canonical(fields + hash_prev))
}

// Chain integrity: hash_self = SHA-256(canonical_json(timestamp, actor, action, resource_type,
//                                       resource_id, metadata, hash_prev))
// Canonical JSON: keys sorted alphabetically, no whitespace, stable float repr
```

**Key integration points:**
- Migration V14: `CREATE TABLE audit_log (id TEXT PK, timestamp TEXT NOT NULL, actor TEXT NOT NULL, action TEXT NOT NULL, resource_type TEXT NOT NULL, resource_id TEXT, metadata TEXT, hash_prev TEXT NOT NULL, hash_self TEXT NOT NULL UNIQUE); CREATE INDEX idx_audit_timestamp ON audit_log(timestamp); CREATE INDEX idx_audit_actor ON audit_log(actor); CREATE INDEX idx_audit_action ON audit_log(action);`
- **Separate audit.db preferred** over same-database storage: isolates audit chain from main DB operations, allows different backup cadence, prevents accidental cascade deletes
- `AuditLogger` wrapped in `Arc<AuditLogger>` -- injected into serve.rs and passed to AgentLoop
- Async append via buffered mpsc channel: agent loop sends audit events, background task flushes to SQLite in batches (never blocks hot path)
- EventBus integration: `AuditLogger` subscribes as reliable subscriber to EventBus, auto-generates audit entries from all event variants
- CLI: `blufio audit verify` (checks full hash chain), `blufio audit export --format json --since 2026-03-01`
- **Critical design decision:** Canonical serialization uses sorted keys and no pretty-printing -- without deterministic serialization, verification produces false positives

### 6. Data Classification -- blufio-core (traits)

**Current state:** No data classification. All data treated uniformly in logs, exports, API responses.

**Architecture change:**

```rust
// New file: blufio-core/src/classification.rs (or add to types.rs)

/// Data sensitivity classification following a four-level model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DataClassification {
    /// System status, model names, public configuration, version info.
    Public,
    /// Session IDs, message counts, operational metrics, timestamps.
    Internal,
    /// Message content, memory facts, user preferences, conversation history.
    Confidential,
    /// Credentials, PII, encryption keys, audit chain hashes, vault entries.
    Restricted,
}

/// Types that carry a data sensitivity classification.
pub trait Classifiable {
    fn classification(&self) -> DataClassification;
}
```

**Key integration points:**
- `Classifiable` trait implemented on core types: `Message` (Confidential), `Session` (Internal), `Memory` (Confidential), vault entries (Restricted)
- Classification drives: log redaction depth (Restricted always redacted), export filtering (GDPR export respects level), API response field filtering
- blufio-security redaction uses classification: Restricted fields always redacted in logs, Confidential redacted at info/debug level
- No migration needed -- classification is derived from type, not stored as a column
- DataClassification is `PartialOrd` so you can write `if classification >= Confidential { redact() }`
- Re-exported from blufio-core crate root alongside other traits

### 7. Retention Policies -- blufio-storage + blufio-scheduler

**Architecture:**

```
blufio-storage/
  src/
    retention.rs          # NEW: RetentionPolicy struct, RetentionEnforcer
```

**TOML configuration:**

```toml
[retention]
enabled = true

[retention.messages]
max_age_days = 90
soft_delete = true       # Mark deleted_at, don't DROP rows

[retention.sessions]
max_age_days = 365
archive_closed = true    # Move to archived_at after close

[retention.memories]
max_age_days = 0          # 0 = never expire
min_access_count = 0      # Memories accessed < N times eligible for pruning

[retention.audit]
max_age_days = 730        # 2 years for compliance
immutable = true          # Never auto-delete audit entries
```

**Key integration points:**
- Migration V15: `ALTER TABLE messages ADD COLUMN deleted_at TEXT DEFAULT NULL; ALTER TABLE sessions ADD COLUMN archived_at TEXT DEFAULT NULL;` (soft delete support)
- `RetentionEnforcer::enforce()` called by scheduler job `retention:enforce` on configured cron
- Enforcer respects `DataClassification` -- Restricted data has minimum retention period (configurable)
- Audit entries are immutable by default: retention policy skips audit_log unless `immutable = false`
- EventBus: `BusEvent::Retention(RetentionEvent::Enforced { deleted_messages, archived_sessions })` for observability
- Soft delete means `get_messages()` needs `WHERE deleted_at IS NULL` filter (backward-compatible: existing queries see only active messages)

### 8. Hook System -- blufio-hooks (NEW CRATE)

**Rationale for new crate:** Hooks are a distinct extension mechanism -- shell-based external commands triggered by lifecycle events. This is orthogonal to the plugin system (compiled-in Rust adapters) and the skill system (WASM sandboxed). Hooks bridge Blufio to external tooling.

**Architecture:**

```
blufio-hooks/
  Cargo.toml             # Deps: blufio-core, blufio-bus, blufio-config, tokio
  src/
    lib.rs               # HookRegistry with register(), trigger()
    runner.rs            # HookRunner: tokio::process::Command with timeout + env isolation
    config.rs            # [[hooks]] TOML config parsing
    events.rs            # 11 lifecycle hook points mapped from BusEvent variants
```

**11 lifecycle hook points (mapped to BusEvent):**

| Hook | BusEvent Source | Environment Variables |
|------|----------------|----------------------|
| `on_startup` | AgentLoop init | BLUFIO_EVENT=startup |
| `on_shutdown` | CancellationToken | BLUFIO_EVENT=shutdown |
| `on_session_start` | Session(Created) | SESSION_ID, CHANNEL |
| `on_session_end` | Session(Closed) | SESSION_ID |
| `on_message_received` | Channel(MessageReceived) | CHANNEL, SENDER_ID |
| `on_message_sent` | Channel(MessageSent) | CHANNEL |
| `on_skill_invoked` | Skill(Invoked) | SKILL_NAME, SESSION_ID |
| `on_skill_completed` | Skill(Completed) | SKILL_NAME, IS_ERROR |
| `on_memory_extracted` | Memory(Extracted) | MEMORY_COUNT (new event) |
| `on_error` | Error events | ERROR_TYPE, SEVERITY |
| `on_health_degraded` | Resilience(DegradationLevelChanged) | FROM_LEVEL, TO_LEVEL |

**TOML configuration:**

```toml
[[hooks]]
event = "on_session_start"
command = "/usr/local/bin/notify-new-session.sh"
timeout_secs = 10
priority = 100            # BTreeMap ordering (lower = earlier)
environment = { SESSION_ID = "{{session_id}}", CHANNEL = "{{channel}}" }
enabled = true
```

**Key integration points:**
- HookRegistry subscribes to EventBus as reliable subscriber (guaranteed delivery)
- Background task matches events to registered hooks, executes in BTreeMap priority order
- Shell execution: `tokio::process::Command` with configurable timeout, stdin closed, stdout/stderr captured and logged
- Sandbox: hooks run with restricted environment -- only TOML-specified env vars plus `BLUFIO_EVENT_TYPE`
- No migration -- hooks are TOML-defined and stateless (fire-and-forget)
- Hook failures logged and emitted as `BusEvent::Hook(HookEvent::Failed { ... })` but **never block the agent loop**
- Maximum concurrent hooks: configurable (default 4) to prevent fork bombs

### 9. Hot Reload -- blufio-config

**Current state:** Config loaded once at startup via `figment` (TOML + env vars). No runtime reload. `BlufioConfig` passed by value/clone to subsystems.

**Architecture change:**

```
blufio-config/
  src/
    hot_reload.rs         # NEW: ConfigWatcher using notify + ArcSwap
    loader.rs             # Existing (unchanged)
    model.rs              # Existing (unchanged)
```

**Core pattern using ArcSwap:**

```rust
use arc_swap::ArcSwap;
use notify::{RecommendedWatcher, RecursiveMode, Event};

pub struct ConfigWatcher {
    config: Arc<ArcSwap<BlufioConfig>>,
    _watcher: RecommendedWatcher,
}

impl ConfigWatcher {
    pub fn new(initial: BlufioConfig, config_path: PathBuf) -> Result<Self, BlufioError> {
        let config = Arc::new(ArcSwap::from_pointee(initial));
        let config_clone = config.clone();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, _>| {
            if let Ok(event) = res {
                if event.kind.is_modify() {
                    // Reload, validate, swap
                    match load_and_validate_from_path(&config_path) {
                        Ok(new_config) => {
                            config_clone.store(Arc::new(new_config));
                            tracing::info!("configuration reloaded");
                        }
                        Err(errors) => {
                            tracing::error!(?errors, "config reload failed validation, keeping current");
                        }
                    }
                }
            }
        })?;

        watcher.watch(&config_path, RecursiveMode::NonRecursive)?;
        Ok(Self { config, _watcher: watcher })
    }

    pub fn current(&self) -> arc_swap::Guard<Arc<BlufioConfig>> {
        self.config.load()
    }

    pub fn handle(&self) -> Arc<ArcSwap<BlufioConfig>> {
        self.config.clone()
    }
}
```

**Reload scope (what can vs. cannot be hot-reloaded):**

| Config Section | Hot Reload? | Reason |
|----------------|-------------|--------|
| `[agent]` system_prompt | Yes | Re-builds StaticZone on next assemble |
| `[context]` thresholds | Yes | DynamicZone reads config per-request |
| `[security]` TLS certs | Yes | Rebuild reqwest client on change |
| `[cost]` budgets | Yes | BudgetTracker reads config per-check |
| `[routing]` thresholds | Yes | ModelRouter reads config per-request |
| `[hooks]` definitions | Yes | HookRegistry re-registers hooks |
| `[scheduler.jobs]` | Yes | Scheduler reloads job list |
| `[retention]` policies | Yes | Enforcer reads config per-run |
| `[telegram]` token | **No** | Requires channel disconnect/reconnect |
| `[storage]` path/key | **No** | Requires DB close/reopen |
| `[anthropic]` API key | **No** | Requires provider re-initialization |

**Key integration points:**
- New workspace dependencies: `arc-swap = "1"` (lock-free reads), `notify = "7"` (file watching)
- ConfigWatcher created in serve.rs, `Arc<ArcSwap<BlufioConfig>>` shared to all subsystems
- Subsystems reading config per-request (router, context, cost) get latest config via `config.load()` on each call
- Subsystems caching config (StaticZone, TLS client) need explicit reload handlers triggered by ConfigWatcher callback
- EventBus: `BusEvent::Config(ConfigEvent::Reloaded { changed_sections })` variant
- **Critical safety rule:** ConfigWatcher runs full `validation::validate_config()` on new config. If validation fails, keep old config and log error. Never swap invalid config.

### 10. New Channel Adapters (iMessage/Email/SMS)

Each follows the established pattern: new crate implementing `ChannelAdapter` + `PluginAdapter` traits from blufio-core, feature-gated in the main binary. This is the same pattern used by all 8 existing channel adapters.

#### blufio-imessage (NEW CRATE)

```
blufio-imessage/
  Cargo.toml             # Deps: blufio-core, blufio-config, reqwest, async-trait, serde, serde_json
  src/
    lib.rs               # BlueBubblesChannel implementing ChannelAdapter
    api.rs               # BlueBubbles REST API client (HTTP + password auth)
    webhook.rs           # BlueBubbles webhook receiver
    types.rs             # BlueBubbles API response types
```

- Talks to BlueBubbles macOS server via REST API (HTTP, password query param auth)
- Receives messages via BlueBubbles webhooks (POST to gateway extra public route)
- Webhook registration via REST API on connect()
- Feature flag: `imessage = ["dep:blufio-imessage"]` in main binary
- Config: `[imessage]` section with `server_url`, `password_vault_key`, `webhook_port`
- Requires BlueBubbles server running on macOS -- documented as prerequisite

#### blufio-email (NEW CRATE)

```
blufio-email/
  Cargo.toml             # Deps: blufio-core, blufio-config, lettre, mail-parser, async-trait, tokio
  src/
    lib.rs               # EmailChannel implementing ChannelAdapter
    imap.rs              # IMAP polling loop for receive()
    smtp.rs              # SMTP for send() via lettre
    types.rs             # Email message types
```

- IMAP poll loop for receive(), SMTP send for send()
- Config: `[email]` section with `imap_host`, `smtp_host`, `username`, `password_vault_key`, `poll_interval_secs`, `allowed_senders`
- Feature flag: `email = ["dep:blufio-email"]`
- New workspace dependencies: `lettre = "0.11"` (SMTP), `mail-parser = "0.9"` (IMAP message parsing)

#### blufio-sms (NEW CRATE)

```
blufio-sms/
  Cargo.toml             # Deps: blufio-core, blufio-config, reqwest, async-trait
  src/
    lib.rs               # TwilioSmsChannel implementing ChannelAdapter
    api.rs               # Twilio REST API client
    webhook.rs           # Twilio webhook handler (receives SMS via HTTP POST)
```

- Twilio API for outbound, Twilio webhook for inbound (same pattern as WhatsApp adapter)
- Webhook endpoint registered on gateway as extra public route (same mechanism as `blufio-whatsapp`)
- Config: `[sms]` section with `account_sid`, `auth_token_vault_key`, `from_number`, `webhook_url`
- Feature flag: `sms = ["dep:blufio-sms"]`
- No new dependencies (uses reqwest, already in workspace)

### 11. PII Redaction Expansion -- blufio-security

**Current state:** `redact.rs` has 4 `LazyLock` regex patterns (Anthropic keys, generic sk-*, Bearer tokens, Telegram bot tokens) plus exact-match vault values. The `RedactingWriter` wraps `std::io::Write`.

**Architecture change:**

```
blufio-security/
  src/
    pii.rs                # NEW: PII-specific patterns and detection logic
    redact.rs             # Import and use PII patterns alongside existing secret patterns
```

**New PII patterns:**

```rust
// Email addresses
Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap(),
// Phone numbers (international format)
Regex::new(r"(?:\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}").unwrap(),
// US SSN
Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap(),
// Credit card numbers (space or dash separated)
Regex::new(r"\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b").unwrap(),
// IPv4 addresses
Regex::new(r"\b(?:(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.){3}(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\b").unwrap(),
```

**Key integration points:**
- `SecurityConfig` gains `[security.pii]` subsection with `redact_email`, `redact_phone`, `redact_ssn`, `redact_credit_card`, `redact_ip` booleans (all default true)
- PII redaction applied in two places: (1) log output via `RedactingWriter`, (2) optional pre-storage redaction of LLM responses
- `DataClassification::Restricted` auto-triggers all PII redaction regardless of per-type config
- PII detection events published to EventBus for audit logging
- `redact()` function gains an optional `PiiConfig` parameter to control which patterns are active

### 12. GDPR Tooling -- blufio (CLI binary)

**Architecture:** New CLI subcommands in the main binary. No separate crate because GDPR operations orchestrate across existing crates (storage, memory, audit) and the CLI is the right surface.

```
blufio/src/
  gdpr.rs                 # NEW: GDPR CLI commands module
```

**CLI commands:**

```
blufio gdpr erase --user <user_id> [--dry-run]
blufio gdpr export --user <user_id> [--format json|csv] [--since 2026-01-01] [--classification-max confidential]
blufio gdpr report --user <user_id>
```

**Key integration points:**
- `gdpr erase`: cascading delete across sessions, messages, memories for a user_id. Audit entries for the user are **redacted** (content replaced with "[ERASED]") but **not deleted** (hash chain integrity preserved). Queue entries purged.
- `gdpr export`: JSON/CSV export filtered by user, date range, data type. Uses `DataClassification` to filter fields (e.g., `--classification-max internal` excludes Confidential content).
- `gdpr report`: generates transparency report listing: what data types are stored, retention policies in effect, processing purposes, data recipients (channels).
- All GDPR operations create audit entries with `actor = "admin"`, `action = "gdpr.erase"` / `"gdpr.export"`.
- Dry-run mode for erase: shows what would be deleted without executing.

### 13. OpenTelemetry -- blufio-otel (NEW CRATE)

**Rationale for new crate:** Observability adapters are feature-gated. OpenTelemetry is optional alongside Prometheus (which has its own crate, blufio-prometheus). Keeping OTel in a separate crate avoids pulling opentelemetry + OTLP + protobuf dependencies when not needed.

**Architecture:**

```
blufio-otel/
  Cargo.toml             # Deps: blufio-core, opentelemetry, opentelemetry-otlp,
                          #   opentelemetry_sdk, tracing-opentelemetry
  src/
    lib.rs               # OtelSetup with init(), shutdown(), create_layer()
    config.rs            # OtelConfig validation
```

**Integration pattern:**

```rust
// In serve.rs, alongside existing Prometheus setup:
#[cfg(feature = "otel")]
{
    let otel_layer = blufio_otel::create_tracing_layer(&config.otel)?;
    // Added as tracing-subscriber layer alongside existing env-filter and fmt layers
}
```

**Key integration points:**
- Feature flag: `otel = ["dep:blufio-otel"]` (disabled by default per PRD requirement)
- Config: `[otel]` section with `enabled`, `endpoint`, `service_name`, `protocol` (grpc/http), `sample_rate`
- Coexists with Prometheus: both can be active simultaneously (Prometheus for pull metrics, OTel for push traces)
- Uses `tracing-opentelemetry` layer which piggybacks on existing `#[instrument]` and `tracing::info!()` calls -- **zero code changes needed in any subsystem**
- Gateway handlers should extract/inject W3C trace context headers for distributed tracing
- New workspace dependencies: `opentelemetry = "0.28"`, `opentelemetry-otlp = "0.28"`, `opentelemetry_sdk = "0.28"`, `tracing-opentelemetry = "0.29"`

### 14. OpenAPI Spec -- blufio-gateway

**Architecture change:**

```
blufio-gateway/
  src/
    openapi.rs            # NEW: OpenAPI spec composition and /openapi.json handler
    handlers.rs           # Add #[utoipa::path(...)] annotations to existing handlers
    server.rs             # Mount /openapi.json route
```

**Key integration points:**
- New workspace dependencies: `utoipa = "5"`, `utoipa-axum = "0.2"`
- Annotate existing handler functions with `#[utoipa::path]` proc macros
- Request/response types annotated with `#[derive(utoipa::ToSchema)]`
- Spec served at GET `/openapi.json` (compile-time generated, zero runtime cost)
- Optional Swagger UI at `/docs` gated behind `[gateway.swagger_ui]` config boolean
- Does NOT require restructuring routes -- `utoipa-axum` works with existing axum Router patterns
- Covers: `/v1/chat/completions`, `/v1/responses`, `/v1/tools`, `/v1/sessions`, `/v1/health`, `/v1/keys`, `/v1/webhooks`, `/v1/batch`

### 15. Litestream Replication -- External Sidecar

**Architecture:** Litestream is NOT embedded in the Rust binary. It runs as a separate Go process alongside Blufio. This is the correct integration pattern because Litestream is designed as a sidecar that monitors SQLite WAL files.

**Integration approach:**

```bash
# Option A: Litestream wraps the blufio process
litestream replicate -config /etc/litestream.yml -exec "blufio serve"

# Option B: Litestream as separate sidecar
litestream replicate /var/lib/blufio/blufio.db s3://bucket/blufio/
```

**Blufio provides:**
1. Documentation on Litestream setup (docs/litestream.md)
2. CLI helper: `blufio litestream init` (generates litestream.yml template)
3. Docker compose with Litestream sidecar container
4. `blufio doctor` gains Litestream health check (process running, replication lag)

**Critical compatibility issue:** Litestream requires exclusive control of WAL checkpointing. When Litestream is active, SQLite must not auto-checkpoint. blufio-storage must disable auto-checkpoint via `PRAGMA wal_autocheckpoint=0` when `[storage.litestream_mode]` is true. Without this, Litestream and SQLite will race on checkpoint operations, causing replication gaps.

**Key integration points:**
- No new crate or Rust dependency
- blufio-storage already uses WAL mode (Litestream requirement satisfied)
- SQLCipher compatibility: Litestream replicates encrypted WAL pages (transparent)
- New config: `[storage] litestream_mode = false` (when true, disables auto-checkpoint)
- Docker compose template: `services: { litestream: { image: litestream/litestream, ... } }`

---

## Data Flow Changes

### Pre-v1.5 Data Flow

```
Channel.receive() -> AgentLoop.handle_inbound()
  -> ContextEngine.assemble() [static + conditional + dynamic]
  -> Provider.stream() -> consume_stream()
  -> Channel.send() -> Storage.insert_message()
```

### Post-v1.5 Data Flow (additions marked with >>)

```
Channel.receive()
  >> InjectionDefense.classify_input()           # L1 pattern check
  >> AuditLogger.append("message.received")      # tamper-evident log
  >> HookRegistry.trigger("on_message_received") # shell hooks
  -> AgentLoop.handle_inbound()
  -> ContextEngine.assemble()
     >> CompactionEngine.compact_if_needed()      # multi-level with quality gates
     >> boundary::wrap_user_content()             # L3 HMAC boundary tokens
  -> Provider.stream() -> consume_stream()
  >> OutputValidator.scan()                       # L4 check for prompt leakage
  >> PiiRedactor.redact_response()                # if configured
  -> Channel.send()
  -> Storage.insert_message()
  >> AuditLogger.append("message.sent")
  >> HookRegistry.trigger("on_message_sent")
```

### New Background Tasks

```
Scheduler (tokio task, 500ms tick):
  -> Check cron expressions against current time
  -> Execute due jobs:
     -> retention:enforce -> RetentionEnforcer.enforce()
     -> memory:cleanup -> MemoryStore.evict_lru()
     -> backup:run -> backup logic (existing)

ConfigWatcher (notify file events):
  -> Detect blufio.toml modification
  -> Reload + validate via load_and_validate()
  -> ArcSwap::store(new_config)
  -> Publish ConfigEvent::Reloaded to EventBus

MemoryValidator (periodic tokio task):
  -> Check embedding dimensionality consistency
  -> Re-index stale entries via file watcher events
  -> LRU eviction if over max_index_size

AuditLogger (mpsc consumer):
  -> Receive audit entries from mpsc channel
  -> Compute hash chain (hash_self = SHA-256(canonical + hash_prev))
  -> Batch insert to audit.db
```

---

## Dependency Graph (New Edges)

```
blufio (binary)
  +-- blufio-scheduler (NEW) --> blufio-core, blufio-bus, blufio-config
  +-- blufio-audit (NEW) --> blufio-core, sha2, serde_json, tokio-rusqlite
  +-- blufio-hooks (NEW) --> blufio-core, blufio-bus, blufio-config
  +-- blufio-otel (NEW) --> blufio-core, opentelemetry, tracing-opentelemetry
  +-- blufio-imessage (NEW) --> blufio-core, blufio-config, reqwest
  +-- blufio-email (NEW) --> blufio-core, blufio-config, lettre, mail-parser
  +-- blufio-sms (NEW) --> blufio-core, blufio-config, reqwest

  blufio-context --> (no new deps, compaction is internal refactor)
  blufio-security --> hmac (already in workspace)
  blufio-memory --> notify (NEW workspace dep)
  blufio-config --> arc-swap (NEW), notify (NEW)
  blufio-gateway --> utoipa (NEW), utoipa-axum (NEW)
```

**New workspace-level dependencies (12 total):**

| Dependency | Version | Purpose | Used By |
|-----------|---------|---------|---------|
| `arc-swap` | 1 | Lock-free config hot reload | blufio-config |
| `notify` | 7 | File system watching | blufio-config, blufio-memory |
| `utoipa` | 5 | OpenAPI spec generation | blufio-gateway |
| `utoipa-axum` | 0.2 | Axum route integration for utoipa | blufio-gateway |
| `croner` | 2 | Cron expression parsing | blufio-scheduler |
| `lettre` | 0.11 | SMTP email sending | blufio-email |
| `mail-parser` | 0.9 | IMAP email parsing | blufio-email |
| `opentelemetry` | 0.28 | OTel API | blufio-otel |
| `opentelemetry-otlp` | 0.28 | OTLP exporter | blufio-otel |
| `opentelemetry_sdk` | 0.28 | OTel SDK | blufio-otel |
| `tracing-opentelemetry` | 0.29 | tracing <-> OTel bridge | blufio-otel |

Note: `hmac`, `sha2`, `tokio-rusqlite`, `reqwest`, `serde`, `serde_json`, `chrono`, `uuid` are already workspace dependencies.

---

## Suggested Build Order (Dependency-Driven)

The build order is determined by which features are prerequisites for others. Features within the same phase have no inter-dependencies and can be parallelized.

### Phase 1: Foundation Layer
*No inter-dependencies. Everything else builds on these.*

1. **Data Classification (blufio-core)** -- 1 plan
   - Adds `DataClassification` enum and `Classifiable` trait
   - Zero breaking changes, purely additive
   - Required by: PII redaction, retention policies, GDPR, audit trail

2. **PII Redaction Expansion (blufio-security)** -- 1 plan
   - Extends `redact.rs` with PII patterns, adds `pii.rs` module
   - Uses DataClassification for redaction depth decisions
   - Required by: GDPR tooling, prompt injection output validator

3. **Hot Reload Infrastructure (blufio-config)** -- 1 plan
   - ArcSwap + notify file watcher for live config
   - Required by: scheduler (reload jobs), hooks (reload definitions), all subsystems reading config

### Phase 2: Storage & Data Infrastructure
*Migrations and data infrastructure that later features persist into.*

4. **Audit Trail (blufio-audit)** -- 2 plans (crate + integration wiring)
   - New crate, hash-chained SQLite log, migration V14
   - Required by: GDPR (audit entries on erasure), retention (audit enforcement events)
   - Depends on: DataClassification (Phase 1)

5. **Memory Enhancements (blufio-memory)** -- 2 plans (retrieval pipeline + background tasks)
   - Temporal decay, MMR, LRU eviction, background validator, migration V13
   - Depends on: Hot Reload (Phase 1, for live config updates)

6. **Retention Policies (blufio-storage)** -- 1 plan
   - RetentionEnforcer with soft-delete, migration V15
   - Depends on: DataClassification (Phase 1), Audit Trail (Phase 2)

### Phase 3: Context & Security Pipeline
*Agent loop modifications that change message processing flow.*

7. **Multi-Level Compaction (blufio-context)** -- 2 plans (engine refactor + quality gates)
   - Promote compaction.rs to module, add L0-L3 levels and quality scoring
   - Depends on: Hot Reload (Phase 1, for live threshold tuning)

8. **Prompt Injection Defense (blufio-security + blufio-agent)** -- 2 plans (classifier + boundary/output)
   - L1 pattern classifier, L3 HMAC boundaries, L4 output validator, L5 human-in-loop
   - Modifies AgentLoop::handle_inbound() flow
   - Depends on: PII Redaction (Phase 1, shared redaction infrastructure), Audit Trail (Phase 2, security event logging)

### Phase 4: Operational Automation
*Scheduler and hooks that automate operations from earlier phases.*

9. **Cron/Scheduler (blufio-scheduler)** -- 2 plans (crate + systemd export)
   - New crate, migration V12, job execution loop
   - Depends on: Retention Policies (Phase 2, scheduler runs `retention:enforce`)
   - Depends on: Memory Enhancements (Phase 2, scheduler runs `memory:cleanup`)
   - Depends on: Hot Reload (Phase 1, live job list updates)

10. **Hook System (blufio-hooks)** -- 1 plan
    - New crate, BusEvent-driven shell execution with BTreeMap priority
    - Depends on: EventBus event variants from Phases 2-3 (new Compaction, Security, Memory events)

### Phase 5: Channels & API
*New adapters and API surface. Fully independent of each other and mostly independent of Phases 1-4.*

11. **iMessage Adapter (blufio-imessage)** -- 1 plan
12. **Email Adapter (blufio-email)** -- 1 plan
13. **SMS Adapter (blufio-sms)** -- 1 plan
14. **OpenAPI Spec (blufio-gateway)** -- 1 plan

### Phase 6: Observability & Infrastructure

15. **OpenTelemetry (blufio-otel)** -- 1 plan
    - Benefits from all Phase 1-4 tracing spans but has no hard dependency

16. **Litestream Replication** -- 1 plan
    - Documentation, CLI helper, Docker compose, storage.litestream_mode config

### Phase 7: Compliance & Export

17. **GDPR Tooling (blufio CLI)** -- 1 plan
    - CLI subcommands for erasure, export, reporting
    - Depends on: DataClassification, PII Redaction, Audit Trail, Retention Policies (all earlier phases)

### Phase 8: Code Quality Hardening

18. **Clippy unwrap enforcement** -- 1 plan
19. **Test coverage expansion** -- 1 plan
20. **Bug fixes and tech debt** -- 1 plan

**Total: ~24 plans across 8 phases (vs. 16 plans across 7 phases for v1.4)**

---

## EventBus Extensions

The existing `BusEvent` enum (7 variants) needs these new variants for v1.5:

```rust
pub enum BusEvent {
    // Existing 7 variants (unchanged):
    Session(SessionEvent),
    Channel(ChannelEvent),
    Skill(SkillEvent),
    Node(NodeEvent),
    Webhook(WebhookEvent),
    Batch(BatchEvent),
    Resilience(ResilienceEvent),

    // New v1.5 variants (7 additions):
    Compaction(CompactionEvent),      // Triggered, Completed with level/quality
    Security(SecurityEvent),          // InjectionDetected, PiiFound
    Memory(MemoryEvent),              // Extracted, Evicted, Validated
    Scheduler(SchedulerEvent),        // JobTriggered, JobCompleted, JobFailed
    Config(ConfigEvent),              // Reloaded, ValidationFailed
    Retention(RetentionEvent),        // Enforced, PolicyUpdated
    Hook(HookEvent),                  // Executed, Failed, TimedOut
}
```

Each new variant requires: `event_type_string()` match arm, `Serialize`/`Deserialize` derives, and serde roundtrip tests. The exhaustive match on `BusEvent` in `event_type_string()` ensures the compiler catches any unhandled variants.

---

## Anti-Patterns to Avoid

### Anti-Pattern 1: Monolithic Migration
**What:** Putting all 4 schema changes in one massive migration file.
**Why bad:** If any part fails, the entire migration rolls back. Refinery tracks migrations individually.
**Instead:** One migration per feature domain (V12 scheduler, V13 memory, V14 audit, V15 retention). Each independently testable and rollback-safe.

### Anti-Pattern 2: Circular Crate Dependencies
**What:** blufio-audit depending on blufio-agent which depends on blufio-audit.
**Why bad:** Compile error. Rust workspace does not allow circular dependencies.
**Instead:** Audit subscribes to EventBus (one-way). Agent publishes to EventBus. No direct crate dependency between them. AuditLogger injected as `Arc<AuditLogger>` via serve.rs wiring.

### Anti-Pattern 3: Synchronous Audit Writes in Hot Path
**What:** Blocking the agent loop's `handle_inbound()` to write audit entries to SQLite.
**Why bad:** Audit storage latency (1-5ms per write) directly impacts response latency.
**Instead:** Audit writes go through a buffered mpsc channel. Background tokio task flushes to SQLite in batches (every 100ms or 50 entries). Agent loop sends and continues immediately.

### Anti-Pattern 4: Hot Reload Without Validation
**What:** Swapping config on file change without re-running `validate_config()`.
**Why bad:** Invalid config silently takes effect, causing runtime errors (e.g., negative thresholds, missing required fields).
**Instead:** ConfigWatcher runs full validation pipeline on new config. If validation fails, keep current config, log error, publish `ConfigEvent::ValidationFailed`. Never swap invalid config.

### Anti-Pattern 5: PII Stored Then Redacted on Read
**What:** Storing PII in plain text and redacting every time data is read.
**Why bad:** PII remains in DB (compliance risk), redaction cost on every read.
**Instead:** Optionally redact PII before storage write (configurable). Store redacted version. Original only in encrypted audit log for compliance evidence.

### Anti-Pattern 6: Hook Failures Blocking Agent Loop
**What:** Agent loop awaiting hook completion before processing next message.
**Why bad:** A hanging hook script blocks all message processing.
**Instead:** Hooks triggered via EventBus subscription. Hook execution is fire-and-forget with per-hook timeout. Failures logged and emitted as events but never propagate to the agent loop or block message processing.

### Anti-Pattern 7: Global Config Mutex for Hot Reload
**What:** Using `Arc<RwLock<BlufioConfig>>` for hot reload.
**Why bad:** Every config read takes a read lock. Under high concurrency, writer starvation or reader contention.
**Instead:** `ArcSwap` provides lock-free reads (just a pointer load). Writes are also lock-free (atomic pointer swap). Zero contention on reads, which happen thousands of times per second.

---

## Scalability Considerations

| Concern | At 100 users | At 10K users | At 1M users |
|---------|--------------|--------------|-------------|
| Audit log size | ~10MB/day, negligible | ~1GB/day, partition by month | Out of scope (single-instance) |
| Compaction CPU | Negligible (few Haiku calls) | Linear with active sessions | LLM API cost dominates |
| Memory index | <10K entries, all in LRU | LRU keeps at 10K via eviction | Not applicable |
| Hook execution | Serial execution fine | Parallel with pool (max 4) | Not applicable |
| PII regex | ~1ms/message (5 patterns) | ~1ms/message (regex is O(n)) | Not applicable |
| Config reload | Instant via ArcSwap | Instant (single pointer swap) | N/A |
| Audit chain verify | <1s for full chain | ~10s for 1M entries | Chunk-based verification needed |
| Retention enforcement | <1s for 100K messages | ~5s with DELETE batching | Paginated DELETE with LIMIT |

---

## Sources

- Direct analysis of 80,101 LOC across 35 crates (HIGH confidence)
- [ArcSwap patterns documentation](https://docs.rs/arc-swap/latest/arc_swap/docs/patterns/index.html)
- [ArcSwap crate](https://crates.io/crates/arc-swap)
- [OpenTelemetry Rust documentation](https://opentelemetry.io/docs/languages/rust/)
- [tracing-opentelemetry crate](https://docs.rs/tracing-opentelemetry)
- [OpenTelemetry tracing integration guide (Feb 2026)](https://oneuptime.com/blog/post/2026-02-06-opentelemetry-tracing-rust-tracing-crate/view)
- [Litestream - How it works](https://litestream.io/how-it-works/)
- [Litestream tips and caveats](https://litestream.io/tips/)
- [utoipa - OpenAPI generation for Rust](https://github.com/juhaku/utoipa)
- [utoipa-axum integration](https://docs.rs/utoipa-axum)
- [OWASP Prompt Injection Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/LLM_Prompt_Injection_Prevention_Cheat_Sheet.html)
- [OWASP LLM01:2025 Prompt Injection](https://genai.owasp.org/llmrisk/llm01-prompt-injection/)
- [tldrsec/prompt-injection-defenses](https://github.com/tldrsec/prompt-injection-defenses)
- [BlueBubbles REST API & Webhooks](https://docs.bluebubbles.app/server/developer-guides/rest-api-and-webhooks)
- [tokio-cron-scheduler](https://crates.io/crates/tokio-cron-scheduler)
- [croner crate](https://crates.io/crates/croner)
- [Hash-chained tamper-evident audit log](https://dev.to/veritaschain/building-a-tamper-evident-audit-log-with-sha-256-hash-chains-zero-dependencies-h0b)
- [Clawprint: tamper-evident audit trail for agent runs](https://github.com/cyntrisec/clawprint)
