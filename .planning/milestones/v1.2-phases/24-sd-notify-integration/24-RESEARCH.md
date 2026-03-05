# Phase 24: sd_notify Integration - Research

**Researched:** 2026-03-03
**Domain:** systemd service notification protocol (sd_notify), Rust integration
**Confidence:** HIGH

## Summary

This phase integrates systemd's `sd_notify` protocol into Blufio so that `systemctl` accurately reflects service readiness, shutdown progress, and liveness. The implementation uses the `sd-notify` crate (v0.4.5), a battle-tested Rust library with zero transitive dependencies that maps directly to sd_notify(3). The crate's `notify()` function silently returns `Ok(())` when `NOTIFY_SOCKET` is absent, which satisfies SYSD-05 (macOS/Docker no-op) without any `#[cfg(target_os)]` gates.

The integration touches three files: a new `sdnotify.rs` module in `blufio-agent` (STATUS=/READY=/STOPPING=/WATCHDOG= helpers), modifications to `serve.rs` (inserting notify calls at initialization milestones and spawning the watchdog task), and modifications to `shutdown.rs` (STOPPING=1 before token cancellation). The systemd unit file `contrib/blufio.service` is updated from `Type=simple` to `Type=notify` with `WatchdogSec=30`. The watchdog background task follows the identical `tokio::spawn` + `select!` + `CancellationToken` pattern already used by the memory monitor at `serve.rs:576-587`.

**Primary recommendation:** Use `sd-notify` 0.4.5 with a thin wrapper module (`sdnotify.rs`) that exposes `notify_ready()`, `notify_stopping()`, `notify_status()`, and `spawn_watchdog()` -- keep all sd_notify calls behind these helpers so the rest of the codebase never imports `sd_notify` directly.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Key milestones only -- 3-4 STATUS= messages, not every init step
- Milestones: vault initialization, channel connection, ready
- READY=1 paired with STATUS= summary (e.g., "Ready: 2 channels, memory enabled")
- READY=1 sent after mux.connect() completes (all channels connected, accepting messages)
- STATUS= used during both startup and shutdown lifecycle
- Simple heartbeat ping -- no health checks, just WATCHDOG=1 on schedule
- Interval derived from WATCHDOG_USEC environment variable (set by systemd), ping at half that value
- No runtime STATUS= updates from watchdog -- STATUS= stays at "Ready: ..." from startup
- Separate tokio::spawn background task with CancellationToken (same pattern as memory_monitor)
- Module in blufio-agent crate: sdnotify.rs alongside shutdown.rs
- Both deal with process lifecycle -- natural companion
- Use sd-notify crate from crates.io (battle-tested, no unsafe, ~200 lines, no transitive deps)
- Runtime NOTIFY_SOCKET check -- sd-notify crate silently no-ops when socket absent
- No #[cfg(target_os)] gates needed in serve.rs -- SYSD-05 satisfied automatically
- All sd_notify logging at debug level -- quiet at default info, visible with RUST_LOG=blufio=debug
- STOPPING=1 sent when shutdown signal received (integrate with existing shutdown.rs signal handler)
- STATUS= updates during shutdown: "Draining N active sessions...", "Shutdown complete"
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

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| SYSD-01 | Binary sends sd_notify READY=1 after all initialization completes | `sd_notify::notify(false, &[NotifyState::Ready, NotifyState::Status("...")])` called after `mux.connect().await?` in serve.rs |
| SYSD-02 | Binary sends sd_notify STOPPING=1 when shutdown begins | `sd_notify::notify(false, &[NotifyState::Stopping, NotifyState::Status("...")])` in shutdown.rs signal handler before `token.cancel()` |
| SYSD-03 | Binary sends watchdog ping at half the WatchdogSec interval | `sd_notify::watchdog_enabled(false, &mut usec)` reads WATCHDOG_USEC, spawn background task pinging at usec/2 interval |
| SYSD-04 | systemd unit file uses Type=notify with WatchdogSec=30 | Update `contrib/blufio.service`: Type=notify, WatchdogSec=30, remove ExecStartPost health check |
| SYSD-05 | sd_notify is a silent no-op on non-systemd platforms (macOS, Docker) | `sd-notify` crate returns `Ok(())` when NOTIFY_SOCKET is absent -- no code changes needed |
| SYSD-06 | Binary sends STATUS= messages during startup phases | `NotifyState::Status(&str)` sent at vault init, channel connection, and ready milestones |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `sd-notify` | 0.4.5 | Rust bindings for sd_notify(3) protocol | Zero transitive deps, ~200 lines, safe Rust, 7.6M+ downloads, dual MIT/Apache-2.0 |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tokio` | 1.x (workspace) | Async runtime for watchdog background task | Already in workspace, spawn watchdog task |
| `tokio-util` | 0.7.x (workspace) | CancellationToken for watchdog shutdown | Already in workspace, same pattern as memory_monitor |
| `tracing` | 0.1.x (workspace) | debug!() logging for sd_notify calls | Already in workspace |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `sd-notify` | Raw Unix socket writes | 50 lines of unsafe socket code vs. proven crate; not worth it |
| `sd-notify` | `systemd` crate | Much heavier (~20 deps), links libsystemd; overkill for notify-only |
| `sd-notify` | `libsystemd` crate | Requires libsystemd.so at runtime; breaks single-binary deployment |

**Installation (add to blufio-agent/Cargo.toml):**
```toml
[dependencies]
sd-notify = "0.4"
```

No workspace-level addition needed -- this dependency is specific to blufio-agent.

## Architecture Patterns

### Recommended Module Structure
```
crates/blufio-agent/src/
    lib.rs          # Add: pub mod sdnotify;
    sdnotify.rs     # NEW: sd_notify wrapper functions + watchdog task
    shutdown.rs     # MODIFIED: Add STOPPING=1 + STATUS= before token.cancel()
    ...

crates/blufio/src/
    serve.rs        # MODIFIED: Add STATUS= at milestones, READY=1 after mux.connect(), spawn watchdog

contrib/
    blufio.service  # MODIFIED: Type=notify, WatchdogSec=30, TimeoutStartSec=90, remove ExecStartPost
```

### Pattern 1: Thin Wrapper Module (sdnotify.rs)
**What:** A module that wraps `sd_notify::notify()` calls behind descriptive functions, centralizing all systemd notification logic.
**When to use:** Always -- never call `sd_notify::notify()` directly from serve.rs or shutdown.rs.
**Why:** Isolates the sd-notify dependency, makes testing easier, keeps debug logging consistent.
**Example:**
```rust
// Source: sd-notify docs.rs + systemd sd_notify(3) man page
use sd_notify::NotifyState;
use tracing::debug;

/// Notify systemd that initialization is complete and the service is ready.
/// Pairs READY=1 with a STATUS= summary message.
/// No-ops silently when NOTIFY_SOCKET is absent (macOS, Docker).
pub fn notify_ready(status: &str) {
    debug!(status, "sd_notify: READY=1");
    if let Err(e) = sd_notify::notify(false, &[NotifyState::Ready, NotifyState::Status(status)]) {
        debug!(error = %e, "sd_notify: failed to send READY=1 (best-effort)");
    }
}

/// Notify systemd that shutdown is beginning.
pub fn notify_stopping(status: &str) {
    debug!(status, "sd_notify: STOPPING=1");
    if let Err(e) = sd_notify::notify(false, &[NotifyState::Stopping, NotifyState::Status(status)]) {
        debug!(error = %e, "sd_notify: failed to send STOPPING=1 (best-effort)");
    }
}

/// Send a STATUS= message to systemd (visible in `systemctl status`).
pub fn notify_status(status: &str) {
    debug!(status, "sd_notify: STATUS=");
    if let Err(e) = sd_notify::notify(false, &[NotifyState::Status(status)]) {
        debug!(error = %e, "sd_notify: failed to send STATUS (best-effort)");
    }
}

/// Send a WATCHDOG=1 ping to systemd.
fn notify_watchdog() {
    if let Err(e) = sd_notify::notify(false, &[NotifyState::Watchdog]) {
        debug!(error = %e, "sd_notify: failed to send WATCHDOG=1 (best-effort)");
    }
}
```

### Pattern 2: Watchdog Background Task (spawn_watchdog)
**What:** A function that checks if systemd watchdog is enabled and spawns a background task to send WATCHDOG=1 pings at half the configured interval.
**When to use:** Called once from serve.rs after mux.connect(), alongside the memory_monitor spawn.
**Example:**
```rust
// Source: sd-notify docs.rs watchdog_enabled(), systemd sd_watchdog_enabled(3)
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

/// Spawns a watchdog ping task if systemd watchdog is enabled.
/// Returns the JoinHandle if spawned, None if watchdog is not active.
pub fn spawn_watchdog(cancel: CancellationToken) -> Option<tokio::task::JoinHandle<()>> {
    let mut usec: u64 = 0;
    if !sd_notify::watchdog_enabled(false, &mut usec) || usec == 0 {
        debug!("sd_notify: watchdog not enabled, skipping");
        return None;
    }

    // Ping at half the watchdog interval (systemd best practice).
    let interval = Duration::from_micros(usec / 2);
    info!(
        watchdog_sec = usec / 1_000_000,
        ping_interval_sec = interval.as_secs(),
        "sd_notify: watchdog enabled, spawning ping task"
    );

    let handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        // Skip the first immediate tick.
        ticker.tick().await;

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    notify_watchdog();
                }
                _ = cancel.cancelled() => {
                    debug!("sd_notify: watchdog task shutting down");
                    break;
                }
            }
        }
    });

    Some(handle)
}
```

### Pattern 3: Shutdown Integration
**What:** STOPPING=1 + STATUS= sent before CancellationToken is cancelled, and STATUS= updates during drain.
**When to use:** In shutdown.rs signal handler and during drain_sessions flow.
**Example:**
```rust
// In shutdown.rs install_signal_handler(), before token_clone.cancel():
crate::sdnotify::notify_stopping("Shutting down...");
token_clone.cancel();

// In drain_sessions(), before the drain loop:
crate::sdnotify::notify_status(&format!("Draining {} active sessions...", active_count));

// After drain completes:
crate::sdnotify::notify_status("Shutdown complete");
```

### Pattern 4: Startup STATUS= at Milestones
**What:** STATUS= messages at 3 key points during serve.rs initialization.
**When to use:** At vault unlock, channel connection, and final ready.
**Example insertion points in serve.rs:**
```rust
// After vault startup check (line ~98):
sdnotify::notify_status("Initializing: vault unlocked");

// After mux.connect() (line ~499):
let status_msg = format!("Ready: {} channels", mux.channel_count());
sdnotify::notify_ready(&status_msg);
```

### Anti-Patterns to Avoid
- **Calling sd_notify::notify(true, ...):** The `true` parameter unsets NOTIFY_SOCKET, breaking all subsequent calls. Always pass `false`.
- **STATUS= on every init step:** User decision locks this to 3-4 milestones only. Excessive STATUS= creates log noise.
- **Health checks in watchdog:** User decision: simple WATCHDOG=1 ping only. No I/O, no health probes.
- **Logging sd_notify at info level:** User decision: debug level only. Info log stays clean.
- **Using #[cfg(target_os = "linux")]:** Not needed. The crate handles absent NOTIFY_SOCKET gracefully.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Unix socket notification | Raw `std::os::unix::net::UnixDatagram` writes | `sd-notify` crate | Socket path parsing, datagram vs stream, abstract sockets, error handling |
| WATCHDOG_USEC parsing | Manual `std::env::var` + parse | `sd_notify::watchdog_enabled()` | Also checks WATCHDOG_PID matches current process, handles edge cases |
| systemd boot detection | Check for `/run/systemd/system` manually | `sd_notify::booted()` | Encapsulates the check, returns Result |

**Key insight:** The sd_notify protocol looks trivial (just write bytes to a socket) but has edge cases: abstract vs filesystem sockets, datagram size limits, WATCHDOG_PID process matching. The `sd-notify` crate handles all of these in ~200 lines of safe Rust.

## Common Pitfalls

### Pitfall 1: Passing `true` to notify() (unset_env)
**What goes wrong:** First sd_notify call works, all subsequent calls silently fail because NOTIFY_SOCKET was unset.
**Why it happens:** The `unset_env` parameter is the first parameter and looks like a "verbose" flag.
**How to avoid:** Always pass `false` to `sd_notify::notify()`. Document in wrapper functions.
**Warning signs:** READY=1 works but WATCHDOG=1 and STOPPING=1 never reach systemd.

### Pitfall 2: Sending READY=1 Too Early
**What goes wrong:** systemd marks service as "active (running)" before channels are connected. If channel init fails, systemd thinks the service is healthy.
**Why it happens:** Tempting to send READY=1 after basic setup instead of waiting for full initialization.
**How to avoid:** READY=1 goes strictly after `mux.connect().await?` (line ~499 in serve.rs). This is after all channels are connected.
**Warning signs:** `systemctl start blufio` returns success but service crashes shortly after.

### Pitfall 3: Watchdog Interval Too Close to Timeout
**What goes wrong:** Occasional GC pauses or load spikes cause watchdog timeout, systemd kills the service.
**Why it happens:** Pinging at exactly WatchdogSec instead of WatchdogSec/2.
**How to avoid:** Use `sd_notify::watchdog_enabled()` to get WATCHDOG_USEC, divide by 2 for ping interval. With WatchdogSec=30, ping every 15 seconds.
**Warning signs:** Sporadic service restarts under load visible in `journalctl`.

### Pitfall 4: TimeoutStartSec Too Short
**What goes wrong:** systemd kills Blufio during first-run model download (which can take 30-60 seconds).
**Why it happens:** Default TimeoutStartSec is 90s in some systemd versions, but Type=notify waits for READY=1.
**How to avoid:** Set `TimeoutStartSec=90` explicitly in the unit file.
**Warning signs:** First deployment fails, subsequent starts work fine.

### Pitfall 5: Forgetting to Remove ExecStartPost Health Check
**What goes wrong:** ExecStartPost curl loop runs after READY=1 is sent, adding unnecessary delay (up to 15 seconds of polling). Also, the health check endpoint may not exist if gateway is disabled.
**Why it happens:** ExecStartPost was the old way to detect readiness with Type=simple.
**How to avoid:** Remove the entire ExecStartPost line when switching to Type=notify.
**Warning signs:** `systemctl start` takes 15 extra seconds even though READY=1 was sent instantly.

### Pitfall 6: STATUS= During Drain Without Session Count
**What goes wrong:** STATUS= says "Draining sessions..." without the count, providing no useful info.
**Why it happens:** `drain_sessions()` takes a `HashMap<String, SessionActor>` reference but the STATUS= call is before the function.
**How to avoid:** Count active sessions first, include count in STATUS= message.
**Warning signs:** `systemctl status` shows unhelpful shutdown messages.

## Code Examples

Verified patterns from official sources:

### Complete sdnotify.rs Module
```rust
// Source: sd-notify 0.4.5 docs.rs, systemd sd_notify(3) man page
//! systemd sd_notify integration for process lifecycle signaling.
//!
//! Wraps the `sd-notify` crate to provide typed helpers for READY=1,
//! STOPPING=1, STATUS=, and WATCHDOG=1 notifications. All calls are
//! best-effort: failures are logged at debug level and never propagated.
//! On non-systemd platforms (macOS, Docker), all calls are silent no-ops.

use std::time::Duration;

use sd_notify::NotifyState;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

/// Notify systemd that the service is ready.
/// Sends READY=1 paired with a STATUS= summary.
pub fn notify_ready(status: &str) {
    debug!(status, "sd_notify: READY=1");
    if let Err(e) = sd_notify::notify(false, &[NotifyState::Ready, NotifyState::Status(status)]) {
        debug!(error = %e, "sd_notify: failed to send READY (best-effort)");
    }
}

/// Notify systemd that shutdown is beginning.
/// Sends STOPPING=1 paired with a STATUS= message.
pub fn notify_stopping(status: &str) {
    debug!(status, "sd_notify: STOPPING=1");
    if let Err(e) = sd_notify::notify(false, &[NotifyState::Stopping, NotifyState::Status(status)]) {
        debug!(error = %e, "sd_notify: failed to send STOPPING (best-effort)");
    }
}

/// Send a STATUS= progress message to systemd.
/// Visible in `systemctl status blufio`.
pub fn notify_status(status: &str) {
    debug!(status, "sd_notify: STATUS=");
    if let Err(e) = sd_notify::notify(false, &[NotifyState::Status(status)]) {
        debug!(error = %e, "sd_notify: failed to send STATUS (best-effort)");
    }
}

/// Send WATCHDOG=1 keep-alive ping.
fn notify_watchdog() {
    if let Err(e) = sd_notify::notify(false, &[NotifyState::Watchdog]) {
        debug!(error = %e, "sd_notify: failed to send WATCHDOG (best-effort)");
    }
}

/// Spawn a watchdog ping background task if systemd watchdog is enabled.
///
/// Reads WATCHDOG_USEC from the environment (set by systemd when
/// WatchdogSec= is configured). Pings at half the timeout interval,
/// per systemd best practice.
///
/// Returns `Some(JoinHandle)` if watchdog is active, `None` otherwise.
pub fn spawn_watchdog(cancel: CancellationToken) -> Option<tokio::task::JoinHandle<()>> {
    let mut usec: u64 = 0;
    if !sd_notify::watchdog_enabled(false, &mut usec) || usec == 0 {
        debug!("sd_notify: watchdog not enabled, skipping");
        return None;
    }

    let interval = Duration::from_micros(usec / 2);
    info!(
        watchdog_timeout_sec = usec / 1_000_000,
        ping_interval_sec = interval.as_secs(),
        "watchdog enabled, spawning ping task"
    );

    let handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.tick().await; // skip immediate first tick

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    notify_watchdog();
                }
                _ = cancel.cancelled() => {
                    debug!("sd_notify: watchdog task shutting down");
                    break;
                }
            }
        }
    });

    Some(handle)
}
```

### shutdown.rs Integration Points
```rust
// Source: Existing shutdown.rs pattern + sd_notify STOPPING=1 behavior
// In install_signal_handler(), just before token_clone.cancel():

// NEW: Notify systemd that shutdown is beginning
crate::sdnotify::notify_stopping("Shutting down...");
token_clone.cancel();

// In drain_sessions(), at the start (after counting active sessions):
if active_count > 0 {
    crate::sdnotify::notify_status(&format!("Draining {} active sessions...", active_count));
}

// After drain completes (before return):
crate::sdnotify::notify_status("Shutdown complete");
```

### serve.rs Integration Points
```rust
// After vault startup check (~line 98, after "vault unlocked" log):
blufio_agent::sdnotify::notify_status("Initializing: vault unlocked");

// After mux.connect() (~line 499, after "channel multiplexer connected" log):
let ready_status = format!(
    "Ready: {} channels{}",
    mux.channel_count(),
    if memory_provider.is_some() { ", memory enabled" } else { "" }
);
blufio_agent::sdnotify::notify_ready(&ready_status);

// Spawn watchdog task (alongside memory monitor spawn, ~line 587):
{
    let wd_cancel = cancel.clone();
    let _watchdog_handle = blufio_agent::sdnotify::spawn_watchdog(wd_cancel);
}
```

### Updated blufio.service
```ini
[Service]
Type=notify
NotifyAccess=main
ExecStartPre=-/var/lib/blufio/hooks/pre-start.sh
ExecStart=/usr/local/bin/blufio serve
# ExecStartPost removed -- Type=notify makes health-check polling redundant
ExecStopPost=-/var/lib/blufio/hooks/post-stop.sh
Restart=on-failure
RestartSec=5s
StartLimitBurst=3
StartLimitIntervalSec=60
TimeoutStartSec=90
TimeoutStopSec=30
WatchdogSec=30
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Type=simple + ExecStartPost health poll | Type=notify + sd_notify READY=1 | Always available in systemd | Instant readiness detection, no polling delay |
| Manual NOTIFY_SOCKET handling | `sd-notify` crate handles all socket logic | sd-notify 0.1.0 (2019) | No unsafe code, correct abstract socket handling |
| libsystemd FFI bindings | Pure Rust `sd-notify` crate | sd-notify 0.4.0+ | No C dependency, works on all platforms |

**Deprecated/outdated:**
- `systemd` crate: Requires libsystemd.so at runtime, breaks single-binary deployment model
- `libsystemd` crate: Same issue -- dynamic linking to libsystemd
- sd-notify 0.1.x API: Older enum variants, prefer 0.4.x

## Open Questions

1. **Should sdnotify functions be pub or pub(crate)?**
   - What we know: The module lives in blufio-agent, which is a dependency of blufio. Other crates (e.g., future phases) might want to send STATUS= updates.
   - What's unclear: Whether any future code outside blufio-agent will call these functions.
   - Recommendation: Make `notify_ready`, `notify_stopping`, `notify_status`, and `spawn_watchdog` `pub`. The `notify_watchdog` helper stays private (only used by spawn_watchdog). This costs nothing and keeps the door open.

2. **Test strategy: mock NOTIFY_SOCKET or test the abstraction?**
   - What we know: The sd-notify crate silently no-ops without NOTIFY_SOCKET. Unit tests run without systemd.
   - What's unclear: Whether to create a Unix socket in tests to verify actual notification bytes.
   - Recommendation: Unit test the wrapper functions (they should not panic, should handle errors gracefully). Verify that `spawn_watchdog` returns `None` when WATCHDOG_USEC is unset. Do NOT create a mock Unix socket -- integration testing against real systemd is the proper validation, done during deployment. Keep unit tests simple.

3. **Should vault-not-found also get a STATUS=?**
   - What we know: Current milestones are vault init, channel connection, ready. When no vault exists, code logs "no vault found" at debug and continues.
   - What's unclear: Whether "no vault" deserves a STATUS= message.
   - Recommendation: Skip it. The STATUS= milestones should only report significant events. No vault is the common case for new installations.

## Sources

### Primary (HIGH confidence)
- [sd-notify crate docs.rs](https://docs.rs/sd-notify/latest/sd_notify/) - API reference, NotifyState enum, notify() signature, watchdog_enabled() signature
- [sd-notify source code](https://docs.rs/sd-notify/latest/src/sd_notify/lib.rs.html) - Verified NOTIFY_SOCKET absent behavior (returns Ok(())), unset_env parameter
- [systemd sd_notify(3) man page](https://www.freedesktop.org/software/systemd/man/latest/sd_notify.html) - Protocol specification, READY/STOPPING/STATUS/WATCHDOG behavior
- [systemd sd_watchdog_enabled(3)](https://www.freedesktop.org/software/systemd/man/latest/sd_watchdog_enabled.html) - WATCHDOG_USEC parsing, half-interval recommendation, WATCHDOG_PID check

### Secondary (MEDIUM confidence)
- [sd-notify crates.io](https://crates.io/crates/sd-notify) - Version 0.4.5, 7.6M+ downloads, MIT/Apache-2.0
- [systemd.service(5)](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html) - Type=notify, WatchdogSec, NotifyAccess documentation

### Tertiary (LOW confidence)
None -- all findings verified against official sources.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - sd-notify crate is well-documented, source code verified, 7.6M+ downloads
- Architecture: HIGH - Integration points identified in existing source code, patterns match established project conventions
- Pitfalls: HIGH - Verified against systemd man pages and crate source code

**Research date:** 2026-03-03
**Valid until:** 2026-04-03 (30 days -- sd-notify is a stable, mature crate)
