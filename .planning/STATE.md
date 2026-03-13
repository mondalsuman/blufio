---
gsd_state_version: 1.0
milestone: v1.5
milestone_name: PRD Gap Closure
status: executing
stopped_at: Completed 63-04-PLAN.md
last_updated: "2026-03-13T14:42:20.448Z"
last_activity: 2026-03-13 -- Phase 63 Plan 04 complete (integration tests + property-based tests)
progress:
  total_phases: 11
  completed_phases: 8
  total_plans: 40
  completed_plans: 45
  percent: 100
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-12)

**Core value:** An always-on personal AI agent that is secure enough to trust, efficient enough to afford, and simple enough to deploy by copying one file.
**Current focus:** v1.5 PRD Gap Closure -- Phase 61 Channel Adapters

## Current Position

Phase: 63 of 63 (Code Quality & Hardening) -- eleventh of 11 phases in v1.5
Plan: 4 of 5
Status: In Progress
Last activity: 2026-03-13 -- Phase 63 Plan 04 complete (integration tests + property-based tests)

Progress: [██████████] 100%

## Performance Metrics

**Velocity (v1.0-v1.4):**
- Total plans completed: 151
- Total execution time: ~11 days
- Average: ~14 plans/day

**By Milestone:**

| Milestone | Plans | Days | Avg/Day |
|-----------|-------|------|---------|
| v1.0 | 43 | 3 | ~14 |
| v1.1 | 32 | 2 | ~16 |
| v1.2 | 13 | 1 | ~13 |
| v1.3 | 47 | 4 | ~12 |
| v1.4 | 16 | 1 | ~16 |
| Phase 53 P05 | 18min | 2 tasks | 6 files |
| Phase 54 P01 | 13min | 2 tasks | 11 files |
| Phase 54 P02 | 15min | 2 tasks | 10 files |
| Phase 54 P03 | 18min | 2 tasks | 5 files |
| Phase 55 P01 | 8min | 2 tasks | 6 files |
| Phase 55 P02 | 5min | 2 tasks | 1 files |
| Phase 55 P03 | 7min | 2 tasks | 5 files |
| Phase 55 P04 | 11min | 2 tasks | 7 files |
| Phase 56 P01 | 7min | 2 tasks | 13 files |
| Phase 56 P02 | 10min | 2 tasks | 8 files |
| Phase 56 P03 | 17min | 2 tasks | 6 files |
| Phase 56 P04 | 22min | 2 tasks | 6 files |
| Phase 56 P05 | 12min | 2 tasks | 8 files |
| Phase 56 P06 | 6min | 2 tasks | 4 files |
| Phase 57 P01 | 13min | 2 tasks | 11 files |
| Phase 57 P02 | 5min | 1 tasks | 2 files |
| Phase 57 P03 | 11min | 2 tasks | 5 files |
| Phase 57 P04 | 57min | 2 tasks | 17 files |
| Phase 57 P05 | 4min | 2 tasks | 2 files |
| Phase 58 P01 | 18min | 2 tasks | 19 files |
| Phase 58 P02 | 17min | 2 tasks | 13 files |
| Phase 58 P03 | 12min | 1 tasks | 6 files |
| Phase 58 P04 | 4min | 1 tasks | 2 files |
| Phase 59 P02 | 6min | 1 tasks | 3 files |
| Phase 59 P03 | 5min | 2 tasks | 3 files |
| Phase 59 P04 | 6min | 2 tasks | 3 files |
| Phase 59 P01 | 8min | 2 tasks | 8 files |
| Phase 60 P01 | 8min | 2 tasks | 10 files |
| Phase 60 P02 | 8min | 2 tasks | 6 files |
| Phase 60 P03 | 8min | 2 tasks | 7 files |
| Phase 61 P01 | 5min | 2 tasks | 17 files |
| Phase 61 P02 | 8min | 2 tasks | 6 files |
| Phase 61 P03 | 7min | 2 tasks | 10 files |
| Phase 61 P04 | 3min | 2 tasks | 2 files |
| Phase 62 P01 | 6min | 2 tasks | 4 files |
| Phase 62 P05 | 8min | 2 tasks | 4 files |
| Phase 62 P02 | 11min | 2 tasks | 2 files |
| Phase 62 P04 | 14min | 2 tasks | 17 files |
| Phase 62 P03 | 25min | 2 tasks | 9 files |
| Phase 63 P05 | 11min | 2 tasks | 7 files |
| Phase 63 P04 | 15min | 2 tasks | 12 files |

## Accumulated Context

### Decisions

All decisions logged in PROJECT.md Key Decisions table.
Recent: v1.5 roadmap derives 11 phases from 93 requirements across 17 categories at fine granularity.
- Phase 53 Plan 01: PII patterns in single source-of-truth array preventing RegexSet index mismatch
- Phase 53 Plan 01: Overlapping PII match deduplication (longest match wins)
- Phase 53 Plan 01: DataClassification uses derive(Default) with #[default] per clippy
- Phase 53 Plan 02: ClassificationEvent uses String fields to avoid blufio-bus -> blufio-core dependency
- Phase 53 Plan 02: PII redaction runs before secret redaction in combined pipeline
- Phase 53 Plan 02: Restricted data excluded from memory retrieval via SQL WHERE clause
- Phase 53 Plan 03: API routes use {param} syntax (axum v0.8+) for route path parameters
- Phase 53 Plan 03: PII detection in agent uses catch_unwind for panic safety
- Phase 53 Plan 03: Context filtering uses defense-in-depth (SQL primary + guard reference)
- Phase 53 Plan 04: Default::default() for classification field in struct literals across workspace
- Phase 53 Plan 04: row_to_message/row_to_session helpers with unwrap_or_default for resilient parsing
- Phase 53 Plan 04: Closure-based condition builder in bulk_update to avoid dry_run/execute duplication
- Phase 53 Plan 05: CLI uses Database::open (not raw open_connection) for classification query access
- Phase 53 Plan 05: Context defense-in-depth filtering placed in dynamic.rs where Message has classification field
- Phase 53 Plan 05: Export utility split into redact_for_export (single) + filter_for_export (batch)
- Phase 54 Plan 01: PII fields excluded from SHA-256 hash for GDPR erasure safety
- Phase 54 Plan 01: EventFilter prefix matching requires dot separator (session.* not sessionX)
- Phase 54 Plan 01: AuditWriter uses tokio::select! with interval for time-based flush
- Phase 54 Plan 01: Channel overflow drops entries with warning counter, never blocks caller
- Phase 54 Plan 01: Chain head recovered from last entry_hash on writer restart
- [Phase 54]: All sub-enums use String fields to avoid cross-crate dependencies
- [Phase 54]: MemoryStore uses Optional<Arc<EventBus>> pattern (None for tests/CLI)
- [Phase 54]: ProviderEvent emitted in persist_response after cost recording
- [Phase 54]: Gateway audit middleware uses tokio::spawn fire-and-forget for event emission
- [Phase 54]: ApiEvent actor derived from AuthContext (user:master, api-key:{id}, anonymous)
- [Phase 54]: CLI audit reads use sync open_connection_sync for direct SQL queries
- [Phase 54]: Audit init in serve.rs after EventBus, before resilience subsystem
- [Phase 54]: Doctor checks last 100 entries for speed; full verify via blufio audit verify
- [Phase 54]: Backup stores audit.db as {stem}.audit.db alongside main backup
- Phase 55 Plan 01: FileWatcherConfig uses manual Default impl (not derive) for correct max_file_size=102400
- Phase 55 Plan 01: MemorySource::from_str_value matches file_watcher before Extracted catch-all
- [Phase 55]: Relevance scores normalized to [0,1] range inside MMR for balanced lambda weighting
- [Phase 55]: FileWatcher memories skip temporal decay entirely (always 1.0) for stable file-sourced knowledge
- Phase 55 Plan 03: Eviction scores computed in Rust (not SQL) because SQLite lacks native power() function
- Phase 55 Plan 03: Pairwise O(n^2) validation acceptable for bounded max_entries (10k default)
- Phase 55 Plan 03: Conflict resolution uses newer-wins (created_at lexicographic comparison)
- Phase 55 Plan 03: Test embeddings use single-hot-dimension vectors for deterministic similarity control
- Phase 55 Plan 04: File memory IDs use file: + SHA-256(canonical_path) for deterministic collision-free IDs
- Phase 55 Plan 04: File update re-indexes by hard-delete then save (FTS5 trigger consistency)
- Phase 55 Plan 04: notify callback uses tx.blocking_send() (not async) since it runs on notify's own thread
- Phase 55 Plan 04: conn() accessor added to MemoryStore for advanced SQL operations
- Phase 56 Plan 01: compaction_threshold changed to Option<f64> with effective_soft_trigger() deprecation bridge
- Phase 56 Plan 01: CompactionEvent uses String fields (no cross-crate deps) following bus event pattern
- Phase 56 Plan 01: delete_messages_by_ids uses parameterized IN clause with dynamic placeholder generation
- Phase 56 Plan 01: Archive session_ids stored as JSON text with LIKE-based GDPR erasure
- [Phase 56]: Phase 56 Plan 02: Entity extraction returns strings to caller to avoid circular dependency blufio-context <-> blufio-memory
- [Phase 56]: Phase 56 Plan 02: compaction_usage changed to compaction_usages Vec<TokenUsage> for cascade compaction support
- [Phase 56]: Phase 56 Plan 03: Quality scoring via separate LLM call with entity/decision/action/numerical dimensions
- [Phase 56]: Phase 56 Plan 03: JSON parse failure for quality scores treats as 0.5 (retry range)
- [Phase 56]: Phase 56 Plan 03: L2 quality scoring uses L1 summary text as reference (raw messages already deleted)
- [Phase 56]: Phase 56 Plan 03: blufio-storage added as dependency in blufio-context (no circular)
- [Phase 56]: Phase 56 Plan 03: Classification escalation: restricted > confidential > internal for merged archives
- Phase 56 Plan 04: 10% safety margin on conditional zone hardcoded as SAFETY_MARGIN constant
- Phase 56 Plan 04: Static zone advisory-only warning (never truncates system prompt)
- Phase 56 Plan 04: Provider-priority truncation drops lowest-priority (last-registered) first
- Phase 56 Plan 04: DynamicZone::assemble_messages() accepts dynamic_budget parameter (adaptive)
- Phase 56 Plan 04: Soft/hard compaction thresholds apply to adaptive dynamic budget, not total
- Phase 56 Plan 05: CLI uses SqliteStorage + StorageAdapter trait for message access (not direct Database query)
- Phase 56 Plan 05: ArchiveConditionalProvider registered last in serve.rs (lowest priority after memory, skills, trust zone)
- Phase 56 Plan 05: Prometheus compaction metrics use facade pattern (describe_histogram!, describe_counter!)
- Phase 56 Plan 05: Separate Database::open for ArchiveConditionalProvider (SqliteStorage doesn't expose connection)
- Phase 56 Plan 06: Entity persistence uses 0.6 confidence (lower than explicit 0.9) matching MemoryExtractor convention
- Phase 56 Plan 06: Entity persistence is best-effort: embedding/save failures logged and skipped, never fatal
- Phase 56 Plan 06: CLI quality scores confirmed working from Plan 05 -- no code changes needed
- [Phase 57]: Config types defined inline in blufio-config/model.rs (following ClassificationConfig pattern), re-exported from blufio-injection
- [Phase 57]: SecurityEvent defined inline in blufio-bus/events.rs (following all event sub-enums), re-exported from blufio-injection
- [Phase 57]: Custom regex patterns assigned default severity 0.3 and InstructionOverride category
- [Phase 57]: Regex uses non-greedy source capture (.+?) to handle colon-containing sources like mcp:server_name
- [Phase 57]: HKDF expand uses hmac::HMAC_SHA256 (owned, not &reference) per ring 0.17 KeyType trait
- [Phase 57]: Hex encoding (64 chars) for HMAC tags over base64, leveraging existing hex crate in workspace
- [Phase 57]: Credential patterns ordered most-specific first (sk-ant-, sk-proj- before sk-) because Rust regex has no lookahead
- [Phase 57]: serde_json moved to runtime dependency (OutputScreener and HitlManager accept &serde_json::Value)
- [Phase 57]: HitlManager.check_tool returns (HitlDecision, Vec<SecurityEvent>) tuple for event-driven architecture
- [Phase 57]: ConfirmationChannel trait uses async-trait following workspace pattern
- [Phase 57]: BoundaryManager per-session (not in pipeline) because HMAC tokens are session-scoped
- [Phase 57]: InjectionPipeline wrapped in Option<Arc<Mutex<>>> for async sharing in SessionActor
- [Phase 57]: MCP classifier shared via Arc<InjectionClassifier> (RegexSet not Clone)
- [Phase 57]: assemble_with_boundaries() created alongside assemble() to avoid breaking API
- [Phase 57]: 0.98 blocking threshold for tool output (higher than 0.95 for user input)
- [Phase 57]: All open-world tool output scanned at session level for defense-in-depth
- [Phase 57]: MCP classifier created as separate instance before MCP init, intentionally separate from pipeline classifier
- [Phase 58]: CronConfig/RetentionConfig use serde(default) on BlufioConfig following existing pattern
- [Phase 58]: CronEvent uses String fields to avoid cross-crate deps (following established bus event pattern)
- [Phase 58]: Soft-delete filtering added to classification queries in addition to CRUD queries
- [Phase 58]: Test DB schemas updated with deleted_at column across 6 files for consistency
- [Phase 58]: CronScheduler dispatches tasks inline (not tokio::spawn) since CronTask is not Clone
- [Phase 58]: croner v3 find_next_occurrence returns Result (not Option), handled with Ok/Err
- [Phase 58]: History module uses String errors (not BlufioError) to avoid cross-crate complexity
- [Phase 58]: Memory cleanup uses soft-delete for consistency with retention model
- [Phase 58]: Retention soft-delete uses format!() for table/days interpolation (safe: internal values only)
- [Phase 58]: CLI cron handler uses sync DB for list/add/remove/generate, async DB for run-now/history
- [Phase 58]: Cron-to-OnCalendar conversion is best-effort with hourly fallback for unsupported specifiers
- [Phase 58]: CronScheduler opens own DB connection (isolation), init failure non-fatal (warn + continue)
- [Phase 59]: Watch parent directory (not file directly) for editor compatibility (atomic save creates temp file)
- [Phase 59]: Non-reloadable fields compared via explicit match arms for compile-time safety
- [Phase 59]: load_config returns Arc<BlufioConfig> snapshot for session isolation (HTRL-05)
- [Phase 59]: Configurable debounce from hot_reload.debounce_ms (default 500ms)
- [Phase 59]: child.wait() with manual stdout/stderr reads instead of wait_with_output() for kill-on-timeout support
- [Phase 59]: HookEvent uses String fields (no cross-crate deps) following established bus event pattern
- [Phase 59]: io-util tokio feature added to blufio-hooks for async stdin/stdout pipe handling
- [Phase 59]: LIFECYCLE_EVENT_MAP resolves TOML names to EventBus type strings at dispatch time (not constructor time)
- [Phase 59]: TLS hot reload stub (rustls only transitively available, full impl deferred to HTRL-02)
- [Phase 59]: Skill watcher is detection+notification layer only; SkillStore update in Plan 04 serve.rs
- [Phase 59]: HookManager::run takes &self, enabling Arc<HookManager> shared between run loop and direct lifecycle calls
- [Phase 59]: Config path for hot reload determined at runtime from XDG hierarchy (local > user > system)
- [Phase 59]: pre_shutdown fires after agent_loop.run() returns, post_shutdown after audit trail cleanup
- [Phase 60]: GdprConfig defined inline in blufio-config/model.rs (following ClassificationConfig pattern), re-exported from blufio-gdpr
- [Phase 60]: BlufioError::Gdpr uses simple String variant (following Config/Vault/Security pattern)
- [Phase 60]: GdprEvent added to BusEvent with SHA-256 hashed user_id (never plaintext in events)
- [Phase 60]: Audit subscriber gets explicit match arms for all 4 GdprEvent variants (not wildcard)
- [Phase 60]: UserSession lightweight struct defined locally in erasure.rs to avoid coupling to blufio-core::types::Session
- [Phase 60]: CSV export uses single-file format with data_type discriminator column for mixed record types
- [Phase 60]: Audit entry count uses LIKE matching on actor/session_id/details_json with pii_marker=0 filter
- [Phase 61]: EmailConfig uses default_email_poll_interval (30s) with serde default helper
- [Phase 61]: New channel config fields placed between matrix and bridge in BlufioConfig
- [Phase 61]: BlueBubblesClient uses query-param auth (?password=) per BlueBubbles API convention
- [Phase 61]: TwilioClient builds form-urlencoded body manually (serde_urlencoded) since workspace reqwest lacks form feature
- [Phase 61]: HMAC-SHA1 with Base64 encoding for Twilio (distinct from WhatsApp HMAC-SHA256 hex)
- [Phase 61]: async-imap switched to runtime-tokio feature (default async-std incompatible with project)
- [Phase 61]: mail-parser DateTime manual ISO 8601 conversion (no built-in method)
- [Phase 61]: IMAP connect-per-cycle pattern for simplicity over persistent connections
- [Phase 61]: axum added as runtime dependency to blufio crate for Router::merge() in webhook composition
- [Phase 62]: opentelemetry_sdk uses underscore crate name (not opentelemetry-sdk)
- [Phase 62]: utoipa non-optional (always compiled) in both blufio and blufio-gateway; OTel deps optional behind otel feature
- [Phase 62]: ObservabilityConfig wraps OpenTelemetryConfig; OpenApiConfig nested inside GatewayConfig
- [Phase 62]: SQLCipher detection uses BLUFIO_DB_KEY env var (existing convention from doctor.rs)
- [Phase 62]: WAL autocheckpoint pragma set via separate open_connection (isolation pattern)
- [Phase 62]: otel.rs always compiled (not cfg-gated module) for dual otel_span! macro variants
- [Phase 62]: TracingState struct with cfg-gated otel_provider field for clean feature-conditional API
- [Phase 62]: ParentBased(TraceIdRatioBased) sampler for proper distributed trace propagation
- [Phase 62]: OTel init/shutdown uses eprintln (subscriber may not be ready/available)
- [Phase 62]: Module named openapi.rs (not openapi_doc.rs) -- no utoipa::openapi namespace conflict at crate level
- [Phase 62]: ModelsListResponse.data uses schema(value_type = Vec<Object>) since ModelInfo is from blufio-core without ToSchema
- [Phase 62]: swagger_ui_enabled added to ServerConfig (config-driven toggle, not just feature gate)
- [Phase 62]: /openapi.json is public (no auth) to support CI tooling, Postman imports, and code generators
- [Phase 62]: Span handles (not .entered()) for async functions; tracing::Instrument for wrapping specific futures
- [Phase 62]: rmcp traceparent injection deferred (rmcp manages own transport); blufio.mcp.call span for Blufio-level correlation
- [Phase 62]: Conditional otel feature propagation via crate?/feature syntax (only if crate enabled)
- [Phase 63]: Benchmarks placed in crates/blufio/benches/ (not workspace root) because root Cargo.toml has no [package] section
- [Phase 63]: CPU-bound hot paths only benchmarked (no LLM/DB I/O) for deterministic reproducible results
- [Phase 63]: PII proptest placed in blufio-security (where pii.rs lives) not blufio-core as plan specified
- [Phase 63]: TwilioClient refactored with base_url field and test constructors for wiremock testability

### Pending Todos

None.

### Blockers/Concerns

- Claude tokenizer accuracy: Xenova/claude-tokenizer is community artifact (~80-95% accuracy for Claude 3+)
- tiktoken-rs binary size: Embeds BPE vocabulary data. Measure impact against <50MB binary constraint
- v1.5 scope is largest milestone yet (93 requirements). Monitor velocity against prior milestones
- Litestream + SQLCipher incompatibility: Must document and provide application-level backup alternative
- Hot reload complexity: Research recommends careful phasing. ArcSwap swap is atomic but downstream propagation is not

## Session Continuity

Last session: 2026-03-13T14:42:20.445Z
Stopped at: Completed 63-04-PLAN.md
Resume file: None
