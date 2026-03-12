# Phase 60: GDPR Tooling & Data Export - Research

**Researched:** 2026-03-12
**Domain:** GDPR data subject rights tooling (erasure, portability, transparency) via CLI
**Confidence:** HIGH

## Summary

Phase 60 implements GDPR data subject rights tooling for Blufio operators: the right to erasure (Art. 17), data portability (Art. 20), and transparency (Art. 15) through a `blufio gdpr` CLI subcommand with four operations: `erase`, `report`, `export`, and `list-users`. The implementation is built on extensive existing infrastructure from Phases 53-58 (PII detection, audit trail with GDPR erasure support, data classification, retention/soft-delete, cost ledger).

The primary technical challenge is orchestrating a multi-table cascading erasure across separate database systems (main SQLite + audit SQLCipher) while maintaining atomicity within the main database and providing a safety-net export-before-erasure workflow. The existing codebase provides all necessary building blocks: `erase_audit_entries()` for audit erasure with hash chain preservation, `ClassificationGuard::redact_for_export()` for PII redaction, `delete_archives_by_session_ids()` for archive cleanup, and `delete_messages_by_ids()` for message deletion. The new `blufio-gdpr` crate primarily orchestrates these existing capabilities into a cohesive GDPR workflow.

**Primary recommendation:** Create a `blufio-gdpr` crate that depends on existing storage/audit/memory/cost/security crates, implement the four CLI subcommands in the main binary crate, and add a new `GdprConfig` section, `GdprEvent` bus variant, `GdprError` variant, and Prometheus metrics following the exact patterns established in Phases 53-59.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Hard delete everything: messages, sessions, memories, compaction archives permanently removed (not soft-deleted)
- Cost records anonymized: set session_id to NULL, preserve token/cost aggregates for billing accuracy (GDPR-02)
- Audit entries redacted via existing `erase_audit_entries()` from Phase 54 (actor/session_id/details_json set to '[ERASED]', pii_marker=1, hash chain preserved)
- User identification: exact user_id match on sessions table -- no fuzzy matching, no cross-field scanning
- Memory re-index triggered synchronously after erasure -- blocks until embedding vectors cleaned up
- Erasure manifest (counts + IDs, no content) always written to {data_dir}/exports/ even with --skip-export
- Compaction archives: hard delete entirely (not anonymize) -- `delete_archives_by_session_ids()` already exists
- Erasure cascade: sessions -> messages -> memories -> archives -> cost anonymize -> audit redact
- Shared/bridged sessions: delete only target user's messages, preserve session and other users' messages
- Cross-channel identity: out of scope -- operator runs erase per user_id if same person has multiple IDs
- Auto-export by default before erasure unless --skip-export passed
- Export saved to {data_dir}/exports/gdpr-export-{user_id}-{timestamp}.json
- If export fails (disk full, permissions), abort erasure entirely -- data safety takes priority
- Transparency report shows summary counts only (no data previews)
- Single combined file output for exports (not per-type files)
- JSON envelope with metadata for exports
- Pretty-printed JSON by default
- CSV format: flatten nested data to columns
- Raw data by default -- --redact opt-in flag applies PII redaction via ClassificationGuard
- Restricted data excluded from exports with warning count
- Reasonable data volumes assumed (load into memory, no streaming pagination)
- Interactive confirmation (default) + --yes for non-interactive + --dry-run for preview-only
- Refuse if user has active (open) sessions unless --force passed
- Non-existent user_id: exit with info message and code 0
- Atomic transaction for main data erasure (all-or-nothing within main DB)
- Audit erasure is best-effort (separate DB) -- if fails, log warning but report main erasure as successful
- --timeout flag with generous default (5 minutes)
- Encryption: fail early if DB encrypted and BLUFIO_DB_KEY not set
- New blufio-gdpr crate with erasure/export/report/config logic
- CLI handlers in main binary crate (crates/blufio/src/)
- New [gdpr] section in TOML config with export_dir, export_before_erasure, default_format
- New GdprEvent variant on BusEvent enum: ErasureStarted, ErasureCompleted, ExportCompleted, ReportGenerated
- User ID hashed (SHA-256) in event payloads
- New BlufioError::Gdpr(GdprError) variant with sub-variants
- Prometheus metrics: counters, histograms, per-type counters

### Claude's Discretion
- Exact CSV column layout and flattening rules
- Internal module structure within blufio-gdpr
- Exact Prometheus metric label values
- Test fixture data and organization
- Migration version numbering (if any schema changes needed)
- Batch DELETE SQL optimization
- Exact interactive confirmation prompt text

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| GDPR-01 | CLI `blufio gdpr erase --user <id>` deletes all user data (messages, memories, session metadata, cost records) | Existing storage queries (delete_messages_by_ids, delete_archives_by_session_ids), memory store conn() for hard delete, cost ledger anonymization via SQL UPDATE. New blufio-gdpr crate orchestrates. |
| GDPR-02 | Cost record anonymization preserves aggregates but removes user association on erasure | CostLedger schema has session_id field. SQL `UPDATE cost_ledger SET session_id = NULL WHERE session_id IN (...)` preserves all token/cost aggregates. |
| GDPR-03 | Erasure logged as audit trail entry (audit entries themselves not deleted) | `erase_audit_entries()` in blufio-audit::chain already implemented. Redacts PII fields, sets pii_marker=1, preserves hash chain. New audit entry for the erasure event itself via AuditWriter. |
| GDPR-04 | `blufio gdpr report --user <id>` generates transparency report of held data | COUNT queries across sessions, messages, memories, archives, cost_ledger, audit_entries tables. Table + JSON output. |
| GDPR-05 | Export before erasure as configurable safety net | GdprConfig.export_before_erasure (bool, default true). Export logic shared between standalone `export` and pre-erasure safety. Abort erasure on export failure. |
| GDPR-06 | Data export supports JSON and CSV formats with filtering by session, date range, and data type | JSON envelope with metadata. CSV flattening. ClassificationGuard for --redact. Filters: --session, --since/--until, --type. |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tokio-rusqlite | (workspace) | Async SQLite access for main DB operations | Already used by blufio-storage, blufio-cost, blufio-memory, blufio-audit |
| rusqlite | (workspace) | Sync SQLite for CLI-context queries and transactions | Used in cron_cmd.rs, audit CLI, classify CLI for direct queries |
| clap | (workspace) | CLI argument parsing with derive | All 20+ existing subcommands use clap derive |
| serde/serde_json | (workspace) | JSON serialization for export envelope and --json output | Universal across all crates |
| chrono | (workspace) | Timestamp parsing/formatting for --since/--until filters | Already used in events.rs, cost ledger, audit |
| sha2 | (workspace) | SHA-256 hashing for user_id in event payloads | Already used in blufio-audit::chain |
| hex | (workspace) | Hex encoding for SHA-256 hashes | Already used in audit chain, injection defense |
| csv | (workspace or add) | CSV export format | Lightweight, standard Rust CSV crate |
| uuid | (workspace) | UUID generation for manifest IDs | Already used throughout (event IDs, cost record IDs) |
| metrics | (workspace) | Prometheus metric facade | Used by blufio-prometheus recording module |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| blufio-storage | (workspace) | Database access, session/message/archive queries | All data retrieval and deletion |
| blufio-memory | (workspace) | Memory store hard delete + re-index | Erasure of user memories |
| blufio-audit | (workspace) | erase_audit_entries() + AuditWriter | Audit trail GDPR erasure + logging erasure events |
| blufio-cost | (workspace) | CostLedger + CostRecord | Cost anonymization + export |
| blufio-security | (workspace) | ClassificationGuard for export redaction | --redact flag on export |
| blufio-bus | (workspace) | EventBus + GdprEvent | Event emission for metrics/hooks |
| blufio-config | (workspace) | GdprConfig struct | [gdpr] TOML section |
| blufio-core | (workspace) | BlufioError, types (Message, Session) | Error handling, data models |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| In-memory collect then write | Streaming export | CONTEXT decision: reasonable data volumes assumed, no streaming needed |
| Soft-delete cascade | Hard delete | CONTEXT decision: hard delete for GDPR compliance completeness |
| Per-type export files | Single combined file | CONTEXT decision: single combined file with JSON envelope |

**Installation:**
No new external dependencies needed beyond what's in the workspace. The `csv` crate may need to be added to `Cargo.toml` if not already present.

```bash
# New crate creation
cargo new crates/blufio-gdpr --lib
```

## Architecture Patterns

### Recommended Project Structure
```
crates/blufio-gdpr/
  src/
    lib.rs           # Module-level rustdoc with data flow diagram, re-exports
    config.rs        # GdprConfig struct + defaults
    erasure.rs       # Erasure orchestrator (cascade delete logic)
    export.rs        # Export logic (JSON envelope, CSV, filtering)
    report.rs        # Transparency report (count queries)
    manifest.rs      # Erasure manifest generation
    models.rs        # GdprError, ErasureManifest, ExportMetadata, ReportData
    events.rs        # Helper for constructing GdprEvent instances

crates/blufio/src/
    gdpr_cmd.rs      # CLI handler (matches cron_cmd.rs pattern)
    main.rs          # New Gdpr { action: GdprCommands } variant in Commands enum
```

### Pattern 1: Erasure Orchestration (Single Transaction)
**What:** All main DB deletions in a single SQLite transaction via `conn.call()`. Audit erasure separate (best-effort).
**When to use:** The `blufio gdpr erase` command.
**Example:**
```rust
// Source: established tokio-rusqlite pattern from storage queries
pub async fn execute_erasure(
    db: &Database,
    session_ids: &[String],
    user_id: &str,
) -> Result<ErasureResult, GdprError> {
    let session_ids = session_ids.to_vec();
    let user_id = user_id.to_string();
    db.connection()
        .call(move |conn| {
            let tx = conn.transaction()?;
            // 1. Delete messages for all user sessions (all roles)
            // 2. Delete memories linked to user sessions
            // 3. Delete compaction archives referencing user sessions
            // 4. Anonymize cost records (SET session_id = NULL)
            // 5. Delete session records (or preserve shared sessions)
            tx.commit()?;
            Ok(result)
        })
        .await
        .map_err(|e| GdprError::ErasureFailed(e.to_string()))
}
```

### Pattern 2: CLI Handler Pattern (Matching cron_cmd.rs)
**What:** Top-level handler function that matches on subcommand enum, delegates to individual command functions.
**When to use:** `gdpr_cmd.rs` in the main binary crate.
**Example:**
```rust
// Source: crates/blufio/src/cron_cmd.rs pattern
pub async fn handle_gdpr_command(
    action: GdprCommands,
    config: &BlufioConfig,
) -> Result<(), BlufioError> {
    match action {
        GdprCommands::Erase { user, yes, dry_run, skip_export, force, timeout } => {
            cmd_erase(config, &user, yes, dry_run, skip_export, force, timeout).await
        }
        GdprCommands::Report { user, json } => cmd_report(config, &user, json).await,
        GdprCommands::Export { user, format, session, since, until, data_type, redact, output } => {
            cmd_export(config, &user, format, session, since, until, data_type, redact, output).await
        }
        GdprCommands::ListUsers { json } => cmd_list_users(config, json).await,
    }
}
```

### Pattern 3: EventBus String-Only Fields
**What:** All event sub-enum fields are String/primitive types to avoid cross-crate dependencies.
**When to use:** GdprEvent definition in blufio-bus/events.rs.
**Example:**
```rust
// Source: blufio-bus/events.rs established pattern (17 existing event sub-enums)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GdprEvent {
    ErasureStarted {
        event_id: String,
        timestamp: String,
        user_id_hash: String,  // SHA-256 hash, never plaintext
    },
    ErasureCompleted {
        event_id: String,
        timestamp: String,
        user_id_hash: String,
        messages_deleted: u64,
        sessions_deleted: u64,
        memories_deleted: u64,
        archives_deleted: u64,
        cost_records_anonymized: u64,
        duration_ms: u64,
    },
    ExportCompleted {
        event_id: String,
        timestamp: String,
        user_id_hash: String,
        format: String,
        file_path: String,
        size_bytes: u64,
    },
    ReportGenerated {
        event_id: String,
        timestamp: String,
        user_id_hash: String,
    },
}
```

### Pattern 4: Error Hierarchy (Matching BlufioError)
**What:** New BlufioError::Gdpr variant wrapping a GdprError sub-enum with is_retryable/severity/category classification.
**When to use:** All GDPR operations.
**Example:**
```rust
// Source: blufio-core/error.rs pattern (Provider, Channel, Storage, Audit variants)
// GdprError in blufio-gdpr/src/models.rs
#[derive(Debug, thiserror::Error)]
pub enum GdprError {
    #[error("erasure failed: {0}")]
    ErasureFailed(String),
    #[error("export failed: {0}")]
    ExportFailed(String),
    #[error("report failed: {0}")]
    ReportFailed(String),
    #[error("no data found for user: {0}")]
    UserNotFound(String),
    #[error("user has {0} active sessions -- close them first or pass --force")]
    ActiveSessionsExist(usize),
    #[error("export directory not writable: {0}")]
    ExportDirNotWritable(String),
}

// In blufio-core/error.rs, add variant:
// Gdpr(GdprError)  -- but use string-based to avoid cross-crate dep
// Actually: new variant #[error("gdpr: {0}")] Gdpr(String)
// Following the simpler pattern of Config(String), Vault(String), Security(String)
```

### Pattern 5: Config with serde(default) and deny_unknown_fields
**What:** New GdprConfig struct with optional defaults, validated on first use.
**When to use:** [gdpr] TOML section.
**Example:**
```rust
// Source: blufio-config/model.rs pattern (RetentionConfig, HookConfig, CronConfig)
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GdprConfig {
    /// Custom export directory. Default: {data_dir}/exports/
    #[serde(default)]
    pub export_dir: Option<String>,

    /// Whether to auto-export before erasure. Default: true
    #[serde(default = "default_export_before_erasure")]
    pub export_before_erasure: bool,

    /// Default export format. Default: "json"
    #[serde(default = "default_gdpr_format")]
    pub default_format: String,
}

fn default_export_before_erasure() -> bool { true }
fn default_gdpr_format() -> String { "json".to_string() }
```

### Anti-Patterns to Avoid
- **Streaming/pagination for reasonable data volumes:** CONTEXT explicitly assumes in-memory load. Do not add streaming complexity.
- **Soft-delete for GDPR erasure:** CONTEXT explicitly requires hard delete. Soft-delete is for retention policies, not GDPR.
- **Cross-crate type dependencies in events:** All event fields must be String/primitive. Do not import types from blufio-gdpr into blufio-bus.
- **Blocking on audit failure:** Audit erasure is best-effort (separate DB). Main erasure succeeds even if audit redaction fails.
- **Fuzzy user matching:** CONTEXT explicitly requires exact user_id match on sessions table only.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Audit erasure | Custom audit update queries | `erase_audit_entries()` from blufio-audit::chain | Already handles hash chain preservation, PII field replacement, pii_marker flag |
| PII redaction for exports | Custom regex redaction | `ClassificationGuard::redact_for_export()` from blufio-security | Handles all 4 PII types, respects classification levels, already tested |
| Archive deletion by session | Custom archive queries | `delete_archives_by_session_ids()` from blufio-storage::queries::archives | Already implements LIKE-based JSON session_ids matching |
| Message deletion | Custom message queries | `delete_messages_by_ids()` from blufio-storage::queries::messages | Already handles parameterized IN clause |
| Export data filtering by classification | Manual classification checking | `ClassificationGuard::can_export()` + `filter_for_export()` | Enforces Restricted exclusion, handles warning counts |
| Event emission | Direct bus.publish() calls | Shared helper in blufio-gdpr/events.rs using `new_event_id()` + `now_timestamp()` | Consistent event construction, SHA-256 hashing of user_id |
| CSV generation | Manual string building | `csv` crate Writer | Handles escaping of commas, newlines, quotes in content |
| Confirmation prompts | Raw stdin/stdout | `std::io::stdin().read_line()` with colored prompt | Simple enough; no library needed |

**Key insight:** This phase is primarily an orchestration layer. Over 80% of the underlying functionality already exists in the dependency crates (Phases 53-58). The new blufio-gdpr crate sequences these existing capabilities into GDPR-compliant workflows.

## Common Pitfalls

### Pitfall 1: Incomplete Erasure Coverage
**What goes wrong:** Missing a table during erasure cascade, leaving user data fragments.
**Why it happens:** Data is spread across 6 tables (sessions, messages, memories, compaction_archives, cost_ledger, audit_entries) plus FTS5 virtual tables.
**How to avoid:** The GDPR completeness integration test must create data in ALL tables, erase, then scan every table for zero remaining references. Use a checklist-style approach in the erasure orchestrator.
**Warning signs:** Test coverage gaps, missing table in the DELETE cascade.

### Pitfall 2: FTS5 Desync After Memory Hard Delete
**What goes wrong:** FTS5 virtual table retains stale entries after hard-deleting memory rows.
**Why it happens:** FTS5 content-sync triggers fire on DELETE from the main table but may not fire if using raw SQL without going through the trigger path.
**How to avoid:** Use `DELETE FROM memories WHERE ...` (which fires FTS5 triggers automatically). The existing eviction code in store.rs uses this pattern successfully. After erasure, call `rebuild` on the FTS5 index or verify counts match.
**Warning signs:** Stale search results, FTS5 count != memories table count.

### Pitfall 3: Shared Session Partial Erasure
**What goes wrong:** Deleting all messages in a shared/bridged session when only one user's messages should be removed.
**Why it happens:** Naive `DELETE FROM messages WHERE session_id IN (...)` deletes all messages regardless of sender.
**How to avoid:** For shared sessions (sessions with messages from multiple user_ids), delete only messages where the originating user matches the target. Identify shared sessions by checking if other users also have messages in that session.
**Warning signs:** Other users' messages disappearing after erasure of a different user.

### Pitfall 4: Export Failure After Partial Erasure
**What goes wrong:** Export fails mid-way (disk full), but some data has already been deleted.
**Why it happens:** Export and erasure run as separate steps, not atomically.
**How to avoid:** CONTEXT mandates: export BEFORE erasure. If export fails, abort entirely. The export step must complete successfully (file written, flushed, synced) before erasure begins.
**Warning signs:** Empty or truncated export files alongside erased data.

### Pitfall 5: cost_ledger Anonymization Breaking Aggregates
**What goes wrong:** Using DELETE instead of UPDATE on cost records removes cost data needed for billing.
**Why it happens:** Confusing GDPR-erasure with data deletion.
**How to avoid:** CONTEXT explicitly requires `UPDATE cost_ledger SET session_id = NULL WHERE session_id IN (...)`. This preserves all aggregate data (token counts, costs, timestamps) while removing the user association.
**Warning signs:** Monthly/daily cost totals changing after erasure.

### Pitfall 6: tokio-rusqlite Transaction Scope
**What goes wrong:** Transaction doesn't encompass all operations, leading to partial erasure on error.
**Why it happens:** tokio-rusqlite's `conn.call()` runs in a background thread; transaction scope must be within a single `call()` closure.
**How to avoid:** All main DB erasure operations (messages, memories, archives, cost anonymize, sessions) must happen within a single `conn.call(move |conn| { let tx = conn.transaction()?; ... tx.commit()?; })`.
**Warning signs:** Partial data remaining after a failed erasure attempt.

### Pitfall 7: Audit DB Connection in CLI Context
**What goes wrong:** Audit erasure fails because the audit DB connection requires different setup than main DB.
**Why it happens:** Audit uses a separate SQLCipher database (audit.db) with its own connection.
**How to avoid:** Use blufio-audit's existing `erase_audit_entries()` which takes a `&tokio_rusqlite::Connection`. Open the audit DB separately following the backup.rs pattern (stem.audit.db alongside main).
**Warning signs:** "audit DB not found" errors during GDPR erasure.

## Code Examples

Verified patterns from the existing codebase:

### Erasure Cascade SQL (within single transaction)
```rust
// Source: established blufio-storage transaction patterns
let tx = conn.transaction()?;

// 1. Delete all messages in user's sessions
let msg_count = tx.execute(
    &format!("DELETE FROM messages WHERE session_id IN ({})", placeholders),
    params_slice,
)?;

// 2. Hard delete memories linked to user sessions
let mem_count = tx.execute(
    &format!("DELETE FROM memories WHERE session_id IN ({})", placeholders),
    params_slice,
)?;

// 3. Delete compaction archives referencing user sessions
// Use LIKE for each session_id in JSON array
for sid in &session_ids {
    let pattern = format!("%{}%", sid);
    tx.execute(
        "DELETE FROM compaction_archives WHERE session_ids LIKE ?1",
        rusqlite::params![pattern],
    )?;
}

// 4. Anonymize cost records
let cost_count = tx.execute(
    &format!("UPDATE cost_ledger SET session_id = NULL WHERE session_id IN ({})", placeholders),
    params_slice,
)?;

// 5. Delete sessions (only fully-owned, not shared)
let sess_count = tx.execute(
    &format!("DELETE FROM sessions WHERE id IN ({})", owned_placeholders),
    owned_params,
)?;

tx.commit()?;
```

### Export JSON Envelope
```rust
// Source: CONTEXT decision on export format
#[derive(Serialize)]
struct ExportEnvelope {
    export_metadata: ExportMetadata,
    data: ExportData,
}

#[derive(Serialize)]
struct ExportMetadata {
    timestamp: String,
    user_id: String,
    blufio_version: String,
    filter_criteria: FilterCriteria,
}

#[derive(Serialize)]
struct ExportData {
    messages: Vec<ExportMessage>,
    sessions: Vec<ExportSession>,
    memories: Vec<ExportMemory>,
    cost_records: Vec<ExportCostRecord>,
}
```

### CLI Subcommand Definition (clap derive)
```rust
// Source: crates/blufio/src/main.rs Commands enum pattern
/// GDPR data subject rights tooling.
#[command(
    after_help = "GDPR data subject rights tooling. Supports right to erasure (Art. 17), \
    data portability (Art. 20), and transparency (Art. 15).\n\n\
    Workflow:\n  \
    1. blufio gdpr list-users\n  \
    2. blufio gdpr report --user <id>\n  \
    3. blufio gdpr export --user <id>\n  \
    4. blufio gdpr erase --user <id>"
)]
Gdpr {
    #[command(subcommand)]
    action: GdprCommands,
},
```

### SHA-256 User ID Hashing for Events
```rust
// Source: blufio-audit/chain.rs SHA-256 pattern
use sha2::{Digest, Sha256};

fn hash_user_id(user_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(user_id.as_bytes());
    hex::encode(hasher.finalize())
}
```

### Prometheus Metric Registration
```rust
// Source: blufio-prometheus/recording.rs pattern
pub fn register_gdpr_metrics() {
    describe_counter!("blufio_gdpr_erasures_total", "Total GDPR erasure operations");
    describe_counter!("blufio_gdpr_exports_total", "Total GDPR data exports");
    describe_counter!("blufio_gdpr_reports_total", "Total GDPR transparency reports");
    describe_histogram!("blufio_gdpr_erasure_duration_seconds", "GDPR erasure duration");
    describe_histogram!("blufio_gdpr_export_size_bytes", "GDPR export file size");
    describe_counter!("blufio_gdpr_records_erased_total", "Records erased by type");
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Manual SQL deletion per user request | CLI-driven automated erasure with audit trail | Phase 60 | Operators can fulfill GDPR DSR in seconds |
| No data export capability | JSON/CSV export with classification filtering | Phase 60 | GDPR Art. 20 data portability |
| No transparency mechanism | Count-based report of held data | Phase 60 | GDPR Art. 15 transparency |

**Existing infrastructure leveraged:**
- Phase 53 PII detection: `ClassificationGuard::redact_for_export()` ready for --redact flag
- Phase 54 audit trail: `erase_audit_entries()` with hash chain preservation fully implemented and tested
- Phase 56 compaction: `delete_archives_by_session_ids()` for GDPR archive cleanup
- Phase 58 retention: `deleted_at` column on all tables, soft-delete infrastructure (but GDPR uses hard delete)

## Open Questions

1. **Memory Re-index Implementation**
   - What we know: CONTEXT says "Memory re-index triggered synchronously after erasure." MemoryStore has `conn()` accessor for raw SQL. Eviction uses `DELETE FROM memories WHERE id IN (...)` which triggers FTS5 sync.
   - What's unclear: Whether a full FTS5 rebuild is needed after bulk deletion, or whether the per-row triggers suffice.
   - Recommendation: Use per-row DELETE (triggers handle FTS5). After erasure, verify FTS5 count matches memories count. If mismatch, `INSERT INTO memories_fts(memories_fts) VALUES ('rebuild')`.

2. **CSV Column Layout**
   - What we know: CONTEXT says "flatten nested data to columns (metadata JSON expanded to individual columns where possible)."
   - What's unclear: Exact columns for each data type, how to handle variable-depth JSON metadata.
   - Recommendation: Use fixed columns per data type. For metadata JSON, use a single `metadata` column with the raw JSON string if expansion is ambiguous. Test with golden file snapshots.

3. **Shared Session Detection**
   - What we know: "For shared/bridged sessions: delete only the target user's messages, preserve session and other users' messages."
   - What's unclear: How to detect if a session is shared. The `sessions` table has a single `user_id` field.
   - Recommendation: A session is "shared" if it has messages from multiple distinct senders. Query `SELECT DISTINCT session_id FROM messages WHERE session_id IN (...) GROUP BY session_id HAVING COUNT(DISTINCT role) > 1` or check for bridge-attributed messages. Since user messages in the `messages` table don't have a per-message user_id, shared sessions may need to be identified by the bridge metadata or by checking if multiple sessions reference the same messages. Given the complexity, implement a simpler approach: if `session.user_id = target`, the session is fully owned. Shared sessions are those where the session's user_id differs from the target but the target has messages there (via bridge). For Phase 60, delete messages where session_id is in the user's sessions list. For bridged sessions where user_id != target, those sessions won't be in the initial query results.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in + proptest + tokio::test |
| Config file | Cargo.toml [dev-dependencies] per crate |
| Quick run command | `cargo test -p blufio-gdpr` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| GDPR-01 | Erase all user data across all tables | integration | `cargo test -p blufio-gdpr -- erasure_completeness` | Wave 0 |
| GDPR-01 | Property: random user data -> erase -> zero references | integration | `cargo test -p blufio-gdpr -- proptest_erasure` | Wave 0 |
| GDPR-02 | Cost anonymization preserves aggregates | unit | `cargo test -p blufio-gdpr -- cost_anonymization` | Wave 0 |
| GDPR-03 | Erasure creates audit entry, existing entries redacted | integration | `cargo test -p blufio-gdpr -- audit_erasure` | Wave 0 |
| GDPR-04 | Report shows correct counts per data type | unit | `cargo test -p blufio-gdpr -- report_counts` | Wave 0 |
| GDPR-05 | Export-before-erasure: export fails -> erasure aborted | integration | `cargo test -p blufio-gdpr -- export_before_erase_safety` | Wave 0 |
| GDPR-06 | JSON and CSV export with filtering | unit | `cargo test -p blufio-gdpr -- export_json_csv` | Wave 0 |
| GDPR-06 | --redact applies PII redaction | unit | `cargo test -p blufio-gdpr -- export_redaction` | Wave 0 |
| N/A | Active session refusal | unit | `cargo test -p blufio-gdpr -- active_session_refusal` | Wave 0 |
| N/A | Dry-run counts match actual data | integration | `cargo test -p blufio-gdpr -- dry_run` | Wave 0 |
| N/A | Timeout enforcement | unit | `cargo test -p blufio-gdpr -- timeout` | Wave 0 |
| N/A | CSV escaping (commas, newlines, quotes) | unit | `cargo test -p blufio-gdpr -- csv_escaping` | Wave 0 |
| N/A | Golden file snapshots for JSON/CSV | snapshot | `cargo test -p blufio-gdpr -- golden` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p blufio-gdpr`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full workspace test suite green before /gsd:verify-work

### Wave 0 Gaps
- [ ] `crates/blufio-gdpr/` -- entire new crate (Cargo.toml, src/*.rs, tests/)
- [ ] `crates/blufio-gdpr/tests/integration_tests.rs` -- GDPR completeness, export-then-erase, audit redaction
- [ ] `crates/blufio-gdpr/tests/golden/` -- snapshot files for JSON/CSV export format

## Sources

### Primary (HIGH confidence)
- Codebase: `crates/blufio-audit/src/chain.rs` -- `erase_audit_entries()` implementation verified, hash chain design confirmed
- Codebase: `crates/blufio-security/src/classification_guard.rs` -- `redact_for_export()`, `filter_for_export()`, `can_export()` verified
- Codebase: `crates/blufio-storage/src/queries/archives.rs` -- `delete_archives_by_session_ids()` verified with LIKE-based JSON matching
- Codebase: `crates/blufio-storage/src/queries/messages.rs` -- `delete_messages_by_ids()` with parameterized IN clause verified
- Codebase: `crates/blufio-storage/src/queries/sessions.rs` -- Session model with `user_id: Option<String>`, state field verified
- Codebase: `crates/blufio-cost/src/ledger.rs` -- CostRecord schema with session_id field, CostLedger operations verified
- Codebase: `crates/blufio-memory/src/store.rs` -- MemoryStore with `conn()` accessor for hard delete, `soft_delete()` verified
- Codebase: `crates/blufio-memory/src/types.rs` -- Memory struct with `session_id: Option<String>` verified
- Codebase: `crates/blufio-bus/src/events.rs` -- 17 existing event sub-enums with String fields pattern verified
- Codebase: `crates/blufio-config/src/model.rs` -- BlufioConfig with 30+ sections all using `#[serde(default)]` pattern verified
- Codebase: `crates/blufio-core/src/error.rs` -- BlufioError hierarchy with typed and string-based variants verified
- Codebase: `crates/blufio/src/main.rs` -- 20+ Commands enum variants, clap derive pattern verified
- Codebase: `crates/blufio/src/cron_cmd.rs` -- CLI handler pattern (handle_cron_command dispatch) verified
- Codebase: `crates/blufio/src/doctor.rs` -- Health check pattern for adding GDPR readiness check verified
- Codebase: `crates/blufio-prometheus/src/recording.rs` -- Metric registration with describe_counter!/describe_histogram! verified

### Secondary (MEDIUM confidence)
- CONTEXT.md: All implementation decisions locked by user, cross-verified with codebase capabilities
- REQUIREMENTS.md: GDPR-01 through GDPR-06 requirement definitions

### Tertiary (LOW confidence)
- None. All findings verified from codebase.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all libraries already in workspace, patterns established across 7+ prior phases
- Architecture: HIGH -- crate structure, CLI pattern, config pattern, event pattern all directly observed in codebase
- Pitfalls: HIGH -- all pitfalls derived from actual codebase structure (FTS5 triggers, separate audit DB, shared sessions, transaction scope)
- Code examples: HIGH -- all examples derived from existing code in the repository

**Research date:** 2026-03-12
**Valid until:** 2026-04-12 (stable -- all internal codebase patterns, no external API dependencies)
