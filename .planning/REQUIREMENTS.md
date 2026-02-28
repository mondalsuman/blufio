# Requirements: Blufio

**Defined:** 2026-02-28
**Core Value:** An always-on personal AI agent that is secure enough to trust, efficient enough to afford, and simple enough to deploy by copying one file.

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Core Agent

- [ ] **CORE-01**: Agent executes FSM-per-session loop: receive → assemble context → call LLM → execute tools → respond
- [ ] **CORE-02**: Agent handles streaming responses from LLM providers with partial output delivery
- [ ] **CORE-03**: Agent gracefully shuts down on SIGTERM, draining active sessions before exit
- [ ] **CORE-04**: Agent runs as background daemon, auto-restarts on crash via systemd
- [ ] **CORE-05**: Binary ships as single static executable (~25MB core) with musl static linking
- [ ] **CORE-06**: Process uses jemalloc allocator with bounded LRU caches, bounded channels (backpressure), and lock timeouts
- [ ] **CORE-07**: Idle memory stays within 50-80MB (including embedding model weights)
- [ ] **CORE-08**: Memory under load stays within 100-200MB with no unbounded growth

### LLM Provider

- [ ] **LLM-01**: Provider trait abstracts LLM interaction (complete, stream, embed) behind pluggable interface
- [ ] **LLM-02**: Anthropic provider adapter supports Claude models with streaming and tool calling
- [ ] **LLM-03**: Three-zone context engine assembles prompts from static (system prompt, cached), conditional (skills/memory per-relevance), and dynamic (current turn) zones
- [ ] **LLM-04**: Context engine aligns prompt structure to exploit Anthropic prompt caching (target 50-65% cache hit rate)
- [ ] **LLM-05**: Model router classifies query complexity and routes to Haiku (simple), Sonnet (standard), or Opus (complex)
- [ ] **LLM-06**: Smart heartbeats run on Haiku with skip-when-unchanged logic, costing ≤$10/month
- [ ] **LLM-07**: Token overhead per turn stays ≤3,000 for simple queries and ≤5,000 weighted average
- [ ] **LLM-08**: System prompt and agent personality are configurable via TOML + optional markdown files

### Channel

- [ ] **CHAN-01**: Telegram channel adapter receives and sends messages via Telegram Bot API
- [ ] **CHAN-02**: Channel adapter trait (`ChannelAdapter`) enables future channel plugins without core changes
- [ ] **CHAN-03**: Telegram adapter handles message types: text, images, documents, voice (with transcription hook)
- [ ] **CHAN-04**: Telegram adapter implements reliable long-polling with automatic reconnection

### Persistence

- [ ] **PERS-01**: All state stored in single SQLite database with WAL mode and ACID transactions
- [ ] **PERS-02**: Sessions persist across restarts — user can resume conversation after reboot
- [ ] **PERS-03**: Message queue is SQLite-backed and crash-safe — zero message loss on crash
- [ ] **PERS-04**: Backup is `cp blufio.db blufio.db.bak` — single file, no coordination needed
- [ ] **PERS-05**: Single-writer-thread pattern prevents SQLITE_BUSY under concurrent sessions

### Memory

- [ ] **MEM-01**: Memory system stores and retrieves long-term facts using hybrid search (vector + BM25)
- [ ] **MEM-02**: Local ONNX embedding model runs inference without external API calls
- [ ] **MEM-03**: Context engine loads only relevant memories per-turn based on semantic similarity
- [ ] **MEM-04**: Conversation history compacts automatically when approaching context window limits
- [ ] **MEM-05**: Memory embeddings stored in SQLite with efficient cosine similarity search

### Security

- [ ] **SEC-01**: Binary binds to 127.0.0.1 by default — no open ports to the internet
- [ ] **SEC-02**: Device keypair authentication required — no optional auth mode
- [ ] **SEC-03**: AES-256-GCM encrypted credential vault stores all API keys and bot tokens
- [ ] **SEC-04**: Vault key derived from passphrase via Argon2id — never stored on disk
- [ ] **SEC-05**: WASM skill sandbox (wasmtime) with capability manifests — skills cannot escape sandbox
- [ ] **SEC-06**: WASM sandbox enforces fuel limits (CPU), memory limits, and epoch interruption
- [ ] **SEC-07**: Ed25519 signed inter-agent messages — prevents impersonation in multi-agent setups
- [ ] **SEC-08**: Secrets redacted from all logs and persisted data before storage
- [ ] **SEC-09**: SSRF prevention (private IP blocking) enabled by default
- [ ] **SEC-10**: TLS required for all remote connections

### Cost & Observability

- [ ] **COST-01**: Unified cost ledger tracks every token across all features (messages, heartbeats, tools, compaction)
- [ ] **COST-02**: Per-session and per-model cost attribution in real-time
- [ ] **COST-03**: Configurable daily and monthly budget caps with hard kill switch when exhausted
- [ ] **COST-04**: Prometheus metrics endpoint exports token usage, latency percentiles, error rates, memory usage
- [ ] **COST-05**: Structured error handling with Result<T,E> everywhere — zero empty catch blocks
- [ ] **COST-06**: All errors logged with context using tracing crate — structured, filterable

### Skills & Tools

- [ ] **SKILL-01**: Built-in tools: bash execution, HTTP requests, file I/O with capability controls
- [ ] **SKILL-02**: WASM skill sandbox executes third-party skills in isolated wasmtime instances
- [ ] **SKILL-03**: Skill capability manifests declare required permissions (network, filesystem paths, etc.)
- [ ] **SKILL-04**: Progressive skill discovery: agent sees skill names + descriptions in prompt, loads full SKILL.md on demand
- [ ] **SKILL-05**: Skill registry tracks installed skills with version, capabilities, and verification status
- [ ] **SKILL-06**: `blufio skill init` creates working skill scaffold in 3 commands

### Plugin System

- [ ] **PLUG-01**: Plugin host loads adapter plugins implementing Channel, Provider, Storage, Embedding, Observability, Auth traits
- [ ] **PLUG-02**: `blufio plugin list/search/install/remove/update` CLI commands for plugin management
- [ ] **PLUG-03**: Plugin manifest (`plugin.toml`) declares name, version, adapter type, capabilities, minimum Blufio version
- [ ] **PLUG-04**: Default install ships with: Telegram, Anthropic, SQLite, local ONNX, Prometheus, device keypair

### CLI & Operations

- [ ] **CLI-01**: `blufio serve` starts the agent with zero-config defaults (Telegram + Anthropic + SQLite)
- [ ] **CLI-02**: `blufio status` shows running agent state, active sessions, memory usage, cost summary
- [ ] **CLI-03**: `blufio config` manages TOML configuration with `get/set/set-secret/validate` subcommands
- [ ] **CLI-04**: `blufio doctor` runs diagnostics: LLM connectivity, DB integrity, channel status, memory usage
- [ ] **CLI-05**: `blufio shell` provides interactive REPL for testing agent responses
- [ ] **CLI-06**: TOML config with deny_unknown_fields catches typos at startup
- [ ] **CLI-07**: systemd unit file with health checks and auto-restart
- [ ] **CLI-08**: Shell automation scripts for backup, log rotation, and lifecycle hooks

### Infrastructure

- [ ] **INFRA-01**: Dual-license MIT + Apache-2.0 from first commit with SPDX headers
- [ ] **INFRA-02**: cargo-deny.toml enforces license compatibility in CI
- [ ] **INFRA-03**: cargo-audit runs in CI for vulnerability scanning
- [ ] **INFRA-04**: CONTRIBUTING.md, CODE_OF_CONDUCT.md, SECURITY.md, GOVERNANCE.md from day one
- [ ] **INFRA-05**: HTTP/WebSocket gateway (axum) for API access alongside channel messaging
- [ ] **INFRA-06**: Multi-agent routing with session-based delegation between specialized agents

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Channels

- **CHAN-10**: Discord channel adapter via plugin
- **CHAN-11**: WhatsApp channel adapter via plugin (Meta business verification required)
- **CHAN-12**: Signal channel adapter via plugin (AGPL-isolated binary)
- **CHAN-13**: Slack, Matrix, IRC channel adapters via plugin

### Providers

- **LLM-10**: OpenAI provider adapter (GPT models)
- **LLM-11**: Ollama provider adapter (local inference)
- **LLM-12**: Google/Groq/DeepSeek provider adapters

### Advanced

- **ADV-01**: DAG workflow engine for complex multi-step orchestration
- **ADV-02**: MCP server/client for Model Context Protocol interoperability
- **ADV-03**: Client SDKs (Python, TypeScript, Go)
- **ADV-04**: Web UI / admin dashboard
- **ADV-05**: OpenClaw skill migration shim (JS subprocess + JSON-RPC)
- **ADV-06**: Multi-node distributed mode
- **ADV-07**: International PII detection pattern packs (Nordic, EU)

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Visual/GUI workflow builder | Target audience is developers. CLI + TOML. Massively increases scope. |
| 15+ channels at launch | Each channel is 2-4 weeks. One channel done well beats five done poorly. |
| Windows native builds | WSL2 is the path. Cross-compilation maintenance burden too high. |
| RAG pipeline with document ingestion | RAG is a product in itself. Memory system + HTTP tools cover the use case. |
| Real-time collaborative editing | CRDT/OT complexity for niche use case. Single-operator sessions. |
| SOC 2 / HIPAA compliance tooling | Business process, not feature. Security architecture provides foundation. |
| Native plugin system (libloading) | No sandbox guarantees. WASM-only for v1.0. |
| Voice-first interface | Audio pipeline is a separate product. Telegram voice + transcription hook suffices. |
| Bug bounty program | Requires funding. Responsible disclosure via SECURITY.md. |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| (populated during roadmap creation) | | |

**Coverage:**
- v1 requirements: 52 total
- Mapped to phases: 0
- Unmapped: 52 (pending roadmap)

---
*Requirements defined: 2026-02-28*
*Last updated: 2026-02-28 after initial definition*
