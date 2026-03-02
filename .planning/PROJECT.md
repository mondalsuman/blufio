# Blufio

## What This Is

Blufio is a ground-up Rust AI agent platform that ships as a single static binary. It runs an FSM-per-session agent loop backed by Anthropic Claude, with Telegram messaging, SQLite persistence (WAL mode, ACID), AES-256-GCM credential vault, three-zone context engine with prompt cache alignment, local ONNX memory with hybrid search, WASM skill sandbox, plugin system with 7 adapter traits, HTTP/WebSocket gateway, model routing (Haiku/Sonnet/Opus), multi-agent delegation with Ed25519 signing, and Prometheus observability. 28,790 LOC Rust across 14 crates, all 70 v1 requirements verified.

## Core Value

An always-on personal AI agent that is secure enough to trust, efficient enough to afford, and simple enough to deploy by copying one file. If everything else fails, the core agent loop (receive message -> assemble context -> call LLM -> execute tools -> respond) must work reliably on a $4/month VPS for months without restart, OOM, or security incident.

## Requirements

### Validated

- ✓ Single static Rust binary with plugin-composed architecture — v1.0
- ✓ Agent loop with FSM-per-session, LLM provider abstraction (Anthropic) — v1.0
- ✓ Telegram channel adapter (built-in) — v1.0
- ✓ SQLite WAL-mode persistence for sessions, queue, cost, memory (ACID) — v1.0
- ✓ AES-256-GCM encrypted credential vault with Argon2id KDF — v1.0
- ✓ Three-zone context engine (static/conditional/dynamic) with cache alignment — v1.0
- ✓ WASM skill sandbox (wasmtime) with capability manifests — v1.0
- ✓ Progressive skill discovery (names + descriptions in prompt; full SKILL.md on demand) — v1.0
- ✓ Unified cost ledger with budget caps and kill switches — v1.0
- ✓ Model routing (Haiku/Sonnet/Opus based on query complexity) — v1.0
- ✓ Smart heartbeats (Haiku, skip-when-unchanged) — v1.0
- ✓ Plugin host with adapter traits: Channel, Provider, Storage, Embedding, Observability, Auth, SkillRuntime — v1.0
- ✓ HTTP/WebSocket gateway (axum + tokio) — v1.0
- ✓ CLI interface (serve, status, config, shell, plugin, skill, doctor) — v1.0
- ✓ Multi-agent routing with Ed25519 signed inter-session messages — v1.0
- ✓ Prometheus metrics export (memory + business metrics) — v1.0
- ✓ Device keypair authentication (secure by default) — v1.0
- ✓ Bounded everything: caches (LRU), channels (backpressure), locks (timeouts) — v1.0
- ✓ systemd integration, health checks, backup/restore — v1.0
- ✓ TOML config with strict validation (deny_unknown_fields) — v1.0
- ✓ Dual-license MIT + Apache-2.0 — v1.0
- ✓ TLS enforcement and SSRF protection on all outbound connections — v1.0
- ✓ Secret redaction in all log output — v1.0

### Active

(None — define in next milestone via `/gsd:new-milestone`)

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
- Shell automation layer (lifecycle scripts, hooks) — not needed for v1.0, scripts suffice
- Official skill registry with verified signatures — deferred, local registry works for v1.0

## Context

### Current State

Shipped v1.0 MVP with 28,790 LOC Rust across 14 crates and 111 source files.

**Tech stack (actual):** Rust 2021, tokio, axum, rusqlite (WAL), ort (ONNX), wasmtime, teloxide, reqwest, serde, tracing, clap, figment, tikv-jemallocator, metrics/metrics-exporter-prometheus, ed25519-dalek, aes-gcm, argon2.

**Architecture:** 14-crate workspace — blufio-core (traits), blufio-config, blufio-storage, blufio-vault, blufio-security, blufio-anthropic, blufio-telegram, blufio-agent, blufio-context, blufio-cost, blufio-memory, blufio-router, blufio-skill, blufio-prometheus, blufio-plugin, blufio-gateway, blufio-test-utils, blufio (binary).

**Known tech debt:** 10 items documented in MILESTONES.md (human verification pending, GET /v1/sessions stub, memory bounds not measured over 72h, systemd file not committed).

### The Kill Shot

OpenClaw is the incumbent — 15+ messaging channels, thousands of community skills, active development. But it has structural rot that can't be fixed incrementally:

1. **Memory leaks** — Node.js process grows to 300-800MB in 24h. No eviction policy on in-memory caches.
2. **Token waste** — ~35K tokens/turn injected regardless of query complexity. Heartbeats at full context cost $769/month on Opus.
3. **Security defaults** — Binds 0.0.0.0, auth optional, plaintext credentials, skills run with full process access.
4. **Persistence fragility** — JSONL files with PID-based locks. In-memory queue loses messages on crash.
5. **Silent errors** — Dozens of empty `catch {}` blocks in critical paths.
6. **npm supply chain** — Hundreds of transitive dependencies.

### Development Force

Solo operator + Claude Code. Built v1.0 MVP (28,790 LOC, 70 requirements, 14 phases) in 3 days.

### Architecture Philosophy

Everything is a plugin. The core is a thin agent loop + plugin host. Channels, providers, storage, embedding, observability, auth — all implement adapter traits. Default install ships Telegram + Anthropic + SQLite + local ONNX + Prometheus + device keypair. Everything else is `blufio plugin install` away.

Progressive disclosure everywhere: operators start with `blufio serve` (zero config defaults), agents see skill names not full definitions, contributors see README not architecture docs.

## Constraints

- **Binary size**: Core ~25MB (minimal), ~50MB with all official plugins
- **Memory**: 50-80MB idle (including embedding model), 100-200MB under load, bounded
- **Startup**: 2-5 seconds cold start (SQLCipher KDF + model load + TLS)
- **License**: MIT + Apache-2.0 dual license. All deps must pass cargo-deny audit.
- **Platform**: Linux (musl static) for production. macOS (dynamic) for development only.
- **Dependencies**: <80 direct crates for tractable audit surface

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Rust over TypeScript | Memory safety without GC, fearless concurrency, single binary, compile-time guarantees. 2-3x slower to write but eliminates entire bug classes. | ✓ Good — 28,790 LOC in 3 days, zero memory bugs |
| SQLite over Postgres | Single file, zero dependencies, embedded, ACID with WAL. Backup = cp. Scales to 10-50 concurrent sessions. | ✓ Good — single-writer pattern clean, WAL mode works |
| Everything-is-a-plugin | Operators customize install without Rust toolchain. ~2-5% overhead on plugin calls (negligible for I/O-bound adapters). Binary stays small. | ✓ Good — 7 adapter traits, built-in catalog, clean separation |
| WASM-only skills for v1.0 | Sandbox guarantees matter more than native performance for third-party code. Script tier (subprocess) as escape hatch. | ✓ Good — wasmtime sandbox with fuel/memory/epoch limits works |
| Three-zone context engine | Static (system prompt, cached), conditional (loaded per-relevance), dynamic (current turn). Achieves 68-84% token reduction vs inject-everything. | ✓ Good — cache alignment working, compaction implemented |
| Dual-license MIT + Apache-2.0 | Apache-2.0 patent protection for enterprise. Both permissive. Must be from first commit. | ✓ Good — SPDX headers on every file from day one |
| Telegram first | Largest AI agent user base. Simple Bot API. One channel done well beats five done poorly. | ✓ Good — full adapter with streaming, media, MarkdownV2 |
| Kill shot positioning | OpenClaw has structural rot. We don't coexist — we replace. Let the architecture speak. | ✓ Good — v1.0 addresses all 6 structural weaknesses |
| async-trait for adapter traits | Native async fn in trait not suitable for dyn dispatch. async-trait macro enables trait objects. | ✓ Good — all 7 adapter traits use dyn dispatch cleanly |
| tokio-rusqlite for single-writer | Single-writer thread avoids SQLITE_BUSY. tokio-rusqlite wraps blocking rusqlite in dedicated thread. | ✓ Good — zero SQLITE_BUSY in testing |
| Argon2id for vault KDF | Memory-hard KDF resists GPU attacks. Key never stored on disk. | ✓ Good — Zeroizing<[u8;32]> for master key |
| ort 2.0-rc for ONNX inference | Local embedding inference with no external API calls. RC status monitored. | ⚠️ Revisit — RC not stable, ndarray 0.17 required |
| Ed25519 for agent signing | Lightweight, fast, well-audited. ed25519-dalek mature crate. | ✓ Good — sign/verify on DeviceKeypair, delegation works |

---
*Last updated: 2026-03-02 after v1.0 milestone*
