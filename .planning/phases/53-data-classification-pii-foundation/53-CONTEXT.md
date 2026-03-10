# Phase 53: Data Classification & PII Foundation - Context

**Gathered:** 2026-03-10
**Status:** Ready for planning

<domain>
## Phase Boundary

Every piece of data in Blufio has a classification level (Public/Internal/Confidential/Restricted), and PII is automatically detected and redacted across logs, exports, and context. Foundation for the compliance stack — audit trail, GDPR tooling, and retention policies build on this.

</domain>

<decisions>
## Implementation Decisions

### Classification Storage
- Column per table: `classification TEXT NOT NULL DEFAULT 'internal'` on memories, messages, sessions tables
- DataClassification enum in blufio-core with ascending sensitivity ordering (Public < Internal < Confidential < Restricted) — derive PartialOrd/Ord
- Classifiable trait in blufio-core: simple getter/setter (`fn classification(&self)` + `fn set_classification(&mut self, level)`)
- Default classification for existing and new data: Internal
- Allow reclassification in both directions (with audit trail in Phase 54)
- No propagation across relationships — each entity independently classified
- Classification is queryable and filterable via SQL WHERE
- Batch update API supports filter by session ID, date range, current level, and content pattern
- SQL index on classification column for memories and messages tables
- Serialized as lowercase strings: 'public', 'internal', 'confidential', 'restricted' (matches MemorySource/MemoryStatus pattern)
- Rust-side enum validation only (no SQL CHECK constraint — SQLite ALTER TABLE limitation)
- Runtime data only — memories, messages, sessions, exports. Config values protected by vault separately.
- Both blufio-storage (messages, sessions) and blufio-memory (memories) get the classification column
- Single migration file adds column to all three tables plus creates indexes

### PII Detection
- Extend existing REDACTION_PATTERNS in blufio-security/redact.rs — same pipeline for secrets and PII
- PII detection returns Vec<PiiMatch> with { pii_type: PiiType, span: Range<usize>, matched_value: String }
- PiiType enum is #[non_exhaustive]: Email, Phone, Ssn, CreditCard (allows future additions)
- Type-specific redaction placeholders: [EMAIL], [PHONE], [SSN], [CREDIT_CARD] — existing secrets stay [REDACTED]
- Phone numbers: common US/UK/EU formats with ~3-4 regex patterns
- SSN: US format only (XXX-XX-XXXX with area number validation)
- Credit card: Luhn algorithm validation after regex match to reduce false positives
- Context-aware skipping: pre-strip fenced code blocks, inline code, and URLs before detection (new string allocation with stripped zones)
- RegexSet for fast two-phase detection: check if any pattern matches, then run individual regexes for details
- PII detection at write time (synchronous, before INSERT) — auto-classify as Confidential when PII found
- All text content scanned: user messages, assistant responses, tool arguments, tool results, memory content
- Built-in patterns only (no TOML-configurable custom patterns for now)
- No caching — regex is fast enough (microseconds)
- No content length cutoff — always scan regardless of size
- Log PII detection at info level: "PII detected: {count} match(es) [{types}] — auto-classified as Confidential"
- CLI command: `blufio pii scan <text>` / `--file <path>` / stdin pipe support

### Per-Level Enforcement
- Central ClassificationGuard in blufio-security: stateless with static rules, global singleton (LazyLock)
- Methods: can_export(level), can_include_in_context(level), must_redact_in_logs(level)
- Restricted: silent skip from LLM context (no placeholder, no error), excluded from memory retrieval at SQL level, excluded from exports with warning count, never in any LLM context including tool results
- Confidential: PII redacted within content (not entire field) in logs, SQLCipher encryption satisfies "encrypted at rest" requirement (no additional field-level encryption)
- Internal: audit-logged only (Phase 54), no export/context restrictions
- Public: no restrictions
- Enforcement active immediately (not deferred)
- Filter tool results from WASM skills and MCP tools before LLM sees them
- Warn but allow when non-SQLCipher (plaintext) database stores Confidential data
- Exports exclude Restricted data with warning: "N items excluded due to classification restrictions"
- Prometheus metrics: `blufio_classification_blocked_total{level,action}` for enforcement actions
- Context engine: dynamic zone filtering only (static zone is system prompt, conditional zone already filtered at SQL)
- Model router not affected — classification is data handling, not model selection

### CLI/API Interface
- CLI: `blufio classify set|get|list|bulk` subcommands
- API: PUT/GET /v1/classify/{type}/{id}, POST /v1/classify/bulk endpoints in blufio-gateway classify.rs module
- Confirm downgrades: require --force flag for Confidential→Public etc.
- New 'classify' scope for scoped API keys
- Auto-inference from PII: opt-out (enabled by default, disable via `[classification] auto_classify_pii = false`)
- Bulk operations: --dry-run mode, filter by session_id/date range/current level/content pattern
- Partial success for bulk: return { total, succeeded, failed, errors }
- --json flag on classify list (matches existing CLI output patterns)
- Downgrade rejection includes current and requested levels in error message

### Migration Strategy
- ALTER TABLE + DEFAULT: `ALTER TABLE memories ADD COLUMN classification TEXT NOT NULL DEFAULT 'internal'`
- No backfill PII scan on existing data — operators use `blufio classify bulk` if needed
- Single migration file for all three tables
- No SQL CHECK constraint (SQLite limitation) — Rust enum validation enforces valid values

### EventBus Integration
- New event variants added to existing Event enum in blufio-bus:
  - ClassificationChanged { entity_type, entity_id, old_level, new_level, changed_by }
  - PiiDetected { entity_type, entity_id, pii_types, count }
  - ClassificationEnforced { entity_type, entity_id, level, action_blocked }
  - BulkClassificationChanged { entity_type, count, old_level, new_level, changed_by }
- Events fire only when PII actually found (not on every scan)
- Events carry metadata only (never actual PII values)
- Single bulk event (not per-item) for bulk operations
- Follow existing EventBus timestamp pattern
- Fire-and-forget (non-blocking) emission
- blufio-security gains blufio-bus dependency for event emission

### Crate Organization
- PII detection: new pii.rs module in blufio-security
- ClassificationGuard: in blufio-security
- DataClassification enum + Classifiable trait: in blufio-core
- CLI handlers: in main blufio binary crate (crates/blufio/src/)
- API endpoints: new classify.rs module in blufio-gateway
- Prometheus metrics: blufio-prometheus subscribes to EventBus events

### Config Schema
- New top-level `[classification]` TOML section
- Fields: enabled (bool, default true), auto_classify_pii (bool, default true), default_level (DataClassification, default 'internal'), warn_unencrypted (bool, default true)
- #[serde(deny_unknown_fields)] for strict validation
- Parse-time validation: default_level deserialized as DataClassification enum directly
- Fully optional section — all defaults apply if omitted

### Error Handling
- New BlufioError::Classification(ClassificationError) variant with sub-variants: InvalidLevel, DowngradeRejected, EntityNotFound, BulkOperationFailed
- Full error classification traits: is_retryable(), severity(), category() (Security)
- PII detection failures: log and continue, never block agent loop
- HTTP status codes: 400 invalid level, 403 insufficient scope, 404 entity not found, 409 downgrade without force
- Consistent CLI error formatting (colored output matching existing commands)

### Testing
- Comprehensive PII test vectors: 10+ per type, boundary cases, false positives, international formats (~50+ tests)
- Full pipeline integration tests: store → classify → retrieve → verify exclusion
- Property-based tests (proptest) for Luhn validation
- Real-world edge cases for context-aware skipping (GitHub URLs, stack traces, Docker hashes, UUIDs)
- Snapshot/golden file test for PII detection output stability
- Criterion benchmark for PII detection throughput (100 chars, 1KB, 10KB)
- CLI integration tests for `blufio classify` and `blufio pii scan`

### Performance
- Target: <1ms per message for PII detection
- No content length cutoff
- SQL-level filtering (WHERE classification != 'restricted') — zero Rust-side overhead
- Negligible storage overhead with indexed column (~20 bytes per row)
- New string allocation for pre-strip (code block/URL removal) — acceptable
- RegexSet two-phase detection for fast common case (no PII found)

### Privacy Report Integration
- DataType and DataClassification are separate orthogonal concepts — both stay
- New PrivacyReport field: `classification_distribution: Option<ClassificationDistribution>` with counts per level per entity type
- Optional DB section: skip classification distribution if database unavailable
- JSON output includes classification_distribution data
- New PII Detection Status section: auto-classification enabled/disabled, active patterns, context-aware exclusion types
- SkillPermissionInfo annotated with PII exposure risk for skills with message access
- Enforcement stats deferred to Prometheus metrics (not in report)
- No manual/auto split in distribution counts

### Documentation
- Standard module + public API rustdoc (match existing blufio-security style)
- Regex patterns documented in rustdoc for transparency
- Commented [classification] section in blufio.example.toml
- clap after_help examples on `blufio classify` and `blufio pii`
- Commit convention only (no manual CHANGELOG)

### Claude's Discretion
- Exact regex patterns for email, phone, SSN, credit card
- Internal module structure within pii.rs (sub-modules vs flat)
- Exact Prometheus metric names and label values
- Test fixture organization
- Migration version numbering

</decisions>

<specifics>
## Specific Ideas

- PII detection extends the existing REDACTION_PATTERNS LazyLock in redact.rs — same pipeline, same RedactingWriter
- ClassificationGuard is a static singleton with pure functions — no config, no state, deterministic and auditable
- Privacy report gains a "Data Classification Distribution" table and "PII Detection Status" section
- Bulk classification supports all four filter criteria: session_id, date range, current level, content pattern

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `blufio-security::redact::RedactingWriter` and `REDACTION_PATTERNS`: PII patterns extend this directly
- `blufio-security::redact::redact()` function: PII redaction uses same mechanism
- `privacy.rs::DataType` enum: separate concept from DataClassification, both coexist
- `privacy.rs::PrivacyReport` struct: gains new classification_distribution field
- `blufio-core::types::Memory/Message/Session` structs: gain classification field
- `blufio-memory::types::MemorySource/MemoryStatus` enums: pattern for as_str()/from_str_value() serialization
- `blufio-bus` EventBus: existing event emission pattern for classification/PII events
- `blufio-core::error::BlufioError` hierarchy: pattern for new Classification variant with severity/category

### Established Patterns
- LazyLock<Vec<Regex>> for compiled regex patterns (redact.rs)
- as_str()/from_str_value() for SQLite text column serialization (MemorySource, MemoryStatus)
- #[serde(deny_unknown_fields)] on config structs
- #[non_exhaustive] on classification enums (ErrorCategory, Severity)
- EventBus fire-and-forget for async event emission
- CLI subcommands in main binary crate, library logic in crate libraries
- Prometheus metrics via EventBus subscription in blufio-prometheus

### Integration Points
- blufio-storage: ALTER TABLE migrations for messages, sessions
- blufio-memory: ALTER TABLE migration for memories table, SQL WHERE filter in retrieval queries
- blufio-context: dynamic zone assembly filters Restricted content
- blufio-agent: PII detection before message storage in SessionActor
- blufio-gateway: new classify.rs module for REST API endpoints
- blufio-config: new ClassificationConfig struct in model.rs
- blufio-prometheus: new EventBus subscriber for classification metrics
- blufio (binary): new CLI subcommands for classify and pii

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 53-data-classification-pii-foundation*
*Context gathered: 2026-03-10*
