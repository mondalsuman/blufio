# Blufio

## What This Is

Blufio is a ground-up Rust + Shell AI agent platform built to kill OpenClaw. It ships as a single static binary (~25MB core, ~50MB with all plugins), uses SQLite for all persistence, enforces security by default, and achieves 68-84% token reduction through smart context injection, cache-aligned prompts, and model routing. Everything that makes OpenClaw useful — multi-channel messaging, persistent memory, skill ecosystem, multi-agent routing, always-on operation — rebuilt from scratch with an architecture that doesn't rot.

## Core Value

An always-on personal AI agent that is secure enough to trust, efficient enough to afford, and simple enough to deploy by copying one file. If everything else fails, the core agent loop (receive message → assemble context → call LLM → execute tools → respond) must work reliably on a $4/month VPS for months without restart, OOM, or security incident.

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] Single static Rust binary with plugin-composed architecture
- [ ] Agent loop with FSM-per-session, LLM provider abstraction (Anthropic at launch)
- [ ] Telegram channel adapter (built-in; other channels as plugins)
- [ ] SQLite WAL-mode persistence for sessions, queue, cron, cost, memory (ACID)
- [ ] AES-256-GCM encrypted credential vault
- [ ] Three-zone context engine (static/conditional/dynamic) with cache alignment
- [ ] WASM skill sandbox (wasmtime) with capability manifests
- [ ] Progressive skill discovery (names + descriptions in prompt; full SKILL.md loaded on demand)
- [ ] Unified cost ledger with budget caps and kill switches
- [ ] Model routing (Haiku/Sonnet/Opus based on query complexity)
- [ ] Smart heartbeats (Haiku, skip-when-unchanged)
- [ ] Plugin host with adapter traits: Channel, Provider, Storage, Embedding, Observability, Auth, SkillRuntime
- [ ] HTTP/WebSocket gateway (axum + tokio)
- [ ] CLI interface (serve, status, config, shell, plugin, skill, doctor)
- [ ] Multi-agent routing with Ed25519 signed inter-session messages
- [ ] Prometheus metrics export
- [ ] Device keypair authentication (secure by default)
- [ ] Bounded everything: caches (LRU), channels (backpressure), locks (timeouts), network (timeouts)
- [ ] systemd integration, health checks, backup/restore
- [ ] Shell automation layer (lifecycle scripts, log rotation, hooks)
- [ ] Official skill registry with verified signatures
- [ ] TOML config with strict validation (deny_unknown_fields)
- [ ] Dual-license MIT + Apache-2.0

### Out of Scope

- Visual builder / GUI — CLI and config files only
- More than 2-4 channels at launch — Telegram first, expand later
- SOC 2 / HIPAA compliance tooling — post-v1.0
- DAG workflow engine — post-v1.0
- Client SDKs (Python, TypeScript, Go) — post-v1.0
- Multi-node sharding — single-instance only for v1.0
- MCP server/client — post-v1.0
- Native plugin system (libloading) — WASM-only for v1.0
- Windows native builds — WSL2 is the path
- Full OpenAI-compatible API — basic subset in v1.0

## Context

### The Kill Shot

OpenClaw is the incumbent — 15+ messaging channels, thousands of community skills, active development. But it has structural rot that can't be fixed incrementally:

1. **Memory leaks** — Node.js process grows to 300-800MB in 24h. No eviction policy on in-memory caches. OOM crashes documented across GitHub issues.
2. **Token waste** — ~35K tokens/turn injected regardless of query complexity. Heartbeats at full context cost $769/month on Opus. No cost tracking.
3. **Security defaults** — Binds 0.0.0.0, auth optional, plaintext credentials, skills run with full process access. CVEs have enabled token theft and RCE.
4. **Persistence fragility** — JSONL files with PID-based locks. In-memory queue loses messages on crash. No ACID.
5. **Silent errors** — Dozens of empty `catch {}` blocks in critical paths. Stale locks accumulate. No logs, no metrics.
6. **npm supply chain** — Hundreds of transitive dependencies. Each is an attack surface.

These aren't bugs. They're architecture. You can't fix them with PRs — you replace the foundation.

### Technical Stack

- **Language**: Rust (2021 edition) with tokio async runtime
- **Web framework**: axum for HTTP/WebSocket gateway
- **Database**: SQLite (rusqlite) with WAL mode, SQLCipher for encryption
- **LLM**: Anthropic API at launch (provider abstraction for OpenAI, Ollama later)
- **Messaging**: Telegram Bot API at launch (teloxide or custom)
- **WASM**: wasmtime for skill sandboxing
- **Embedding**: Local ONNX model (~80MB) via Candle
- **Metrics**: Prometheus via metrics crate
- **Config**: TOML with serde + deny_unknown_fields
- **Crypto**: AES-256-GCM (ring or RustCrypto), Ed25519 for signing
- **Memory allocator**: jemalloc (tikv-jemallocator) for predictable allocation

### Development Force

Solo operator + Claude Code + 5 coding bots. Equivalent to 30+ human developers. Targeting 2-week sprint for first working agent (Telegram + Anthropic + SQLite + CLI).

### Architecture Philosophy

Everything is a plugin. The core is a thin agent loop + plugin host. Channels, providers, storage, embedding, observability, auth — all implement adapter traits. Default install ships Telegram + Anthropic + SQLite + local ONNX + Prometheus + device keypair. Everything else is `blufio plugin install` away.

Progressive disclosure everywhere: operators start with `blufio serve` (zero config defaults), agents see skill names not full definitions, contributors see README not architecture docs.

## Constraints

- **Binary size**: Core ~25MB (minimal), ~50MB with all official plugins
- **Memory**: 50-80MB idle (including embedding model), 100-200MB under load, bounded
- **Startup**: 2-5 seconds cold start (SQLCipher KDF + model load + TLS)
- **License**: MIT + Apache-2.0 dual license. All deps must pass cargo-deny audit. presage (Signal, AGPL) isolated in separate binary.
- **Platform**: Linux (musl static) for production. macOS (dynamic) for development only.
- **Dependencies**: <80 direct crates for tractable audit surface

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Rust over TypeScript | Memory safety without GC, fearless concurrency, single binary, compile-time guarantees. 2-3x slower to write but eliminates entire bug classes. | — Pending |
| SQLite over Postgres | Single file, zero dependencies, embedded, ACID with WAL. Backup = cp. Scales to 10-50 concurrent sessions. | — Pending |
| Everything-is-a-plugin | Operators customize install without Rust toolchain. ~2-5% overhead on plugin calls (negligible for I/O-bound adapters). Binary stays small. | — Pending |
| WASM-only skills for v1.0 | Sandbox guarantees matter more than native performance for third-party code. Script tier (subprocess) as escape hatch. | — Pending |
| Three-zone context engine | Static (system prompt, cached), conditional (loaded per-relevance), dynamic (current turn). Achieves 68-84% token reduction vs inject-everything. | — Pending |
| Dual-license MIT + Apache-2.0 | Apache-2.0 patent protection for enterprise. Both permissive. Must be from first commit — retroactive licensing is legally messy. | — Pending |
| Telegram first | Largest AI agent user base. Simple Bot API. One channel done well beats five done poorly. | — Pending |
| Kill shot positioning | OpenClaw has structural rot. We don't coexist — we replace. Let the architecture speak. | — Pending |

---
*Last updated: 2026-02-28 after initialization*
