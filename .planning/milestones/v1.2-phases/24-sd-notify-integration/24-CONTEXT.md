# Phase 24: sd_notify Integration - Context

**Gathered:** 2026-03-03
**Status:** Ready for planning

<domain>
## Phase Boundary

systemd knows exactly when Blufio is ready, when it is shutting down, and that it is still alive -- enabling proper Type=notify service management. Covers READY=1, STOPPING=1, STATUS= messages, and watchdog pings. Cross-platform no-op on macOS/Docker.

</domain>

<decisions>
## Implementation Decisions

### Startup status reporting
- Key milestones only -- 3-4 STATUS= messages, not every init step
- Milestones: vault initialization, channel connection, ready
- READY=1 paired with STATUS= summary (e.g., "Ready: 2 channels, memory enabled")
- READY=1 sent after mux.connect() completes (all channels connected, accepting messages)
- STATUS= used during both startup and shutdown lifecycle

### Watchdog behavior
- Simple heartbeat ping -- no health checks, just WATCHDOG=1 on schedule
- Interval derived from WATCHDOG_USEC environment variable (set by systemd), ping at half that value
- No runtime STATUS= updates from watchdog -- STATUS= stays at "Ready: ..." from startup
- Separate tokio::spawn background task with CancellationToken (same pattern as memory_monitor)

### Crate organization
- Module in blufio-agent crate: sdnotify.rs alongside shutdown.rs
- Both deal with process lifecycle -- natural companion
- Use sd-notify crate from crates.io (battle-tested, no unsafe, ~200 lines, no transitive deps)
- Runtime NOTIFY_SOCKET check -- sd-notify crate silently no-ops when socket absent
- No #[cfg(target_os)] gates needed in serve.rs -- SYSD-05 satisfied automatically
- All sd_notify logging at debug level -- quiet at default info, visible with RUST_LOG=blufio=debug

### Shutdown integration
- STOPPING=1 sent when shutdown signal received (integrate with existing shutdown.rs signal handler)
- STATUS= updates during shutdown: "Draining N active sessions...", "Shutdown complete"

### Unit file changes
- Type=notify replaces Type=simple
- Remove ExecStartPost curl health-check loop (redundant with Type=notify)
- Add WatchdogSec=30 (as specified in SYSD-04)
- Add TimeoutStartSec=90 (covers first-run model download)
- Add explicit NotifyAccess=main (default for Type=notify but clearer documentation)

### Claude's Discretion
- Exact STATUS= message wording
- Error handling for sd_notify failures (should be best-effort)
- Whether to expose sd_notify functions via blufio-agent's public API or keep internal
- Test strategy for sd_notify (mock NOTIFY_SOCKET or unit test the abstraction layer)

</decisions>

<specifics>
## Specific Ideas

- Watchdog follows same background task pattern as memory_monitor in serve.rs (tokio::spawn + select! + CancellationToken)
- READY=1 goes at serve.rs line ~500 after mux.connect().await? succeeds
- STOPPING=1 integrates with install_signal_handler() in shutdown.rs
- STATUS= during shutdown pairs with existing drain_sessions() flow

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/blufio-agent/src/shutdown.rs`: Signal handler with CancellationToken -- STOPPING=1 integrates here
- `crates/blufio/src/serve.rs`: Full initialization sequence with clear phases for STATUS= messages
- `contrib/blufio.service`: Existing unit file to update (Type=simple -> Type=notify)
- Memory monitor background task pattern (serve.rs:576-587): Template for watchdog task

### Established Patterns
- Feature gating: `#[cfg(feature = "...")]` throughout workspace -- sd-notify could optionally use this
- CancellationToken: All background tasks use this for graceful shutdown coordination
- Structured tracing: `info!()`, `debug!()`, `warn!()` with named fields
- Error handling: `BlufioError` enum, `map_err` pattern

### Integration Points
- `serve.rs:run_serve()` -- STATUS= calls inserted at vault, channels, and ready milestones
- `serve.rs:499` (`mux.connect().await?`) -- READY=1 goes immediately after this
- `shutdown.rs:install_signal_handler()` -- STOPPING=1 goes in signal handler before token.cancel()
- `serve.rs:576-587` (memory monitor spawn) -- watchdog task spawned in same section
- `contrib/blufio.service` -- unit file updated in place

</code_context>

<deferred>
## Deferred Ideas

None -- discussion stayed within phase scope

</deferred>

---

*Phase: 24-sd-notify-integration*
*Context gathered: 2026-03-03*
