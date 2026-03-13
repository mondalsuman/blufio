# Blufio

## What This Is

Blufio is a ground-up Rust AI agent platform that ships as a single static binary. It runs an FSM-per-session agent loop backed by Anthropic Claude (with OpenAI, Ollama, OpenRouter, and Gemini provider plugins), with 8 channel adapters (Telegram, Discord, Slack, WhatsApp, Signal, IRC, Matrix, plus cross-channel bridging), SQLite persistence (WAL mode, ACID, SQLCipher encryption), AES-256-GCM credential vault, three-zone context engine with accurate token counting and prompt cache alignment, local ONNX memory with hybrid search, WASM skill sandbox with Ed25519 code signing, plugin system with 7 adapter traits, OpenAI-compatible gateway API (/v1/chat/completions, /v1/responses, tools, scoped keys, webhooks, batch), model routing (Haiku/Sonnet/Opus), multi-agent delegation with Ed25519 signing, node system for paired device mesh, per-dependency circuit breakers with 6-level graceful degradation, 5-layer prompt injection defense (L1 pattern classifier, L3 HMAC boundary tokens, L4 output screening, L5 human-in-the-loop), Prometheus observability, full MCP integration (server + client), Docker deployment, and migration/CLI utilities. 80,101 LOC Rust across 36 crates, 264 requirements verified across 5 milestones.

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
- ✓ Event bus (internal pub/sub with tokio broadcast + mpsc) — v1.3
- ✓ Provider-agnostic ToolDefinition and TTS/Transcription/Image media traits — v1.3
- ✓ OpenAI provider plugin (streaming + tool calling + vision) — v1.3
- ✓ Ollama provider plugin (native /api/chat + model discovery) — v1.3
- ✓ OpenRouter provider plugin (provider fallback ordering) — v1.3
- ✓ Google/Gemini provider plugin (native API + function calling) — v1.3
- ✓ OpenAI-compatible /v1/chat/completions API with wire type separation — v1.3
- ✓ OpenResponses /v1/responses API with semantic streaming — v1.3
- ✓ Tools API (/v1/tools, /v1/tools/invoke) — v1.3
- ✓ Scoped API keys with per-key rate limiting and revocation — v1.3
- ✓ Webhook delivery with HMAC-SHA256 signing and exponential backoff — v1.3
- ✓ Batch operations API with per-item results — v1.3
- ✓ Discord channel adapter (serenity, slash commands, ephemeral responses) — v1.3
- ✓ Slack channel adapter (slack-morphism, Events API, Socket Mode, Block Kit) — v1.3
- ✓ WhatsApp Cloud API adapter + WhatsApp Web experimental adapter — v1.3
- ✓ Signal adapter via signal-cli JSON-RPC sidecar — v1.3
- ✓ IRC adapter with TLS and NickServ/SASL authentication — v1.3
- ✓ Matrix adapter (matrix-sdk 0.11, room join, invite auto-accept) — v1.3
- ✓ Cross-channel bridging with configurable TOML bridge rules — v1.3
- ✓ Skill registry with install/list/remove/update and SHA-256 content hashes — v1.3
- ✓ Ed25519 code signing with pre-execution verification gate — v1.3
- ✓ Docker multi-stage distroless image with docker-compose deployment — v1.3
- ✓ Multi-instance systemd template (blufio@.service) — v1.3
- ✓ Node system (Ed25519 pairing, WebSocket heartbeat, fleet CLI, approval routing) — v1.3
- ✓ OpenClaw migration tool (migrate, preview, config translate) — v1.3
- ✓ CLI utilities (bench, privacy evidence-report, config recipe, uninstall, bundle) — v1.3
- ✓ All 71 v1.3 requirements verified with VERIFICATION.md reports — v1.3
- ✓ Typed error hierarchy with is_retryable(), severity(), category() classification — v1.4 Phase 46
- ✓ ChannelCapabilities extension (streaming_type, formatting_support, rate_limit, supports_code_blocks) — v1.4 Phase 46
- ✓ FormatPipeline Table/List content types with 3-tier degradation — v1.4 Phase 46
- ✓ Accurate token counting via tiktoken-rs and HuggingFace tokenizers (replace len()/4 heuristic) — v1.4 Phase 47
- ✓ Per-dependency circuit breaker FSM with configurable thresholds and Prometheus metrics — v1.4 Phase 48
- ✓ 6-level graceful degradation ladder with automatic escalation/de-escalation, fallback routing, and notifications — v1.4 Phase 48
- ✓ ADR-001: ORT ONNX inference decision record with upgrade plan — v1.4 Phase 50
- ✓ ADR-002: Compiled-in plugin architecture decision record with migration roadmap — v1.4 Phase 50
- ✓ Circuit breaker events wired to EventBus, enabling full resilience pipeline in production — v1.4 Phase 51
- ✓ FormatPipeline wired into all 8 channel adapters with paragraph-boundary splitting — v1.4 Phase 49
- ✓ Multi-level compaction (L0-L3) with quality scoring, quality gates, and entity extraction — v1.5 Phase 56
- ✓ Soft/hard trigger thresholds with archive system and cold storage retrieval — v1.5 Phase 56
- ✓ Per-zone token budget enforcement (static advisory, conditional hard, dynamic adaptive) — v1.5 Phase 56
- ✓ Prompt injection defense: L1 pattern classifier, L3 HMAC boundary tokens, L4 output screening, L5 human-in-the-loop, pipeline coordinator — v1.5 Phase 57
- ✓ Cron scheduler with TOML config, single-instance locking, job history, 5 built-in tasks, systemd timer generation — v1.5 Phase 58
- ✓ Retention policies with two-phase soft-delete/permanent-delete, classification-aware enforcement, configurable per-type periods — v1.5 Phase 58
- ✓ Hook system (11 lifecycle hooks with BTreeMap priority, shell-based, sandboxed) — v1.5 Phase 59
- ✓ Hot reload (config, skills, plugins via ArcSwap/file watchers) — v1.5 Phase 59
- ✓ GDPR tooling (right to erasure, retention enforcement, data export, transparency disclosures) — v1.5 Phase 60
- ✓ iMessage (BlueBubbles), Email (IMAP), SMS (Twilio) channel adapters — v1.5 Phase 61
- ✓ OpenTelemetry distributed tracing (optional, disabled by default) — v1.5 Phase 62
- ✓ OpenAPI spec auto-generated from route definitions with Swagger UI — v1.5 Phase 62
- ✓ Clippy unwrap enforcement (#![deny(clippy::unwrap_used)]) across 43 library crates — v1.5 Phase 63
- ✓ Module decomposition (serve.rs, main.rs), integration tests, property-based tests, benchmark regression CI — v1.5 Phase 63
- ✓ Cross-phase integration wiring: channel_interactive from adapter capabilities, PII pattern sharing with OutputScreener, GDPR erasure audit trail — v1.5 Phase 64

### Active

<!-- No active requirements — v1.5 milestone complete -->

**Infrastructure:**
- [ ] Litestream WAL-based replication to object storage

## Current Milestone: v1.5 PRD Gap Closure

**Goal:** Close all remaining PRD gaps — compaction overhaul, prompt injection defense, cron/hooks/hot-reload, memory enhancements, audit trail, data classification, retention policies, GDPR tooling, additional channels, OpenTelemetry, and code quality hardening.

**Target features:**
- Multi-level compaction with quality scoring
- 5-layer prompt injection defense
- Cron scheduler with systemd timer generation
- Memory temporal decay, MMR, LRU eviction
- Hash-chained audit trail
- Data classification and retention policies
- Hook system with 11 lifecycle events
- Hot reload (config, TLS, plugins)
- iMessage, Email, SMS channel adapters
- OpenTelemetry, OpenAPI spec, Litestream replication
- GDPR erasure + data export
- Clippy unwrap enforcement + test coverage expansion

## Shipped Milestones

- **v1.4 Quality & Resilience** -- 7 phases (46-52), 16 plans, 39 requirements (2026-03-09)
- **v1.3 Ecosystem Expansion** -- 17 phases (29-45), 47 plans, 71 requirements (2026-03-05 -> 2026-03-08)
- **v1.2 Production Hardening** -- 6 phases, 13 plans, 30 requirements (2026-03-03 -> 2026-03-04)
- **v1.1 MCP Integration** -- 8 phases, 32 plans, 48 requirements (2026-03-02 -> 2026-03-03)
- **v1.0 MVP** -- 14 phases, 43 plans, 70 requirements (2026-02-28 -> 2026-03-02)

### Out of Scope

- Visual builder / GUI — CLI and config files only
- SOC 2 / HIPAA compliance tooling — post-v1.0
- DAG workflow engine — v2.0 per PRD
- Client SDKs (Python, TypeScript, Go) — post-v1.0
- Multi-node sharding — single-instance only
- Native plugin system (libloading) — WASM-only; memory safety boundary. See [ADR-002](docs/adr/ADR-002-compiled-in-plugin-architecture.md) migration roadmap
- Windows native builds — WSL2 is the path
- Remote skill registry / marketplace with CDN distribution — local registry works
- Browser extension — post-v1.0 per PRD
- Matrix E2E encryption — requires matrix-sdk-crypto
- Media provider implementations (TTS, Transcription, Image) — traits defined, implementations deferred

## Context

### Current State

Shipped v1.4 Quality & Resilience and completed v1.5 PRD Gap Closure — all 12 phases (53-64) verified. 80,101 LOC Rust across 36 crates. 264+ requirements verified across 5 milestones (v1.0: 70, v1.1: 48, v1.2: 30, v1.3: 71, v1.4: 39, v1.5: 93). v1.5 delivered: PII/data classification (53), audit trail (54), memory enhancements (55), multi-level compaction (56), prompt injection defense (57), cron/retention (58), hooks/hot-reload (59), GDPR (60), email/iMessage/SMS channels (61), OpenTelemetry/OpenAPI (62), code quality hardening (63), cross-phase integration wiring (64).

**Tech stack (actual):** Rust 2021, tokio, axum, rusqlite (WAL), ort (ONNX), wasmtime, teloxide, reqwest 0.13, rmcp 0.17, schemars 1.0, jsonschema 0.28, serde, tracing, clap, figment, tikv-jemallocator, metrics/metrics-exporter-prometheus, ed25519-dalek, aes-gcm, argon2, tower, serenity (Discord), slack-morphism, matrix-sdk 0.11, irc.

**Architecture:** 37-crate workspace — blufio-agent, blufio-anthropic, blufio-auth-keypair, blufio-bridge, blufio-bus, blufio-config, blufio-context, blufio-core (traits), blufio-cost, blufio-cron, blufio-discord, blufio-gateway, blufio-gemini, blufio-irc, blufio-matrix, blufio-mcp-client, blufio-mcp-server, blufio-memory, blufio-node, blufio-ollama, blufio-openai, blufio-openrouter, blufio-plugin, blufio-prometheus, blufio-router, blufio-security, blufio-signal, blufio-skill, blufio-slack, blufio-storage, blufio-telegram, blufio-test-utils, blufio-vault, blufio-verify, blufio-whatsapp, plus blufio (binary).

**Known tech debt:** 12 carry-forward items from v1.1 (5 deferred MCP integration items, 4 human verification items, 3 SUMMARY frontmatter gaps). WasmSkillRuntime EventBus wiring deferred. Media provider trait implementations deferred (EXT-03/04/05). Claude tokenizer accuracy (~80-95% for Claude 3+, community Xenova artifact).

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
| Everything-is-a-plugin | Operators customize install without Rust toolchain. ~2-5% overhead on plugin calls (negligible for I/O-bound adapters). Binary stays small. See [ADR-002](docs/adr/ADR-002-compiled-in-plugin-architecture.md) | ✓ Good — 7 adapter traits, built-in catalog, clean separation |
| WASM-only skills for v1.0 | Sandbox guarantees matter more than native performance for third-party code. Script tier (subprocess) as escape hatch. | ✓ Good — wasmtime sandbox with fuel/memory/epoch limits works |
| Three-zone context engine | Static (system prompt, cached), conditional (loaded per-relevance), dynamic (current turn). Achieves 68-84% token reduction vs inject-everything. | ✓ Good — cache alignment working, compaction implemented |
| Dual-license MIT + Apache-2.0 | Apache-2.0 patent protection for enterprise. Both permissive. Must be from first commit. | ✓ Good — SPDX headers on every file from day one |
| Telegram first | Largest AI agent user base. Simple Bot API. One channel done well beats five done poorly. | ✓ Good — full adapter with streaming, media, MarkdownV2 |
| Kill shot positioning | OpenClaw has structural rot. We don't coexist — we replace. Let the architecture speak. | ✓ Good — v1.0 addresses all 6 structural weaknesses |
| async-trait for adapter traits | Native async fn in trait not suitable for dyn dispatch. async-trait macro enables trait objects. | ✓ Good — all 7 adapter traits use dyn dispatch cleanly |
| tokio-rusqlite for single-writer | Single-writer thread avoids SQLITE_BUSY. tokio-rusqlite wraps blocking rusqlite in dedicated thread. | ✓ Good — zero SQLITE_BUSY in testing |
| Argon2id for vault KDF | Memory-hard KDF resists GPU attacks. Key never stored on disk. | ✓ Good — Zeroizing<[u8;32]> for master key |
| ort 2.0-rc for ONNX inference | Local embedding inference with no external API calls. RC status monitored. See [ADR-001](docs/adr/ADR-001-ort-onnx-inference.md) | ⚠️ Revisit — RC not stable, ndarray 0.17 required |
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
| Cow<'static, str> for user_message() | Zero-allocation static messages, owned strings only when dynamic | ✓ Good — no allocation overhead for static error messages |
| ChannelCapabilities derives Default | Ergonomic ..Default::default() for adapter capabilities | ✓ Good — reduces boilerplate in all 8 adapters |
| Legacy record_error() kept | Backward compatibility alongside new record_error_classified() | ✓ Good — gradual migration path for Prometheus consumers |
| sd-notify best-effort wrapper | Silent no-op on non-systemd platforms, never blocks or errors | ✓ Good — zero-impact on macOS/Docker development |
| OpenAI wire types separate from internal | OpenAI request/response types in blufio-openai, internal types in blufio-core | ✓ Good — clean boundary, no leaky abstractions |
| Ollama native /api/chat (not compat shim) | Full feature access including tool calling and model discovery | ✓ Good — NDJSON streaming works cleanly |
| Gemini native API (not OpenAI shim) | systemInstruction, functionDeclarations, inlineData — best feature support | ✓ Good — brace-depth JSON parser handles chunked stream |
| Provider crate decoupling | Each provider owns its wire types independently (no cross-crate deps) | ✓ Good — providers can evolve independently |
| Query-param auth for Gemini | ?key= per Google convention (not Authorization header) | ✓ Good — matches Gemini API docs exactly |
| Event bus before features | EventBus unblocks webhooks, bridging, nodes, batch — must come first | ✓ Good — clean dependency chain, all consumers wired |
| TOFU key management for skills | First publisher key trusted, key changes hard-blocked | ✓ Good — simple trust model, prevents supply chain attacks |
| Pre-execution verification gate | Signature + capability check before every WASM invoke() | ✓ Good — TOCTOU prevented by in-memory WASM bytes |
| Global EventBus capacity 1024 | Handles all event types system-wide (up from scoped 128) | ✓ Good — no lag warnings in testing |
| Bridge outbound-only dispatch | adapter.send() called directly to prevent infinite loops | ✓ Good — loop prevention built into architecture |
| Distroless cc-debian12 (not static) | ONNX Runtime ships glibc-linked .so files | ✓ Good — TLS and SQLCipher work in distroless |
| signal-cli sidecar (not native Rust) | No production Rust Signal library exists | ✓ Good — JSON-RPC bridge works cleanly |
| matrix-sdk 0.11 pinned | 0.12+ requires Rust 1.88 (not yet stable) | ✓ Good — room join + messaging work |

| Custom CB over failsafe/tower crates | failsafe incompatible with dyn dispatch, tower-limit too inflexible for per-dep breakers | ✓ Good — ~200 LOC, Arc<Mutex<Clock>> injection for testing |
| Tier-based fallback provider mapping | Model names mapped to capability tiers for cross-provider fallback | ✓ Good — contains()-based family detection, clean iteration |
| Degradation notifications via EventBus | Background task subscribes to level changes, sends to all channels with dedup | ✓ Good — 60s dedup window prevents notification storms |
| tiktoken-rs for OpenAI token counting | Exact BPE tokenizer matching OpenAI's production encoding (o200k/cl100k) | ✓ Good — zero token estimation error for OpenAI models |
| HuggingFace tokenizers for Claude counting | Best available local tokenizer via Xenova/claude-tokenizer community vocabulary | ⚠️ Revisit — ~80-95% accuracy, monitor for official tokenizer |
| FormatPipeline 4-step adapter pipeline | detect → format → split → escape enforced in all 8 adapters | ✓ Good — consistent formatting across all channels |
| ADR documentation in MADR 4.0.0 format | Standardized decision records with context, options, consequences | ✓ Good — ADR-001 (ORT) and ADR-002 (plugins) documented |

---
*Last updated: 2026-03-13 after Phase 64 (v1.5 milestone complete)*
