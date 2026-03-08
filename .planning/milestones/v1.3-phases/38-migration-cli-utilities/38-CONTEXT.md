# Phase 38: Migration & CLI Utilities - Context

**Gathered:** 2026-03-07
**Status:** Ready for planning

<domain>
## Phase Boundary

Users migrating from OpenClaw have a clear path, and operators have essential CLI tools for benchmarking, privacy auditing, config generation, cleanup, and air-gapped deployment. This phase delivers: `blufio migrate` (with preview and config translate), `blufio bench`, `blufio privacy evidence-report`, `blufio config recipe`, `blufio uninstall`, and `blufio bundle`.

</domain>

<decisions>
## Implementation Decisions

### Migration UX & data mapping
- Auto-detect OpenClaw data directory from known default paths (~/.openclaw, $OPENCLAW_HOME), allow `--data-dir` override; fail with helpful message if not found
- `migrate preview` IS the dry-run mechanism — no separate `--dry-run` flag on `migrate` itself
- Preview report uses categorized summary: Will Import, Needs Manual Attention, Cannot Import — with counts and estimated cost comparison (map OpenClaw cost records to Blufio provider pricing)
- `config translate` outputs a single complete `blufio.toml` file; unmappable fields become commented-out lines (`# UNMAPPED: field_name = value`) with a summary warning at the end
- Copy-only migration — never touch or modify the OpenClaw source directory; user deletes manually after verifying
- Auto-vault secrets found in OpenClaw config (API keys, tokens) — store directly in Blufio's encrypted vault, show what was vaulted in report
- Import ALL markdown files from OpenClaw workspace/personality dirs — map known files (SOUL.md -> personality), copy unknown ones to a blufio import directory
- Idempotent/resumable migration — track imported items, re-running `migrate` skips already-imported data, safe to retry on failure
- Progress bar (indicatif-style) during import, categorized summary at end; `--json` flag for machine output
- Session history referencing OpenClaw-specific unsupported features: import with metadata tags (source: openclaw, unmapped_features: [...]) — data preserved and searchable, just not actionable

### Benchmark scope & output
- Table output by default, `--json` for machine-parseable — consistent with existing status/doctor CLI patterns
- Selectable benchmarks: `blufio bench` runs all; `blufio bench --only startup,sqlite` runs specific ones
- Save results to SQLite; `--compare` shows delta from last run; `--baseline` saves current as reference point
- 3 iterations per benchmark, report median; `--iterations N` to override
- 1 warm-up run before measuring to warm caches
- Report system info in header: CPU model, RAM, OS version, Blufio version
- `--ci` mode exits non-zero if any benchmark exceeds baseline by >20% (configurable via `--threshold`)
- Measure peak RSS (memory) alongside timing for each benchmark
- Use synthetic deterministic dataset for context assembly benchmark (reproducible across machines)

### Privacy evidence report
- Full audit: outbound endpoints (API calls, webhook destinations), local data stores (DB, vault, logs, personality files), data retention policies, WASM skill permissions
- Static config analysis (parse config files, works without running server, deterministic and auditable)
- Generate markdown report; print to terminal by default; `--output path` saves to file; `--json` for machine-parseable
- Classify data types: PII (user messages), credentials (API keys), usage (cost/token counts), system (logs)
- Per-skill WASM permission breakdown: list each installed skill with its granted permissions (network, filesystem, env vars)
- Flag potential privacy concerns (e.g., "Skill X has network + message access — could exfiltrate conversations") — advisory, not blocking
- Include retention/deletion information for each data store: what's stored, retention period (if configured), how to delete
- `--output path` flag to save report; without it, prints to terminal only

### Bundle & air-gapped deployment
- Archive includes: blufio binary, current config (secrets excluded), installed WASM skills, manifest, and install.sh script
- install.sh copies binary to /usr/local/bin, installs service file, sets up config directory
- tar.gz format with .minisig signature alongside — universal, works on air-gapped Linux
- Current platform only (no cross-platform bundling)
- Verify Minisign signature of current binary before bundling — refuse if unsigned or invalid
- Optional `--include-data` flag to add SQLite database backup to bundle for full state transfer

### Uninstall behavior
- Always remove: binary, systemd/launchd service files, shell completions
- Interactively ask about data removal; `--purge` flag skips prompt and removes everything
- Auto-backup to timestamped archive before purging data — print backup path as last-chance recovery
- Detect and stop running Blufio processes (via systemd/launchd or PID) gracefully before uninstalling; refuse if can't stop

### Config recipe
- `blufio config recipe personal|team|production|iot` generates commented TOML template with sensible defaults
- Generate relevant subset only (personal: agent + one channel + anthropic; production: all channels + rate limits + monitoring) — not the full config schema

### Claude's Discretion
- Exact progress bar styling and animation
- Internal migration data tracking schema
- Benchmark warm-up implementation details
- Privacy report markdown template/styling
- install.sh platform detection logic
- Config recipe default values per preset

</decisions>

<specifics>
## Specific Ideas

- Migration should feel safe and non-destructive — "preview first, import second, never touch the source"
- Benchmark output should feel like `hyperfine` — clean table with timing, comparison deltas, and system context
- Privacy report should be compliance-ready — could be handed to a GDPR auditor or security reviewer
- Bundle is for air-gapped/isolated environments — everything needed to deploy without internet access
- Uninstall should be the opposite of the update/install flow — clean, reversible, safe

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `clap` CLI with `Parser`/`Subcommand` pattern: main.rs already has Serve, Shell, Status, Doctor, Backup, Restore, Config, Skill, Plugin, Verify, Update, Healthcheck commands — new commands follow same pattern
- `--json` / `--plain` flags already used on Status and Doctor commands — bench/privacy can follow this convention
- `blufio-vault` crate has `migrate_plaintext_secrets()` — pattern for auto-vaulting OpenClaw secrets
- `blufio-storage` has SQLite migrations infrastructure — bench results storage can use this
- Minisign verify command already exists — bundle can reuse signature verification logic
- `blufio-config::model::BlufioConfig` — complete TOML config model for config translate target schema
- Backup/Restore commands exist — uninstall auto-backup can reuse backup logic

### Established Patterns
- All CLI commands are separate module files in `crates/blufio/src/` (backup.rs, doctor.rs, etc.)
- Config management uses clap subcommands (ConfigCommands enum)
- Storage layer uses SQLite with migration versioning
- Vault handles encrypted secret storage with key derivation

### Integration Points
- New commands added to `Commands` enum in main.rs
- Migration needs read access to blufio-storage (sessions), blufio-cost (ledger), blufio-vault (secrets)
- Bench needs access to blufio-storage, blufio-context (context assembly), blufio-skill (WASM)
- Privacy report needs to inspect blufio-config, blufio-skill permissions, blufio-storage schema
- Bundle needs access to binary path, config files, skill directory
- Uninstall needs platform-specific service management (systemd/launchd)

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 38-migration-cli-utilities*
*Context gathered: 2026-03-07*
