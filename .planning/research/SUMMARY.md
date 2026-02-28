# Project Research Summary

**Project:** Blufio
**Domain:** Always-on personal AI agent platform (Rust, single-binary, plugin-composed)
**Researched:** 2026-02-28
**Confidence:** HIGH

## Executive Summary

Blufio is a ground-up Rust replacement for OpenClaw, targeting the always-on personal AI agent market. Research confirms that the space is dominated by Node.js/Python platforms (OpenClaw, AutoGPT, CrewAI, Dify, n8n) that all share structural weaknesses: memory leaks, token waste, insecure defaults, and complex multi-container deployments. No competitor ships as a single static binary. The recommended approach is a plugin-composed Rust binary built on tokio/axum/rusqlite/wasmtime, where every integration point (channels, LLM providers, storage, embedding, auth) is a trait implemented by swappable adapters. The three-zone context engine (static/conditional/dynamic) with Anthropic prompt cache alignment is the core cost differentiator, projected to achieve 68-84% token reduction versus OpenClaw's inject-everything approach.

The Rust async ecosystem is mature and well-suited for this architecture. All core dependencies (tokio 1.49, axum 0.8, rusqlite 0.38, wasmtime 40, teloxide 0.17) are verified at current stable versions with no compatibility conflicts. The key architectural pattern is an enum-based FSM per session running in its own tokio task, with a single-writer SQLite persistence layer accessed through tokio-rusqlite. Research identified 8 critical pitfalls, of which 5 must be addressed in the foundation phase: blocking the tokio runtime with sync operations, SQLite single-writer contention, async cancellation safety, Telegram bot reliability in always-on operation, and premature LLM provider over-abstraction. Getting these wrong early means expensive rewrites later.

The primary risks are: (1) blocking the async runtime with synchronous SQLite/crypto/WASM operations -- mitigated by using tokio-rusqlite and spawn_blocking from day one; (2) prompt cache invalidation destroying the cost advantage -- mitigated by designing the three-zone context pipeline with deterministic serialization (BTreeMap, not HashMap) and static system prompts from the start; (3) WASM sandbox resource exhaustion from malicious skills -- mitigated by mandatory fuel metering, memory caps, and capability manifests before accepting any third-party code.

## Key Findings

### Recommended Stack

The stack is pure Rust with zero runtime dependencies (no Node.js, no Python, no Docker). All crates are verified at current stable versions via docs.rs and Context7. The version compatibility matrix shows no conflicts across the dependency tree.

**Core technologies:**
- **tokio 1.49 + axum 0.8**: Async runtime and HTTP/WebSocket gateway. Axum is built by the tokio team and natively supports WebSocket upgrade and tower middleware composition.
- **rusqlite 0.38 (bundled-sqlcipher)**: Embedded SQLite with encryption. Chosen over sqlx because SQLite is inherently synchronous; sqlx adds async overhead without benefit for embedded use. SQLCipher compiles directly into the binary for encrypted credential vault.
- **wasmtime 40**: WASM skill sandboxing with Component Model and WIT interfaces. Chosen over wasmer for better WASI P2 support and Bytecode Alliance backing.
- **reqwest 0.13 (custom LLM client)**: HTTP client for all outbound calls (Anthropic API, Telegram Bot API, webhooks). Custom thin provider client chosen over rig-core because Rig's agent abstractions conflict with Blufio's own FSM design.
- **ort 2.0-rc.11 (ONNX Runtime)**: Local embedding inference with hardware acceleration. Chosen over candle for production inference speed. Ships with all-MiniLM-L6-v2 (384-dim, ~80MB).
- **teloxide 0.17**: Full-featured Telegram Bot API framework. Wraps behind a ChannelAdapter trait so the agent core never imports teloxide directly.
- **ring 0.17 + ed25519-dalek 2.2**: AES-256-GCM for credential vault encryption, Ed25519 for inter-agent message signing. ring for symmetric crypto (hardware AES-NI), dalek for asymmetric (cleaner key management API).
- **tracing 0.1 + metrics 0.24**: Structured logging with async-aware spans, plus Prometheus metrics export. Separate concerns: tracing for request flows and errors, metrics for numerical time-series.
- **tikv-jemallocator 0.6**: Production memory allocator. Reduces fragmentation in long-running processes. Critical for months-long uptime on resource-constrained VPS.

**Critical version requirements:** tokio 1.49+ (LTS through Sep 2026), axum 0.8+ (tower 0.5 required), wasmtime 40+ (Component Model support), Rust edition 2024 (MSRV 1.85).

### Expected Features

**Must have (table stakes):**
- Agent loop with FSM-per-session (core value proposition)
- LLM provider abstraction (Anthropic at launch, trait for expansion)
- Telegram channel adapter (primary user interface)
- Persistent conversation history (SQLite WAL, resume across restarts)
- System prompt / personality configuration (TOML + markdown)
- Tool/function calling (built-in tools + WASM sandbox)
- Memory system with short and long-term recall (three-zone, hybrid search)
- Credential management (AES-256-GCM encrypted vault)
- CLI interface (serve, status, config, doctor)
- Health checks and self-diagnostics
- Background/always-on operation (systemd, signal handling)
- Graceful error handling (structured errors, no silent swallowing)

**Should have (differentiators):**
- Single static binary deployment (zero-dependency scp-and-run, unique in the market)
- Smart context engine with 68-84% token reduction (the cost moat)
- Model routing Haiku/Sonnet/Opus by query complexity (75-85% cost reduction)
- Unified cost ledger with budget caps and kill switches (OpenClaw has zero cost tracking)
- WASM skill sandboxing with capability manifests (OpenClaw has 800+ malicious ClawHub skills)
- Memory-safe bounded resource usage (50-80MB idle vs OpenClaw's 300-800MB leak)
- ACID persistence via SQLite WAL (vs OpenClaw's JSONL files that lose data on crash)
- Security-by-default (bind 127.0.0.1, auth required, encrypted vault, Ed25519 signing)
- Prometheus metrics export (OpenClaw has no observability)

**Defer (v2+):**
- Visual/GUI workflow builder (massive frontend scope, target audience prefers config files)
- DAG workflow engine (FSM-per-session covers v1.0 use cases)
- 15+ messaging channels (Telegram first, 2-4 post-launch based on demand)
- Client SDKs (HTTP/WebSocket API is the universal SDK)
- MCP server/client (spec still evolving, WASM skills cover the same ground)
- RAG pipeline (memory system handles personal context; external RAG via skill/HTTP tool)
- Multi-node distributed mode (single-instance covers target workload)

### Architecture Approach

The system is layered into five tiers: Ingest Layer (channel adapters normalize platform-specific messages into canonical Envelopes), Gateway/Router (bounded mpsc channels with backpressure), Agent Layer (session manager + FSM-per-session tokio tasks), Intelligence Layer (three-zone context assembly, model routing, cost ledger, embedding engine), and Execution Layer (WASM skill sandbox + built-in tool registry). All durable state lives in a single SQLite database (WAL mode) accessed through a centralized persistence crate via tokio-rusqlite. The project is organized as a Cargo workspace with 8 crates that enforce clear dependency boundaries.

**Major components:**
1. **blufio-core** -- Zero-dependency crate containing all trait definitions (ChannelAdapter, LlmProvider, StorageAdapter, EmbeddingAdapter, AuthAdapter, SkillRuntime), canonical types (Envelope, Session, Message), error types, and config structs. Every other crate depends only on core.
2. **blufio-agent** -- Agent loop FSM, session manager (DashMap registry with LRU eviction), context assembly pipeline (three-zone with cache alignment), model router, and cost ledger. This is the core reasoning engine.
3. **blufio-persist** -- Centralized SQLite persistence via tokio-rusqlite. Single-writer pattern. WAL mode. Manages all tables (sessions, messages, memory, queue, cron, cost_ledger, embeddings). Credential vault via separate SQLCipher database.
4. **blufio-gateway** -- axum HTTP/WebSocket server, message bus routing, middleware (auth, rate limiting, metrics, CORS).
5. **blufio-skills** -- wasmtime WASM runtime with WIT interface definitions, capability manifest parsing, skill registry, host function implementations, fuel metering.
6. **blufio-telegram** -- Telegram channel adapter implementing ChannelAdapter trait. Handles long polling, update offset persistence, rate-limited message sending.
7. **blufio-anthropic** -- Anthropic LLM provider implementing LlmProvider trait. Streaming SSE, prompt caching, tool use, model routing.
8. **blufio-cli** -- Binary entry point, clap-based CLI, application bootstrap wiring.

### Critical Pitfalls

1. **Blocking the tokio runtime with sync operations** -- SQLite, crypto, ONNX inference, and WASM compilation are all synchronous. Calling them from async contexts starves worker threads. Prevention: use tokio-rusqlite for all DB access, spawn_blocking for crypto and embedding, dedicated thread pool for ONNX inference. Must be correct from Phase 1; retrofitting requires touching every callsite.

2. **SQLite single-writer contention** -- WAL mode allows concurrent readers but only one writer. Multiple sessions writing simultaneously causes SQLITE_BUSY at 10+ concurrent sessions. Prevention: single-writer pattern (one dedicated writer thread, writes submitted via mpsc channel, batched in transactions). Use BEGIN IMMEDIATE for all write transactions. Must be the architectural foundation from Phase 1.

3. **Context window overflow and prompt cache invalidation** -- Unbounded context assembly wastes tokens and breaks cache alignment. Any dynamic content in the static zone (timestamps, user IDs) invalidates Anthropic's prefix cache, turning a $50/month agent into $500/month. Prevention: token budget enforcer on every LLM call, truncation hierarchy (older turns first, then verbose tool outputs, then low-relevance memories, never system prompt), static system prompts with BTreeMap for deterministic JSON serialization. Must be designed into the context pipeline in Phase 2.

4. **Telegram bot reliability in always-on operation** -- Long polling connections silently die after 12-48 hours. Update offsets not persisted cause duplicate processing on restart. Rate limits (30 msg/s global, 1/s per chat) cause dropped messages. Prevention: explicit HTTP client timeouts (35s > polling timeout), TCP keepalive, persisted update_id offsets in SQLite, outgoing message queue with token-bucket rate limiting. Must be rock-solid in Phase 1.

5. **Async cancellation safety violations** -- tokio select! drops futures at the last .await point. Multi-step operations (DB write then cache update) leave inconsistent state when cancelled. Prevention: wrap critical operations in tokio::task::spawn (runs to completion), use CancellationToken for cooperative shutdown, transactional state updates through single-writer DB thread. Must be designed into the agent loop in Phase 1.

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: Core Foundation
**Rationale:** Everything depends on core types, persistence, and the agent loop. The five most critical pitfalls (async runtime blocking, SQLite contention, Telegram reliability, cancellation safety, provider abstraction) must be addressed here. The dependency graph shows that agent loop requires session persistence, and both channel adapters and context pipeline require the agent loop.
**Delivers:** A working Telegram bot backed by Claude with persistent conversations. The minimum viable "always-on agent" that validates the entire architecture.
**Addresses:** Agent loop (FSM), Anthropic LLM provider, Telegram channel adapter, SQLite WAL persistence, credential vault, CLI skeleton (serve, status, doctor), security defaults (bind 127.0.0.1, auth required), graceful shutdown, health checks.
**Avoids:** Blocking async runtime (tokio-rusqlite from day one), SQLite contention (single-writer pattern), Telegram reliability failures (proper HTTP timeouts, offset persistence, rate limiting), cancellation safety (CancellationToken, atomic operations), provider over-abstraction (build Anthropic client directly, extract trait later).
**Stack elements:** tokio, axum, rusqlite, reqwest, teloxide, ring, clap, tracing, serde, toml.

### Phase 2: Intelligence Layer
**Rationale:** With a working agent, make it smart and affordable. The three-zone context engine, model routing, and cost ledger are the competitive differentiators. They depend on persistence and the agent loop (Phase 1) and are prerequisites for the skill system (Phase 3). Prompt cache alignment must be designed into the context pipeline from this phase.
**Delivers:** Agent that remembers across sessions (semantic memory search), optimizes token usage (68-84% reduction), routes to appropriate model (Haiku/Sonnet/Opus by complexity), tracks costs with budget caps, and exploits Anthropic prompt caching.
**Addresses:** Three-zone context engine (static/conditional/dynamic), embedding model (ONNX via ort), semantic memory (vector + BM25 hybrid search), model routing, cost ledger with budget caps and kill switches, smart heartbeats (Haiku, skip-when-unchanged).
**Avoids:** Context window overflow (token budget enforcer on every LLM call), prompt cache invalidation (static system prompt, BTreeMap for deterministic JSON, cache breakpoint placement), unbounded conversation history (sliding window with summarization).
**Stack elements:** ort, dashmap, lru, chrono, BTreeMap (stdlib).

### Phase 3: Skill Sandbox
**Rationale:** Skills depend on a working agent loop, persistence (for skill state), and the context pipeline (for skill discovery via progressive disclosure). The WASM sandbox is complex but architecturally isolated -- it adds a new execution path for tool calls without changing the agent loop structure. Must be fully secured before accepting any third-party skills.
**Delivers:** Third-party skills loadable from .wasm files, discovered by the agent via progressive skill descriptions, executed in a capability-gated sandbox with fuel metering and memory limits.
**Addresses:** WASM skill sandbox (wasmtime + WIT), capability manifests, skill registry, built-in tools (bash, HTTP, file I/O), progressive skill discovery.
**Avoids:** WASM sandbox escape via resource exhaustion (fuel metering, StoreLimits, epoch interruption), unbounded skill output injected into context (4K token hard cap, summarize with Haiku), skills with unrestricted WASI capabilities (capability manifest review).
**Stack elements:** wasmtime, wasmtime-wasi.

### Phase 4: Production Hardening
**Rationale:** With core features working, harden for production deployment. These are quality-of-life and operational features that don't change the architecture but are required for a production-ready product. Each is independently addable because trait boundaries are established.
**Delivers:** Production-ready single binary suitable for always-on deployment on a $4/month VPS with full observability, multi-agent routing, and systemd integration.
**Addresses:** HTTP/WebSocket gateway (full axum server), Prometheus metrics export, multi-agent routing with Ed25519 signing, systemd integration, backup/restore, CLI completions, log rotation, configuration validation (blufio doctor).
**Avoids:** Ed25519 key generation with insufficient entropy (use OsRng), unbounded channels and queues (bounded everywhere with backpressure), credential vault key from weak source (Argon2id KDF).
**Stack elements:** tower-http, metrics-exporter-prometheus, ed25519-dalek, tokio-util (CancellationToken).

### Phase 5: Ecosystem Expansion
**Rationale:** Post-launch features driven by user demand. The plugin architecture (trait system) established in Phases 1-4 makes these additions modular. Each is a new adapter crate, not a modification of existing code.
**Delivers:** Additional LLM providers, messaging channels, plugin hot-loading, skill marketplace with verified signatures.
**Addresses:** OpenAI/Ollama provider adapters, second channel adapter (Discord or WhatsApp), plugin host with hot-loading, skill marketplace, progressive skill discovery improvements.

### Phase Ordering Rationale

- **Phase 1 before Phase 2**: The agent loop, persistence, and Telegram adapter are the foundation that everything else builds on. Five of eight critical pitfalls must be addressed here because retrofitting them later is HIGH cost.
- **Phase 2 before Phase 3**: The intelligence layer (context engine, embedding, model routing) is both a competitive differentiator and a prerequisite for skill discovery. Skills need the context pipeline to be discoverable by the agent.
- **Phase 3 before Phase 4**: The WASM sandbox must be secured before production hardening. Shipping production without sandbox security is a liability.
- **Phase 4 before Phase 5**: Operational hardening (metrics, multi-agent, systemd) must be solid before expanding the ecosystem. Don't add new channels to a system that can't be monitored.
- **Feature grouping follows the dependency graph**: The FEATURES.md dependency tree shows agent loop and persistence as roots, context engine and skills as mid-level, and hardening and ecosystem as leaves. The phasing mirrors this topology.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 2 (Intelligence Layer):** Token counting accuracy across models, optimal embedding model selection (all-MiniLM-L6-v2 vs alternatives), Anthropic prompt caching API details and minimum token thresholds, hybrid search ranking algorithm (BM25 weight vs vector similarity weight). The context engine is the core differentiator and requires precise API-level implementation research.
- **Phase 3 (Skill Sandbox):** wasmtime Component Model WIT interface design, WASI P2 capability mapping to skill manifest declarations, fuel budget calibration (how many fuel units per operation type), AOT compilation and caching strategy. The WASM ecosystem is mature but the Component Model is still evolving.

Phases with standard patterns (skip research-phase):
- **Phase 1 (Core Foundation):** All patterns are well-documented: tokio async patterns, axum server setup, rusqlite WAL mode, teloxide bot integration, FSM in Rust. Research is already comprehensive enough to build from.
- **Phase 4 (Production Hardening):** Prometheus metrics export, systemd integration, Ed25519 signing, and graceful shutdown are all established Rust patterns with extensive documentation.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All crates verified at current stable versions via docs.rs and Context7. Version compatibility matrix confirmed. No conflicts in dependency tree. Every recommendation includes rationale and alternatives considered. |
| Features | HIGH | Verified across 7 competitor platforms with official documentation and architecture analyses. Feature prioritization grounded in competitor gaps (OpenClaw's specific weaknesses). MVP definition is clear and dependency-ordered. |
| Architecture | HIGH (core), MEDIUM (WASM) | Core patterns (FSM, trait-based plugins, tokio-rusqlite, three-zone context) are established Rust idioms with strong source backing. WASM Component Model and WIT interfaces are stable but the ecosystem is still maturing (WASI 0.3 preview in progress). |
| Pitfalls | HIGH | All 8 critical pitfalls sourced from official documentation, CVE databases, community post-mortems, and Rust-specific async safety literature. Prevention strategies include concrete code patterns. Phase mapping identifies when each pitfall must be addressed. |

**Overall confidence:** HIGH

### Gaps to Address

- **Token counting accuracy**: Research recommends tiktoken-rs or cl100k_base tokenizer but doesn't verify exact compatibility with Anthropic's tokenizer. Need to validate that token estimates match actual API usage within 5% during Phase 2 implementation.
- **WASM Component Model maturity**: wasmtime 40.x supports Component Model, but the WIT interface design for skill host functions needs hands-on prototyping. The `-Smax-resources` and `-Shostcall-fuel` flags referenced are from wasmtime 42.0+ which is not yet released. Need to verify available security features in wasmtime 40.x during Phase 3 planning.
- **ort 2.0 release candidate stability**: ort is at 2.0.0-rc.11 (release candidate, not stable). Monitor for breaking changes before Phase 2. Fallback: pin to rc.11 or evaluate candle as alternative.
- **Embedding model performance on musl**: ONNX Runtime performance on musl static builds is not validated. May need to test ort with musl target and verify that hardware acceleration (AVX2, etc.) works correctly in static builds.
- **SQLCipher key derivation UX**: Research specifies Argon2id KDF with 64MB memory parameter, but the UX for passphrase entry on a headless server (systemd service) needs design work. Consider: passphrase file, environment variable, or key file approaches.
- **Telegram webhook vs long-polling trade-off**: Research recommends long-polling for simplicity, but webhook mode eliminates polling timeout issues. If Telegram reliability proves problematic during Phase 1 testing, webhook mode (with reverse proxy TLS termination) should be evaluated.

## Sources

### Primary (HIGH confidence)
- Context7: tokio 1.49.0, axum 0.8.4, wasmtime 38.0.4 (verified 40.0.1 latest), teloxide 0.17.0, candle (HuggingFace)
- docs.rs: rusqlite 0.38.0, tracing 0.1.44, tikv-jemallocator 0.6.1, ort 2.0.0-rc.11, metrics 0.24.3, clap 4.5.60, reqwest 0.13.2, ring 0.17.14, ed25519-dalek 2.2.0, wasmtime 40.0.1, tokio-rusqlite
- Anthropic official: prompt caching documentation, context engineering blog
- SQLite official: WAL mode, locking documentation
- Bytecode Alliance: wasmtime security documentation, Component Model docs
- Tokio official: graceful shutdown guide, spawn_blocking documentation

### Secondary (MEDIUM confidence)
- OpenClaw architecture analysis (Substack deep-dive, GitHub discussions, security reports)
- Competitor platform analysis: AutoGPT, CrewAI, LangGraph, Botpress, n8n, Dify
- Rust ecosystem guides: async pitfalls (JetBrains blog), cancellation safety (Oxide RFD 400), WASM plugin architecture (multiple)
- CVE databases: wasmtime CVE-2025-53901, CVE-2026-27572
- Industry research: LLM cost management, AI agent sandboxing, observability patterns

### Tertiary (LOW confidence)
- ort 2.0 rc stability (release candidate, may change before stable)
- wasmtime 42.0+ features referenced but not yet released (-Smax-resources, -Shostcall-fuel)
- Context rot research (Chroma, arXiv) -- validates approach but specific percentages (68-84% reduction) need validation in Blufio's specific context

---
*Research completed: 2026-02-28*
*Ready for roadmap: yes*
