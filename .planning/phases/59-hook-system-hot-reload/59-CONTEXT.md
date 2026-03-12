# Phase 59: Hook System & Hot Reload - Context

**Gathered:** 2026-03-12
**Status:** Ready for planning

<domain>
## Phase Boundary

Operators can extend Blufio behavior via shell-based lifecycle hooks (11 events, TOML-configured, priority-ordered, sandboxed), and configuration/TLS/plugin changes take effect at runtime without restart via ArcSwap-based hot reload with EventBus propagation.

</domain>

<decisions>
## Implementation Decisions

### Hook System Architecture
- New `blufio-hooks` crate following workspace convention (like blufio-cron, blufio-injection)
- HookConfig and HotReloadConfig types defined in blufio-config/model.rs (following CronConfig/RetentionConfig pattern)
- HookEvent variant added to BusEvent enum in blufio-bus/events.rs (following established sub-enum pattern with String fields)
- Hooks subscribe to EventBus events — HookManager listens to bus, matches events to registered hooks, executes shell commands
- BTreeMap<u32, Vec<HookDefinition>> for priority ordering (lower number = higher priority, same priority = insertion order)

### Hook Execution Model
- Shell-based: hooks are shell commands/scripts specified in TOML
- JSON event context piped to stdin (serde_json::to_string of the triggering BusEvent)
- stdout captured as optional response (e.g., pre_compaction hook could emit skip signal)
- Configurable timeout per hook (default: 30s) with tokio::time::timeout
- Restricted PATH: configurable allowed directories, defaults to /usr/bin:/usr/local/bin
- Optional network isolation: hooks can be restricted from network access (platform-dependent)

### Recursion Prevention
- Global AtomicU32 recursion depth counter
- Hook execution increments on entry, decrements on exit
- Configurable max depth (default: 3) — exceeding logs warning and skips hook
- Prevents hook-triggered-hook infinite loops (e.g., config_reloaded hook modifies config)

### 11 Lifecycle Events
- pre_start, post_start: before/after serve initialization
- pre_shutdown, post_shutdown: before/after graceful shutdown
- session_created, session_closed: session lifecycle
- pre_compaction, post_compaction: compaction lifecycle
- degradation_changed: resilience level changes
- config_reloaded: fires after successful hot reload
- memory_extracted: fires after memory extraction/save

### Hot Reload Architecture
- ArcSwap for BlufioConfig — atomic pointer swap, readers never block
- File watcher (notify crate, already in workspace from memory watcher) on blufio.toml
- Reload flow: detect change -> parse TOML -> validate (deny_unknown_fields) -> ArcSwap swap -> emit ConfigEvent::Reloaded on EventBus
- Active sessions continue with config snapshot at session start; new sessions pick up reloaded config
- Validation failure: log error, keep current config, do NOT swap

### TLS Certificate Hot Reload
- rustls ResolvesServerCert trait implementation with Arc<ArcSwap<CertifiedKey>>
- File watcher on cert/key paths from TLS config
- On change: reload cert chain + private key, validate, swap
- No connection interruption — existing connections continue with old cert, new connections use new cert

### Plugin/Skill Hot Reload
- File watcher on skill directory (from existing SkillConfig)
- On change: re-scan directory, detect new/modified/removed .wasm files
- Re-verify Ed25519 signatures on changed modules (existing verification gate)
- Update skill registry in-place — remove stale, add new, replace modified
- Emit ConfigEvent on EventBus after skill reload

### Claude's Discretion
- Exact hook stdin JSON schema (beyond serialized BusEvent)
- Hook stdout parsing format for response hooks
- File watcher debounce timing for config reload (500ms recommended, matching memory watcher)
- Whether to create a separate blufio-hooks crate or add to blufio-cron

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `notify` crate: Already used in blufio-memory/src/watcher.rs for file watching with debounce
- `EventBus` (blufio-bus): Established pub/sub pattern — hooks subscribe to bus events
- `BusEvent` enum: 16 existing variants — add HookEvent following same pattern
- `CancellationToken`: Used throughout for graceful shutdown — hook manager needs one
- `blufio-config/model.rs`: Config types pattern (serde(default), Option fields) well established
- `tokio::process::Command`: Available for async shell execution

### Established Patterns
- Sub-enum with String fields for bus events (avoids cross-crate deps)
- Optional<Arc<EventBus>> pattern for testability (None in tests/CLI)
- Non-fatal init in serve.rs (warn + continue, following cron/audit pattern)
- serde(default) on top-level config sections for backward compat

### Integration Points
- serve.rs: Hook manager init after EventBus, before main loop (like CronScheduler)
- serve.rs: Config hot reload watcher spawned as background task
- blufio-config/model.rs: HookConfig, HotReloadConfig sections in BlufioConfig
- blufio-bus/events.rs: HookEvent variant on BusEvent
- doctor.rs: Health checks for hook system status and config reload status

</code_context>

<specifics>
## Specific Ideas

No specific requirements — all implementation decisions derive from the detailed requirements (HOOK-01 through HOOK-06, HTRL-01 through HTRL-06) and established codebase patterns.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 59-hook-system-hot-reload*
*Context gathered: 2026-03-12*
