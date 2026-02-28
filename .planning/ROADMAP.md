# Roadmap: Blufio

## Overview

Blufio ships in 10 phases that follow the natural dependency graph: workspace and build pipeline first, then persistence and security foundations, then the core agent loop with Telegram and Anthropic, then the intelligence layer (context engine, memory, model routing), then the skill sandbox and plugin system, and finally production hardening and multi-agent routing. Each phase delivers a coherent, verifiable capability. The architecture is vertical-slice where possible -- complete features over horizontal layers -- with the exception of Phase 1 (foundation) and Phase 9 (hardening) which are cross-cutting by nature.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Project Foundation & Workspace** - Cargo workspace, core traits, config system, build pipeline, licensing
- [x] **Phase 2: Persistence & Security Vault** - SQLite WAL persistence, credential vault, security defaults
- [ ] **Phase 3: Agent Loop & Telegram** - FSM agent loop, Anthropic provider, Telegram adapter, basic CLI
- [ ] **Phase 4: Context Engine & Cost Tracking** - Three-zone context assembly, prompt caching, cost ledger, budget caps
- [ ] **Phase 5: Memory & Embeddings** - ONNX embedding model, semantic memory, hybrid search
- [ ] **Phase 6: Model Routing & Smart Heartbeats** - Query complexity classification, Haiku/Sonnet/Opus routing
- [ ] **Phase 7: WASM Skill Sandbox** - wasmtime sandbox, capability manifests, built-in tools, skill registry
- [ ] **Phase 8: Plugin System & Gateway** - Plugin host, plugin CLI, HTTP/WebSocket gateway
- [ ] **Phase 9: Production Hardening** - systemd, daemon mode, memory bounds, Prometheus, diagnostics, operations
- [ ] **Phase 10: Multi-Agent & Final Integration** - Multi-agent routing, Ed25519 signing, end-to-end validation

## Phase Details

### Phase 1: Project Foundation & Workspace
**Goal**: The project builds, tests, and enforces quality gates from the first commit -- Cargo workspace with core trait definitions, TOML config with strict validation, CI pipeline with license and vulnerability auditing, and dual licensing in place
**Depends on**: Nothing (first phase)
**Requirements**: CORE-05, CORE-06, CLI-06, INFRA-01, INFRA-02, INFRA-03, INFRA-04
**Success Criteria** (what must be TRUE):
  1. `cargo build --release` produces a single static binary with musl linking and jemalloc allocator
  2. `cargo test` runs and passes across all workspace crates with no warnings
  3. TOML configuration with `deny_unknown_fields` rejects invalid config keys at startup with clear error messages
  4. `cargo deny check` passes for license compatibility and `cargo audit` finds no known vulnerabilities
  5. Every source file contains SPDX dual-license header (MIT + Apache-2.0) and LICENSE files exist at repo root
**Plans**: 2 plans

Plans:
- [x] 01-01-PLAN.md -- Cargo workspace, core crate with 7 adapter trait stubs, binary crate with jemalloc, licensing, community docs, cargo-deny, CI pipelines
- [x] 01-02-PLAN.md -- TOML config system with deny_unknown_fields, figment-to-miette error bridge, fuzzy match typo suggestions, XDG lookup, env var overrides

### Phase 2: Persistence & Security Vault
**Goal**: All application state persists in a single SQLite database with WAL mode and ACID guarantees, credentials are encrypted at rest with AES-256-GCM, and security defaults (localhost binding, TLS, secret redaction, SSRF prevention) are enforced from this point forward
**Depends on**: Phase 1
**Requirements**: PERS-01, PERS-02, PERS-03, PERS-04, PERS-05, SEC-01, SEC-03, SEC-04, SEC-08, SEC-09, SEC-10
**Success Criteria** (what must be TRUE):
  1. Sessions, messages, and queue data persist across process restarts -- killing and restarting the process loses zero data
  2. `cp blufio.db blufio.db.bak` creates a complete backup with no coordination or downtime needed
  3. API keys and bot tokens stored in the credential vault are encrypted with AES-256-GCM and the vault key (derived via Argon2id) is never written to disk
  4. The binary binds to 127.0.0.1 by default, all outbound connections require TLS, and secrets are redacted from all log output
  5. Concurrent write operations from multiple sessions never produce SQLITE_BUSY errors (single-writer pattern enforced)
**Plans**: 2 plans

Plans:
- [x] 02-01-PLAN.md -- SQLite persistence layer (blufio-storage crate): WAL mode, embedded migrations, single-writer via tokio-rusqlite, session/message/queue CRUD, StorageAdapter implementation, config/error extensions
- [x] 02-02-PLAN.md -- Credential vault + network security (blufio-vault, blufio-security crates): AES-256-GCM encryption, Argon2id KDF, key wrapping, TLS enforcement, SSRF prevention, secret redaction, plaintext config migration

### Phase 3: Agent Loop & Telegram
**Goal**: A working always-on Telegram bot backed by Claude -- the minimum viable agent that receives messages, assembles basic context, calls Anthropic, and responds, with persistent conversations and graceful shutdown
**Depends on**: Phase 2
**Requirements**: CORE-01, CORE-02, CORE-03, LLM-01, LLM-02, LLM-08, CHAN-01, CHAN-02, CHAN-03, CHAN-04, CLI-01, CLI-05
**Success Criteria** (what must be TRUE):
  1. Sending a text message to the Telegram bot produces a coherent Claude response within seconds, with streaming partial output visible
  2. The agent handles text, images, documents, and voice messages from Telegram (with transcription hook for voice)
  3. Conversations persist across restarts -- rebooting the agent and sending a follow-up message continues the prior conversation
  4. `blufio serve` starts the agent with zero-config defaults (Telegram + Anthropic + SQLite) and `blufio shell` provides an interactive REPL for testing
  5. Sending SIGTERM triggers graceful shutdown -- active sessions drain before exit, no messages are lost
**Plans**: TBD

Plans:
- [ ] 03-01: TBD
- [ ] 03-02: TBD
- [ ] 03-03: TBD

### Phase 4: Context Engine & Cost Tracking
**Goal**: The agent assembles prompts intelligently using three-zone context (static/conditional/dynamic) with Anthropic prompt cache alignment, tracks every token spent across all features, and enforces budget caps with kill switches
**Depends on**: Phase 3
**Requirements**: LLM-03, LLM-04, LLM-07, MEM-04, COST-01, COST-02, COST-03, COST-05, COST-06
**Success Criteria** (what must be TRUE):
  1. System prompt and static context are assembled identically across turns, achieving measurable Anthropic prompt cache hits (target 50-65% cache hit rate)
  2. Token overhead per turn stays at or below 3,000 tokens for simple queries and 5,000 weighted average across all query types
  3. Conversation history automatically compacts (summarizes older turns) when approaching context window limits, without losing critical context
  4. Every token spent (messages, tools, compaction) is tracked in the cost ledger with per-session and per-model attribution visible in real-time
  5. When a configured daily or monthly budget cap is reached, the agent stops making LLM calls and reports the budget exhaustion clearly
**Plans**: TBD

Plans:
- [ ] 04-01: TBD
- [ ] 04-02: TBD

### Phase 5: Memory & Embeddings
**Goal**: The agent remembers long-term facts across conversations using local embedding inference and hybrid search, loading only relevant memories into the context window per-turn
**Depends on**: Phase 4
**Requirements**: MEM-01, MEM-02, MEM-03, MEM-05
**Success Criteria** (what must be TRUE):
  1. The agent recalls facts told in previous conversations (e.g., "my dog's name is Max") when they become relevant in a new conversation
  2. Embedding inference runs locally via ONNX model with zero external API calls -- works fully offline for memory operations
  3. Memory retrieval uses hybrid search (vector similarity + BM25 keyword matching) and returns relevant results within 100ms
  4. Only memories with sufficient semantic similarity to the current turn are loaded into context -- irrelevant memories do not consume tokens
**Plans**: TBD

Plans:
- [ ] 05-01: TBD
- [ ] 05-02: TBD

### Phase 6: Model Routing & Smart Heartbeats
**Goal**: The agent automatically routes queries to the appropriate Claude model (Haiku for simple, Sonnet for standard, Opus for complex) based on query complexity classification, and runs background heartbeats cheaply on Haiku
**Depends on**: Phase 4
**Requirements**: LLM-05, LLM-06
**Success Criteria** (what must be TRUE):
  1. Simple queries ("what time is it?", "hi") are routed to Haiku, standard queries to Sonnet, and complex multi-step reasoning queries to Opus -- verifiable via cost ledger model attribution
  2. Smart heartbeats run on Haiku with skip-when-unchanged logic, costing no more than $10/month for always-on operation
**Plans**: TBD

Plans:
- [ ] 06-01: TBD

### Phase 7: WASM Skill Sandbox
**Goal**: Third-party skills execute in isolated WASM sandboxes with capability manifests, fuel metering, and memory limits -- the agent discovers skills progressively and executes them safely alongside built-in tools
**Depends on**: Phase 4
**Requirements**: SEC-05, SEC-06, SKILL-01, SKILL-02, SKILL-03, SKILL-04, SKILL-05, SKILL-06
**Success Criteria** (what must be TRUE):
  1. Built-in tools (bash execution, HTTP requests, file I/O) work with capability controls -- the agent can execute shell commands and make HTTP requests when permitted
  2. A .wasm skill file executes in an isolated wasmtime sandbox with enforced fuel limits (CPU), memory limits, and epoch interruption -- a malicious skill cannot escape the sandbox or exhaust host resources
  3. Skill capability manifests declare required permissions (network access, filesystem paths, etc.) and the sandbox enforces these declarations -- a skill without network permission cannot make HTTP calls
  4. The agent sees skill names and one-line descriptions in its prompt, and loads full SKILL.md documentation only when invoking a skill (progressive discovery)
  5. `blufio skill init` scaffolds a working skill project and the skill registry tracks installed skills with version, capabilities, and verification status
**Plans**: TBD

Plans:
- [ ] 07-01: TBD
- [ ] 07-02: TBD

### Phase 8: Plugin System & Gateway
**Goal**: The plugin host loads adapter plugins implementing the seven adapter traits (Channel, Provider, Storage, Embedding, Observability, Auth, SkillRuntime), a CLI manages the plugin lifecycle, and an HTTP/WebSocket gateway enables API access alongside channel messaging
**Depends on**: Phase 7
**Requirements**: PLUG-01, PLUG-02, PLUG-03, PLUG-04, INFRA-05
**Success Criteria** (what must be TRUE):
  1. `blufio plugin list` shows installed plugins, `blufio plugin install/remove/update` manages the plugin lifecycle, and `blufio plugin search` discovers available plugins
  2. Plugin manifests (plugin.toml) declare name, version, adapter type, capabilities, and minimum Blufio version -- incompatible plugins are rejected with clear errors
  3. Default install ships with Telegram, Anthropic, SQLite, local ONNX, Prometheus, and device keypair as the standard plugin bundle
  4. HTTP API and WebSocket connections via the axum gateway can send messages and receive responses alongside Telegram channel messaging
**Plans**: TBD

Plans:
- [ ] 08-01: TBD
- [ ] 08-02: TBD

### Phase 9: Production Hardening
**Goal**: The agent runs as a production daemon on a $4/month VPS for months without restart, OOM, or security incident -- with systemd integration, memory bounds, Prometheus observability, full CLI diagnostics, and operational tooling
**Depends on**: Phase 8
**Requirements**: CORE-04, CORE-07, CORE-08, COST-04, SEC-02, CLI-02, CLI-03, CLI-04, CLI-07, CLI-08
**Success Criteria** (what must be TRUE):
  1. The agent runs as a systemd service with health checks and auto-restart on crash -- `systemctl status blufio` shows healthy
  2. Idle memory stays within 50-80MB and memory under load stays within 100-200MB with no unbounded growth over 72+ hours of continuous operation
  3. Prometheus metrics endpoint exports token usage, latency percentiles, error rates, and memory usage -- scrapeable by standard Prometheus setup
  4. `blufio status` shows running agent state, active sessions, memory usage, and cost summary; `blufio doctor` runs full diagnostics (LLM connectivity, DB integrity, channel status); `blufio config get/set/set-secret/validate` manages configuration
  5. Device keypair authentication is required (no optional auth mode), backup/restore and log rotation scripts work, and shell lifecycle hooks execute correctly
**Plans**: TBD

Plans:
- [ ] 09-01: TBD
- [ ] 09-02: TBD
- [ ] 09-03: TBD

### Phase 10: Multi-Agent & Final Integration
**Goal**: Multiple specialized agents can delegate work to each other via Ed25519-signed inter-session messages, and the complete system passes end-to-end integration validation across all 70 v1 requirements
**Depends on**: Phase 9
**Requirements**: SEC-07, INFRA-06
**Success Criteria** (what must be TRUE):
  1. A primary agent can delegate a sub-task to a specialized agent via session-based routing, receive the result, and incorporate it into its response -- with Ed25519 signed messages preventing impersonation
  2. The complete Blufio binary with all default plugins passes end-to-end smoke tests covering: Telegram messaging, persistent conversations, context assembly, memory recall, model routing, skill execution, plugin loading, cost tracking, and Prometheus metrics export
**Plans**: TBD

Plans:
- [ ] 10-01: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3 -> 4 -> 5 -> 6 -> 7 -> 8 -> 9 -> 10
Note: Phases 5, 6, and 7 all depend on Phase 4 and could potentially execute in parallel.

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Project Foundation & Workspace | 2/2 | Complete | 2026-02-28 |
| 2. Persistence & Security Vault | 2/2 | Complete | 2026-02-28 |
| 3. Agent Loop & Telegram | 0/3 | Not started | - |
| 4. Context Engine & Cost Tracking | 0/2 | Not started | - |
| 5. Memory & Embeddings | 0/2 | Not started | - |
| 6. Model Routing & Smart Heartbeats | 0/1 | Not started | - |
| 7. WASM Skill Sandbox | 0/2 | Not started | - |
| 8. Plugin System & Gateway | 0/2 | Not started | - |
| 9. Production Hardening | 0/3 | Not started | - |
| 10. Multi-Agent & Final Integration | 0/1 | Not started | - |
