# Phase 59: Hook System & Hot Reload - Research

**Researched:** 2026-03-12
**Domain:** Shell-based lifecycle hooks, config/TLS/plugin hot reload, event bus integration
**Confidence:** HIGH

## Summary

Phase 59 implements two interconnected subsystems: (1) a shell-based lifecycle hook system that allows operators to extend Blufio behavior by executing shell commands in response to 11 lifecycle events, and (2) a hot reload system that applies configuration, TLS, and plugin changes at runtime without restart.

Both systems build heavily on existing codebase patterns: the EventBus for event-driven architecture, the `notify`/`notify-debouncer-mini` crates for file watching (already used by memory watcher), and established config/bus event patterns. The key new dependency is `arc-swap` for lock-free atomic config swapping, and potentially `axum-server` with `rustls` for TLS termination if TLS hot reload requires native TLS (current gateway uses plain TCP).

**Primary recommendation:** Create a `blufio-hooks` crate following the `blufio-cron` pattern (EventBus subscriber, CancellationToken lifecycle, non-fatal init). Implement hot reload in the main `blufio` crate (serve.rs) since it needs direct access to the config and subsystem references. Use `ArcSwap<BlufioConfig>` as the shared config holder, replacing the current owned `BlufioConfig`.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- New `blufio-hooks` crate following workspace convention (like blufio-cron, blufio-injection)
- HookConfig and HotReloadConfig types defined in blufio-config/model.rs (following CronConfig/RetentionConfig pattern)
- HookEvent variant added to BusEvent enum in blufio-bus/events.rs (following established sub-enum pattern with String fields)
- Hooks subscribe to EventBus events -- HookManager listens to bus, matches events to registered hooks, executes shell commands
- BTreeMap<u32, Vec<HookDefinition>> for priority ordering (lower number = higher priority, same priority = insertion order)
- Shell-based: hooks are shell commands/scripts specified in TOML
- JSON event context piped to stdin (serde_json::to_string of the triggering BusEvent)
- stdout captured as optional response (e.g., pre_compaction hook could emit skip signal)
- Configurable timeout per hook (default: 30s) with tokio::time::timeout
- Restricted PATH: configurable allowed directories, defaults to /usr/bin:/usr/local/bin
- Optional network isolation: hooks can be restricted from network access (platform-dependent)
- Global AtomicU32 recursion depth counter
- Hook execution increments on entry, decrements on exit
- Configurable max depth (default: 3) -- exceeding logs warning and skips hook
- 11 lifecycle events: pre_start, post_start, pre_shutdown, post_shutdown, session_created, session_closed, pre_compaction, post_compaction, degradation_changed, config_reloaded, memory_extracted
- ArcSwap for BlufioConfig -- atomic pointer swap, readers never block
- File watcher (notify crate, already in workspace from memory watcher) on blufio.toml
- Reload flow: detect change -> parse TOML -> validate (deny_unknown_fields) -> ArcSwap swap -> emit ConfigEvent::Reloaded on EventBus
- Active sessions continue with config snapshot at session start; new sessions pick up reloaded config
- Validation failure: log error, keep current config, do NOT swap
- rustls ResolvesServerCert trait implementation with Arc<ArcSwap<CertifiedKey>>
- File watcher on cert/key paths from TLS config
- File watcher on skill directory (from existing SkillConfig)
- On change: re-scan directory, detect new/modified/removed .wasm files
- Re-verify Ed25519 signatures on changed modules (existing verification gate)
- Update skill registry in-place -- remove stale, add new, replace modified
- Emit ConfigEvent on EventBus after skill reload

### Claude's Discretion
- Exact hook stdin JSON schema (beyond serialized BusEvent)
- Hook stdout parsing format for response hooks
- File watcher debounce timing for config reload (500ms recommended, matching memory watcher)
- Whether to create a separate blufio-hooks crate or add to blufio-cron

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| HOOK-01 | 11 lifecycle hooks: pre_start, post_start, pre_shutdown, post_shutdown, session_created, session_closed, pre_compaction, post_compaction, degradation_changed, config_reloaded, memory_extracted | BusEvent enum already has matching event variants for all 11 events. HookEvent sub-enum needed for hook execution tracking. |
| HOOK-02 | TOML-defined hooks with BTreeMap priority ordering (lower number = higher priority) | HookConfig/HookDefinition structs in model.rs following CronConfig/CronJobConfig pattern. BTreeMap<u32, Vec<HookDefinition>> for ordered dispatch. |
| HOOK-03 | Shell-based hook execution with JSON stdin (event context) and stdout (optional response) | tokio::process::Command for async shell execution. serde_json::to_string(&BusEvent) for stdin. Stdout capture with configurable max buffer. |
| HOOK-04 | Hook sandboxing with configurable timeout, restricted PATH, and optional network isolation | tokio::time::timeout for timeout. Command::env("PATH", restricted_path). Platform-dependent network isolation (macOS sandbox-exec, Linux unshare -- optional). |
| HOOK-05 | Hooks subscribe to EventBus events for asynchronous trigger | HookManager subscribes via event_bus.subscribe_reliable(). Matches event_type_string() against hook trigger events. |
| HOOK-06 | Recursion depth counter prevents hook-triggered-hook infinite loops | Global AtomicU32 counter. Increment on hook entry, decrement on exit (RAII guard pattern). Configurable max_depth (default: 3). |
| HTRL-01 | Config hot reload: file watcher on blufio.toml triggers parse -> validate -> ArcSwap swap | notify-debouncer-mini for 500ms debounce. load_config_from_path() for parsing. validate_config() for validation. ArcSwap::store() for atomic swap. |
| HTRL-02 | TLS certificate hot reload via rustls ResolvesServerCert with file watcher | Requires adding TLS support to gateway. rustls ServerConfig with custom ResolvesServerCert. ArcSwap<CertifiedKey> for cert swapping. File watcher on cert/key paths. |
| HTRL-03 | Plugin hot reload: re-scan skill directory, reload changed WASM modules, verify signatures | File watcher on skill.skills_dir. Compare file hashes to detect changes. Re-verify via existing Ed25519 signing module. Update SkillStore. |
| HTRL-04 | Config propagation via ordered EventBus events with validation-before-swap | Emit ConfigEvent::Reloaded on EventBus after successful swap. Subscribers react to config changes asynchronously. |
| HTRL-05 | Active sessions continue on current config; new sessions use reloaded config | Sessions load config snapshot on creation via ArcSwap::load(). ArcSwap guarantees readers see consistent snapshot even during swap. |
| HTRL-06 | config_reloaded lifecycle hook fires after successful reload | HookManager listens for ConfigEvent::Reloaded, triggers config_reloaded hooks. Recursion guard prevents infinite loop if hook modifies config. |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| arc-swap | 1.8 | Lock-free atomic config swapping | Standard Rust pattern for read-heavy config sharing. Load is wait-free, swap is atomic. Used by production systems for hot reload. |
| notify | 8.2 | File system event watching | Already in workspace (memory watcher). Cross-platform, proven. |
| notify-debouncer-mini | 0.7 | Debounced file events | Already in workspace. 500ms debounce prevents rapid-fire reloads. |
| tokio | 1 (workspace) | Async runtime, process spawning | Already in workspace. tokio::process::Command for async shell exec. |
| serde_json | 1 (per-crate) | JSON event context for hook stdin | Already used across 33 crates. Serialize BusEvent to JSON for hook stdin. |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| axum-server | 0.7 | TLS-enabled axum server | Only for HTRL-02 (TLS cert hot reload). Replaces plain axum::serve with rustls support. |
| rustls | 0.23+ | TLS implementation | Already used transitively (reqwest, ort, tokio-tungstenite). Direct dep only for HTRL-02 ResolvesServerCert. |
| tokio-rustls | 0.26 | Async rustls integration | For TLS acceptor with hot-reloadable certs. |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| arc-swap | RwLock<Arc<Config>> | RwLock has write contention on reload. ArcSwap is lock-free for reads -- better for hot path. |
| Custom TLS reload | tls-hot-reload crate | tls-hot-reload provides turnkey solution but adds external dependency. Custom impl with ArcSwap<CertifiedKey> is straightforward and keeps control. |
| Separate blufio-hooks crate | Add to blufio-cron | Separate crate is cleaner separation of concerns. Hooks have different lifecycle (event-driven vs time-driven). Recommend separate crate. |

**Installation (workspace Cargo.toml):**
```toml
arc-swap = "1.8"
# axum-server, rustls, tokio-rustls only needed for HTRL-02
```

## Architecture Patterns

### Recommended Project Structure
```
crates/
  blufio-hooks/
    src/
      lib.rs           # Public API, re-exports
      manager.rs       # HookManager: EventBus subscriber + shell executor
      executor.rs      # ShellExecutor: process spawning, stdin/stdout, sandboxing
      recursion.rs     # RecursionGuard: AtomicU32 depth counter with RAII
    Cargo.toml
  blufio-config/
    src/
      model.rs         # + HookConfig, HookDefinition, HotReloadConfig
  blufio-bus/
    src/
      events.rs        # + HookEvent sub-enum
  blufio/
    src/
      serve.rs         # + hot reload watcher, ArcSwap<BlufioConfig>
      hot_reload.rs    # Config reload logic (parse, validate, swap, propagate)
```

### Pattern 1: HookManager as EventBus Subscriber
**What:** HookManager subscribes to reliable EventBus channel, pattern-matches incoming events against registered hooks, executes matching hooks in priority order.
**When to use:** All hook triggering.
**Example:**
```rust
// Source: Follows blufio-audit AuditSubscriber pattern
pub struct HookManager {
    hooks: BTreeMap<u32, Vec<HookDefinition>>,
    recursion_counter: Arc<AtomicU32>,
    max_depth: u32,
    default_timeout: Duration,
    allowed_path: String,
}

impl HookManager {
    pub async fn run(self, mut rx: mpsc::Receiver<BusEvent>, event_bus: Arc<EventBus>) {
        while let Some(event) = rx.recv().await {
            let event_type = event.event_type_string();
            for (_priority, hooks) in &self.hooks {
                for hook in hooks {
                    if hook.matches_event(event_type) {
                        self.execute_hook(hook, &event, &event_bus).await;
                    }
                }
            }
        }
    }
}
```

### Pattern 2: ArcSwap Config Hot Reload
**What:** Replace owned `BlufioConfig` with `Arc<ArcSwap<BlufioConfig>>`. File watcher detects blufio.toml changes, parses and validates new config, atomically swaps.
**When to use:** Config hot reload (HTRL-01).
**Example:**
```rust
// Source: arc-swap docs patterns
use arc_swap::ArcSwap;

// In serve.rs initialization:
let config_swap = Arc::new(ArcSwap::from_pointee(config));

// In hot reload watcher:
async fn reload_config(
    config_swap: &ArcSwap<BlufioConfig>,
    config_path: &Path,
    event_bus: &EventBus,
) {
    match load_config_from_path(config_path) {
        Ok(new_config) => {
            if let Err(errors) = validate_config(&new_config) {
                warn!(?errors, "config reload validation failed, keeping current config");
                return;
            }
            config_swap.store(Arc::new(new_config));
            event_bus.publish(BusEvent::Config(ConfigEvent::Reloaded {
                event_id: new_event_id(),
                timestamp: now_timestamp(),
                source: "hot_reload".into(),
            })).await;
            info!("config hot-reloaded successfully");
        }
        Err(e) => {
            warn!(error = %e, "config reload parse failed, keeping current config");
        }
    }
}
```

### Pattern 3: RecursionGuard (RAII depth counter)
**What:** AtomicU32 incremented on hook entry, decremented on drop. Prevents hook-triggered-hook infinite loops.
**When to use:** Every hook execution.
**Example:**
```rust
pub struct RecursionGuard {
    counter: Arc<AtomicU32>,
}

impl RecursionGuard {
    pub fn try_enter(counter: Arc<AtomicU32>, max_depth: u32) -> Option<Self> {
        let prev = counter.fetch_add(1, Ordering::SeqCst);
        if prev >= max_depth {
            counter.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Self { counter })
    }
}

impl Drop for RecursionGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}
```

### Pattern 4: Shell Execution with Sandboxing
**What:** tokio::process::Command with restricted PATH, stdin piping, timeout, and stdout capture.
**When to use:** All hook execution.
**Example:**
```rust
use tokio::process::Command;
use tokio::io::AsyncWriteExt;

async fn execute_shell_hook(
    command: &str,
    stdin_json: &str,
    timeout: Duration,
    allowed_path: &str,
) -> Result<Option<String>, HookError> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .env_clear()
        .env("PATH", allowed_path)
        .env("HOME", "/tmp")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(stdin_json.as_bytes()).await?;
        drop(stdin); // Close stdin to signal EOF
    }

    let output = tokio::time::timeout(timeout, child.wait_with_output()).await??;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(if stdout.is_empty() { None } else { Some(stdout) })
    } else {
        Err(HookError::NonZeroExit(output.status.code()))
    }
}
```

### Anti-Patterns to Avoid
- **Blocking on hook execution in event bus publish path:** Hooks MUST execute asynchronously. The EventBus subscriber runs hooks in a separate task, never blocking event propagation.
- **Sharing mutable config reference:** Use ArcSwap::load() to get a snapshot, never hold a mutable reference to config across await points.
- **Trusting hook stdout blindly:** Hook stdout should be treated as untrusted input. Parse defensively, validate format, reject malformed responses.
- **Forgetting to decrement recursion counter on panic:** The RAII RecursionGuard pattern handles this automatically via Drop impl.
- **Hot-reloading non-reloadable config fields:** Some config fields (bind_address, database_path, storage encryption) cannot be hot-reloaded. Only reload safe fields; warn if non-reloadable fields changed.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| File debouncing | Custom timer-based dedup | notify-debouncer-mini | Already in workspace. Handles edge cases (rapid saves, editor temp files, OS-specific quirks). |
| Atomic config swap | RwLock with manual version tracking | arc-swap ArcSwap | Lock-free reads, well-tested, handles Arc lifetime correctly. |
| Shell sandboxing | Custom chroot/namespace setup | env_clear() + PATH restriction | Full sandboxing is platform-specific and complex. PATH restriction is sufficient for v1.5. Network isolation is optional. |
| TOML parsing + validation | Manual TOML parsing | Existing load_config_from_path() + validate_config() | Already handles deny_unknown_fields, defaults, env overrides. |
| TLS cert loading | Manual PEM parsing | rustls-pemfile + rustls CertifiedKey | Standard, handles cert chain ordering, key format detection. |

**Key insight:** The codebase already has well-established patterns for every subsystem this phase touches. The hook system is essentially the CronScheduler pattern (EventBus subscriber, async execution, timeout) applied to shell commands instead of built-in tasks. Hot reload is the memory FileWatcher pattern (notify + debounce + tokio::spawn) applied to config/certs/skills instead of workspace files.

## Common Pitfalls

### Pitfall 1: Config Reload Ordering
**What goes wrong:** Subsystems see config changes in different order, causing inconsistent state.
**Why it happens:** EventBus broadcast is unordered; different subscribers process events at different speeds.
**How to avoid:** ArcSwap swap is atomic -- all readers immediately see new config. EventBus notification is informational only (for logging/hooks), not the source of truth. Subsystems should load config from ArcSwap, not from event payload.
**Warning signs:** Race conditions in tests where different subsystems disagree on current config.

### Pitfall 2: Hook Recursion Spiral
**What goes wrong:** config_reloaded hook modifies config, triggering another reload, triggering another hook, ad infinitum.
**Why it happens:** No recursion depth limit.
**How to avoid:** AtomicU32 recursion counter with max depth 3. RecursionGuard RAII pattern ensures counter is always decremented.
**Warning signs:** CPU spike with hook-related log messages.

### Pitfall 3: Non-Reloadable Config Fields
**What goes wrong:** Hot reload changes bind_address or database_path, but these cannot be applied without restart.
**Why it happens:** Some config binds resources at startup that cannot be re-bound.
**How to avoid:** Maintain a list of non-reloadable fields. On reload, compare old vs new for these fields. If changed, log warning "field X changed but requires restart to take effect."
**Warning signs:** Config appears to reload successfully but behavior doesn't change.

### Pitfall 4: File Watcher on macOS (FSEvents)
**What goes wrong:** macOS FSEvents can be delayed or coalesced, causing missed or duplicate events.
**Why it happens:** macOS FSEvents has different guarantees than Linux inotify.
**How to avoid:** Use notify-debouncer-mini with 500ms debounce (already proven in memory watcher). Accept that config reload may have up to 500ms latency.
**Warning signs:** Tests pass on Linux CI but fail on macOS development machines.

### Pitfall 5: Shell Command Injection via TOML Config
**What goes wrong:** Malicious or misconfigured hook commands could execute arbitrary system commands.
**Why it happens:** Hook commands are shell strings executed via `sh -c`.
**How to avoid:** This is by design -- hooks ARE arbitrary commands defined by the operator (who controls blufio.toml). Document that hook commands have full shell access within PATH restrictions. No sanitization needed since the operator is the trust boundary.
**Warning signs:** None -- this is expected behavior.

### Pitfall 6: Stdin Pipe Deadlock
**What goes wrong:** Large JSON payloads to hook stdin can deadlock if the child process doesn't read stdin before writing stdout (and stdout pipe buffer fills).
**Why it happens:** OS pipe buffers are finite (typically 64KB). If both stdin and stdout are piped, deadlock is possible.
**How to avoid:** Write stdin then close it (drop) BEFORE waiting for stdout. Use wait_with_output() which handles this correctly. Limit stdin JSON size.
**Warning signs:** Hooks hang indefinitely despite timeout.

### Pitfall 7: ArcSwap Load in Hot Path
**What goes wrong:** Calling config_swap.load() on every request creates unnecessary overhead.
**Why it happens:** ArcSwap::load() is cheap but not free -- it involves atomic operations.
**How to avoid:** Load config once at session creation and hold the Arc for the session lifetime. This also naturally implements HTRL-05 (active sessions use old config).
**Warning signs:** Unnecessary Arc clones in profiling.

## Code Examples

### HookConfig TOML Schema
```toml
# Source: Follows CronConfig pattern from blufio-config/model.rs
[hooks]
enabled = true
max_recursion_depth = 3
default_timeout_secs = 30
allowed_path = "/usr/bin:/usr/local/bin"

[[hooks.definitions]]
name = "notify-slack"
event = "session_created"
command = "/usr/local/bin/notify-session.sh"
priority = 10
timeout_secs = 10
enabled = true

[[hooks.definitions]]
name = "backup-pre-compaction"
event = "pre_compaction"
command = "/usr/local/bin/pre-compact-backup.sh"
priority = 1
timeout_secs = 60
enabled = true

[hot_reload]
enabled = true
debounce_ms = 500
# TLS cert paths for hot reload (optional)
tls_cert_path = ""
tls_key_path = ""
watch_skills = true
```

### HookConfig Struct (model.rs)
```rust
// Source: Follows CronConfig pattern
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HookConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_max_recursion_depth")]
    pub max_recursion_depth: u32,
    #[serde(default = "default_hook_timeout_secs")]
    pub default_timeout_secs: u64,
    #[serde(default = "default_allowed_path")]
    pub allowed_path: String,
    #[serde(default)]
    pub definitions: Vec<HookDefinition>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HookDefinition {
    pub name: String,
    pub event: String,
    pub command: String,
    #[serde(default = "default_hook_priority")]
    pub priority: u32,
    #[serde(default = "default_hook_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HotReloadConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
    #[serde(default)]
    pub tls_cert_path: Option<String>,
    #[serde(default)]
    pub tls_key_path: Option<String>,
    #[serde(default = "default_true")]
    pub watch_skills: bool,
}
```

### HookEvent Sub-Enum (events.rs)
```rust
// Source: Follows CronEvent pattern from blufio-bus/events.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HookEvent {
    /// A hook was triggered.
    Triggered {
        event_id: String,
        timestamp: String,
        hook_name: String,
        trigger_event: String,
        priority: u32,
    },
    /// A hook execution completed.
    Completed {
        event_id: String,
        timestamp: String,
        hook_name: String,
        trigger_event: String,
        status: String,       // "success", "failed", "timeout", "skipped"
        duration_ms: u64,
        stdout: Option<String>,
    },
}
```

### blufio-hooks Cargo.toml
```toml
[package]
name = "blufio-hooks"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
publish = false
description = "Shell-based lifecycle hook system for the Blufio agent framework"

[dependencies]
blufio-bus = { path = "../blufio-bus" }
blufio-config = { path = "../blufio-config" }
async-trait.workspace = true
serde.workspace = true
serde_json = "1"
thiserror.workspace = true
tokio = { workspace = true, features = ["sync", "time", "rt", "macros", "process"] }
tokio-util.workspace = true
tracing.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| RwLock<Config> | ArcSwap<Config> | arc-swap stable since 2020 | Lock-free reads, no reader starvation during config reload |
| Manual file polling | notify + debouncer | notify 8.x (2024) | Cross-platform, event-driven, lower CPU usage |
| Full process restart for config | ArcSwap atomic swap | Standard Rust pattern | Zero-downtime config changes |
| axum::serve (plain TCP) | axum-server with rustls | axum-server 0.7 | TLS termination with hot-reloadable certs |

**Deprecated/outdated:**
- notify-debouncer-full: More complex, not needed for simple file watching. notify-debouncer-mini is sufficient.
- arc-swap < 1.0: Pre-1.0 API had different method names. Current 1.8.x is stable.

## Open Questions

1. **TLS Architecture Decision**
   - What we know: The current gateway uses plain `axum::serve` with `TcpListener`. There is no TLS termination in Blufio itself.
   - What's unclear: Does HTRL-02 require adding TLS support to the gateway (axum-server + rustls), or is TLS assumed to be handled by a reverse proxy?
   - Recommendation: Add optional TLS support gated behind config (tls_cert_path + tls_key_path). When paths are set, use axum-server with rustls. When not set, continue with plain TCP. This matches the "require_tls" field in SecurityConfig.

2. **Hook stdout response protocol**
   - What we know: pre_compaction hook could emit a "skip" signal via stdout.
   - What's unclear: Exact format for hook response parsing. JSON? Simple strings?
   - Recommendation: Parse stdout as JSON if it starts with `{`, otherwise treat as plain text. For response hooks (pre_*), check for `{"action": "skip"}` pattern. Document the protocol.

3. **Non-reloadable config field list**
   - What we know: Some fields (bind_address, database_path, storage encryption) cannot be applied at runtime.
   - What's unclear: Complete list of non-reloadable fields.
   - Recommendation: Start with obviously non-reloadable fields: security.bind_address, storage.database_path, gateway.host, gateway.port, agent.log_level (tracing subscriber). Log warnings for changes to these fields on reload.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test + tokio::test |
| Config file | None -- tests use inline setup |
| Quick run command | `cargo test -p blufio-hooks` |
| Full suite command | `cargo test -p blufio-hooks -p blufio-bus -p blufio-config` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| HOOK-01 | 11 lifecycle event types recognized | unit | `cargo test -p blufio-hooks -- hook_event_matching` | No -- Wave 0 |
| HOOK-02 | BTreeMap priority ordering | unit | `cargo test -p blufio-hooks -- priority_ordering` | No -- Wave 0 |
| HOOK-03 | JSON stdin, stdout capture | integration | `cargo test -p blufio-hooks -- shell_execution` | No -- Wave 0 |
| HOOK-04 | Timeout enforcement, PATH restriction | integration | `cargo test -p blufio-hooks -- sandbox` | No -- Wave 0 |
| HOOK-05 | EventBus subscription + dispatch | integration | `cargo test -p blufio-hooks -- event_bus_integration` | No -- Wave 0 |
| HOOK-06 | Recursion depth counter | unit | `cargo test -p blufio-hooks -- recursion_guard` | No -- Wave 0 |
| HTRL-01 | Config parse + validate + swap | unit | `cargo test -p blufio -- hot_reload_config` | No -- Wave 0 |
| HTRL-02 | TLS cert reload | integration | `cargo test -p blufio -- tls_hot_reload` | No -- Wave 0 |
| HTRL-03 | Skill directory rescan + verify | integration | `cargo test -p blufio -- skill_hot_reload` | No -- Wave 0 |
| HTRL-04 | EventBus propagation after swap | unit | `cargo test -p blufio -- config_event_propagation` | No -- Wave 0 |
| HTRL-05 | Session config isolation | unit | `cargo test -p blufio -- session_config_snapshot` | No -- Wave 0 |
| HTRL-06 | config_reloaded hook fires | integration | `cargo test -p blufio-hooks -- config_reloaded_hook` | No -- Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p blufio-hooks --lib`
- **Per wave merge:** `cargo test -p blufio-hooks -p blufio-bus -p blufio-config && cargo clippy -p blufio-hooks -- -D warnings`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/blufio-hooks/` -- entire new crate (lib.rs, manager.rs, executor.rs, recursion.rs, Cargo.toml)
- [ ] `crates/blufio-hooks/src/lib.rs` -- crate root with re-exports and test module
- [ ] Test helper for spawning hook scripts in tempdir (reuse tempfile from workspace)

## Sources

### Primary (HIGH confidence)
- Codebase inspection: blufio-bus/events.rs -- BusEvent enum pattern with 16 existing variants
- Codebase inspection: blufio-bus/lib.rs -- EventBus pub/sub with broadcast + reliable mpsc
- Codebase inspection: blufio-config/model.rs -- CronConfig/RetentionConfig pattern for new config sections
- Codebase inspection: blufio-config/loader.rs -- load_config_from_path() for hot reload parsing
- Codebase inspection: blufio-config/validation.rs -- validate_config() for hot reload validation
- Codebase inspection: blufio-memory/watcher.rs -- notify-debouncer-mini pattern for file watching
- Codebase inspection: blufio-cron/scheduler.rs -- CronScheduler pattern for EventBus subscriber
- Codebase inspection: blufio-skill/store.rs -- SkillStore CRUD for plugin hot reload
- Codebase inspection: blufio-skill/signing.rs -- Ed25519 verification for plugin re-verification
- Codebase inspection: serve.rs -- Subsystem initialization order and EventBus wiring

### Secondary (MEDIUM confidence)
- [arc-swap docs](https://docs.rs/arc-swap/latest/arc_swap/docs/patterns/index.html) - ArcSwap patterns for config hot reload
- [rustls ResolvesServerCert](https://docs.rs/rustls/latest/rustls/server/trait.ResolvesServerCert.html) - TLS cert resolver trait
- [notify-debouncer-mini docs](https://docs.rs/notify-debouncer-mini/latest/notify_debouncer_mini/) - Debouncer API
- [tls-hot-reload crate](https://github.com/sebadob/tls-hot-reload) - Reference implementation for rustls cert hot reload

### Tertiary (LOW confidence)
- TLS hot reload architecture: No existing TLS in gateway. HTRL-02 implementation depends on adding axum-server, which needs verification of compatibility with current axum 0.8 setup.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All libraries verified in workspace or crates.io. ArcSwap is well-established.
- Architecture: HIGH - Follows exact patterns already proven in codebase (CronScheduler, FileWatcher, AuditSubscriber).
- Hook system: HIGH - Straightforward EventBus subscriber + tokio::process::Command pattern.
- Hot reload (config): HIGH - ArcSwap + notify is textbook Rust hot reload.
- Hot reload (TLS): MEDIUM - No existing TLS in gateway. Implementation path clear but requires adding new infra.
- Hot reload (plugins): MEDIUM - Requires understanding wasmtime module reloading behavior.
- Pitfalls: HIGH - Based on direct codebase patterns and well-known Rust async pitfalls.

**Research date:** 2026-03-12
**Valid until:** 2026-04-12 (stable domain, 30-day validity)
