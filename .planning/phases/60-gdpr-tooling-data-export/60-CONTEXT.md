# Phase 60: GDPR Tooling & Data Export - Context

**Gathered:** 2026-03-12
**Status:** Ready for planning

<domain>
## Phase Boundary

Operators can fulfill GDPR data subject requests (erasure, portability, transparency) through CLI commands. `blufio gdpr erase` deletes all user data, `blufio gdpr report` generates transparency reports, `blufio gdpr export` produces filtered data exports in JSON/CSV, and `blufio gdpr list-users` shows identifiable users in the system. Builds on Phase 53 (PII/classification), Phase 54 (audit trail erasure), and Phase 58 (retention/soft-delete infrastructure).

</domain>

<decisions>
## Implementation Decisions

### Erasure Strategy
- Hard delete everything: messages, sessions, memories, compaction archives permanently removed (not soft-deleted)
- Cost records anonymized: set session_id to NULL, preserve token/cost aggregates for billing accuracy (GDPR-02)
- Audit entries redacted via existing `erase_audit_entries()` from Phase 54 (actor/session_id/details_json set to '[ERASED]', pii_marker=1, hash chain preserved)
- User identification: exact user_id match on sessions table — no fuzzy matching, no cross-field scanning
- Memory re-index triggered synchronously after erasure — blocks until embedding vectors cleaned up
- Erasure manifest (counts + IDs, no content) always written to {data_dir}/exports/ even with --skip-export
- Compaction archives: hard delete entirely (not anonymize) — `delete_archives_by_session_ids()` already exists

### Erasure Scope & Cascade
- Find sessions WHERE user_id = target
- Delete ALL messages in user's sessions (all roles: user, assistant, system, tool)
- For shared/bridged sessions: delete only the target user's messages, preserve session and other users' messages
- Delete memories traced via session_id (memories linked to user's sessions)
- Delete compaction archives for user
- Anonymize cost records (null out session_id)
- Redact audit entries via erase_audit_entries()
- Cross-channel identity: out of scope — operator runs erase per user_id if same person has multiple IDs
- References to user in other sessions: out of scope — operator's responsibility

### Export-Before-Erasure Safety Net
- Auto-export by default: export user data before erasing unless --skip-export passed
- Export saved to {data_dir}/exports/gdpr-export-{user_id}-{timestamp}.json
- If export fails (disk full, permissions), abort erasure entirely — data safety takes priority
- Export path not configurable via CLI (uses data_dir); configurable via TOML [gdpr] section

### Transparency Report
- `blufio gdpr report --user <id>` shows summary counts only: N messages, N sessions, N memories, N archives, N cost records
- Includes audit trail info: "N audit entries referencing this user (not deletable per retention policy)"
- Supports both table output (default) and --json for machine consumption
- No data previews, no entity listings — just counts per data type

### Data Export
- Standalone `blufio gdpr export --user <id>` command — also used internally by auto-export-before-erasure
- Single combined file output (not per-type files)
- JSON envelope with metadata: export_metadata (timestamp, user_id, blufio_version, filter_criteria) + data sections (messages, sessions, memories, cost_records)
- Pretty-printed JSON by default (indented, human-readable)
- CSV format: flatten nested data to columns (metadata JSON expanded to individual columns where possible)
- Raw data by default — --redact opt-in flag applies PII redaction via ClassificationGuard
- Restricted data excluded from exports (follows existing ClassificationGuard rules) with warning count
- Filtering: --session <id>, --since/--until (ISO timestamps), --type messages|memories|sessions|cost_records
- File output by default to {data_dir}/exports/gdpr-export-{user_id}-{timestamp}.{json|csv}
- --output flag for custom path
- Reasonable data volumes assumed (load into memory, no streaming pagination)

### CLI Safety & Confirmation
- Both modes: interactive confirmation (default) + --yes for non-interactive + --dry-run for preview-only
- Interactive: show preview of counts, then prompt "Type YES to confirm erasure for user <id>:"
- Dry-run: counts per type only ("Would delete: 15 messages, 3 sessions, 7 memories...")
- One user per invocation — no multi-user erasure
- Refuse if user has active (open) sessions — "User has N active sessions. Close them first or pass --force."
- Non-existent user_id: exit with info message and code 0 ("No data found for user <id>")
- Atomic transaction for main data erasure (all-or-nothing within main DB)
- Audit erasure is best-effort (separate DB) — if fails, log warning but report main erasure as successful
- Timeout: --timeout flag with generous default (5 minutes). Abort if exceeded.
- Encryption: fail early if DB encrypted and BLUFIO_DB_KEY not set
- SQLite single-writer serialization sufficient for concurrent operations — no additional locking
- Works while server is running (WAL mode handles concurrent access)

### CLI Structure
- Top-level `blufio gdpr` subcommand in Commands enum
- Subcommands: erase, report, export, list-users
- `blufio gdpr list-users` — shows distinct user_ids with record counts per type
- GDPR context in help text: "GDPR data subject rights tooling. Supports right to erasure (Art. 17), data portability (Art. 20), and transparency (Art. 15)."
- Workflow example in after_help: list-users → report → export → erase
- Colored output matching existing CLI style (green success, red errors, yellow warnings, cyan info)

### Config Schema
- New [gdpr] section in TOML config
- Fields: export_dir (Option<String>, default {data_dir}/exports/), export_before_erasure (bool, default true), default_format (String, default "json")
- #[serde(deny_unknown_fields)] — consistent with other config sections
- Fully optional — all defaults apply if omitted
- Validated on first use (when running GDPR commands), not at serve startup
- Commented [gdpr] section in blufio.example.toml with operator-facing GDPR context comments

### EventBus Integration
- New GdprEvent variant on BusEvent enum: ErasureStarted, ErasureCompleted, ExportCompleted, ReportGenerated
- All use String fields (no cross-crate deps) following established bus event pattern
- User ID hashed (SHA-256) in event payloads — events observable without leaking PII
- Optional<Arc<EventBus>> pattern — events emitted only when EventBus available (None in standalone CLI)

### Error Handling
- New BlufioError::Gdpr(GdprError) variant with sub-variants: ErasureFailed, ExportFailed, ReportFailed, UserNotFound, ActiveSessionsExist, ExportDirNotWritable
- Error classification: is_retryable=true for DB/IO errors, false for UserNotFound/ActiveSessions. severity=Error. category=Security
- All error messages include actionable next steps

### Prometheus Metrics
- Counters: blufio_gdpr_erasures_total{status}, blufio_gdpr_exports_total{status}, blufio_gdpr_reports_total{status}
- Histograms: blufio_gdpr_erasure_duration_seconds, blufio_gdpr_export_size_bytes
- Per-type: blufio_gdpr_records_erased_total{type=messages|sessions|memories|archives|cost_records}
- Emitted via EventBus (Prometheus subscriber pattern) — Optional when EventBus unavailable
- Recommended alerting rules documented in rustdoc

### Crate Organization
- New blufio-gdpr crate: erasure logic, export logic, report logic, models, config types
- Dependencies: blufio-storage, blufio-memory, blufio-audit, blufio-cost, blufio-security, blufio-bus, blufio-config, blufio-core
- CLI handlers in main binary crate (crates/blufio/src/)
- Module-level rustdoc with text-based data flow diagram

### Testing
- Comprehensive test coverage:
  - GDPR completeness integration test: create data across ALL tables → erase → scan every table for zero remaining references
  - Property-based test (proptest): generate random user data → erase → verify completeness
  - E2E export-then-erase flow: create → export → verify file → erase → verify data gone → verify export file persists
  - Audit redaction verification: after erasure, verify entries exist with '[ERASED]' fields, pii_marker=1, chain intact
  - PII redaction per type: test --redact flag verifies all 4 PII types ([EMAIL], [PHONE], [SSN], [CREDIT_CARD])
  - Golden file snapshots for JSON and CSV export format
  - Dry-run test: verify preview counts match actual data without deletion
  - Active session refusal test
  - Timeout test
  - CSV escaping test (commas, newlines, quotes in content)

### Documentation
- Workflow example in `blufio gdpr --help` after_help
- Commented [gdpr] section in blufio.example.toml with GDPR Article references
- Module-level rustdoc with data flow diagram: find sessions → collect data → export (optional) → atomic delete → audit erase → re-index → emit events
- Operator-facing TOML comments explaining GDPR context

### Claude's Discretion
- Exact CSV column layout and flattening rules
- Internal module structure within blufio-gdpr
- Exact Prometheus metric label values
- Test fixture data and organization
- Migration version numbering (if any schema changes needed)
- Batch DELETE SQL optimization
- Exact interactive confirmation prompt text

</decisions>

<specifics>
## Specific Ideas

- Erasure is atomic within main DB (single transaction), best-effort for audit DB (separate SQLCipher database)
- Memory re-index is synchronous — erasure blocks until vectors cleaned up, ensuring complete removal
- Manifest file (counts + IDs) always written, even with --skip-export — provides operator audit record
- Export JSON uses envelope pattern with metadata for self-documentation
- list-users command helps operators identify exact user_id before running destructive operations

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `blufio-audit::chain::erase_audit_entries()`: Ready-to-use audit erasure with hash chain preservation
- `blufio-security::classification_guard::redact_for_export()` + `filter_for_export()`: PII redaction for exports
- `blufio-storage::queries::archives::delete_archives_by_session_ids()`: Archive cleanup by session IDs
- `blufio-storage::queries::messages::delete_messages_by_ids()`: Message deletion
- `blufio-storage::queries::sessions`: Session queries with user_id field
- `blufio-cost::ledger::CostRecord`: Cost record with session_id field for anonymization
- `blufio-memory::store::MemoryStore::soft_delete()`: Memory deletion (will use hard delete for GDPR)
- `blufio-bus::events::BusEvent`: EventBus with existing event patterns for new GdprEvent variant

### Established Patterns
- clap derive with Subcommand enum in main.rs (20+ existing subcommands)
- #[serde(deny_unknown_fields)] on config structs
- Optional<Arc<EventBus>> for components that may or may not have EventBus (tests/CLI)
- String fields in BusEvent sub-enums (no cross-crate deps)
- tokio-rusqlite single-writer thread for all SQLite writes
- CLI colored output (green/red/yellow/cyan) matching classify, audit, pii commands
- after_help with workflow examples on CLI subcommands
- BlufioError typed hierarchy with is_retryable/severity/category classification

### Integration Points
- main.rs: new Gdpr { action: GdprCommands } in Commands enum
- blufio-config/model.rs: new GdprConfig struct with #[serde(default)]
- blufio-bus/events.rs: new Gdpr(GdprEvent) variant in BusEvent
- blufio-core/error.rs: new Gdpr(GdprError) variant in BlufioError
- blufio-prometheus: EventBus subscriber for GDPR metrics
- blufio.example.toml: commented [gdpr] section
- doctor.rs: GDPR readiness health check (export dir writable, audit enabled, PII detection active)
- blufio-storage: queries for user data across tables
- blufio-memory: hard delete + re-index for user memories
- blufio-cost: anonymization query (SET session_id = NULL)

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 60-gdpr-tooling-data-export*
*Context gathered: 2026-03-12*
