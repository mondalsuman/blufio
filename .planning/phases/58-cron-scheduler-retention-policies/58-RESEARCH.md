# Phase 58: Cron Scheduler & Retention Policies - Research

**Researched:** 2026-03-12
**Domain:** In-process cron scheduling, data retention lifecycle, systemd integration
**Confidence:** HIGH

## Summary

Phase 58 implements two tightly coupled subsystems: (1) a TOML-configurable cron scheduler that runs inside `blufio serve` using tokio-based scheduling, and (2) per-type retention policies with soft-delete and grace-period permanent deletion. The cron scheduler is both the scheduling engine and the execution vehicle for retention enforcement.

The project already has well-established patterns for background tasks (tokio::spawn with interval loops + CancellationToken), EventBus integration, config model definition (serde + deny_unknown_fields), CLI subcommands (clap derive), and SQLite persistence (tokio-rusqlite single-writer). This phase extends those patterns to two new tables (`cron_jobs`, `cron_history`) and adds `deleted_at` columns to four existing tables (messages, sessions, cost_ledger, memories). The cron expression parsing is handled by the `croner` crate, chosen for POSIX compliance and lightweight design.

The systemd timer generation (CRON-03) is a pure code-generation feature -- it writes `.timer` and `.service` unit files from templates, with no runtime systemd dependency. This is the simplest part of the phase.

**Primary recommendation:** Create a new `blufio-cron` crate for the scheduler core and retention logic, following the single-crate-per-concern pattern. Config types go in blufio-config/model.rs (following ClassificationConfig, AuditConfig pattern). CronEvent goes in blufio-bus/events.rs. CLI commands go in main.rs.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- In-process tokio-based scheduler running inside `blufio serve` -- follows existing background task pattern (heartbeat, memory eviction, webhook delivery all use `tokio::spawn` in serve.rs)
- Cron expression parsing via a Rust crate (e.g., cron or croner) -- no custom parser
- `blufio cron generate-timers` produces systemd timer/service unit files as an ALTERNATIVE external scheduling option -- does not require systemd at runtime
- Last-run timestamps stored in SQLite (new `cron_jobs` table) to survive restarts
- Job execution is single-instance (no concurrent runs of same job) -- use a lock flag in the DB row
- Soft-delete via `deleted_at` timestamp column added to messages, sessions, cost_records, memories tables
- Grace period is configurable per type (default: 7 days) -- after grace period, permanent DELETE runs
- Two-phase enforcement: mark soft-delete -> later sweep permanently deletes expired soft-deletes
- Audit trail entries (blufio-audit) are EXEMPT from retention -- RETN-04 requirement
- Retention respects data classification -- Restricted data can have separate (shorter or longer) retention rules per RETN-05
- 5 built-in tasks registered at startup (CRON-05): memory_cleanup, backup, cost_report, health_check, retention_enforcement
- All built-in tasks are disabled by default -- operator enables via TOML config
- New `cron_history` SQLite table: job_name, started_at, finished_at, status (success/failed/timeout), duration_ms, output (truncated)
- Failed jobs emit CronEvent on EventBus (follows existing event patterns)
- No automatic retry -- jobs run on schedule, failures are logged
- CLI: list, add, remove, run-now, history, generate-timers
- Follows existing CLI pattern: clap derive with Subcommand enum in main.rs

### Claude's Discretion
- Exact cron expression parsing library choice (cron vs croner vs other)
- Schema design details for cron_jobs and cron_history tables
- Exact systemd timer template format
- Job timeout default value
- Whether to create a new blufio-cron crate or add to existing blufio crate

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| CRON-01 | TOML-configured cron jobs with standard cron expression syntax | CronConfig model in blufio-config, croner crate for parsing |
| CRON-02 | CLI `blufio cron` subcommand for list, add, remove, run-now, history | Clap derive Subcommand pattern, CronCommands enum |
| CRON-03 | systemd timer unit file generation via `blufio cron generate-timers` | Template-based .timer/.service file generation |
| CRON-04 | Job execution history tracked in SQLite with status, duration, and output | cron_history table, V14 migration |
| CRON-05 | Built-in tasks: memory cleanup, backup, cost report, health check, retention enforcement | CronTask trait with 5 built-in implementations |
| CRON-06 | Persisted last-run timestamps survive process restarts | cron_jobs table with last_run_at column |
| RETN-01 | TOML-configurable retention periods per data type | RetentionConfig model with per-type RetentionPeriod |
| RETN-02 | Background retention enforcement runs on configurable schedule | retention_enforcement built-in cron task |
| RETN-03 | Soft-delete support with configurable grace period before permanent removal | deleted_at column, two-phase sweep |
| RETN-04 | Audit trail entries exempt from retention deletion | Retention logic skips audit.db entirely |
| RETN-05 | Retention enforcement respects data classification | Classification-aware WHERE clauses in retention queries |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| croner | 3.0 | Cron expression parsing + next occurrence | POSIX/Vixie-cron compliant, lightweight, no heavy deps, supports L/#/W extensions |
| tokio | 1.x (workspace) | Async runtime, spawn, interval, select | Already in workspace, powers all background tasks |
| tokio-rusqlite | 0.7 (workspace) | Async SQLite access | Already in workspace, single-writer pattern |
| rusqlite | 0.37 (workspace) | SQLite operations | Already in workspace |
| chrono | 0.4 (workspace) | DateTime handling | Already in workspace, croner integrates with chrono |
| serde | 1 (workspace) | Config serialization | Already in workspace |
| clap | 4.5 (workspace) | CLI parsing | Already in workspace |
| tokio-util | 0.7 (workspace) | CancellationToken | Already in workspace |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| blufio-bus | (workspace) | EventBus + CronEvent | Job completion/failure events |
| blufio-config | (workspace) | CronConfig + RetentionConfig | TOML config model |
| blufio-storage | (workspace) | Database access for retention + cron tables | Migrations, queries |
| blufio-audit | (workspace) | Audit exemption verification | RETN-04 audit exemption |
| blufio-core | (workspace) | DataClassification enum | RETN-05 classification-aware retention |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| croner | cron (crate) | `cron` uses 7-field format with year, non-POSIX weekday numbering (1=SUN). `croner` uses standard 5-field POSIX format. Operators expect standard crontab syntax. |
| croner | tokio-cron-scheduler | Full scheduler framework -- too heavy, Blufio already has its own tokio::spawn pattern. Only need parsing, not a framework. |
| In-process scheduler | External cron/systemd only | External-only would break `blufio serve` as self-contained. generate-timers provides the external option as complement. |

**Recommendation: Use `croner` 3.0.** It follows POSIX/Vixie-cron standards (0=SUN), supports standard 5-field expressions that operators know from crontab, integrates with chrono, and is lightweight. The `cron` crate uses non-standard 7-field format with year and Quartz-style weekday numbering, which would confuse operators.

**Installation:**
```bash
# Add to new blufio-cron crate Cargo.toml:
[dependencies]
croner = "3"
```

Also add to workspace Cargo.toml:
```toml
croner = "3"
```

## Architecture Patterns

### Recommended Project Structure
```
crates/
  blufio-cron/
    src/
      lib.rs              # Public API: CronScheduler, CronTask trait
      scheduler.rs        # In-process scheduler loop
      tasks/
        mod.rs            # CronTask trait + task registry
        memory_cleanup.rs # Built-in: calls blufio-memory eviction
        backup.rs         # Built-in: calls Database::backup()
        cost_report.rs    # Built-in: cost aggregation + log
        health_check.rs   # Built-in: doctor-lite checks
        retention.rs      # Built-in: soft-delete + permanent delete
      retention/
        mod.rs            # RetentionEnforcer
        soft_delete.rs    # Phase 1: mark deleted_at
        permanent.rs      # Phase 2: DELETE WHERE deleted_at + grace expired
      history.rs          # Job execution history read/write
      systemd.rs          # Timer/service unit file generation
    Cargo.toml
  blufio-config/src/
    model.rs              # Add CronConfig, RetentionConfig
  blufio-bus/src/
    events.rs             # Add CronEvent variant
  blufio-storage/
    migrations/
      V14__cron_retention.sql  # New tables + deleted_at columns
  blufio/src/
    main.rs               # Add Cron { action: CronCommands } variant
    cron_cmd.rs           # CLI handler: list/add/remove/run-now/history/generate-timers
    serve.rs              # Spawn CronScheduler after EventBus init
    doctor.rs             # Add cron health check
```

### Pattern 1: CronTask Trait
**What:** A trait that all cron job implementations satisfy, enabling both built-in and custom tasks.
**When to use:** Every built-in task and future custom tasks.
**Example:**
```rust
// Source: project convention (async-trait pattern from blufio-core)
use async_trait::async_trait;

#[async_trait]
pub trait CronTask: Send + Sync {
    /// Unique name of this task (used as job_name in DB).
    fn name(&self) -> &str;

    /// Human-readable description for CLI display.
    fn description(&self) -> &str;

    /// Execute the task. Returns Ok(output_string) or Err.
    async fn execute(&self) -> Result<String, CronTaskError>;

    /// Default timeout for this task.
    fn timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(300) // 5 minutes default
    }
}
```

### Pattern 2: Scheduler Loop (follows background.rs pattern)
**What:** In-process tokio loop that checks all registered jobs every minute and fires those due.
**When to use:** The main scheduler running inside `blufio serve`.
**Example:**
```rust
// Source: blufio-memory/src/background.rs pattern
pub async fn run_scheduler(
    jobs: Arc<RwLock<Vec<CronJob>>>,
    task_registry: Arc<HashMap<String, Box<dyn CronTask>>>,
    db: Arc<Database>,
    event_bus: Option<Arc<EventBus>>,
    cancel: CancellationToken,
) {
    let mut check_interval = tokio::time::interval(Duration::from_secs(60));
    check_interval.tick().await; // Skip first immediate tick

    loop {
        tokio::select! {
            _ = check_interval.tick() => {
                let now = chrono::Utc::now();
                // For each job: parse cron expression, check if due, acquire lock, execute
                for job in jobs.read().await.iter() {
                    if job.is_due(&now) && !job.is_locked() {
                        // Spawn execution in background (single-instance via DB lock)
                        // ...
                    }
                }
            }
            _ = cancel.cancelled() => {
                tracing::info!("Cron scheduler shutting down");
                break;
            }
        }
    }
}
```

### Pattern 3: Two-Phase Retention Enforcement
**What:** Soft-delete marks records with `deleted_at`, then a separate sweep permanently deletes records past their grace period.
**When to use:** The retention_enforcement built-in task.
**Example:**
```rust
// Phase 1: Mark expired records for soft-delete
// WHERE deleted_at IS NULL AND created_at < retention_cutoff
//   AND (classification != 'restricted' OR created_at < restricted_cutoff)
// SET deleted_at = now()

// Phase 2: Permanently delete soft-deleted records past grace period
// DELETE FROM messages WHERE deleted_at IS NOT NULL
//   AND deleted_at < (now - grace_period_days)
```

### Pattern 4: Single-Instance Job Lock (DB flag)
**What:** Prevent concurrent runs of the same job using a boolean flag in the cron_jobs table.
**When to use:** Every job execution.
**Example:**
```rust
// Acquire lock: UPDATE cron_jobs SET running = 1 WHERE name = ? AND running = 0
// Returns rows_affected = 1 if lock acquired, 0 if already running
// Release lock + update last_run: UPDATE cron_jobs SET running = 0, last_run_at = ? WHERE name = ?
```

### Anti-Patterns to Avoid
- **External scheduler dependency:** Do NOT require systemd/cron at runtime. The in-process scheduler is primary; generate-timers is optional complement.
- **Hard-delete without soft-delete:** Always soft-delete first. Hard-delete only runs on records that have been soft-deleted AND whose grace period has expired.
- **Deleting audit entries:** NEVER include audit.db tables in retention sweeps. Audit trail is append-only (AUDT-05).
- **Custom cron parser:** Do NOT hand-roll cron expression parsing. Use croner.
- **Blocking the scheduler loop:** Tasks MUST be spawned as separate tokio tasks. The scheduler loop only checks timing and dispatches.
- **Ignoring FTS5 triggers:** When permanently deleting from `memories`, the existing FTS5 triggers handle cleanup automatically. Do not bypass with raw SQL that skips triggers.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Cron expression parsing | Custom parser for `*/5 * * * *` | `croner` crate | Edge cases: L/W/# specifiers, timezone, DST, Feb 29, leap seconds |
| Next-occurrence calculation | Manual date math | `croner::Cron::find_next_occurrence()` | Month length variation, DST transitions, weekday calculations |
| Systemd calendar conversion | Cron-to-OnCalendar translator | Template-based generation | Systemd OnCalendar format is different enough that direct translation is lossy. Generate from job metadata instead. |
| Job scheduling framework | Full scheduler with priority queues | Simple 60-second check loop | Blufio is single-instance. Check-every-minute is sufficient for cron resolution. Over-engineering adds no value. |

**Key insight:** The cron subsystem is intentionally simple because Blufio is a single-instance personal agent. There is no distributed scheduling, no job queue, no worker pool. One process, one scheduler loop, one SQLite writer.

## Common Pitfalls

### Pitfall 1: FTS5 Trigger Mismatch on Memory Deletion
**What goes wrong:** Permanently deleting memory rows without going through the table (e.g., using `DELETE FROM memories WHERE ...` on the FTS table directly) breaks the FTS5 index.
**Why it happens:** Memories have FTS5 triggers (memories_ai, memories_ad, memories_au) that keep the FTS index in sync.
**How to avoid:** Always delete from the `memories` table directly. The AFTER DELETE trigger on `memories` automatically cleans up `memories_fts`. Never touch `memories_fts` directly for deletion.
**Warning signs:** Search returning deleted memories, or FTS queries failing after retention sweep.

### Pitfall 2: Classification Column Not Checked in Retention
**What goes wrong:** Retention deletes Restricted data at the same rate as Internal data, violating RETN-05.
**Why it happens:** Forgetting to add `classification` to the WHERE clause in retention queries.
**How to avoid:** Every retention query MUST have classification-aware filtering. Test with Restricted records to verify separate treatment.
**Warning signs:** Restricted data disappearing at the same rate as Internal data.

### Pitfall 3: Concurrent Job Execution Race Condition
**What goes wrong:** Two scheduler ticks fire the same job simultaneously because the lock check and lock acquire are not atomic.
**Why it happens:** Reading `running = 0` then setting `running = 1` in separate statements.
**How to avoid:** Use atomic `UPDATE cron_jobs SET running = 1 WHERE name = ? AND running = 0` and check `changes()` == 1. This is an atomic test-and-set in SQLite.
**Warning signs:** Duplicate entries in cron_history for the same execution window.

### Pitfall 4: Audit Exemption Forgotten
**What goes wrong:** Retention sweep deletes audit entries, violating AUDT-05 and RETN-04.
**Why it happens:** Audit lives in a separate `audit.db` file, but if someone adds retention to "all tables" they might scan the audit DB too.
**How to avoid:** Retention logic operates ONLY on the main database connection. It has no access to audit.db. This is architectural isolation, not a WHERE clause.
**Warning signs:** Audit verification (`blufio audit verify`) showing missing entries after retention sweep.

### Pitfall 5: Soft-Delete Breaking Existing Queries
**What goes wrong:** After adding `deleted_at` column, existing SELECT queries return soft-deleted records.
**Why it happens:** Existing queries don't filter on `deleted_at IS NULL`.
**How to avoid:** Add `AND deleted_at IS NULL` to all existing read queries for messages, sessions, cost_ledger, and memories. This is a critical migration step that must happen atomically with adding the column.
**Warning signs:** Deleted messages appearing in conversation history, deleted memories returning in search.

### Pitfall 6: Migration Ordering with deny_unknown_fields
**What goes wrong:** Adding `CronConfig`/`RetentionConfig` to `BlufioConfig` without `#[serde(default)]` causes existing TOML files without those sections to fail parsing.
**Why it happens:** `deny_unknown_fields` rejects unknown fields, but missing optional sections need `#[serde(default)]` to default gracefully.
**How to avoid:** Always use `#[serde(default)]` on new config sections in BlufioConfig. This is the existing pattern for all sections.
**Warning signs:** `blufio serve` failing to start with "unknown field" or "missing field" errors after upgrade.

## Code Examples

### Cron Expression Parsing with croner
```rust
// Source: croner docs (https://docs.rs/croner/latest/croner/)
use std::str::FromStr;
use chrono::Utc;
use croner::Cron;

// Standard 5-field cron expression
let cron = Cron::from_str("0 2 * * *")  // Daily at 2:00 AM
    .map_err(|e| format!("Invalid cron expression: {e}"))?;

let now = Utc::now();

// Get next occurrence
if let Some(next) = cron.find_next_occurrence(&now, false) {
    println!("Next run: {next}");
}

// Iterate upcoming occurrences
let upcoming: Vec<_> = cron.iter_from(&now).take(5).collect();
```

### CronConfig TOML Model (following existing pattern)
```rust
// Source: blufio-config/model.rs pattern (ClassificationConfig, AuditConfig)

/// Cron scheduler configuration.
///
/// ```toml
/// [cron]
/// enabled = true
///
/// [[cron.jobs]]
/// name = "nightly-backup"
/// schedule = "0 2 * * *"
/// task = "backup"
/// enabled = true
///
/// [[cron.jobs]]
/// name = "retention-sweep"
/// schedule = "0 3 * * *"
/// task = "retention_enforcement"
/// enabled = true
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CronConfig {
    /// Whether the cron scheduler is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// Job timeout in seconds (default: 300 = 5 minutes).
    #[serde(default = "default_job_timeout_secs")]
    pub job_timeout_secs: u64,

    /// Maximum history entries to keep per job (default: 1000).
    #[serde(default = "default_max_history")]
    pub max_history: usize,

    /// Configured cron jobs.
    #[serde(default)]
    pub jobs: Vec<CronJobConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CronJobConfig {
    /// Unique job name.
    pub name: String,
    /// Cron expression (5-field POSIX format).
    pub schedule: String,
    /// Task to execute (must match a registered CronTask name).
    pub task: String,
    /// Whether this job is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}
```

### RetentionConfig TOML Model
```rust
/// Retention policy configuration.
///
/// ```toml
/// [retention]
/// enabled = true
/// grace_period_days = 7
///
/// [retention.periods]
/// messages = 90
/// sessions = 90
/// cost_records = 365
/// memories = 180
///
/// [retention.restricted]
/// messages = 30
/// sessions = 30
/// cost_records = 90
/// memories = 60
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionConfig {
    /// Whether retention enforcement is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// Grace period in days after soft-delete before permanent removal.
    #[serde(default = "default_grace_period_days")]
    pub grace_period_days: u64,

    /// Retention periods in days per data type (default classification).
    #[serde(default)]
    pub periods: RetentionPeriods,

    /// Separate retention periods for Restricted-classified data.
    #[serde(default)]
    pub restricted: RetentionPeriods,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionPeriods {
    /// Days before messages are soft-deleted (0 = no retention).
    #[serde(default)]
    pub messages: Option<u64>,
    /// Days before sessions are soft-deleted.
    #[serde(default)]
    pub sessions: Option<u64>,
    /// Days before cost records are soft-deleted.
    #[serde(default)]
    pub cost_records: Option<u64>,
    /// Days before memories are soft-deleted.
    #[serde(default)]
    pub memories: Option<u64>,
}
```

### SQLite Migration (V14)
```sql
-- V14: Cron scheduler and retention policy tables.

-- Cron job definitions (persisted for last-run tracking and CLI management).
CREATE TABLE IF NOT EXISTS cron_jobs (
    name TEXT PRIMARY KEY NOT NULL,
    schedule TEXT NOT NULL,        -- Cron expression
    task TEXT NOT NULL,            -- Task name (matches CronTask::name())
    enabled INTEGER NOT NULL DEFAULT 1,
    running INTEGER NOT NULL DEFAULT 0,  -- Single-instance lock
    last_run_at TEXT,             -- ISO 8601 timestamp
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Job execution history.
CREATE TABLE IF NOT EXISTS cron_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_name TEXT NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    status TEXT NOT NULL DEFAULT 'running',  -- 'running', 'success', 'failed', 'timeout'
    duration_ms INTEGER,
    output TEXT,                   -- Truncated output (max 4096 chars)
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_cron_history_job ON cron_history(job_name);
CREATE INDEX IF NOT EXISTS idx_cron_history_started ON cron_history(started_at);

-- Soft-delete columns for retention policies.
ALTER TABLE messages ADD COLUMN deleted_at TEXT;
ALTER TABLE sessions ADD COLUMN deleted_at TEXT;
ALTER TABLE cost_ledger ADD COLUMN deleted_at TEXT;
ALTER TABLE memories ADD COLUMN deleted_at TEXT;

CREATE INDEX IF NOT EXISTS idx_messages_deleted ON messages(deleted_at);
CREATE INDEX IF NOT EXISTS idx_sessions_deleted ON sessions(deleted_at);
CREATE INDEX IF NOT EXISTS idx_cost_ledger_deleted ON cost_ledger(deleted_at);
CREATE INDEX IF NOT EXISTS idx_memories_deleted ON memories(deleted_at);
```

### CronEvent for EventBus
```rust
// Source: blufio-bus/events.rs pattern (all event sub-enums use String fields)

/// Cron scheduler events.
///
/// All fields are `String` following the established pattern where event
/// sub-enums avoid cross-crate type dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CronEvent {
    /// A cron job completed execution.
    Completed {
        event_id: String,
        timestamp: String,
        job_name: String,
        status: String,      // "success", "failed", "timeout"
        duration_ms: u64,
    },
    /// A cron job failed.
    Failed {
        event_id: String,
        timestamp: String,
        job_name: String,
        error: String,
    },
}
```

### Systemd Timer Template
```rust
// Source: systemd.timer(5) manpage format
const TIMER_TEMPLATE: &str = r#"# Generated by blufio cron generate-timers
# Do not edit manually -- regenerate with: blufio cron generate-timers
[Unit]
Description=Blufio cron: {job_name}

[Timer]
OnCalendar={on_calendar}
AccuracySec=1min
Persistent=true

[Install]
WantedBy=timers.target
"#;

const SERVICE_TEMPLATE: &str = r#"# Generated by blufio cron generate-timers
[Unit]
Description=Blufio cron job: {job_name}

[Service]
Type=oneshot
ExecStart={blufio_path} cron run-now {job_name}
"#;
```

### CLI Subcommand Pattern
```rust
// Source: main.rs pattern (existing Subcommand enums)
/// Manage scheduled cron jobs.
#[command(
    after_help = "Examples:\n  blufio cron list\n  blufio cron add my-job \"0 */6 * * *\" backup\n  blufio cron remove my-job\n  blufio cron run-now retention_enforcement\n  blufio cron history --job backup --limit 20\n  blufio cron generate-timers --output-dir /etc/systemd/system"
)]
Cron {
    #[command(subcommand)]
    action: CronCommands,
},

#[derive(Subcommand, Debug)]
enum CronCommands {
    /// List all configured cron jobs.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Add a new cron job.
    Add {
        name: String,
        schedule: String,
        task: String,
    },
    /// Remove a cron job.
    Remove { name: String },
    /// Execute a job immediately.
    RunNow { name: String },
    /// Show job execution history.
    History {
        #[arg(long)]
        job: Option<String>,
        #[arg(long, default_value = "20")]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Generate systemd timer unit files.
    GenerateTimers {
        #[arg(long, default_value = ".")]
        output_dir: String,
    },
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `cron` crate (Quartz-style) | `croner` (POSIX-standard) | 2024 | Standard crontab syntax operators expect |
| External cron/systemd only | In-process + optional systemd export | N/A | Self-contained binary can schedule without external deps |
| Hard-delete only | Soft-delete + grace period | Standard practice | Allows recovery, GDPR compliance, audit trail preservation |
| Single retention policy | Per-type + per-classification policies | RETN-05 | Restricted data gets separate retention rules |

**Deprecated/outdated:**
- `cron` crate: Still maintained but uses non-standard 7-field format with year field and Quartz weekday numbering (1=SUN). Operators will be confused.
- `tokio-cron-scheduler`: Full framework with its own scheduler -- unnecessary when Blufio already has the tokio::spawn pattern.

## Open Questions

1. **Cron-to-OnCalendar conversion accuracy**
   - What we know: Cron and systemd OnCalendar use different syntax. Simple expressions translate directly.
   - What's unclear: Complex expressions (L, #, W) have no direct systemd equivalent.
   - Recommendation: Generate systemd timers from the schedule metadata (minute/hour/day/month/dow) rather than trying to convert the cron string literally. For unsupported patterns, fall back to a conservative schedule and emit a warning.

2. **Job timeout default value**
   - What we know: Tasks have varying execution times. Backup could take minutes; health check takes seconds.
   - What's unclear: What's a reasonable global default?
   - Recommendation: 300 seconds (5 minutes) global default, overridable per-job in TOML config. This matches the existing `eviction_sweep_interval_secs` pattern.

3. **Crate organization**
   - What we know: Project follows single-crate-per-concern pattern. Retention needs DB access (blufio-storage). Scheduler needs both.
   - What's unclear: Whether blufio-cron should depend on blufio-storage directly or use a trait.
   - Recommendation: Create `blufio-cron` crate that depends on `blufio-storage` directly (same pattern as `blufio-context` depending on `blufio-storage`). Retention queries live in blufio-cron since they're cron-specific; the `deleted_at` column migration and base soft-delete filtering go in blufio-storage.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[cfg(test)]` + `#[tokio::test]` |
| Config file | N/A -- standard Cargo test runner |
| Quick run command | `cargo test -p blufio-cron` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| CRON-01 | Parse cron expressions, validate syntax | unit | `cargo test -p blufio-cron -- cron_parse` | Wave 0 |
| CRON-02 | CLI list/add/remove/run-now/history | integration | `cargo test -p blufio -- cron_cli` | Wave 0 |
| CRON-03 | Generate valid systemd .timer files | unit | `cargo test -p blufio-cron -- systemd` | Wave 0 |
| CRON-04 | History persists in SQLite | unit | `cargo test -p blufio-cron -- history` | Wave 0 |
| CRON-05 | All 5 built-in tasks register and execute | unit | `cargo test -p blufio-cron -- builtin_tasks` | Wave 0 |
| CRON-06 | last_run_at survives restart (persisted in DB) | unit | `cargo test -p blufio-cron -- last_run_persists` | Wave 0 |
| RETN-01 | Config parsed with per-type periods | unit | `cargo test -p blufio-config -- retention_config` | Wave 0 |
| RETN-02 | Retention enforcement runs on schedule | unit | `cargo test -p blufio-cron -- retention_schedule` | Wave 0 |
| RETN-03 | Soft-delete + permanent delete after grace | unit | `cargo test -p blufio-cron -- soft_delete_grace` | Wave 0 |
| RETN-04 | Audit entries untouched by retention | unit | `cargo test -p blufio-cron -- audit_exempt` | Wave 0 |
| RETN-05 | Restricted data uses separate retention | unit | `cargo test -p blufio-cron -- classification_retention` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p blufio-cron`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before verification

### Wave 0 Gaps
- [ ] `crates/blufio-cron/` -- entire crate is new (create Cargo.toml, src/lib.rs)
- [ ] `crates/blufio-storage/migrations/V14__cron_retention.sql` -- new migration
- [ ] CronConfig and RetentionConfig in `blufio-config/model.rs` -- new config types
- [ ] CronEvent in `blufio-bus/events.rs` -- new event variant
- [ ] CronCommands in `blufio/src/main.rs` -- new CLI subcommand

## Sources

### Primary (HIGH confidence)
- croner docs (https://docs.rs/croner/latest/croner/) -- API surface, POSIX compliance, chrono integration
- Project codebase -- serve.rs background task pattern, model.rs config pattern, events.rs event pattern, migrations

### Secondary (MEDIUM confidence)
- crates.io croner (https://crates.io/crates/croner) -- version 3.0, active maintenance
- crates.io cron (https://crates.io/crates/cron) -- alternative considered, non-standard format
- croner-rust GitHub (https://github.com/Hexagon/croner-rust) -- POSIX compliance rationale
- systemd timer documentation (https://wiki.archlinux.org/title/Systemd/Timers) -- OnCalendar format

### Tertiary (LOW confidence)
- None -- all findings verified against official sources or project codebase

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- croner verified via docs.rs, all other deps already in workspace
- Architecture: HIGH -- follows established project patterns (background.rs, model.rs, events.rs)
- Pitfalls: HIGH -- identified from existing codebase (FTS5 triggers, classification, deny_unknown_fields)
- Retention design: HIGH -- two-phase soft-delete is well-understood pattern, audit exemption is architectural (separate DB)

**Research date:** 2026-03-12
**Valid until:** 2026-04-12 (stable domain, no fast-moving dependencies)
