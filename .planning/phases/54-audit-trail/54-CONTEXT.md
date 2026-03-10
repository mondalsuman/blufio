# Phase 54: Audit Trail - Context

**Gathered:** 2026-03-10
**Status:** Ready for planning

<domain>
## Phase Boundary

Every security-relevant action in Blufio is recorded in a tamper-evident, hash-chained audit log stored in a dedicated audit.db. The log supports independent verification, GDPR redact-in-place without breaking the chain, and async buffered writes that never block the agent loop. Phase 60 (GDPR Tooling) will call the erasure API; this phase provides the infrastructure.

</domain>

<decisions>
## Implementation Decisions

### Entry Schema
- Split-field design: immutable fields in hash, PII fields redactable
- Hashed (immutable): prev_hash, timestamp, event_type, action, resource_type, resource_id
- Redactable (NOT in hash): actor, session_id, details_json, pii_marker
- Hash format: pipe-delimited canonical — `SHA-256(prev_hash|timestamp|event_type|action|resource_type|resource_id)`
- Entry ID: INTEGER PRIMARY KEY AUTOINCREMENT (enables gap detection, trivial chain ordering)
- details_json: JSON metadata field, erasable to "[ERASED]" without breaking hash chain
- pii_marker: INTEGER DEFAULT 0, set to 1 when GDPR erasure applied
- Encryption: same BLUFIO_DB_KEY (SQLCipher) as main database via existing open_connection()
- Genesis: first real event uses prev_hash of 64 zero hex chars (no synthetic genesis entry)
- Hash chain format is internal implementation — no versioning, migration re-hashes if format changes

### Event Coverage
- Per-invocation tool audit (one entry per tool call: WASM skills, MCP tools, built-in tools)
- Provider calls: metadata only (model, tokens, cost, latency, success) — no prompt/response content
- All operations audited including memory reads (configurable via TOML allowlist)
- TOML event filter: `events = ["all"]` default, dot-prefix matching (e.g., "session.*" matches session.created/closed)
- API requests: mutating only (POST, PUT, DELETE) — GET/health/status excluded
- Actor convention: prefixed strings — "user:{id}", "api-key:{key_id}", "system", "cron:{job_name}"
- Session lifecycle includes channel + user_id in details_json
- Audit enable/disable state changes logged as audit.enabled/audit.disabled events
- Erasure operations logged as audit.erased with sha256(user_id) in details (not plaintext)

### New BusEvent Variants
- 5 new variants added to BusEvent enum in events.rs (same file, single source of truth):
  - Config(ConfigEvent) — ConfigChanged, ConfigReloaded
  - Memory(MemoryEvent) — Created, Updated, Deleted, Retrieved, Evicted
  - Audit(AuditMetaEvent) — Enabled, Disabled, Erased
  - Api(ApiEvent) — Request (mutating HTTP requests)
  - Provider(ProviderEvent) — Called (LLM call metadata)
- All added to event_type_string() match for exhaustive coverage
- Emission sites: Config in reload handler, Memory in blufio-memory CRUD, Api in gateway middleware, Audit in AuditWriter, Provider in agent loop after call returns

### EventBus Subscriber Pattern
- Single AuditSubscriber subscribes to all BusEvent variants
- Internal filtering via TOML allowlist (filter.matches(event_type))
- Converts BusEvent to AuditEntry and sends to AuditWriter via mpsc
- blufio-memory gains blufio-bus dependency (Optional<Arc<EventBus>> pattern for tests/CLI)
- Provider events emitted in blufio-agent SessionActor (after provider.chat() returns)
- Gateway API events emitted via axum middleware layer on mutating routes

### Async Write Pipeline
- Bounded mpsc channel (capacity 1024)
- Background task drains channel, batches entries, single INSERT transaction
- Flush triggers: batch size (64 entries) OR time interval (1 second) OR shutdown signal
- Overflow: try_send() — if full, log warning + increment blufio_audit_dropped_total counter. Never block agent loop.
- SHA-256 hashing in background task only (maintains chain head in memory, single-writer)
- Chain head recovery on startup: SELECT entry_hash ORDER BY id DESC LIMIT 1 (or 64 zeros if empty)
- Public flush() API via oneshot channel — used by shutdown handler and GDPR erasure
- WAL mode + synchronous=NORMAL + foreign_keys=ON (same as main database)

### Verification & CLI
- Three subcommands: `blufio audit verify`, `blufio audit tail`, `blufio audit stats`
- verify: walks hash chain, checks ID sequence gaps, reports GDPR-erased count. Exit code 0 (OK) or 1 (broken)
- verify output: summary with intact hashes, erased entries, gaps, status
- verify reports break location with expected vs actual hash and surrounding entries
- tail: last N entries with filters — --type (dot-prefix), --since/--until (ISO timestamps), --actor (prefix match)
- tail shows GDPR-erased entries with [ERASED] marker (not filtered out)
- tail --follow deferred to future phase
- stats: total entries, first/last timestamp, erased count, counts by event type
- All three support --json output
- CLI reads work even when audit is disabled (read-only mode on existing data, with note)
- clap after_help with workflow examples on all subcommands

### Config Schema
- Simple: enabled (bool, default true), db_path (Option<String>, None = {data_dir}/audit.db), events (Vec<String>, default ["all"])
- #[serde(deny_unknown_fields)] — consistent with other config sections
- Fully optional section — omitting [audit] applies all defaults (enabled=true, all events)
- db_path derives from existing storage.data_dir when None
- Event patterns validated at startup with warning on unrecognized prefixes (not hard fail)
- Config validation on first use (AuditWriter init), not at parse time
- Commented [audit] section in blufio.example.toml

### Error Handling & Resilience
- Agent loop continues with warning if audit.db fails — core value: never block for audit
- Audit DB treated as dependency with circuit breaker (wire to existing degradation ladder from Phase 48)
- New BlufioError::Audit(AuditError) variant: DbUnavailable, ChainBroken, FlushFailed, VerifyFailed
- Error classification: is_retryable()=true, severity=Error, category=Security
- Auto-create audit.db on first use if it doesn't exist (with schema migration)
- blufio doctor includes audit health check (last 100 entries, not full chain walk)
- Prometheus metrics: blufio_audit_entries_total, blufio_audit_batch_flush_total, blufio_audit_dropped_total, blufio_audit_flush_duration_seconds, blufio_audit_errors_total

### GDPR Erasure Mechanics
- Phase 54 provides erase_audit_entries(db, user_id) function — called by Phase 60 GDPR CLI
- Match entries by actor prefix ("user:{user_id}%") OR details_json containing user_id
- Erase actor, session_id, and details_json to "[ERASED]", set pii_marker=1
- All three PII fields excluded from hash — erasure never breaks chain integrity
- Erasure operation logged as audit.erased entry with sha256(user_id) (not plaintext)
- Returns AuditErasureReport struct: entries_found, entries_erased, erased_ids
- Flush pending entries before erasure to ensure complete coverage

### Integration with serve.rs
- Init order: after EventBus, before channel adapters (so adapter startup events are captured)
- Shared via Arc<AuditWriter> — cloned to AuditSubscriber and gateway
- Shutdown order: flush after adapters disconnect, before DB close (reverse of startup)
- Backup command (blufio backup) includes audit.db alongside main database

### Crate Organization
- New blufio-audit crate: lib.rs, writer.rs (AuditWriter), subscriber.rs (AuditSubscriber), chain.rs (hash chain + verify), models.rs (AuditEntry), migrations.rs (schema)
- Dependencies: blufio-bus, blufio-core, blufio-storage (for open_connection), sha2, tokio, rusqlite/tokio-rusqlite, serde_json, chrono
- blufio-memory gains blufio-bus dependency for MemoryEvent emission
- blufio-agent emits ProviderEvent after LLM calls
- blufio-gateway gets audit middleware layer for ApiEvent emission

### Testing
- Unit tests: chain builds correctly, genesis zero hash, GDPR erase preserves chain, gap detection, tamper detection, batch insert chain maintenance
- Property-based (proptest): arbitrary entries always produce valid chain
- Integration: full EventBus -> AuditSubscriber -> AuditWriter -> audit.db pipeline
- CLI integration tests: verify/tail/stats against pre-populated audit.db, --json output, filter behavior
- Overflow test: deliberately fill mpsc, verify drop + warning + Prometheus counter
- Dedicated GDPR test: create entries with PII, erase, verify chain intact + [ERASED] markers
- Event filter tests: "all" matches everything, prefix matching, exact matching, non-matching
- Criterion benchmarks: hash throughput, batch insert 1000 entries, verify chain 1000 entries
- Uses blufio-test-utils for temp directory and database setup

### Documentation
- Rustdoc on all public items in blufio-audit
- Module-level doc comment with hash chain design overview and architecture diagram (text)
- clap after_help with forensic workflow examples on all audit subcommands
- Commented [audit] section in blufio.example.toml with selective auditing example
- No separate ADR — design documented in module-level rustdoc
- Commit convention only (no manual CHANGELOG)

### Claude's Discretion
- Exact crate dependency versions (sha2, chrono pinning)
- Internal module structure within blufio-audit (sub-modules vs flat)
- Exact Prometheus metric label values
- Test fixture data and organization
- Migration version numbering
- Batch INSERT SQL optimization (multi-row vs loop)
- AuditSubscriber BusEvent-to-AuditEntry conversion logic details

</decisions>

<specifics>
## Specific Ideas

- Hash chain format: pipe-delimited `SHA-256(prev_hash|timestamp|event_type|action|resource_type|resource_id)` — simple, deterministic, no JSON parsing
- PII fields (actor, session_id, details_json) deliberately excluded from hash to enable GDPR erasure without chain breaks
- EventBus subscriber pattern reuses existing infrastructure — single subscriber receives all, filters internally
- Audit middleware in gateway: axum layer on mutating routes, transparent to handlers
- Provider audit in agent loop: single emission point after provider.chat() returns, no per-provider crate changes
- Doctor health check: last 100 entries only for speed, full verify via dedicated command

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `blufio-storage::database::open_connection()`: Audit.db uses same connection factory with SQLCipher support
- `blufio-bus::events::BusEvent`: Extend with 5 new variants (Config, Memory, Audit, Api, Provider)
- `blufio-bus::events::event_type_string()`: Existing pattern for dot-separated event type strings
- `blufio-core::error::BlufioError`: Extend with Audit(AuditError) variant following typed hierarchy pattern
- `blufio-test-utils`: Temp directory and database setup helpers for tests
- `blufio-prometheus`: EventBus subscriber pattern for metrics — same pattern for AuditSubscriber
- Circuit breaker infrastructure (Phase 48): Wire audit DB as dependency for degradation

### Established Patterns
- tokio-rusqlite single-writer thread for all SQLite writes
- #[serde(deny_unknown_fields)] on all config structs
- Optional<Arc<EventBus>> for components that emit events (None in tests/CLI)
- LazyLock for compiled regex patterns (event filter can use similar)
- Arc<T> for shared resources passed through startup chain
- EventBus fire-and-forget for async event emission
- CLI subcommands in main binary crate, library logic in crate libraries

### Integration Points
- serve.rs: AuditWriter init after EventBus, before adapters; flush on shutdown
- blufio-config: new AuditConfig struct with #[serde(default)]
- blufio-gateway: new audit middleware layer for API request events
- blufio-agent: ProviderEvent emission after provider.chat()
- blufio-memory: MemoryEvent emission in CRUD methods (new blufio-bus dep)
- blufio (binary): new `blufio audit verify|tail|stats` CLI subcommands
- blufio doctor: audit trail health check (last 100 entries)
- blufio backup: include audit.db in backup/restore

</code_context>

<deferred>
## Deferred Ideas

- `blufio audit tail --follow` (streaming mode) — future enhancement
- External witness integration (cloud KMS, git) for chain head snapshots — v1.6+ per REQUIREMENTS.md
- Time-series breakdown in stats (daily/hourly bucketing) — add if operators request it
- Configurable buffer tuning knobs (buffer_capacity, flush_interval_ms, batch_size) — expose if defaults prove insufficient

</deferred>

---

*Phase: 54-audit-trail*
*Context gathered: 2026-03-10*
