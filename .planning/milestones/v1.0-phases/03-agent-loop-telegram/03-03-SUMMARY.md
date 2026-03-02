---
phase: 03-agent-loop-telegram
plan: 03
subsystem: agent-loop
tags: [agent-loop, session-fsm, context-assembly, graceful-shutdown, serve, shell, repl]

requires: [03-01, 03-02]
provides:
  - blufio-agent crate with AgentLoop coordinator and SessionActor FSM
  - Context assembly with system prompt + last 20 messages
  - Graceful shutdown with SIGTERM/SIGINT signal handling and session draining
  - blufio serve command wiring Telegram + Anthropic + SQLite
  - blufio shell interactive REPL with colored prompt and streaming output
affects: [cli, agent-runtime]

tech-stack:
  added: [tokio-util-0.7, uuid-1, chrono-0.4, futures-0.3, tracing-subscriber-0.3, rustyline-14, colored-2, base64-0.22]
  patterns: [session-fsm, edit-in-place-streaming, cancellation-token, crash-recovery-stale-sessions, repl-readline]

key-files:
  created:
    - crates/blufio-agent/Cargo.toml
    - crates/blufio-agent/src/lib.rs
    - crates/blufio-agent/src/context.rs
    - crates/blufio-agent/src/session.rs
    - crates/blufio-agent/src/shutdown.rs
    - crates/blufio/src/serve.rs
    - crates/blufio/src/shell.rs
  modified:
    - Cargo.toml
    - crates/blufio/Cargo.toml
    - crates/blufio/src/main.rs

key-decisions:
  - "Session lookup key is channel:sender_id for in-memory map, falling back to storage query on miss"
  - "Edit-in-place streaming sends initial message then edits it as tokens arrive (Telegram pattern)"
  - "Non-edit channels accumulate full response then send as single message"
  - "Stale sessions from previous crash marked 'interrupted' on startup before agent loop starts"
  - "Shell creates new session per invocation (no session resume in CLI mode)"
  - "base64 dependency added to blufio-agent for image content block encoding"
  - "tokio fs feature required for system prompt file loading in context.rs"

patterns-established:
  - "AgentLoop::run() uses tokio::select! with cancel token for graceful shutdown"
  - "SessionActor FSM: Idle -> Receiving -> Processing -> Responding -> Idle (Draining for shutdown)"
  - "Context assembly loads last 20 messages from storage and appends current inbound"
  - "Message content to text conversion for storage persistence (images -> caption, voice -> duration)"
  - "Signal handler spawns background task, returns CancellationToken for caller to monitor"
  - "Shell REPL uses rustyline DefaultEditor with /quit and /exit commands"
  - "tracing-subscriber with EnvFilter for configurable log levels"

requirements-completed: [CORE-01, CORE-03, CLI-01, CLI-05]

duration: 20min
completed: 2026-03-01
---

# Plan 03-03: Agent Loop & CLI Wiring Summary

**Agent loop coordinator, session FSM, context assembly, graceful shutdown, serve command, and shell REPL**

## Performance

- **Duration:** ~20 min
- **Completed:** 2026-03-01
- **Tasks:** 2
- **Tests:** 230 total (17 new in blufio-agent, 230 across workspace)
- **Clippy:** Clean

## Accomplishments

- **blufio-agent crate** with complete agent loop coordination:
  - AgentLoop: main coordinator with tokio::select! loop
  - SessionActor: per-session FSM managing message lifecycle
  - Context assembly: system prompt + last 20 messages from storage
  - Graceful shutdown: SIGTERM/SIGINT handlers with CancellationToken
  - Session draining on shutdown with configurable timeout
  - Edit-in-place streaming for channels that support message editing
  - Session resolution: in-memory cache -> storage lookup -> create new
  - Crash recovery: stale active sessions marked as interrupted on startup

- **serve.rs** -- full `blufio serve` command:
  - Initializes SQLite storage with crash recovery
  - Creates Anthropic provider with API key resolution
  - Creates Telegram channel with bot token
  - Installs signal handlers for graceful shutdown
  - Runs agent loop until cancellation

- **shell.rs** -- interactive `blufio shell` REPL:
  - Colored prompt with rustyline readline support
  - Streaming LLM output printed character-by-character
  - Session persistence to SQLite
  - Clean session closure on exit
  - /quit and /exit commands

## Task Commits

1. **Task 1: blufio-agent crate** -- context.rs, session.rs, shutdown.rs, lib.rs
2. **Task 2: CLI wiring** -- serve.rs, shell.rs, main.rs updates

## Files Created/Modified

### Created
- `crates/blufio-agent/` - 4 source files (context, session, shutdown, lib) + Cargo.toml
- `crates/blufio/src/serve.rs` - serve command implementation
- `crates/blufio/src/shell.rs` - shell REPL implementation

### Modified
- `Cargo.toml` - Added workspace deps (tokio-util, uuid, chrono, futures, tracing-subscriber, rustyline, colored)
- `crates/blufio/Cargo.toml` - Added agent, anthropic, telegram, and utility dependencies
- `crates/blufio/src/main.rs` - Wired serve and shell modules, replaced placeholder messages

## Deviations from Plan

### Auto-fixed Issues

**1. tokio::fs feature not enabled**
- **Found during:** Task 1 compilation
- **Issue:** context.rs uses tokio::fs::read_to_string but `fs` feature was not in Cargo.toml
- **Fix:** Added `fs` to tokio features list in blufio-agent Cargo.toml
- **Verification:** All tests pass

**2. Type annotation needed for content.trim()**
- **Found during:** Task 1 compilation
- **Issue:** Compiler could not infer type for `content.trim().to_string()` inside match arm
- **Fix:** Added explicit type annotation `let trimmed: String = content.trim().to_string()`
- **Verification:** All tests pass

**3. ChannelAdapter trait not in scope in serve.rs**
- **Found during:** Task 2 compilation
- **Issue:** `channel.connect()` failed because ChannelAdapter trait was not imported
- **Fix:** Added `use blufio_core::ChannelAdapter` import in serve.rs
- **Verification:** Full workspace builds and tests pass

---

**Total deviations:** 3 auto-fixed (missing feature flag, type inference, trait import)
**Impact on plan:** No scope change. All deviations were trivial compilation fixes.

## Architecture Summary

```
blufio serve
  |
  +-> SqliteStorage::initialize() -- crash recovery (mark stale sessions)
  +-> AnthropicProvider::new() -- API key resolution
  +-> TelegramChannel::connect() -- start long polling
  +-> AgentLoop::run(cancel_token)
       |
       +-> channel.receive() -------> handle_inbound()
       |                                   |
       |                                   +-> resolve_or_create_session()
       |                                   +-> session_actor.handle_message()
       |                                   |       +-> persist user message
       |                                   |       +-> assemble_context()
       |                                   |       +-> provider.stream()
       |                                   +-> consume stream (edit-in-place)
       |                                   +-> session_actor.persist_response()
       |
       +-> cancel.cancelled() ------> drain_sessions() -> storage.close()
```

## Next Phase Readiness
- Full agent pipeline works: Telegram -> Agent Loop -> Anthropic -> Telegram
- Shell REPL works: stdin -> Agent Loop -> Anthropic -> stdout
- All 230 workspace tests pass
- Binary shows serve, shell, config commands via `blufio --help`
- Phase 3 complete: ready for milestone verification

---
*Plan: 03-03-agent-loop-cli-wiring*
*Completed: 2026-03-01*
