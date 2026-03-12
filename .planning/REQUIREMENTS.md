# Requirements: Blufio v1.5 PRD Gap Closure

**Defined:** 2026-03-10
**Core Value:** An always-on personal AI agent that is secure enough to trust, efficient enough to afford, and simple enough to deploy by copying one file.

## v1.5 Requirements

Requirements for PRD gap closure milestone. Each maps to roadmap phases.

### Compaction

- [x] **COMP-01**: Context compaction supports 4 levels (L0 raw -> L1 turn-pair summaries -> L2 session summary -> L3 cross-session archive)
- [x] **COMP-02**: Quality scoring evaluates compaction output with weighted dimensions (entity retention 35%, decision retention 25%, action retention 25%, numerical retention 15%)
- [x] **COMP-03**: Quality gates enforce thresholds (>=0.6 proceed, 0.4-0.6 retry with different prompt, <0.4 abort compaction)
- [x] **COMP-04**: Soft trigger fires compaction at configurable threshold (default 50% context window), hard trigger at 85%
- [x] **COMP-05**: Archive system stores L3 compacted summaries with cold storage retrieval for historical context
- [x] **COMP-06**: Entity/fact extraction runs before compaction to preserve critical facts as separate Memory entries

### Context Engine

- [x] **CTXE-01**: Zone 1 (static) enforces configurable token budget (default 3,000 tokens) with 10% safety margin
- [x] **CTXE-02**: Zone 2 (conditional) enforces configurable token budget (default 8,000 tokens) with 10% safety margin
- [x] **CTXE-03**: Token budget enforcement uses accurate provider-specific token counting (tiktoken-rs/HuggingFace)

### Prompt Injection Defense

- [x] **INJC-01**: L1 pattern classifier detects known injection signatures via regex with 0.0-1.0 confidence scoring
- [x] **INJC-02**: L1 operates in log-not-block mode by default, blocking only at >0.95 confidence (configurable)
- [x] **INJC-03**: L3 HMAC-SHA256 boundary tokens cryptographically separate system/user/external content zones
- [ ] **INJC-04**: L4 output validator screens LLM responses for credential leaks and injection relay before tool execution
- [ ] **INJC-05**: L5 human-in-the-loop confirmation flow for configurable high-risk operations (tool calls, data export, config changes)
- [ ] **INJC-06**: Injection defense integrates with MCP client tool output and WASM skill results

### PII Detection

- [x] **PII-01**: Regex-based PII detection covers email addresses, phone numbers (international formats), SSN patterns, and credit card numbers (Luhn-validated)
- [x] **PII-02**: PII detection integrates with existing RedactingWriter for log output
- [x] **PII-03**: PII detection applies to data exports with configurable redaction
- [x] **PII-04**: PII-containing content auto-classifies as Confidential when data classification is active
- [x] **PII-05**: Context-aware redaction skips PII patterns inside code blocks and URLs

### Data Classification

- [x] **DCLS-01**: DataClassification enum with 4 levels: Public, Internal, Confidential, Restricted
- [x] **DCLS-02**: Classifiable trait allows tagging memories, messages, exports, and config values
- [x] **DCLS-03**: Per-level controls matrix: Restricted = never exported + never in LLM context; Confidential = encrypted at rest + redacted in logs; Internal = audit-logged; Public = no restrictions
- [x] **DCLS-04**: Classification can be set explicitly via API/CLI or inferred from PII detection
- [x] **DCLS-05**: Classification changes logged in audit trail

### Audit Trail

- [x] **AUDT-01**: Hash-chained tamper-evident log where each entry hash = SHA-256(prev_hash || canonical_entry)
- [x] **AUDT-02**: Audit entries cover: tool execution, memory modification, config changes, provider calls, session lifecycle, classification changes, erasure events
- [x] **AUDT-03**: Audit trail stored in dedicated audit.db (separate from main database)
- [x] **AUDT-04**: CLI command `blufio audit verify` walks hash chain and reports any breaks
- [x] **AUDT-05**: Audit entries are append-only -- retention policies never delete them
- [x] **AUDT-06**: Audit schema supports GDPR redact-in-place (PII fields replaceable with [ERASED] without breaking hash chain)
- [x] **AUDT-07**: Async audit writes via buffered mpsc channel with batch flush

### Memory Enhancements

- [x] **MEME-01**: Temporal decay applies configurable decay factor (default 0.95^days) to retrieval scores
- [x] **MEME-02**: Importance boost multiplier distinguishes explicit memories (1.0) from extracted memories (0.6)
- [x] **MEME-03**: MMR diversity reranking reduces redundant results using lambda-weighted relevance vs. similarity penalty
- [x] **MEME-04**: Bounded memory index with configurable max entries (default 10,000) and LRU eviction of lowest-scored entries
- [x] **MEME-05**: Background memory validation detects duplicates, stale entries, and conflicts on configurable interval
- [x] **MEME-06**: File watcher auto re-indexes workspace files on change with 500ms debounce

### Retention Policies

- [ ] **RETN-01**: TOML-configurable retention periods per data type (messages, sessions, cost records, memories)
- [ ] **RETN-02**: Background retention enforcement runs on configurable schedule (default: daily)
- [ ] **RETN-03**: Soft-delete support with configurable grace period before permanent removal
- [ ] **RETN-04**: Audit trail entries exempt from retention deletion
- [ ] **RETN-05**: Retention enforcement respects data classification (Restricted data has separate retention rules)

### Cron/Scheduler

- [ ] **CRON-01**: TOML-configured cron jobs with standard cron expression syntax
- [ ] **CRON-02**: CLI `blufio cron` subcommand for list, add, remove, run-now, history
- [ ] **CRON-03**: systemd timer unit file generation via `blufio cron generate-timers`
- [ ] **CRON-04**: Job execution history tracked in SQLite with status, duration, and output
- [ ] **CRON-05**: Built-in tasks: memory cleanup, backup, cost report, health check, retention enforcement
- [ ] **CRON-06**: Persisted last-run timestamps survive process restarts

### Hook System

- [ ] **HOOK-01**: 11 lifecycle hooks: pre_start, post_start, pre_shutdown, post_shutdown, session_created, session_closed, pre_compaction, post_compaction, degradation_changed, config_reloaded, memory_extracted
- [ ] **HOOK-02**: TOML-defined hooks with BTreeMap priority ordering (lower number = higher priority)
- [ ] **HOOK-03**: Shell-based hook execution with JSON stdin (event context) and stdout (optional response)
- [ ] **HOOK-04**: Hook sandboxing with configurable timeout, restricted PATH, and optional network isolation
- [ ] **HOOK-05**: Hooks subscribe to EventBus events for asynchronous trigger
- [ ] **HOOK-06**: Recursion depth counter prevents hook-triggered-hook infinite loops

### Hot Reload

- [ ] **HTRL-01**: Config hot reload: file watcher on blufio.toml triggers parse -> validate -> ArcSwap swap
- [ ] **HTRL-02**: TLS certificate hot reload via rustls ResolvesServerCert with file watcher
- [ ] **HTRL-03**: Plugin hot reload: re-scan skill directory, reload changed WASM modules, verify signatures
- [ ] **HTRL-04**: Config propagation via ordered EventBus events with validation-before-swap
- [ ] **HTRL-05**: Active sessions continue on current config; new sessions use reloaded config
- [ ] **HTRL-06**: config_reloaded lifecycle hook fires after successful reload

### GDPR Tooling

- [ ] **GDPR-01**: CLI `blufio gdpr erase --user <id>` deletes all user data (messages, memories, session metadata, cost records)
- [ ] **GDPR-02**: Cost record anonymization preserves aggregates but removes user association on erasure
- [ ] **GDPR-03**: Erasure logged as audit trail entry (audit entries themselves not deleted)
- [ ] **GDPR-04**: `blufio gdpr report --user <id>` generates transparency report of held data
- [ ] **GDPR-05**: Export before erasure as configurable safety net
- [ ] **GDPR-06**: Data export supports JSON and CSV formats with filtering by session, date range, and data type

### Channel Adapters

- [ ] **CHAN-01**: Email adapter with IMAP polling for incoming messages and SMTP (lettre) for outgoing
- [ ] **CHAN-02**: Email thread-to-session mapping via In-Reply-To/References headers
- [ ] **CHAN-03**: iMessage adapter via BlueBubbles REST API with webhook for incoming messages
- [ ] **CHAN-04**: iMessage adapter documented as experimental (BlueBubbles requires macOS host)
- [ ] **CHAN-05**: SMS adapter via Twilio Programmable Messaging API (webhook inbound + REST outbound)
- [ ] **CHAN-06**: All three adapters implement existing ChannelAdapter + PluginAdapter traits
- [ ] **CHAN-07**: All three adapters support FormatPipeline integration

### OpenTelemetry

- [ ] **OTEL-01**: OpenTelemetry tracing via tracing-opentelemetry bridge, feature-gated (disabled by default)
- [ ] **OTEL-02**: OTLP export (HTTP mode) to configurable endpoint
- [ ] **OTEL-03**: Key spans: agent loop iteration, LLM provider call, tool execution, memory retrieval, context assembly
- [ ] **OTEL-04**: Trace context propagation through MCP calls
- [ ] **OTEL-05**: Zero overhead when disabled (feature-gate at compile time)
- [ ] **OTEL-06**: Coexists with existing Prometheus metrics (OTel for traces only, Prometheus for metrics)

### OpenAPI

- [ ] **OAPI-01**: OpenAPI 3.1 spec auto-generated from axum route definitions via utoipa annotations
- [ ] **OAPI-02**: Spec served at /openapi.json endpoint
- [ ] **OAPI-03**: Optional Swagger UI served at /docs when enabled in config
- [ ] **OAPI-04**: All existing gateway endpoints annotated with request/response schemas

### Litestream

- [ ] **LITE-01**: Litestream config template generation via `blufio litestream init`
- [ ] **LITE-02**: `blufio litestream status` checks replication lag
- [ ] **LITE-03**: Documentation of SQLCipher incompatibility with mitigation (application-level backup alternative)
- [ ] **LITE-04**: WAL autocheckpoint disabled when Litestream mode active (PRAGMA wal_autocheckpoint=0)

### Code Quality

- [ ] **QUAL-01**: #![deny(clippy::unwrap_used)] enforced across all library crates
- [ ] **QUAL-02**: All 1,444+ unwrap() calls in library crates replaced with proper error handling
- [ ] **QUAL-03**: /api/status endpoint returns actual uptime instead of hardcoded 0
- [ ] **QUAL-04**: Mock provider replaces unimplemented!() with proper stubs
- [ ] **QUAL-05**: serve.rs and other oversized functions decomposed into smaller init functions
- [ ] **QUAL-06**: Integration tests added for channel adapters
- [ ] **QUAL-07**: Property-based testing for core algorithms (compaction quality, PII detection, hash chain verification)
- [ ] **QUAL-08**: Benchmark regression detection in CI

## Future Requirements

Deferred to v1.6+.

### Hot Reload Enhancements
- **HTRL-07**: Full plugin state preservation across hot reload cycles
- **HTRL-08**: Cross-provider session migration on config change

### Advanced PII
- **PII-06**: ML-based NER for context-dependent PII (names, addresses)
- **PII-07**: Real-time PII scanning of all LLM context (opt-in)

### Compliance
- **COMP-07**: External witness integration (cloud KMS, git) for audit trail chain head snapshots
- **GDPR-07**: Automated backup exclusion for pre-erasure data

## Out of Scope

| Feature | Reason |
|---------|--------|
| Blockchain-based audit trail | Massive complexity, zero practical benefit for single-instance system. SHA-256 hash chain is sufficient. |
| ML-based PII detection | No mature Rust crate. Regex covers 95%+ of structured PII. |
| GDPR consent management UI | Blufio has no web UI. Operators are data controllers responsible for consent. |
| Embedded Litestream | Litestream is Go binary. Sidecar pattern preferred (same as signal-cli). |
| Auto PII-based classification | False confidence risk. Rule-based + manual classification preferred. |
| Cross-provider session migration | Different tokenizers, context windows, caching. New sessions use new config. |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| DCLS-01 | Phase 53 | Complete |
| DCLS-02 | Phase 53 | Complete |
| DCLS-03 | Phase 53 | Complete |
| DCLS-04 | Phase 53 | Complete |
| DCLS-05 | Phase 53 | Complete |
| PII-01 | Phase 53 | Complete |
| PII-02 | Phase 53 | Complete |
| PII-03 | Phase 53 | Complete |
| PII-04 | Phase 53 | Complete |
| PII-05 | Phase 53 | Complete |
| AUDT-01 | Phase 54 | Complete |
| AUDT-02 | Phase 54 | Complete |
| AUDT-03 | Phase 54 | Complete |
| AUDT-04 | Phase 54 | Complete |
| AUDT-05 | Phase 54 | Complete |
| AUDT-06 | Phase 54 | Complete |
| AUDT-07 | Phase 54 | Complete |
| MEME-01 | Phase 55 | Complete |
| MEME-02 | Phase 55 | Complete |
| MEME-03 | Phase 55 | Complete |
| MEME-04 | Phase 55 | Complete |
| MEME-05 | Phase 55 | Complete |
| MEME-06 | Phase 55 | Complete |
| COMP-01 | Phase 56 | Complete |
| COMP-02 | Phase 56 | Complete |
| COMP-03 | Phase 56 | Complete |
| COMP-04 | Phase 56 | Complete |
| COMP-05 | Phase 56 | Complete |
| COMP-06 | Phase 56 | Complete |
| CTXE-01 | Phase 56 | Complete |
| CTXE-02 | Phase 56 | Complete |
| CTXE-03 | Phase 56 | Complete |
| INJC-01 | Phase 57 | Complete |
| INJC-02 | Phase 57 | Complete |
| INJC-03 | Phase 57 | Complete |
| INJC-04 | Phase 57 | Pending |
| INJC-05 | Phase 57 | Pending |
| INJC-06 | Phase 57 | Pending |
| CRON-01 | Phase 58 | Pending |
| CRON-02 | Phase 58 | Pending |
| CRON-03 | Phase 58 | Pending |
| CRON-04 | Phase 58 | Pending |
| CRON-05 | Phase 58 | Pending |
| CRON-06 | Phase 58 | Pending |
| RETN-01 | Phase 58 | Pending |
| RETN-02 | Phase 58 | Pending |
| RETN-03 | Phase 58 | Pending |
| RETN-04 | Phase 58 | Pending |
| RETN-05 | Phase 58 | Pending |
| HOOK-01 | Phase 59 | Pending |
| HOOK-02 | Phase 59 | Pending |
| HOOK-03 | Phase 59 | Pending |
| HOOK-04 | Phase 59 | Pending |
| HOOK-05 | Phase 59 | Pending |
| HOOK-06 | Phase 59 | Pending |
| HTRL-01 | Phase 59 | Pending |
| HTRL-02 | Phase 59 | Pending |
| HTRL-03 | Phase 59 | Pending |
| HTRL-04 | Phase 59 | Pending |
| HTRL-05 | Phase 59 | Pending |
| HTRL-06 | Phase 59 | Pending |
| GDPR-01 | Phase 60 | Pending |
| GDPR-02 | Phase 60 | Pending |
| GDPR-03 | Phase 60 | Pending |
| GDPR-04 | Phase 60 | Pending |
| GDPR-05 | Phase 60 | Pending |
| GDPR-06 | Phase 60 | Pending |
| CHAN-01 | Phase 61 | Pending |
| CHAN-02 | Phase 61 | Pending |
| CHAN-03 | Phase 61 | Pending |
| CHAN-04 | Phase 61 | Pending |
| CHAN-05 | Phase 61 | Pending |
| CHAN-06 | Phase 61 | Pending |
| CHAN-07 | Phase 61 | Pending |
| OTEL-01 | Phase 62 | Pending |
| OTEL-02 | Phase 62 | Pending |
| OTEL-03 | Phase 62 | Pending |
| OTEL-04 | Phase 62 | Pending |
| OTEL-05 | Phase 62 | Pending |
| OTEL-06 | Phase 62 | Pending |
| OAPI-01 | Phase 62 | Pending |
| OAPI-02 | Phase 62 | Pending |
| OAPI-03 | Phase 62 | Pending |
| OAPI-04 | Phase 62 | Pending |
| LITE-01 | Phase 62 | Pending |
| LITE-02 | Phase 62 | Pending |
| LITE-03 | Phase 62 | Pending |
| LITE-04 | Phase 62 | Pending |
| QUAL-01 | Phase 63 | Pending |
| QUAL-02 | Phase 63 | Pending |
| QUAL-03 | Phase 63 | Pending |
| QUAL-04 | Phase 63 | Pending |
| QUAL-05 | Phase 63 | Pending |
| QUAL-06 | Phase 63 | Pending |
| QUAL-07 | Phase 63 | Pending |
| QUAL-08 | Phase 63 | Pending |

**Coverage:**
- v1.5 requirements: 93 total
- Mapped to phases: 93
- Unmapped: 0

---
*Requirements defined: 2026-03-10*
*Last updated: 2026-03-10 after roadmap creation (traceability populated)*
