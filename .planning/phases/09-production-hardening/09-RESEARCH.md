# Phase 9: Production Hardening - Research

**Researched:** 2026-03-01
**Domain:** Daemon operations, memory management, observability, CLI diagnostics
**Confidence:** HIGH

## Summary

Phase 9 hardens blufio for production use on a $4/month VPS (1GB RAM, 1 vCPU). The work is cross-cutting: no new crates, only extending existing infrastructure. Five areas: (1) systemd integration with health endpoint, (2) memory bounding via jemalloc stats + /proc/self/statm, (3) Prometheus memory/error gauges, (4) CLI diagnostics (status/doctor/config subcommands), (5) operational scripts (backup/restore/logrotate/lifecycle hooks).

The existing codebase provides strong foundations: jemalloc is already the global allocator with tikv_jemalloc_ctl available, blufio-prometheus already has metrics-rs facade with counters/gauges/histograms, blufio-auth-keypair has Ed25519 auth, and the CLI uses clap with a clean Commands enum. The gateway's axum server already serves /v1/health -- extending it with a standalone /health (unauthenticated) endpoint for systemd polling is straightforward.

**Primary recommendation:** Implement in 3 waves: (1) health endpoint + memory monitoring + Prometheus gauges (foundations), (2) CLI diagnostics + systemd unit file, (3) backup/restore + ops scripts. This order ensures Wave 2 can test against Wave 1's health endpoint.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- `blufio status` displays a human-readable dashboard by default (agent state, active sessions, memory usage, cost summary, channel health); `--json` flag outputs structured JSON for scripts/monitoring
- `blufio doctor` runs quick connectivity checks by default (~2s: LLM API, DB, Telegram, gateway); `--deep` adds DB integrity, disk space, memory baseline, embedding model, vault (~10-15s)
- `blufio config get <key>` reads current config values; `blufio config validate` checks TOML file; no `config set` -- users edit blufio.toml directly. `config set-secret` stays for vault items
- Output uses color + symbols (green checkmarks, red X's) with TTY detection; auto-plain when piped; `--plain` flag forces plain text. Reuse existing `colored` crate
- systemd integration with health check endpoint for readiness (Type=simple + health poll, avoids libsystemd build dependency, works with non-systemd too)
- Static systemd unit file shipped in repo (e.g., contrib/blufio.service); user copies to /etc/systemd/system/
- Restart=on-failure -- only restart on non-zero exit; clean shutdown stays stopped
- `blufio serve` continues to work as foreground process for Docker/container use -- no special daemon fork mode
- Warn + shed caches strategy: emit Prometheus gauge + log warnings at 80% of limit; proactively clear caches (context summaries, embedding cache); stop accepting new sessions if still growing; don't force exit
- Track memory via both jemalloc stats (heap breakdown) and /proc/self/statm (total RSS); export both as Prometheus gauges (`blufio_memory_heap_bytes`, `blufio_memory_rss_bytes`)
- ONNX embedding model memory treated as fixed baseline -- the 50-80MB idle / 100-200MB load targets are for dynamic allocations on top of the model weight
- `blufio backup <path>` and `blufio restore <path>` as CLI subcommands using SQLite backup API for atomicity
- Shell scripts shipped in contrib/ for logrotate config and lifecycle hooks
- Log rotation delegated to system -- blufio logs to stdout/stderr, systemd captures via journald; ship logrotate.conf example for syslog setups (12-factor pattern)

### Claude's Discretion
- Whether sd_notify is also supported alongside health endpoint (optional, behind feature flag)
- Restart policy fine-tuning (RestartSec, StartLimitBurst)
- Exact memory limit thresholds and whether configurable in blufio.toml or hardcoded
- ONNX baseline measurement approach
- Backup scope (DB only vs DB + config + vault)
- Which lifecycle hooks to support (pre-start/post-stop vs full lifecycle)
- Doctor check ordering and timeout per check

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| CORE-04 | Agent runs as background daemon, auto-restarts on crash via systemd | systemd unit file with Type=simple + health poll, Restart=on-failure |
| CORE-07 | Idle memory stays within 50-80MB (including embedding model weights) | jemalloc stats via tikv_jemalloc_ctl + /proc/self/statm RSS tracking |
| CORE-08 | Memory under load stays within 100-200MB with no unbounded growth | Memory monitor task with warn+shed strategy, cache eviction |
| COST-04 | Prometheus metrics endpoint exports token usage, latency percentiles, error rates, memory usage | Extend blufio-prometheus recording.rs with memory gauges, error counter |
| SEC-02 | Device keypair authentication required -- no optional auth mode | Enforce keypair auth in gateway, health endpoint exempt |
| CLI-02 | `blufio status` shows running agent state, active sessions, memory usage, cost summary | New Status subcommand querying health endpoint + local DB |
| CLI-03 | `blufio config get/set/set-secret/validate` manages configuration | Extend Config subcommands with get/validate, keep set-secret |
| CLI-04 | `blufio doctor` runs diagnostics: LLM connectivity, DB integrity, channel status | New Doctor subcommand with quick + deep modes |
| CLI-07 | systemd unit file with health checks and auto-restart | contrib/blufio.service with ExecStartPost health poll |
| CLI-08 | Shell automation scripts for backup, log rotation, and lifecycle hooks | contrib/ scripts + Backup/Restore CLI subcommands |
</phase_requirements>

## Standard Stack

### Core (Already in Workspace)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tikv-jemalloc-ctl | 0.6 | Heap introspection (allocated, resident, mapped bytes) | Already global allocator; ctl crate provides `epoch::advance()` + `stats::*` |
| metrics + metrics-exporter-prometheus | 0.24 / 0.16 | Prometheus gauge/counter/histogram facade | Already installed and recording metrics |
| axum | 0.8 | Health endpoint route in gateway | Already the gateway HTTP framework |
| colored | 2 | TTY-aware colored CLI output | Already used in shell.rs |
| clap | 4.5 | CLI subcommand parsing | Already the CLI framework |
| rusqlite | 0.37 | SQLite backup API | Already the DB driver; backup module available |
| tokio | 1 | Async runtime, timers, spawning memory monitor | Already the runtime |

### Supporting (New Additions)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| sysinfo | 0.33 | Cross-platform RSS/memory reading (fallback for non-Linux) | Only if /proc/self/statm unavailable; macOS dev compat |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| sysinfo for memory | Raw /proc/self/statm parsing | /proc parsing is Linux-only; sysinfo works on macOS for dev but adds 300KB |
| sd_notify crate | No dependency (health poll only) | User decided: health poll primary, sd_notify optional behind feature flag |
| Custom health format | JSON structured health | Simple "OK"/"DEGRADED"/"UNHEALTHY" string is sufficient for systemd ExecStartPost |

## Architecture Patterns

### Health Endpoint Pattern
**What:** Unauthenticated GET /health endpoint returning JSON health status, separate from authenticated /v1/health.
**When to use:** systemd health polling, external monitoring, load balancers.
**Example:**
```rust
// In gateway server.rs, add unauthenticated route
let health_routes = Router::new()
    .route("/health", get(health_handler))
    .route("/metrics", get(metrics_handler))
    .with_state(state.clone());

async fn health_handler(State(state): State<HealthState>) -> impl IntoResponse {
    let status = state.health_check().await;
    let code = match &status.status {
        "healthy" => StatusCode::OK,
        "degraded" => StatusCode::OK,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    (code, Json(status))
}
```

### Memory Monitor Background Task
**What:** Periodic tokio task that reads jemalloc stats + RSS, exports to Prometheus, and triggers cache shedding at thresholds.
**When to use:** Always running in serve mode.
**Example:**
```rust
// jemalloc stats
tikv_jemalloc_ctl::epoch::advance().unwrap();
let allocated = tikv_jemalloc_ctl::stats::allocated::read().unwrap();
let resident = tikv_jemalloc_ctl::stats::resident::read().unwrap();

// /proc/self/statm RSS (Linux)
let statm = std::fs::read_to_string("/proc/self/statm").ok();
let rss_pages = statm.and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok());
let rss_bytes = rss_pages.map(|p| p * 4096); // page size = 4096

// Export to Prometheus
metrics::gauge!("blufio_memory_heap_bytes").set(allocated as f64);
metrics::gauge!("blufio_memory_rss_bytes").set(rss_bytes.unwrap_or(0) as f64);
```

### CLI Status via Health Endpoint
**What:** `blufio status` connects to the running instance's health endpoint to display live data.
**When to use:** When the daemon is running. Falls back to DB-only info if health endpoint unreachable.
**Example:**
```rust
// Try health endpoint first (live data)
let client = reqwest::Client::new();
match client.get("http://127.0.0.1:3000/health").send().await {
    Ok(resp) => display_live_status(resp.json().await?),
    Err(_) => display_offline_status(&config),  // DB-only fallback
}
```

### Diagnostic Runner Pattern
**What:** Each doctor check is a function returning `CheckResult { name, status, message, duration }`. Checks run sequentially with timeout per check.
**When to use:** `blufio doctor` and `blufio doctor --deep`.
```rust
struct CheckResult {
    name: String,
    status: CheckStatus, // Pass, Warn, Fail
    message: String,
    duration: Duration,
}

async fn check_llm_connectivity(config: &BlufioConfig) -> CheckResult { ... }
async fn check_db_integrity(config: &BlufioConfig) -> CheckResult { ... }
async fn check_telegram(config: &BlufioConfig) -> CheckResult { ... }
```

### Anti-Patterns to Avoid
- **Forking daemon:** Don't use double-fork or daemonize crate. systemd manages the process lifecycle; blufio is Type=simple (foreground).
- **libsystemd dependency:** Don't link to libsystemd for sd_notify. Use health endpoint polling for readiness. If sd_notify needed later, use a pure-Rust implementation behind a feature flag.
- **Memory hard-kill:** Don't force-exit on memory threshold. The warn+shed strategy is more resilient -- clear caches, stop accepting new sessions, but let existing sessions complete.
- **Blocking health checks:** Health endpoint must respond in <100ms. Don't include expensive checks (DB queries, LLM calls) in the health endpoint. Doctor handles deep diagnostics.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Memory RSS reading | Custom /proc parser | sysinfo crate (or conditional /proc/self/statm) | Cross-platform, handles edge cases |
| SQLite backup | Manual file copy | rusqlite::backup module (Backup::new_with_name) | Atomic, consistent, handles WAL correctly |
| Config validation | Manual TOML parsing | figment + serde(deny_unknown_fields) already works | Already implemented; config validate just loads and reports |
| TTY detection | Custom isatty | std::io::IsTerminal (already used in main.rs) | Standard library trait |
| Process info | /proc/self/stat parsing | std::process::id() for PID, jemalloc for heap | More reliable across platforms |

## Common Pitfalls

### Pitfall 1: jemalloc Epoch Staleness
**What goes wrong:** jemalloc stats are stale until epoch is advanced. Reading stats without `epoch::advance()` returns cached values.
**Why it happens:** jemalloc uses lazy stat collection for performance.
**How to avoid:** Always call `tikv_jemalloc_ctl::epoch::advance()` before reading any stats.
**Warning signs:** Memory stats that never change.

### Pitfall 2: Health Endpoint Auth Lockout
**What goes wrong:** Health endpoint behind auth means systemd can't poll it without token. But shipping token in unit file is a security issue.
**Why it happens:** Applying auth middleware to all routes.
**How to avoid:** Health endpoint (/health) is unauthenticated (returns limited info: status + uptime). Detailed /v1/health stays authenticated.
**Warning signs:** systemd health checks always fail.

### Pitfall 3: Memory Monitor Contention
**What goes wrong:** Memory monitor task holds locks that block request processing.
**Why it happens:** Reading jemalloc stats or checking cache sizes requires locks.
**How to avoid:** jemalloc epoch::advance() is thread-safe and non-blocking. Use try_lock for cache size checks. Memory monitor runs on its own interval, not in request path.

### Pitfall 4: SQLite Backup During Active Writes
**What goes wrong:** Backup captures inconsistent state if writes happen concurrently.
**Why it happens:** SQLite backup API handles this correctly (it retries on WAL changes), but the backup call can take longer.
**How to avoid:** Use rusqlite's Backup API which handles WAL mode correctly. Set a reasonable page count per step (e.g., 100 pages) to avoid blocking the writer thread.

### Pitfall 5: Config `deny_unknown_fields` Breaking Doctor
**What goes wrong:** Adding new config fields in the doctor output format that don't exist in the config struct.
**Why it happens:** `deny_unknown_fields` rejects anything not in the struct.
**How to avoid:** Doctor reads config via the existing loader. Config get/validate use the same path. Don't add runtime-only fields to the TOML model.

### Pitfall 6: RSS vs Heap Confusion
**What goes wrong:** RSS includes memory-mapped files (ONNX model, shared libs) which inflates the number way beyond heap usage.
**Why it happens:** /proc/self/statm RSS includes everything mapped into the process.
**How to avoid:** Track both metrics separately. The ONNX model is ~45MB in RSS but 0 in jemalloc heap. Success criteria targets are for dynamic allocations (jemalloc heap), not total RSS.

## Code Examples

### jemalloc Stats Reading
```rust
use tikv_jemalloc_ctl::{epoch, stats};

fn read_jemalloc_stats() -> (usize, usize, usize) {
    epoch::advance().unwrap();
    let allocated = stats::allocated::read().unwrap(); // bytes actively used
    let resident = stats::resident::read().unwrap();   // bytes mapped by OS
    let mapped = stats::mapped::read().unwrap();       // bytes in jemalloc arenas
    (allocated, resident, mapped)
}
```

### Linux RSS via /proc/self/statm
```rust
fn read_rss_bytes() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let rss_pages = statm.split_whitespace().nth(1)?.parse::<u64>().ok()?;
    let page_size = 4096u64; // sysconf(_SC_PAGESIZE) on Linux
    Some(rss_pages * page_size)
}
```

### rusqlite Backup API
```rust
use rusqlite::{Connection, backup};

fn backup_database(src_path: &str, dst_path: &str) -> rusqlite::Result<()> {
    let src = Connection::open(src_path)?;
    let mut dst = Connection::open(dst_path)?;
    let backup = backup::Backup::new(&src, &mut dst)?;
    // Copy 100 pages per step, sleep 10ms between steps
    backup.run_to_completion(100, std::time::Duration::from_millis(10), None)
}
```

### Colored CLI Output with TTY Detection
```rust
use colored::*;
use std::io::IsTerminal;

fn print_check(name: &str, passed: bool, msg: &str, plain: bool) {
    if plain || !std::io::stdout().is_terminal() {
        let status = if passed { "PASS" } else { "FAIL" };
        println!("[{status}] {name}: {msg}");
    } else {
        let symbol = if passed { "✓".green() } else { "✗".red() };
        let name_colored = if passed { name.normal() } else { name.red() };
        println!("  {symbol} {name_colored}: {msg}");
    }
}
```

### systemd Unit File
```ini
# contrib/blufio.service
[Unit]
Description=Blufio AI Agent
Documentation=https://github.com/blufio/blufio
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/blufio serve
ExecStartPost=/bin/sh -c 'for i in 1 2 3 4 5 6 7 8 9 10; do curl -sf http://127.0.0.1:3000/health && exit 0; sleep 1; done; exit 1'
Restart=on-failure
RestartSec=5s
StartLimitBurst=3
StartLimitIntervalSec=60

User=blufio
Group=blufio
WorkingDirectory=/var/lib/blufio

Environment=RUST_LOG=blufio=info
EnvironmentFile=-/etc/blufio/environment

# Security hardening
NoNewPrivileges=yes
PrivateTmp=yes
PrivateDevices=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/var/lib/blufio
ProtectKernelTunables=yes
ProtectKernelModules=yes
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6
LimitNOFILE=65536

# Memory limit (systemd-level protection on top of app-level)
MemoryMax=256M
MemoryHigh=200M

[Install]
WantedBy=multi-user.target
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Type=notify with libsystemd | Type=simple + health poll | Standard for non-C daemons | No native dependency needed |
| procinfo crate | sysinfo 0.33+ | sysinfo rewritten 2024 | Better API, maintained |
| Custom metrics | metrics-rs facade | metrics 0.24 (2024) | Recorder-agnostic, Prometheus exporter stable |

## Open Questions

1. **sysinfo vs /proc-only for RSS**
   - What we know: /proc/self/statm is Linux-only. macOS uses mach_task_info. sysinfo handles both.
   - What's unclear: Whether sysinfo adds unacceptable binary size or startup overhead.
   - Recommendation: Use conditional compilation -- /proc/self/statm on Linux (target), sysinfo only for macOS dev. Feature flag `sysinfo` if needed.

2. **Memory limit configurability**
   - What we know: Context says 50-80MB idle, 100-200MB load for dynamic allocations.
   - What's unclear: Whether these should be configurable in blufio.toml or hardcoded.
   - Recommendation: Add `[daemon]` config section with `memory_warn_mb` (default: 150) and `memory_limit_mb` (default: 200). Configurable because VPS RAM varies.

3. **Backup scope**
   - What we know: SQLite backup API handles the DB atomically.
   - What's unclear: Whether to also backup config + vault in the same command.
   - Recommendation: DB-only for `blufio backup` (atomic, simple). Config is a TOML file users manage. Vault is in the DB (same SQLite file). So DB backup covers vault too.

## Sources

### Primary (HIGH confidence)
- Context7 /systemd/systemd - systemd service unit files, Type=simple, Restart=on-failure, security hardening directives
- tikv_jemalloc_ctl docs - epoch::advance(), stats::allocated, stats::resident APIs
- Existing codebase: main.rs, serve.rs, recording.rs, server.rs, model.rs -- verified all integration points

### Secondary (MEDIUM confidence)
- rusqlite backup module - Backup::new(), run_to_completion() with page stepping
- metrics-rs 0.24 API - gauge!, counter!, histogram! macros with labels

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - all libraries already in workspace, APIs verified
- Architecture: HIGH - extending existing patterns (gateway routes, CLI commands, metrics)
- Pitfalls: HIGH - based on actual codebase analysis (jemalloc already tested, gateway routes known)

**Research date:** 2026-03-01
**Valid until:** 2026-04-01 (stable domain, no fast-moving dependencies)
