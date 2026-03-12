# Phase 58: Cron Scheduler & Retention Policies - Context

**Gathered:** 2026-03-12
**Status:** Ready for planning

<domain>
## Phase Boundary

Blufio runs background tasks on configurable schedules, and data is automatically pruned according to retention rules. Operator defines cron jobs in TOML, manages via `blufio cron` CLI, generates systemd timer files, and configures per-type retention policies with soft-delete and grace period. 11 requirements: CRON-01 through CRON-06, RETN-01 through RETN-05.

</domain>

<decisions>
## Implementation Decisions

### Scheduler architecture
- In-process tokio-based scheduler running inside `blufio serve` — follows existing background task pattern (heartbeat, memory eviction, webhook delivery all use `tokio::spawn` in serve.rs)
- Cron expression parsing via a Rust crate (e.g., cron or croner) — no custom parser
- `blufio cron generate-timers` produces systemd timer/service unit files as an ALTERNATIVE external scheduling option — does not require systemd at runtime
- Last-run timestamps stored in SQLite (new `cron_jobs` table) to survive restarts
- Job execution is single-instance (no concurrent runs of same job) — use a lock flag in the DB row

### Retention deletion strategy
- Soft-delete via `deleted_at` timestamp column added to messages, sessions, cost_records, memories tables
- Grace period is configurable per type (default: 7 days) — after grace period, permanent DELETE runs
- Two-phase enforcement: mark soft-delete → later sweep permanently deletes expired soft-deletes
- Audit trail entries (blufio-audit) are EXEMPT from retention — RETN-04 requirement
- Retention respects data classification — Restricted data can have separate (shorter or longer) retention rules per RETN-05

### Built-in cron tasks
- 5 built-in tasks registered at startup (CRON-05): memory_cleanup, backup, cost_report, health_check, retention_enforcement
- memory_cleanup: calls existing eviction logic in blufio-memory (already has `eviction_sweep_interval_secs`)
- backup: runs `Database::backup()` to configured backup path
- cost_report: aggregates cost records and logs summary (no external delivery — that's hooks/channels)
- health_check: runs doctor-lite checks (DB connectivity, disk space, memory usage)
- retention_enforcement: runs soft-delete sweep + permanent deletion of expired records
- All built-in tasks are disabled by default — operator enables via TOML config

### Job execution tracking
- New `cron_history` SQLite table: job_name, started_at, finished_at, status (success/failed/timeout), duration_ms, output (truncated)
- `blufio cron history` CLI shows recent executions in table format (--json for programmatic)
- Failed jobs emit CronEvent on EventBus (follows existing event patterns) — enables future hook integration (Phase 59)
- No automatic retry — jobs run on schedule, failures are logged. Operator can `blufio cron run-now <job>` to manually retry

### CLI design
- `blufio cron list` — show all configured jobs with next-run time and last status
- `blufio cron add <name> <expression> <task>` — add custom job via CLI (also configurable in TOML)
- `blufio cron remove <name>` — remove a job
- `blufio cron run-now <name>` — execute immediately regardless of schedule
- `blufio cron history [--job <name>] [--limit N] [--json]` — show execution history
- `blufio cron generate-timers [--output-dir <path>]` — write systemd .timer + .service files
- Follows existing CLI pattern: clap derive with Subcommand enum in main.rs

### Claude's Discretion
- Exact cron expression parsing library choice (cron vs croner vs other)
- Schema design details for cron_jobs and cron_history tables
- Exact systemd timer template format
- Job timeout default value
- Whether to create a new blufio-cron crate or add to existing blufio crate

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `tokio::spawn` background task pattern in serve.rs (heartbeat, memory eviction, webhook delivery, degradation manager)
- blufio-memory::background::spawn_background_task — existing background sweep pattern
- EventBus (blufio-bus) with 15 event variants — add CronEvent as 16th
- blufio-config/src/model.rs — TOML config model pattern with serde + deny_unknown_fields
- clap derive Subcommand pattern in main.rs — 20+ existing subcommands

### Established Patterns
- Config types defined inline in blufio-config/model.rs, re-exported from feature crate
- Event sub-enums use String fields to avoid cross-crate dependencies
- Background tasks use tokio::spawn with interval-based loops in serve.rs
- CLI subcommands use clap derive with #[command(subcommand)] nesting
- SQLite tables via rusqlite with tokio-rusqlite single-writer pattern
- Doctor checks: add cron health check following existing doctor.rs pattern

### Integration Points
- serve.rs: spawn cron scheduler after EventBus init, before main loop
- main.rs: add Cron { action: CronCommands } to Commands enum
- blufio-config/model.rs: add CronConfig and RetentionConfig sections
- blufio-bus/events.rs: add CronEvent variant to BusEvent enum
- blufio-storage or new crate: retention enforcement logic needs DB access
- doctor.rs: add cron/retention health check

</code_context>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches. Project conventions strongly favor:
- Single-crate-per-concern (likely new blufio-cron crate or add retention to blufio-storage)
- TOML config with figment, deny_unknown_fields
- SQLite for all persistence (no external scheduler dependency)
- EventBus integration for observability

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 58-cron-scheduler-retention-policies*
*Context gathered: 2026-03-12*
