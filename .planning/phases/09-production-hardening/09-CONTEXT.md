# Phase 9: Production Hardening - Context

**Gathered:** 2026-03-01
**Status:** Ready for planning

<domain>
## Phase Boundary

Make blufio run reliably as a production daemon on a $4/month VPS for months without restart, OOM, or security incident. Deliver systemd integration, memory bounds, Prometheus observability, full CLI diagnostics, and operational tooling. This is cross-cutting hardening of existing capabilities, not new features.

</domain>

<decisions>
## Implementation Decisions

### CLI Diagnostics
- `blufio status` displays a human-readable dashboard by default (agent state, active sessions, memory usage, cost summary, channel health); `--json` flag outputs structured JSON for scripts/monitoring
- `blufio doctor` runs quick connectivity checks by default (~2s: LLM API, DB, Telegram, gateway); `--deep` adds DB integrity, disk space, memory baseline, embedding model, vault (~10-15s)
- `blufio config get <key>` reads current config values; `blufio config validate` checks TOML file; no `config set` — users edit blufio.toml directly. `config set-secret` stays for vault items
- Output uses color + symbols (green checkmarks, red X's) with TTY detection; auto-plain when piped; `--plain` flag forces plain text. Reuse existing `colored` crate

### Daemon & systemd
- systemd integration with health check endpoint for readiness (Type=simple + health poll, avoids libsystemd build dependency, works with non-systemd too)
- Static systemd unit file shipped in repo (e.g., contrib/blufio.service); user copies to /etc/systemd/system/
- Restart=on-failure — only restart on non-zero exit; clean shutdown stays stopped
- `blufio serve` continues to work as foreground process for Docker/container use — no special daemon fork mode

### Memory Bounding
- Warn + shed caches strategy: emit Prometheus gauge + log warnings at 80% of limit; proactively clear caches (context summaries, embedding cache); stop accepting new sessions if still growing; don't force exit
- Track memory via both jemalloc stats (heap breakdown) and /proc/self/statm (total RSS); export both as Prometheus gauges (`blufio_memory_heap_bytes`, `blufio_memory_rss_bytes`)
- ONNX embedding model memory treated as fixed baseline — the 50-80MB idle / 100-200MB load targets are for dynamic allocations on top of the model weight

### Ops Scripts & Lifecycle
- `blufio backup <path>` and `blufio restore <path>` as CLI subcommands using SQLite backup API for atomicity
- Shell scripts shipped in contrib/ for logrotate config and lifecycle hooks
- Log rotation delegated to system — blufio logs to stdout/stderr, systemd captures via journald; ship logrotate.conf example for syslog setups (12-factor pattern)

### Claude's Discretion
- Whether sd_notify is also supported alongside health endpoint (optional, behind feature flag)
- Restart policy fine-tuning (RestartSec, StartLimitBurst)
- Exact memory limit thresholds and whether configurable in blufio.toml or hardcoded
- ONNX baseline measurement approach
- Backup scope (DB only vs DB + config + vault)
- Which lifecycle hooks to support (pre-start/post-stop vs full lifecycle)
- Doctor check ordering and timeout per check

</decisions>

<specifics>
## Specific Ideas

- The agent must survive months on a $4/month VPS (1GB RAM, 1 vCPU) — this is the target deployment environment
- Success criteria: `systemctl status blufio` shows healthy, idle memory 50-80MB, load memory 100-200MB, no unbounded growth over 72+ hours
- Device keypair authentication is required (no optional auth mode) per SEC-02
- Prometheus metrics must be scrapeable by standard Prometheus setup per COST-04

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `blufio-prometheus` crate: PrometheusAdapter with metrics-rs facade, renders Prometheus text format; existing metrics: messages_total, tokens_total, active_sessions, budget_remaining_usd, response_latency_seconds
- `blufio-auth-keypair` crate: Ed25519 DeviceKeypair with bearer token validation via AuthAdapter trait
- `colored` crate: Already used in shell.rs for colored output
- `clap` CLI: Commands enum in main.rs with serve/shell/config/skill/plugin subcommands — extend with status/doctor/backup/restore
- jemalloc: Already global allocator with tikv_jemallocator; tikv_jemalloc_ctl available for stats
- Signal handler: `shutdown::install_signal_handler()` with CancellationToken in serve.rs
- Crash recovery: `mark_stale_sessions()` already marks interrupted sessions on restart
- CostLedger: Tracks per-session costs with daily/monthly totals — source for status cost summary
- BudgetTracker: Budget utilization percentage available for status display

### Established Patterns
- Feature flags for conditional compilation (`#[cfg(feature = "prometheus")]`, etc.)
- Plugin adapter trait pattern (PluginAdapter → health_check, shutdown)
- Config model with `deny_unknown_fields` and serde defaults
- Tracing via `tracing` crate with configurable log levels (init_tracing in serve.rs)
- Gateway channel serves /metrics endpoint — extend for /health

### Integration Points
- CLI (main.rs Commands enum): Add Status, Doctor, Backup, Restore subcommands
- serve.rs: Add health endpoint, memory monitoring task, sd_notify signal
- blufio-prometheus/recording.rs: Add memory gauges (heap, RSS), error rate counter
- blufio-config/model.rs: Already has PrometheusConfig — may need memory config section
- Gateway /health endpoint: New route for systemd health checks

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 09-production-hardening*
*Context gathered: 2026-03-01*
