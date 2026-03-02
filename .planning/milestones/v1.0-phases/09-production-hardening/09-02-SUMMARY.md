---
phase: 09-production-hardening
plan: 02
type: summary
status: complete
commits:
  - "feat(09-02): add CLI diagnostics (status/doctor/config), systemd unit, and SEC-02 enforcement"
---

# Plan 09-02 Execution Summary

## What was built

### CLI Status Command (crates/blufio/src/status.rs)
- `blufio status` connects to gateway /health endpoint, displays running state and uptime
- `--json` outputs structured JSON for scripting (StatusResponse struct)
- `--plain` disables colored output; auto-detects non-TTY via IsTerminal
- Falls back gracefully to "not running" when agent is unreachable
- Format: uptime as "Xd Yh Zm" human-readable string

### CLI Doctor Command (crates/blufio/src/doctor.rs)
- `blufio doctor` runs 4 quick checks (~2s):
  - Configuration validation
  - Database connectivity (SELECT 1)
  - LLM API reachability (HEAD api.anthropic.com)
  - Health endpoint reachability
- `--deep` adds 3 intensive checks:
  - SQLite PRAGMA integrity_check
  - Disk space (DB file size)
  - Memory baseline (jemalloc heap/resident)
- Colored output with pass/warn/fail symbols and timing

### CLI Config Get/Validate (crates/blufio/src/main.rs)
- `blufio config get <key>` uses generic serde_json traversal (serialize config to Value, walk dotted path)
- `blufio config validate` loads config and reports errors or success
- New ConfigCommands variants: Get { key }, Validate

### systemd Unit File (contrib/)
- `blufio.service`: Type=simple, Restart=on-failure, RestartSec=5s
- ExecStartPost health poll (15 attempts, 1s apart)
- Security hardening: NoNewPrivileges, PrivateTmp, ProtectSystem=strict, MemoryMax=256M
- `blufio-logrotate.conf`: daily rotation, 14 days, compress
- `hooks/pre-start.sh` and `hooks/post-stop.sh` lifecycle scripts

### SEC-02 Enforcement (crates/blufio/src/serve.rs)
- When gateway is enabled without keypair feature, serve returns Security error
- When keypair feature is present, logs info about keypair auth availability
- Warns when gateway runs without bearer_token (keypair auth fallback)

## Requirements covered
- **CLI-02**: `blufio status` with --json and --plain
- **CLI-03**: `blufio doctor` with --deep
- **CLI-04**: `blufio config get/validate`
- **CLI-07**: Colored output with TTY detection
- **SEC-02**: Keypair auth enforcement in serve mode
- **CORE-04**: systemd unit file with security hardening

## Test results
- `cargo test -p blufio`: 55 passed (includes 12 new CLI parsing tests + config_get tests)
- `cargo build -p blufio`: success
