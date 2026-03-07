# Blufio

## What This Is

Blufio is a ground-up Rust AI agent platform that ships as a single static binary. It runs an FSM-per-session agent loop backed by Anthropic Claude (with OpenAI, Ollama, OpenRouter, and Gemini provider plugins), with 8 channel adapters (Telegram, Discord, Slack, WhatsApp, Signal, IRC, Matrix, plus cross-channel bridging), SQLite persistence (WAL mode, ACID, SQLCipher encryption), AES-256-GCM credential vault, three-zone context engine with prompt cache alignment, local ONNX memory with hybrid search, WASM skill sandbox with Ed25519 code signing, plugin system with 7 adapter traits, OpenAI-compatible gateway API (/v1/chat/completions, /v1/responses, tools, scoped keys, webhooks, batch), model routing (Haiku/Sonnet/Opus), multi-agent delegation with Ed25519 signing, node system for paired device mesh, Prometheus observability, full MCP integration (server + client), Docker deployment, and migration/CLI utilities. 70,755 LOC Rust across 35 crates, 219 requirements verified across 4 milestones.

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
- ✓ MCP server exposing Blufio tools/skills/memory as MCP resources (stdio + Streamable HTTP) — v1.1
- ✓ MCP client consuming external MCP servers via TOML config — v1.1
- ✓ Agent discovers and invokes external MCP tools in conversation — v1.1
- ✓ MCP security: namespace enforcement, export allowlist, SHA-256 hash pinning, description sanitization, trust zone labeling — v1.1
- ✓ MCP resources: memory search/lookup, session history, prompt templates — v1.1
- ✓ Critical v1.0 tech debt resolved (GET /v1/sessions, systemd file, SessionActor refactor) — v1.1
- ✓ Backup integrity verification with PRAGMA integrity_check and corruption auto-cleanup — v1.2
- ✓ systemd Type=notify with READY/STOPPING/watchdog/STATUS lifecycle — v1.2
- ✓ SQLCipher database encryption at rest with centralized connection factory — v1.2
- ✓ Database encryption migration CLI with three-file safety strategy — v1.2
- ✓ Minisign Ed25519 signature verification with embedded public key — v1.2
- ✓ Self-update with download, Minisign verify, atomic swap, health check, rollback — v1.2
- ✓ All 30 v1.2 requirements verified with VERIFICATION.md reports — v1.2
- ✓ Event bus (internal pub/sub) — Phase 29
- ✓ OpenAI provider plugin — Phase 30
- ✓ Ollama provider plugin — Phase 30
- ✓ OpenRouter provider plugin — Phase 30
- ✓ Google/Gemini provider plugin — Phase 30
- ✓ OpenAI-compatible /v1/chat/completions API — Phase 31
- ✓ OpenResponses /v1/responses API — Phase 31
- ✓ Tools Invoke API (/v1/tools/invoke) — Phase 31
- ✓ Scoped API keys with rate limiting — Phase 32
- ✓ Webhook management with HMAC signing — Phase 32
- ✓ Batch operations API — Phase 32
- ✓ TTS/Transcription/Image provider traits — Phase 29
- ✓ Discord channel adapter (serenity) — Phase 33
- ✓ Slack channel adapter (slack-morphism) — Phase 33
- ✓ WhatsApp Cloud API adapter — Phase 34
- ✓ Signal adapter (signal-cli sidecar) — Phase 34
- ✓ IRC adapter (TLS + SASL) — Phase 34
- ✓ Matrix adapter (matrix-sdk 0.11) — Phase 34
- ✓ Cross-channel bridging — Phase 34
- ✓ Skill registry with install/list/remove/update — Phase 35
- ✓ Ed25519 code signing for WASM skills — Phase 35
- ✓ Docker image (multi-stage distroless) — Phase 36
- ✓ docker-compose with volumes/env/healthcheck — Phase 36
- ✓ Multi-instance systemd template — Phase 36
- ✓ Node system (Ed25519 pairing, heartbeat, fleet CLI) — Phase 37
- ✓ OpenClaw migration tool — Phase 38
- ✓ blufio bench (built-in benchmarks) — Phase 38
- ✓ blufio privacy evidence-report — Phase 38
- ✓ blufio config recipe + blufio uninstall — Phase 38
- ✓ blufio bundle (air-gapped deployment) — Phase 38
- ✓ All 71 v1.3 requirements verified with VERIFICATION.md reports — Phase 39

### Active

<!-- No active requirements -- v1.3 complete, all moved to Validated -->

(none -- v1.3 Ecosystem Expansion verified and complete)

## Current Milestone: None (v1.3 complete)

v1.3 Ecosystem Expansion verified and complete as of 2026-03-07.

## Shipped Milestones

- **v1.3 Ecosystem Expansion** -- 11 phases (29-39), 36 plans, 71 requirements (2026-03-05 -> 2026-03-07)
- **v1.2 Production Hardening** -- 6 phases, 13 plans, 30 requirements (2026-03-03 -> 2026-03-04)
- **v1.1 MCP Integration** -- 8 phases, 32 plans, 48 requirements (2026-03-02 -> 2026-03-03)
- **v1.0 MVP** -- 14 phases, 43 plans, 70 requirements (2026-02-28 -> 2026-03-02)

### Out of Scope

- Visual builder / GUI — CLI and config files only
- More than 2-4 channels at launch — Telegram first, expand later
- SOC 2 / HIPAA compliance tooling — post-v1.0
- DAG workflow engine — post-v1.0
- Client SDKs (Python, TypeScript, Go) — post-v1.0
- Multi-node sharding — single-instance only for v1.0
- Native plugin system (libloading) — WASM-only for v1.0
- Windows native builds — WSL2 is the path
- Shell automation layer (lifecycle scripts, hooks) — not needed for v1.0, scripts suffice
- Official skill registry with verified signatures — deferred, local registry works for v1.0

## Context

### Current State

Shipped v1.3 Ecosystem Expansion with 70,755 LOC Rust across 35 crates. 219 requirements verified across 4 milestones (v1.0: 70, v1.1: 48, v1.2: 30, v1.3: 71). 1,414 tests passing. All 71 v1.3 requirements verified with formal VERIFICATION.md reports. 4/4 cross-feature integration flows passing.

**Tech stack (actual):** Rust 2021, tokio, axum, rusqlite (WAL), ort (ONNX), wasmtime, teloxide, reqwest 0.13, rmcp 0.17, schemars 1.0, jsonschema 0.28, serde, tracing, clap, figment, tikv-jemallocator, metrics/metrics-exporter-prometheus, ed25519-dalek, aes-gcm, argon2, tower.

**Architecture:** 35-crate workspace — blufio-agent, blufio-anthropic, blufio-auth-keypair, blufio-bridge, blufio-bus, blufio-config, blufio-context, blufio-core (traits), blufio-cost, blufio-discord, blufio-gateway, blufio-gemini, blufio-irc, blufio-matrix, blufio-mcp-client, blufio-mcp-server, blufio-memory, blufio-node, blufio-ollama, blufio-openai, blufio-openrouter, blufio-plugin, blufio-prometheus, blufio-router, blufio-security, blufio-signal, blufio-skill, blufio-slack, blufio-storage, blufio-telegram, blufio-test-utils, blufio-vault, blufio-verify, blufio-whatsapp, plus blufio (binary).

**Known tech debt:** 12 carry-forward items from v1.1 (5 deferred MCP integration items, 4 human verification items, 3 SUMMARY frontmatter gaps). v1.2 introduced no new tech debt.

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
| rmcp 0.17 as MCP SDK | Official Anthropic-maintained Rust SDK, matches MCP spec 2025-11-25 | ✓ Good — stdio + HTTP transports, clean handler API |
| HTTP-only MCP client | No stdio subprocess spawning — preserves single-binary constraint | ✓ Good — Streamable HTTP + SSE fallback covers all cases |
| Security embedded per phase | Namespace (15), allowlist (16), CORS/auth (17), hash pinning (18) — never deferred | ✓ Good — complete security chain from day one |
| SHA-256 hash pinning for tools | Detect tool definition mutations (rug pulls) at discovery time | ✓ Good — PinStore in SQLite, graceful fallback |
| Trust zone labeling | External tools labeled separately in prompt context | ✓ Good — factual tone, no alarmist language |
| SQLCipher over custom encryption | Whole-file encryption, industry standard, single PRAGMA key statement | ✓ Good — transparent encryption, zero code changes for consumers |
| BLUFIO_DB_KEY env var | Consistent with BLUFIO_VAULT_KEY pattern, never stored on disk | ✓ Good — auto-detect hex vs passphrase keys |
| Three-file safety for encrypt migration | Original untouched until verified copy passes integrity check | ✓ Good — zero data loss risk during migration |
| Minisign over GPG | Simpler, Ed25519-only, single embedded key fits single-binary model | ✓ Good — compile-time constant, no key distribution problem |
| self-replace for atomic binary swap | Cross-platform atomic file replacement for running binary | ✓ Good — handles Windows locking, Unix atomic rename |
| Health check via child process | Spawn `blufio doctor` after swap rather than in-process check | ✓ Good — tests actual new binary, 30s timeout with auto-rollback |
| sd-notify best-effort wrapper | Silent no-op on non-systemd platforms, never blocks or errors | ✓ Good — zero-impact on macOS/Docker development |
| OpenAI wire types separate from internal | OpenAI request/response types in blufio-openai, internal types in blufio-core | ✓ Good — clean boundary, no leaky abstractions |
| Ollama native /api/chat (not compat shim) | Full feature access including tool calling and model discovery | ✓ Good — NDJSON streaming works cleanly |
| Gemini native API (not OpenAI shim) | systemInstruction, functionDeclarations, inlineData — best feature support | ✓ Good — brace-depth JSON parser handles chunked stream |
| Provider crate decoupling | Each provider owns its wire types independently (no cross-crate deps) | ✓ Good — providers can evolve independently |
| Query-param auth for Gemini | ?key= per Google convention (not Authorization header) | ✓ Good — matches Gemini API docs exactly |

---
*Last updated: 2026-03-07 after v1.3 Ecosystem Expansion verification complete (71/71 requirements verified)*
