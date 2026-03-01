---
phase: 09-production-hardening
plan: 01
type: summary
status: complete
commits:
  - "feat(09-01): add DaemonConfig, extend Prometheus with memory/error metrics"
  - "feat(09-01): add memory monitor, health/metrics endpoints, and gateway HealthState"
---

# Plan 09-01 Execution Summary

## What was built

### DaemonConfig (crates/blufio-config/src/model.rs)
- Added `DaemonConfig` struct with `memory_warn_mb` (default 150), `memory_limit_mb` (default 200), `health_port` (default 3000)
- Added `#[serde(default)] pub daemon: DaemonConfig` field to `BlufioConfig`
- Full serde support with `deny_unknown_fields`

### Prometheus memory/error metrics (crates/blufio-prometheus/src/recording.rs)
- 5 new metric descriptions in `register_metrics()`:
  - `blufio_memory_heap_bytes` (gauge) -- jemalloc allocated heap
  - `blufio_memory_rss_bytes` (gauge) -- process RSS from /proc/self/statm
  - `blufio_memory_resident_bytes` (gauge) -- jemalloc resident
  - `blufio_memory_pressure` (gauge) -- 0=normal, 1=warning
  - `blufio_errors_total` (counter) -- errors by type label
- 5 helper functions: `set_memory_heap`, `set_memory_rss`, `set_memory_resident`, `set_memory_pressure`, `record_error`
- All re-exported from `blufio_prometheus` crate root

### Memory monitor (crates/blufio/src/serve.rs)
- `memory_monitor()` async task polls jemalloc stats every 5 seconds
- Uses `epoch::advance()` + `stats::allocated` + `stats::resident` for jemalloc introspection
- `read_rss_bytes()` reads `/proc/self/statm` on Linux, returns None on other platforms
- Exports all values to Prometheus gauges when `prometheus` feature is enabled
- Memory pressure detection: logs warning and sets pressure gauge when heap exceeds `memory_warn_mb`
- Spawned alongside heartbeat task with shared CancellationToken for graceful shutdown

### Gateway health/metrics endpoints (crates/blufio-gateway/)
- `HealthState` struct in `server.rs` with `start_time` and `prometheus_render` callback
- Added `health: HealthState` field to `GatewayState`
- Unauthenticated `GET /health` returns `{"status":"healthy","uptime_secs":N}`
- Unauthenticated `GET /metrics` returns Prometheus text format (or 503 if not available)
- Both routes sit outside auth middleware, suitable for systemd health checks and Prometheus scraping
- `prometheus_render` callback wired from `PrometheusAdapter::handle()` through `GatewayChannelConfig`
- Manual `Debug` impl for `GatewayChannelConfig` to handle `Arc<dyn Fn()>` field

## Requirements covered
- **CORE-07**: Memory monitoring with jemalloc introspection and pressure detection
- **CORE-08**: Prometheus memory/error metrics exported via gateway /metrics endpoint
- **COST-04**: Memory threshold configuration in DaemonConfig

## Test results
- `cargo test -p blufio-config`: 21 passed
- `cargo test -p blufio-prometheus`: 4 passed
- `cargo test -p blufio-gateway`: 17 passed
- `cargo build -p blufio`: success
