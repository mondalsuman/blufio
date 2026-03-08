# Phase 38: Migration & CLI Utilities - Research

**Researched:** 2026-03-07
**Domain:** OpenClaw migration tooling, CLI benchmarking, privacy auditing, config generation, uninstall, and air-gapped bundling
**Confidence:** HIGH

## Summary

Phase 38 delivers two categories of CLI functionality: (1) migration tools for users coming from OpenClaw (`migrate`, `migrate preview`, `config translate`), and (2) operational CLI utilities (`bench`, `privacy evidence-report`, `config recipe`, `uninstall`, `bundle`). Both categories follow the established clap subcommand pattern used throughout the `crates/blufio/src/` module files (backup.rs, doctor.rs, status.rs, verify.rs, etc.).

The codebase already has strong foundations for every feature in this phase:
- **Migration:** `blufio-storage` has SQLite session/message storage, `blufio-cost` has CostLedger for cost records, `blufio-vault` has `migrate_plaintext_secrets()` for auto-vaulting API keys, `blufio-config` has complete `BlufioConfig` TOML model as the translation target
- **Benchmarking:** `blufio-storage` has `Database::open()` and migration infrastructure, `blufio-context` has context assembly, `blufio-skill` has WASM loading -- all benchmarkable operations
- **Privacy:** `blufio-config::model::BlufioConfig` contains all outbound endpoints, `blufio-skill` has permission declarations, `blufio-storage` has schema inspection
- **Bundle:** `blufio-verify` has Minisign signature verification, existing binary path detection from `update.rs`
- **Uninstall:** `backup.rs` has atomic backup logic, platform service detection patterns exist

**Primary recommendation:** Add new module files in `crates/blufio/src/` following existing patterns. Migration logic goes in `migrate.rs`, bench in `bench.rs`, privacy in `privacy.rs`, bundle in `bundle.rs`, uninstall in `uninstall.rs`. Config recipe and config translate extend the existing `ConfigCommands` enum. No new crates needed -- all functionality is CLI-layer orchestration of existing crate APIs.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Auto-detect OpenClaw data directory from known default paths (~/.openclaw, $OPENCLAW_HOME), allow `--data-dir` override
- `migrate preview` IS the dry-run mechanism -- no separate `--dry-run` flag
- Preview report: Will Import, Needs Manual Attention, Cannot Import categories with counts and estimated cost comparison
- `config translate` outputs complete `blufio.toml`; unmappable fields become commented-out lines
- Copy-only migration -- never touch or modify OpenClaw source directory
- Auto-vault secrets found in OpenClaw config (API keys, tokens)
- Import ALL markdown files from OpenClaw workspace/personality dirs
- Idempotent/resumable migration -- track imported items, re-running skips already-imported
- Progress bar (indicatif-style) during import; `--json` flag for machine output
- Session history with unsupported features: import with metadata tags
- Bench: table output by default, `--json` for machine-parseable
- Bench: selectable benchmarks via `--only`; save results to SQLite; `--compare` and `--baseline`
- Bench: 3 iterations median, 1 warm-up, report system info, `--ci` mode, measure peak RSS
- Privacy: full audit of outbound endpoints, local data stores, WASM skill permissions
- Privacy: static config analysis (works without running server); markdown report; `--json`; `--output path`
- Privacy: per-skill WASM permission breakdown with advisory flags
- Bundle: tar.gz with .minisig signature; current platform only; verify binary signature before bundling
- Bundle: includes binary, config (secrets excluded), WASM skills, manifest, install.sh
- Uninstall: always remove binary, service files, shell completions; interactively ask about data
- Uninstall: `--purge` flag; auto-backup before purging; detect and stop running processes
- Config recipe: `personal|team|production|iot` presets with relevant subset only

### Claude's Discretion
- Exact progress bar styling and animation
- Internal migration data tracking schema
- Benchmark warm-up implementation details
- Privacy report markdown template/styling
- install.sh platform detection logic
- Config recipe default values per preset

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| MIGR-01 | blufio migrate --from-openclaw reads OpenClaw data directory | Auto-detect ~/.openclaw or $OPENCLAW_HOME; `--data-dir` override; parse OpenClaw JSON config and SQLite data |
| MIGR-02 | Migration imports session history and cost records to SQLite | `blufio-storage` session/message queries; `blufio-cost` CostLedger::record(); idempotent tracking via migration_log table |
| MIGR-03 | Migration imports workspace personality files (SOUL.md, AGENTS.md, USER.md, etc.) | Copy markdown files to Blufio data directory; map known files to personality system; unknown files to import directory |
| MIGR-04 | blufio migrate preview shows dry-run report | Categorized summary: Will Import / Needs Manual Attention / Cannot Import; estimated cost comparison mapping OpenClaw costs to Blufio provider pricing |
| MIGR-05 | blufio config translate maps OpenClaw JSON to Blufio TOML | Parse OpenClaw JSON config; map fields to `BlufioConfig` struct; serialize to TOML; unmappable fields as comments |
| CLI-01 | blufio bench runs built-in benchmarks | Startup timing, context assembly (`blufio-context`), WASM loading (`blufio-skill`), SQLite operations (`blufio-storage`); `sysinfo` for system info and RSS |
| CLI-02 | blufio privacy evidence-report enumerates data flows and stores | Parse `BlufioConfig` for outbound endpoints; inspect `blufio-skill` permissions; enumerate storage schema; classify data types |
| CLI-03 | blufio config recipe generates config templates | Template generation per preset (personal/team/production/iot); write commented TOML with sensible defaults |
| CLI-04 | blufio uninstall removes binary, service files, optionally data | Platform-specific service detection (systemd/launchd); `backup.rs` logic for auto-backup; process detection via PID/service manager |
| CLI-05 | blufio bundle creates Minisign-signed air-gapped deployment archive | `blufio-verify` for signature verification; tar.gz creation; install.sh generation; manifest with checksums |
</phase_requirements>

## Standard Stack

This phase uses exclusively existing project dependencies:
- **clap** — CLI argument parsing (already used for all commands)
- **serde/serde_json** — OpenClaw JSON config parsing
- **toml** — Blufio TOML config serialization (already used by blufio-config)
- **rusqlite/tokio-rusqlite** — SQLite operations (already used by blufio-storage)
- **indicatif** — Progress bars (new dependency, lightweight)
- **sysinfo** — System info for bench and privacy (already used by node system)
- **flate2 + tar** — tar.gz creation for bundle (new dependencies, standard)
- **minisign** — Signature verification (already used by blufio-verify)

## Existing Patterns to Follow

### CLI Command Pattern
Each command is a separate module file in `crates/blufio/src/`:
```
mod migrate;  // new
mod bench;    // new
mod privacy;  // new
mod bundle;   // new
mod uninstall; // new
```

Commands are added to the `Commands` enum in `main.rs` with clap derive macros. Each module exports a `run_*` async function matching the pattern in `doctor.rs`, `backup.rs`, `status.rs`.

### Config Subcommand Pattern
`config translate` and `config recipe` extend the existing `ConfigCommands` enum:
```rust
enum ConfigCommands {
    SetSecret { ... },
    ListSecrets,
    Get { ... },
    Validate,
    Translate { ... },  // new
    Recipe { ... },     // new
}
```

### Output Convention
- Status messages to stderr (`eprintln!`)
- Final result to stdout (`println!`)
- `--json` flag for machine-parseable output (serde_json::to_string_pretty)
- `--plain` flag to disable colored output
- Exit code 0 on success, 1 on failure

### Error Handling
All functions return `Result<(), BlufioError>`. New error variants added to `BlufioError` enum in blufio-core as needed.

## Architecture Decisions

### Migration Data Tracking
Create a `migration_log` table in the existing SQLite database to track imported items:
```sql
CREATE TABLE IF NOT EXISTS migration_log (
    id INTEGER PRIMARY KEY,
    source TEXT NOT NULL,           -- 'openclaw'
    item_type TEXT NOT NULL,        -- 'session', 'cost_record', 'personality_file', 'secret'
    source_id TEXT NOT NULL,        -- original ID/path in source system
    imported_at TEXT NOT NULL,      -- ISO 8601 timestamp
    UNIQUE(source, item_type, source_id)
);
```
This enables idempotent migration -- re-running checks UNIQUE constraint before inserting.

### Benchmark Results Storage
Add a `bench_results` table:
```sql
CREATE TABLE IF NOT EXISTS bench_results (
    id INTEGER PRIMARY KEY,
    benchmark TEXT NOT NULL,
    median_ns INTEGER NOT NULL,
    peak_rss_bytes INTEGER,
    iterations INTEGER NOT NULL,
    system_info TEXT NOT NULL,      -- JSON blob
    is_baseline INTEGER DEFAULT 0,
    created_at TEXT NOT NULL
);
```

### OpenClaw Config Mapping
OpenClaw uses JSON config. Key mappings:
- `openclaw.api_key` -> vault secret `anthropic.api_key`
- `openclaw.model` -> `agent.model`
- `openclaw.system_prompt` -> personality file reference
- `openclaw.max_tokens` -> `agent.max_tokens`
- Channel-specific configs mapped to corresponding Blufio channel configs
- Unknown fields -> commented-out TOML lines with `# UNMAPPED:` prefix

### Bundle Manifest Format
```toml
[bundle]
version = "1.0"
created = "2026-03-07T..."
platform = "aarch64-apple-darwin"
blufio_version = "1.3.0"

[contents]
binary = "blufio"
config = "blufio.toml"
skills = ["skill1.wasm", "skill2.wasm"]
```

## Risk Assessment

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| OpenClaw data format changes | Low | Version-detect and fail gracefully with clear error |
| Large migration datasets | Medium | Streaming/batched import with progress bar; resumable via migration_log |
| Platform-specific uninstall differences | Medium | Detect platform at runtime; skip inapplicable steps with warnings |
| Benchmark reproducibility | Low | Synthetic deterministic dataset for context assembly; warm-up runs |

## Plan Recommendations

**Plan 38-01: OpenClaw Migration** (Wave 1)
- Migrate command with `--from-openclaw`, preview, and config translate
- Requirements: MIGR-01, MIGR-02, MIGR-03, MIGR-04, MIGR-05

**Plan 38-02: CLI Utilities** (Wave 1, parallel with 38-01)
- Bench, privacy evidence-report, config recipe, uninstall, bundle
- Requirements: CLI-01, CLI-02, CLI-03, CLI-04, CLI-05

Both plans can execute in parallel (Wave 1) since they touch different modules and have no cross-dependencies.

---

## RESEARCH COMPLETE
