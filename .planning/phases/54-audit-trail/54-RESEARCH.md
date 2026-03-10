# Phase 54: Audit Trail - Research

**Researched:** 2026-03-10
**Domain:** Hash-chained append-only audit logging, async SQLite write pipeline, EventBus subscriber pattern
**Confidence:** HIGH

## Summary

Phase 54 implements a tamper-evident, hash-chained audit trail for Blufio. Every security-relevant action (tool execution, memory modification, config changes, provider calls, session lifecycle, classification changes, API mutations, and audit meta-events) is recorded in a dedicated `audit.db` SQLite database with append-only semantics. The hash chain uses SHA-256 over immutable fields, while PII fields are deliberately excluded from the hash to enable GDPR redact-in-place without breaking chain integrity.

The implementation follows established Blufio patterns extensively: a new `blufio-audit` crate with the same single-writer `tokio-rusqlite` pattern used by `blufio-storage`, an EventBus subscriber modeled after `blufio-prometheus`, and CLI subcommands in the main binary crate. The async write pipeline uses a bounded mpsc channel (capacity 1024) with a background task that batches entries and flushes on configurable triggers. The design prioritizes non-blocking behavior -- the agent loop must never stall for audit writes.

Five new BusEvent variants (Config, Memory, Audit, Api, Provider) extend the existing event system in `blufio-bus/src/events.rs`, following the established sub-enum pattern. Integration touches serve.rs (init/shutdown ordering), blufio-config (AuditConfig), blufio-memory (MemoryEvent emission), blufio-agent (ProviderEvent emission), blufio-gateway (audit middleware), and the main binary (CLI subcommands and doctor/backup integration).

**Primary recommendation:** Build the blufio-audit crate first (models, chain logic, writer, subscriber), then extend BusEvent, then wire integration points, then add CLI and doctor/backup integration last.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Split-field design: immutable fields in hash, PII fields redactable
- Hashed (immutable): prev_hash, timestamp, event_type, action, resource_type, resource_id
- Redactable (NOT in hash): actor, session_id, details_json, pii_marker
- Hash format: pipe-delimited canonical -- `SHA-256(prev_hash|timestamp|event_type|action|resource_type|resource_id)`
- Entry ID: INTEGER PRIMARY KEY AUTOINCREMENT (enables gap detection, trivial chain ordering)
- details_json: JSON metadata field, erasable to "[ERASED]" without breaking hash chain
- pii_marker: INTEGER DEFAULT 0, set to 1 when GDPR erasure applied
- Encryption: same BLUFIO_DB_KEY (SQLCipher) as main database via existing open_connection()
- Genesis: first real event uses prev_hash of 64 zero hex chars (no synthetic genesis entry)
- Hash chain format is internal implementation -- no versioning, migration re-hashes if format changes
- Per-invocation tool audit (one entry per tool call: WASM skills, MCP tools, built-in tools)
- Provider calls: metadata only (model, tokens, cost, latency, success) -- no prompt/response content
- All operations audited including memory reads (configurable via TOML allowlist)
- TOML event filter: `events = ["all"]` default, dot-prefix matching (e.g., "session.*" matches session.created/closed)
- API requests: mutating only (POST, PUT, DELETE) -- GET/health/status excluded
- Actor convention: prefixed strings -- "user:{id}", "api-key:{key_id}", "system", "cron:{job_name}"
- Session lifecycle includes channel + user_id in details_json
- Audit enable/disable state changes logged as audit.enabled/audit.disabled events
- Erasure operations logged as audit.erased with sha256(user_id) in details (not plaintext)
- 5 new BusEvent variants: Config(ConfigEvent), Memory(MemoryEvent), Audit(AuditMetaEvent), Api(ApiEvent), Provider(ProviderEvent)
- All added to event_type_string() match for exhaustive coverage
- Emission sites: Config in reload handler, Memory in blufio-memory CRUD, Api in gateway middleware, Audit in AuditWriter, Provider in agent loop after call returns
- Single AuditSubscriber subscribes to all BusEvent variants, internal filtering via TOML allowlist
- Converts BusEvent to AuditEntry and sends to AuditWriter via mpsc
- blufio-memory gains blufio-bus dependency (Optional<Arc<EventBus>> pattern for tests/CLI)
- Provider events emitted in blufio-agent SessionActor (after provider.chat() returns)
- Gateway API events emitted via axum middleware layer on mutating routes
- Bounded mpsc channel (capacity 1024)
- Background task drains channel, batches entries, single INSERT transaction
- Flush triggers: batch size (64 entries) OR time interval (1 second) OR shutdown signal
- Overflow: try_send() -- if full, log warning + increment blufio_audit_dropped_total counter. Never block agent loop
- SHA-256 hashing in background task only (maintains chain head in memory, single-writer)
- Chain head recovery on startup: SELECT entry_hash ORDER BY id DESC LIMIT 1 (or 64 zeros if empty)
- Public flush() API via oneshot channel -- used by shutdown handler and GDPR erasure
- WAL mode + synchronous=NORMAL + foreign_keys=ON (same as main database)
- Three subcommands: `blufio audit verify`, `blufio audit tail`, `blufio audit stats`
- verify: walks hash chain, checks ID sequence gaps, reports GDPR-erased count. Exit code 0 (OK) or 1 (broken)
- tail: last N entries with filters -- --type, --since/--until, --actor
- stats: total entries, first/last timestamp, erased count, counts by event type
- All three support --json output
- CLI reads work even when audit is disabled (read-only mode on existing data, with note)
- Config schema: enabled (bool, default true), db_path (Option<String>, None = {data_dir}/audit.db), events (Vec<String>, default ["all"])
- #[serde(deny_unknown_fields)] -- consistent with other config sections
- Fully optional section -- omitting [audit] applies all defaults
- Config validation on first use (AuditWriter init), not at parse time
- Commented [audit] section in blufio.example.toml
- Agent loop continues with warning if audit.db fails -- core value: never block for audit
- Audit DB treated as dependency with circuit breaker (wire to existing degradation ladder from Phase 48)
- New BlufioError::Audit(AuditError) variant: DbUnavailable, ChainBroken, FlushFailed, VerifyFailed
- Error classification: is_retryable()=true, severity=Error, category=Security
- Auto-create audit.db on first use if it doesn't exist (with schema migration)
- blufio doctor includes audit health check (last 100 entries, not full chain walk)
- Prometheus metrics: blufio_audit_entries_total, blufio_audit_batch_flush_total, blufio_audit_dropped_total, blufio_audit_flush_duration_seconds, blufio_audit_errors_total
- GDPR erasure: erase_audit_entries(db, user_id) function -- called by Phase 60
- Match entries by actor prefix ("user:{user_id}%") OR details_json containing user_id
- Erase actor, session_id, and details_json to "[ERASED]", set pii_marker=1
- Erasure operation logged as audit.erased entry with sha256(user_id)
- Returns AuditErasureReport struct: entries_found, entries_erased, erased_ids
- Flush pending entries before erasure to ensure complete coverage
- Init order: after EventBus, before channel adapters
- Shared via Arc<AuditWriter>
- Shutdown order: flush after adapters disconnect, before DB close
- Backup command includes audit.db alongside main database
- New blufio-audit crate: lib.rs, writer.rs, subscriber.rs, chain.rs, models.rs, migrations.rs
- Dependencies: blufio-bus, blufio-core, blufio-storage (for open_connection), sha2, tokio, rusqlite/tokio-rusqlite, serde_json, chrono

### Claude's Discretion
- Exact crate dependency versions (sha2, chrono pinning)
- Internal module structure within blufio-audit (sub-modules vs flat)
- Exact Prometheus metric label values
- Test fixture data and organization
- Migration version numbering
- Batch INSERT SQL optimization (multi-row vs loop)
- AuditSubscriber BusEvent-to-AuditEntry conversion logic details

### Deferred Ideas (OUT OF SCOPE)
- `blufio audit tail --follow` (streaming mode) -- future enhancement
- External witness integration (cloud KMS, git) for chain head snapshots -- v1.6+ per REQUIREMENTS.md
- Time-series breakdown in stats (daily/hourly bucketing) -- add if operators request it
- Configurable buffer tuning knobs (buffer_capacity, flush_interval_ms, batch_size) -- expose if defaults prove insufficient
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| AUDT-01 | Hash-chained tamper-evident log where each entry hash = SHA-256(prev_hash \|\| canonical_entry) | chain.rs module: pipe-delimited canonical format over immutable fields; sha2 crate already in workspace; single-writer background task maintains chain head |
| AUDT-02 | Audit entries cover: tool execution, memory modification, config changes, provider calls, session lifecycle, classification changes, erasure events | 5 new BusEvent variants + existing Session/Skill/Classification; AuditSubscriber maps all to AuditEntry; event filter via TOML allowlist |
| AUDT-03 | Audit trail stored in dedicated audit.db (separate from main database) | Reuse open_connection() from blufio-storage; separate refinery migrations in blufio-audit; same SQLCipher/WAL/PRAGMA setup |
| AUDT-04 | CLI command `blufio audit verify` walks hash chain and reports any breaks | New AuditCommands subcommand enum in main.rs; chain.rs verify logic reads all entries, recomputes hashes, checks ID gaps |
| AUDT-05 | Audit entries are append-only -- retention policies never delete them | Schema enforced (no DELETE in blufio-audit); future retention phase (58) exempts audit entries per RETN-04 |
| AUDT-06 | Audit schema supports GDPR redact-in-place (PII fields replaceable with [ERASED] without breaking hash chain) | Split-field design: actor, session_id, details_json excluded from hash; erase_audit_entries() UPDATE with pii_marker |
| AUDT-07 | Async audit writes via buffered mpsc channel with batch flush | AuditWriter with bounded mpsc(1024), background tokio::spawn, batch INSERT in single transaction, try_send() overflow policy |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| sha2 | 0.10 | SHA-256 hash computation for chain | Already in workspace.dependencies; pure Rust, no OpenSSL |
| tokio-rusqlite | 0.7 | Async single-writer SQLite access for audit.db | Already in workspace; established single-writer pattern in blufio-storage |
| rusqlite | 0.37 | Sync SQLite access for CLI verify/tail/stats | Already in workspace with bundled-sqlcipher-vendored-openssl |
| refinery | 0.9 | Embedded schema migrations for audit.db | Already in workspace; same pattern as blufio-storage migrations |
| serde_json | 1 | Serialize details_json field and --json CLI output | Already in workspace |
| chrono | 0.4 | ISO 8601 timestamps | Already in workspace with serde feature |
| hex | 0.4 | Encode SHA-256 digest to hex string | Already in workspace |
| clap | 4.5 | CLI subcommand definition | Already in workspace with derive feature |
| tokio | 1 | mpsc channel, spawn, oneshot for flush API | Already in workspace |
| metrics | 0.24 | Prometheus counter/histogram recording | Already in workspace; same pattern as blufio-prometheus |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| proptest | 1 | Property-based testing for chain integrity | In dev-dependencies for chain.rs tests |
| tempfile | 3 | Temp directory setup for integration tests | In dev-dependencies |
| serial_test | 3 | Serialize tests that touch env vars (BLUFIO_DB_KEY) | In dev-dependencies |
| criterion | (workspace if exists) | Benchmark hash throughput and batch inserts | Optional dev-dependency for benchmarks |
| tracing | 0.1 | Structured logging for overflow warnings, lifecycle | Already in workspace |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| sha2 crate | ring | ring is also in workspace but sha2 is simpler for standalone hashing without HMAC context |
| refinery | Manual CREATE TABLE | refinery is already established; manual SQL loses migration tracking |
| tokio mpsc | crossbeam channel | tokio mpsc integrates with async runtime; crossbeam adds dependency |

**Installation:**
No new workspace dependencies needed. All required crates are already in `Cargo.toml` workspace.dependencies. The new `blufio-audit` crate will reference them via `workspace = true`.

## Architecture Patterns

### Recommended Crate Structure
```
crates/blufio-audit/
  Cargo.toml
  migrations/
    V1__create_audit_entries.sql
  src/
    lib.rs           # Public API: AuditWriter, AuditSubscriber, erase_audit_entries, verify
    models.rs        # AuditEntry struct, AuditErasureReport, AuditError enum
    writer.rs        # AuditWriter: mpsc receiver, batch flush, chain head maintenance
    subscriber.rs    # AuditSubscriber: EventBus -> AuditEntry conversion, event filtering
    chain.rs         # Hash computation, verify_chain(), gap detection
    migrations.rs    # refinery embed_migrations! for audit.db schema
    filter.rs        # EventFilter: TOML allowlist with dot-prefix matching
```

### Pattern 1: Single-Writer Background Task (AuditWriter)
**What:** A background tokio task owns the audit.db connection and drains an mpsc channel, batching entries and flushing periodically.
**When to use:** Always -- this is the core write path.
**Example:**
```rust
// Modeled after blufio-storage single-writer pattern
pub struct AuditWriter {
    tx: mpsc::Sender<AuditCommand>,
    // Handle to the background task for graceful shutdown
    task_handle: tokio::task::JoinHandle<()>,
}

enum AuditCommand {
    Write(PendingEntry),  // Entry without hash (hash computed in background)
    Flush(oneshot::Sender<Result<(), AuditError>>),
    Shutdown,
}

// PendingEntry has all fields except entry_hash and prev_hash
// Background task assigns id, computes hash, maintains chain_head
```

### Pattern 2: EventBus Subscriber (AuditSubscriber)
**What:** Subscribes to EventBus reliable channel, converts BusEvent to AuditEntry, sends to AuditWriter mpsc.
**When to use:** Exactly once per serve startup.
**Example:**
```rust
// Modeled after DegradationManager EventBus subscription in serve.rs
pub struct AuditSubscriber {
    writer: Arc<AuditWriter>,
    filter: EventFilter,
}

impl AuditSubscriber {
    pub async fn run(self, mut rx: mpsc::Receiver<BusEvent>) {
        while let Some(event) = rx.recv().await {
            let event_type = event.event_type_string();
            if !self.filter.matches(event_type) {
                continue;
            }
            let entry = self.convert_to_pending_entry(&event);
            if self.writer.try_send(entry).is_err() {
                tracing::warn!("audit entry dropped: channel full");
                metrics::counter!("blufio_audit_dropped_total").increment(1);
            }
        }
    }
}
```

### Pattern 3: BusEvent Extension
**What:** Add 5 new variants to the existing BusEvent enum in events.rs.
**When to use:** Follows established pattern for Classification/Resilience events.
**Example:**
```rust
// In blufio-bus/src/events.rs -- extend existing enum
pub enum BusEvent {
    // ... existing 8 variants ...
    /// Configuration change events.
    Config(ConfigEvent),
    /// Memory CRUD events.
    Memory(MemoryEvent),
    /// Audit subsystem meta-events.
    Audit(AuditMetaEvent),
    /// API request events (mutating only).
    Api(ApiEvent),
    /// LLM provider call events.
    Provider(ProviderEvent),
}

// Each sub-enum follows the established pattern with event_id + timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryEvent {
    Created { event_id: String, timestamp: String, memory_id: String, source: String },
    Updated { event_id: String, timestamp: String, memory_id: String },
    Deleted { event_id: String, timestamp: String, memory_id: String },
    Retrieved { event_id: String, timestamp: String, memory_id: String, query: String },
    Evicted { event_id: String, timestamp: String, memory_id: String, reason: String },
}
```

### Pattern 4: Config Section
**What:** AuditConfig added to BlufioConfig following established pattern.
**When to use:** Standard config extension.
**Example:**
```rust
// In blufio-config/src/model.rs
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuditConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default)]
    pub db_path: Option<String>,

    #[serde(default = "default_audit_events")]
    pub events: Vec<String>,
}

fn default_audit_events() -> Vec<String> {
    vec!["all".to_string()]
}
```

### Pattern 5: Error Variant Extension
**What:** New BlufioError::Audit(AuditError) variant following typed hierarchy.
**When to use:** All audit-specific errors.
**Example:**
```rust
// In blufio-core/src/error.rs -- follows existing typed pattern
#[derive(Debug, Clone, Serialize, Deserialize, Display)]
pub enum AuditErrorKind {
    DbUnavailable,
    ChainBroken,
    FlushFailed,
    VerifyFailed,
}

// In BlufioError enum:
#[error("audit: {kind}")]
Audit {
    kind: AuditErrorKind,
    context: ErrorContext,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
},
```

### Anti-Patterns to Avoid
- **Hashing PII fields:** Actor, session_id, and details_json are deliberately excluded from the hash to enable GDPR erasure. Never include them in the canonical hash input.
- **Blocking agent loop for audit:** Always use try_send(), never send().await. If the channel is full, drop the entry and log a warning.
- **Multiple writers to audit.db:** The background task is the single writer. CLI commands are read-only against audit.db.
- **Computing hashes outside the background task:** Only the background task maintains the chain head variable. Computing hashes elsewhere creates race conditions.
- **Using broadcast subscriber for audit:** Use reliable mpsc subscriber (subscribe_reliable) to avoid dropped events from lag.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| SQLite connection with encryption | Custom PRAGMA key handling | `blufio_storage::open_connection()` | Already handles SQLCipher, key format detection, WAL mode |
| Schema migrations | Manual CREATE TABLE in code | `refinery::embed_migrations!` | Migration tracking, version history, idempotency |
| Event bus subscription | Custom channel plumbing | `EventBus::subscribe_reliable(256)` | Established pattern with guaranteed delivery |
| SHA-256 hashing | Manual digest computation | `sha2::Sha256` with `Digest` trait | Pure Rust, constant-time, battle-tested |
| Hex encoding | Manual byte-to-hex | `hex::encode()` | Already in workspace, handles edge cases |
| ISO timestamp generation | Manual formatting | `chrono::Utc::now().to_rfc3339()` or `now_timestamp()` from blufio-bus | Already used project-wide |
| Prometheus metrics | Custom metric tracking | `metrics::counter!()` / `metrics::histogram!()` | Already wired to Prometheus exporter |

**Key insight:** Almost every infrastructure component this phase needs already exists in the workspace. The primary work is wiring them together in a new crate, not building new primitives.

## Common Pitfalls

### Pitfall 1: Chain Head Race Condition
**What goes wrong:** If hash computation happens outside the single background writer task, two concurrent entries could both read the same prev_hash and fork the chain.
**Why it happens:** Temptation to compute hashes eagerly in the subscriber or caller.
**How to avoid:** All hash computation MUST happen in the background writer task, which maintains `chain_head: String` in local state. PendingEntry structs sent via mpsc do NOT include hashes.
**Warning signs:** Any code path that reads chain_head outside the writer task.

### Pitfall 2: Transaction Boundary for Batch Inserts
**What goes wrong:** If individual INSERTs fail mid-batch without a transaction, the chain state becomes inconsistent (some entries written, chain_head updated, but gaps exist).
**Why it happens:** Not wrapping batch inserts in a transaction.
**How to avoid:** Each batch flush must use a single SQL transaction. If the transaction fails, chain_head is NOT updated (it stays at the last successfully committed entry hash).
**Warning signs:** INSERT without BEGIN/COMMIT around the batch.

### Pitfall 3: Startup Chain Head Recovery
**What goes wrong:** If the writer starts with a stale chain_head (or forgets to recover), new entries will have incorrect prev_hash values, silently breaking the chain.
**Why it happens:** Forgetting the recovery query or handling an empty database.
**How to avoid:** On startup, `SELECT entry_hash FROM audit_entries ORDER BY id DESC LIMIT 1`. If no rows, use 64 zero hex chars. This MUST happen before accepting any writes.
**Warning signs:** Chain breaks immediately after restart.

### Pitfall 4: GDPR Erasure + Pending Flush
**What goes wrong:** If erasure runs while entries are still buffered in the mpsc channel, those entries escape erasure. Later flush writes them with the original PII.
**Why it happens:** Not flushing pending entries before running erasure.
**How to avoid:** `erase_audit_entries()` must call `writer.flush().await` before executing the UPDATE. The flush() API uses a oneshot channel to confirm completion.
**Warning signs:** PII remaining in audit.db after erasure when the system was under load.

### Pitfall 5: exhaustive match on BusEvent
**What goes wrong:** Adding 5 new BusEvent variants breaks every existing match statement in the codebase (webhooks, degradation, bridge, etc.).
**Why it happens:** Rust's exhaustive matching is a feature, but it means all match sites must be updated.
**How to avoid:** Add wildcard arms (`_ => {}`) to existing match statements that don't need to handle the new variants. Or use `if let` patterns in existing subscribers. The event_type_string() match MUST be exhaustive (no wildcard).
**Warning signs:** Compilation errors in webhook delivery, degradation manager, bridge, sd-notify subscriber.

### Pitfall 6: Circular Dependency blufio-memory -> blufio-bus
**What goes wrong:** blufio-memory needs blufio-bus to emit MemoryEvent, but if there's a transitive dependency in the other direction, Cargo will reject the cycle.
**Why it happens:** Dependency graph not checked before adding the edge.
**How to avoid:** Verify blufio-bus does NOT depend on blufio-memory (it doesn't -- bus has no crate dependencies beyond serde/chrono/uuid/tokio). Use `Optional<Arc<EventBus>>` in MemoryStore so tests/CLI work without a bus instance.
**Warning signs:** Cargo error about cyclic dependencies.

### Pitfall 7: Migration Conflict with Main Database
**What goes wrong:** Using the same refinery migration runner for audit.db as the main database, causing migration version conflicts.
**Why it happens:** Sharing the refinery_schema_history table between databases.
**How to avoid:** blufio-audit has its own `embed_migrations!("migrations")` pointing to its own migrations/ directory. Each database has independent migration tracking.
**Warning signs:** "Migration already applied" errors on audit.db.

## Code Examples

### Hash Chain Computation
```rust
// Source: CONTEXT.md locked decision
use sha2::{Digest, Sha256};

pub fn compute_entry_hash(
    prev_hash: &str,
    timestamp: &str,
    event_type: &str,
    action: &str,
    resource_type: &str,
    resource_id: &str,
) -> String {
    let canonical = format!(
        "{prev_hash}|{timestamp}|{event_type}|{action}|{resource_type}|{resource_id}"
    );
    let digest = Sha256::digest(canonical.as_bytes());
    hex::encode(digest)
}

pub const GENESIS_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";
```

### Audit Schema (Migration SQL)
```sql
-- V1__create_audit_entries.sql
CREATE TABLE IF NOT EXISTS audit_entries (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    entry_hash    TEXT    NOT NULL,
    prev_hash     TEXT    NOT NULL,
    timestamp     TEXT    NOT NULL,
    event_type    TEXT    NOT NULL,
    action        TEXT    NOT NULL,
    resource_type TEXT    NOT NULL DEFAULT '',
    resource_id   TEXT    NOT NULL DEFAULT '',
    actor         TEXT    NOT NULL DEFAULT '',
    session_id    TEXT    NOT NULL DEFAULT '',
    details_json  TEXT    NOT NULL DEFAULT '{}',
    pii_marker    INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_entries(timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_event_type ON audit_entries(event_type);
CREATE INDEX IF NOT EXISTS idx_audit_actor ON audit_entries(actor);
CREATE INDEX IF NOT EXISTS idx_audit_pii_marker ON audit_entries(pii_marker);
```

### Batch Insert in Transaction
```rust
// Source: blufio-storage single-writer pattern
conn.call(move |conn| {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO audit_entries \
             (entry_hash, prev_hash, timestamp, event_type, action, \
              resource_type, resource_id, actor, session_id, details_json, pii_marker) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
        )?;
        for entry in &batch {
            stmt.execute(rusqlite::params![
                entry.entry_hash, entry.prev_hash, entry.timestamp,
                entry.event_type, entry.action, entry.resource_type,
                entry.resource_id, entry.actor, entry.session_id,
                entry.details_json, entry.pii_marker,
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}).await.map_err(|e| AuditError::FlushFailed)?;
```

### GDPR Erasure
```rust
// Source: CONTEXT.md locked decision
pub struct AuditErasureReport {
    pub entries_found: usize,
    pub entries_erased: usize,
    pub erased_ids: Vec<i64>,
}

pub async fn erase_audit_entries(
    conn: &tokio_rusqlite::Connection,
    user_id: &str,
) -> Result<AuditErasureReport, AuditError> {
    let user_id = user_id.to_string();
    conn.call(move |conn| {
        let like_actor = format!("user:{}%", user_id);
        let like_details = format!("%{}%", user_id);

        // Find matching entries
        let mut find_stmt = conn.prepare(
            "SELECT id FROM audit_entries \
             WHERE (actor LIKE ?1 OR details_json LIKE ?2) AND pii_marker = 0"
        )?;
        let ids: Vec<i64> = find_stmt
            .query_map(rusqlite::params![&like_actor, &like_details], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        let entries_found = ids.len();

        // Erase PII fields
        let mut erase_stmt = conn.prepare(
            "UPDATE audit_entries SET actor = '[ERASED]', session_id = '[ERASED]', \
             details_json = '[ERASED]', pii_marker = 1 WHERE id = ?1"
        )?;
        for &id in &ids {
            erase_stmt.execute(rusqlite::params![id])?;
        }

        Ok(AuditErasureReport {
            entries_found,
            entries_erased: ids.len(),
            erased_ids: ids,
        })
    }).await.map_err(|_| AuditError::FlushFailed)
}
```

### Event Filter (TOML Allowlist)
```rust
// Source: CONTEXT.md locked decision
pub struct EventFilter {
    patterns: Vec<String>,
    match_all: bool,
}

impl EventFilter {
    pub fn new(patterns: Vec<String>) -> Self {
        let match_all = patterns.iter().any(|p| p == "all");
        Self { patterns, match_all }
    }

    pub fn matches(&self, event_type: &str) -> bool {
        if self.match_all {
            return true;
        }
        self.patterns.iter().any(|pattern| {
            if pattern.ends_with(".*") {
                let prefix = &pattern[..pattern.len() - 2];
                event_type.starts_with(prefix)
            } else {
                event_type == pattern
            }
        })
    }
}
```

### CLI Subcommand Structure
```rust
// In main.rs -- follows existing subcommand pattern
/// Audit trail management.
Audit {
    #[command(subcommand)]
    action: AuditCommands,
},

#[derive(Subcommand, Debug)]
enum AuditCommands {
    /// Verify hash chain integrity.
    #[command(after_help = "Examples:\n  blufio audit verify\n  blufio audit verify --json")]
    Verify {
        #[arg(long)]
        json: bool,
    },
    /// Show recent audit entries.
    #[command(after_help = "Examples:\n  blufio audit tail\n  blufio audit tail -n 50 --type session.*")]
    Tail {
        #[arg(short, long, default_value = "20")]
        n: usize,
        #[arg(long, name = "TYPE")]
        r#type: Option<String>,
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        until: Option<String>,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Show audit trail statistics.
    #[command(after_help = "Examples:\n  blufio audit stats\n  blufio audit stats --json")]
    Stats {
        #[arg(long)]
        json: bool,
    },
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Global audit log in main DB | Dedicated audit.db with separate connection | This phase | Isolation, independent backup, no retention interference |
| Synchronous audit writes | Async buffered mpsc with batch flush | This phase | Non-blocking agent loop, higher throughput |
| Full-entry hashing | Split-field design (immutable in hash, PII out) | This phase | Enables GDPR erasure without chain breaks |

**Deprecated/outdated:**
- None -- this is a new subsystem. No prior audit implementation exists.

## Open Questions

1. **Criterion benchmarks setup**
   - What we know: proptest is already used in blufio-core and blufio-security. Criterion is not currently in workspace.dependencies.
   - What's unclear: Whether to add criterion as workspace dependency or skip benchmarks initially.
   - Recommendation: Add criterion to dev-dependencies of blufio-audit only. Benchmark hash throughput and batch insert 1000 entries. Not critical for initial implementation.

2. **Agent loop provider.chat() location**
   - What we know: The session.rs file in blufio-agent contains the agent loop. The exact method name for LLM calls needs to be identified at implementation time.
   - What's unclear: Exact function signature and where to emit ProviderEvent.
   - Recommendation: Grep for the provider call site in session.rs during implementation. Emit ProviderEvent immediately after the call returns with metadata (model, tokens, latency, success).

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in) + proptest 1.x |
| Config file | None -- standard Cargo test configuration |
| Quick run command | `cargo test -p blufio-audit` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| AUDT-01 | Hash chain links entries with SHA-256 | unit | `cargo test -p blufio-audit -- chain` | Wave 0 |
| AUDT-01 | Genesis entry uses 64 zero prev_hash | unit | `cargo test -p blufio-audit -- genesis` | Wave 0 |
| AUDT-01 | Tamper detection (modified entry breaks chain) | unit | `cargo test -p blufio-audit -- tamper` | Wave 0 |
| AUDT-01 | Property: arbitrary entries produce valid chain | proptest | `cargo test -p blufio-audit -- proptest` | Wave 0 |
| AUDT-02 | All event types recorded as audit entries | integration | `cargo test -p blufio-audit -- event_coverage` | Wave 0 |
| AUDT-02 | Event filter matches correctly (all, prefix, exact, non-matching) | unit | `cargo test -p blufio-audit -- filter` | Wave 0 |
| AUDT-03 | Audit writes go to separate audit.db file | integration | `cargo test -p blufio-audit -- separate_db` | Wave 0 |
| AUDT-04 | verify detects chain breaks and reports location | unit | `cargo test -p blufio-audit -- verify` | Wave 0 |
| AUDT-04 | verify detects ID sequence gaps | unit | `cargo test -p blufio-audit -- gap_detection` | Wave 0 |
| AUDT-04 | CLI verify/tail/stats produce correct output | integration | `cargo test -p blufio -- audit_cli` | Wave 0 |
| AUDT-05 | Entries are append-only (no DELETE in crate) | code review | manual | manual-only: grep for DELETE in blufio-audit |
| AUDT-06 | GDPR erasure replaces PII with [ERASED] | unit | `cargo test -p blufio-audit -- gdpr_erase` | Wave 0 |
| AUDT-06 | Chain intact after GDPR erasure | unit | `cargo test -p blufio-audit -- gdpr_chain_intact` | Wave 0 |
| AUDT-06 | Erased entries visible in tail with [ERASED] marker | integration | `cargo test -p blufio-audit -- gdpr_tail` | Wave 0 |
| AUDT-07 | Batch flush on size threshold (64 entries) | unit | `cargo test -p blufio-audit -- batch_size` | Wave 0 |
| AUDT-07 | Batch flush on time interval (1 second) | unit | `cargo test -p blufio-audit -- batch_time` | Wave 0 |
| AUDT-07 | try_send overflow logs warning + increments counter | unit | `cargo test -p blufio-audit -- overflow` | Wave 0 |
| AUDT-07 | flush() API via oneshot completes pending writes | unit | `cargo test -p blufio-audit -- flush_api` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p blufio-audit`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/blufio-audit/` -- entire new crate (Cargo.toml, src/*.rs, migrations/)
- [ ] `crates/blufio-audit/src/lib.rs` -- test modules for chain, filter, writer, subscriber, GDPR, CLI
- [ ] proptest dependency in blufio-audit dev-dependencies

*(All test infrastructure is created as part of the crate -- no pre-existing test files to check)*

## Sources

### Primary (HIGH confidence)
- Codebase inspection: `blufio-bus/src/events.rs` -- BusEvent enum pattern, event_type_string(), sub-enum structure (8 variants, exhaustive match)
- Codebase inspection: `blufio-bus/src/lib.rs` -- EventBus with broadcast + reliable mpsc, subscribe_reliable() API
- Codebase inspection: `blufio-storage/src/database.rs` -- open_connection(), Database::open(), WAL mode PRAGMAs, SQLCipher encryption handling
- Codebase inspection: `blufio-storage/src/migrations.rs` -- refinery embed_migrations! pattern
- Codebase inspection: `blufio-core/src/error.rs` -- BlufioError typed hierarchy with ErrorContext, classification methods
- Codebase inspection: `blufio-config/src/model.rs` -- #[serde(deny_unknown_fields)], Default impl, config section pattern
- Codebase inspection: `blufio-prometheus/src/lib.rs` -- metrics-rs facade, recording module pattern
- Codebase inspection: `blufio/src/serve.rs` -- EventBus init, subscribe_reliable(), shutdown flow, init ordering
- Codebase inspection: `blufio/src/main.rs` -- CLI subcommand structure, Commands enum, dispatch pattern
- Codebase inspection: `blufio/src/doctor.rs` -- diagnostic check pattern (CheckStatus/CheckResult)
- Codebase inspection: `blufio/src/backup.rs` -- database backup/restore with open_connection_sync()
- Codebase inspection: `blufio-gateway/src/server.rs` -- axum middleware layer pattern (rate_limit, auth)
- Codebase inspection: `blufio-memory/src/store.rs` -- MemoryStore CRUD methods (save, get_by_id, soft_delete, etc.)
- Codebase inspection: workspace Cargo.toml -- all dependency versions, workspace.dependencies

### Secondary (MEDIUM confidence)
- sha2 crate API: `Sha256::digest(bytes)` returns 32-byte array, `hex::encode()` for hex string -- verified via existing usage in blufio-skill/src/signing.rs, blufio-gateway/src/api_keys/store.rs, blufio-whatsapp/src/webhook.rs

### Tertiary (LOW confidence)
- None

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all libraries already in workspace with established usage patterns
- Architecture: HIGH -- every pattern mirrors existing crate structure (bus subscriber, storage, config, CLI)
- Pitfalls: HIGH -- derived from deep codebase analysis of integration points and concurrency model
- Code examples: HIGH -- based on locked decisions in CONTEXT.md and verified codebase patterns

**Research date:** 2026-03-10
**Valid until:** 2026-04-10 (stable -- all dependencies are pinned workspace versions)
