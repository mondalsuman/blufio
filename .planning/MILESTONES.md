# Milestones

## v1.5 PRD Gap Closure (Shipped: 2026-03-13)

**Delivered:** PRD gap closure — data classification with PII detection, tamper-evident audit trail, memory enhancements, multi-level compaction with quality gates, 5-layer prompt injection defense, cron scheduler with retention policies, hook system with hot reload, GDPR tooling, 3 new channel adapters (Email, iMessage, SMS), OpenTelemetry distributed tracing, OpenAPI spec generation, and clippy unwrap enforcement across all library crates.

**Phases completed:** 12 phases (53-64), 49 plans
**Timeline:** 4 days (2026-03-10 → 2026-03-13)
**Commits:** 246 total
**Files changed:** 368 (+73,120 / -3,413)
**Total LOC:** 116,827 Rust across 37 crates
**Git range:** cd813fc → 5b1c9ff
**Requirements:** 93/93 satisfied (76 formally verified, 17 verification-doc gap for Phases 53-54)

**Key accomplishments:**
1. Data classification (4-level enum, PII regex detection with Luhn validation, auto-redaction) and hash-chained tamper-evident audit trail in dedicated audit.db with GDPR redact-in-place
2. Memory enhancements: temporal decay (0.95^days), importance boost, MMR diversity reranking, LRU eviction at 10K entries, file watcher auto-reindexing with 500ms debounce
3. Multi-level compaction (L0 raw → L1 turn-pair → L2 session → L3 archive) with quality gates (entity/decision/action/numerical scoring), entity extraction, and per-zone token budget enforcement
4. 5-layer prompt injection defense: L1 pattern classifier with confidence scoring, L3 HMAC-SHA256 boundary tokens, L4 output screening for credential leaks, L5 human-in-the-loop confirmation
5. Cron scheduler with TOML config and systemd timer generation, retention policies with soft-delete and classification-aware enforcement, hook system (11 lifecycle events with BTreeMap priority), config/TLS/plugin hot reload via ArcSwap
6. GDPR tooling (erasure with cost anonymization, JSON/CSV export with PII redaction, transparency reports), plus Email (IMAP/SMTP), iMessage (BlueBubbles), and SMS (Twilio) channel adapters — 11 total channels
7. OpenTelemetry distributed tracing (feature-gated, zero overhead when disabled), OpenAPI 3.1 spec with Swagger UI, Litestream WAL replication support
8. Code quality hardening: `#![deny(clippy::unwrap_used)]` across 43 library crates, serve.rs/main.rs decomposition, property-based tests (proptest), criterion benchmarks with CI regression detection

### Known Gaps
- Phases 53, 54 missing VERIFICATION.md (code complete per SUMMARY files, verification documentation gap)
- 15 tech debt items: 13 human verification items (E2E OTel, Swagger UI, Litestream replication, HITL visual flow, compaction cascade, etc.), 1 TLS hot reload stub, 1 ROADMAP formatting
- Carry-forward from v1.1: 5 deferred MCP integration items, 4 human verification items

---

## v1.4 Quality & Resilience (Shipped: 2026-03-09)

**Delivered:** Quality and resilience hardening — typed error hierarchy with automated retry classification, accurate token counting (tiktoken-rs + HuggingFace tokenizers replacing len()/4), per-dependency circuit breakers with 6-level graceful degradation ladder, FormatPipeline integration into all 8 channel adapters, and architectural decision records.

**Phases completed:** 7 phases (46-52), 16 plans
**Timeline:** 1 day (2026-03-09)
**Commits:** 54 total
**Files changed:** 245 (17,645 insertions, 25,845 deletions — net refactoring)
**Total LOC:** 80,101 Rust across 35 crates
**Git range:** 99136db → 3f45929
**Requirements:** 39/39 satisfied (all formally verified)

**Key accomplishments:**
1. Typed error hierarchy with `is_retryable()`, `severity()`, `category()` classification across all 35 crates, enabling automated retry decisions
2. Accurate token counting via tiktoken-rs (OpenAI o200k/cl100k) and HuggingFace tokenizers (Claude), replacing `len()/4` heuristic for all 5 providers
3. Per-dependency circuit breaker FSM (Closed/Open/HalfOpen) with configurable thresholds, Prometheus metrics, and EventBus integration
4. 6-level graceful degradation ladder (L0-L5) with automatic escalation/de-escalation, hysteresis, fallback provider routing, and user notifications
5. FormatPipeline wired into all 8 channel adapters with paragraph-boundary splitting, adapter-specific formatting, and extended ChannelCapabilities
6. Architectural decision records (ADR-001 ORT ONNX inference, ADR-002 compiled-in plugin architecture) in MADR 4.0.0 format

### Known Tech Debt
- Carry-forward from v1.1: 5 deferred MCP integration items, 4 human verification items
- Claude tokenizer accuracy: Xenova/claude-tokenizer is community artifact (~80-95% accuracy for Claude 3+)
- tiktoken-rs embeds BPE vocabulary data — monitor binary size impact
- Nyquist validation partial for Phases 46-49, 51; missing for Phases 50, 52

---

## v1.3 Ecosystem Expansion (Shipped: 2026-03-08)

**Delivered:** Ecosystem expansion — multi-provider LLM support (OpenAI, Ollama, OpenRouter, Gemini), OpenAI-compatible gateway API, 6 new channel adapters with cross-channel bridging, event bus, skill registry with Ed25519 code signing, Docker deployment, node system, and migration/CLI utilities.

**Phases completed:** 17 phases, 47 plans
**Timeline:** 4 days (2026-03-05 -> 2026-03-08)
**Commits:** 156 total
**Lines added:** ~40,150 (total Rust LOC: 71,808 across 35 crates)
**Git range:** 53f7009 -> 33ceb4c
**Requirements:** 71/71 satisfied (all formally verified)

**Key accomplishments:**
1. Internal event bus (tokio broadcast + mpsc) with provider-agnostic ToolDefinition and TTS/Transcription/Image media provider traits
2. Multi-provider LLM support — OpenAI, Ollama (native /api/chat), OpenRouter, and Gemini with streaming + tool calling across all four
3. OpenAI-compatible gateway API (/v1/chat/completions, /v1/responses, /v1/tools) with complete wire type separation from internal types
4. Scoped API keys with rate limiting, HMAC-SHA256 webhook delivery with exponential backoff, and batch processing API
5. 6 new channel adapters (Discord, Slack, WhatsApp, Signal, IRC, Matrix) with cross-channel bridging via configurable TOML rules
6. Skill registry with Ed25519 code signing, pre-execution verification gate, and capability enforcement at every WASM host function call
7. Docker multi-stage distroless image, docker-compose deployment, and multi-instance systemd template
8. Node system with Ed25519 mutual authentication, WebSocket heartbeat, fleet CLI, and approval routing broadcast
9. OpenClaw migration tool, built-in benchmarks, privacy evidence report, config recipes, air-gapped bundle, and uninstall
10. Gap closure phases (40-44) wired all runtime integrations: EventBus, bridge loop, provider registry, gateway stores, event publishers, node approvals

### Known Tech Debt
- Carry-forward from v1.1: 5 deferred MCP integration items, 4 human verification items
- WasmSkillRuntime.set_event_bus() exists but not called in production serve.rs (skill events deferred until production skill loading path exists)
- INFRA-04 Docker build verified statically only (no Docker daemon available)
- Media traits (TTS, Transcription, Image) defined but no provider implementations yet (future scope: EXT-03/04/05)

---

## v1.2 Production Hardening (Shipped: 2026-03-04)

**Delivered:** Production hardening -- systemd readiness, database encryption at rest, supply chain integrity via Minisign signatures, self-update with rollback, and backup integrity verification.

**Phases completed:** 6 phases, 13 plans
**Timeline:** 1 day (2026-03-03 -> 2026-03-04)
**Commits:** 58 total
**Lines added:** ~2,706 (total Rust LOC: 39,168 across 21 crates)
**Git range:** b412b6d -> bd25dee
**Requirements:** 30/30 satisfied (all formally verified)

**Key accomplishments:**
1. Backup integrity verification with PRAGMA integrity_check post-backup/restore and corruption auto-cleanup
2. sd_notify integration with Type=notify readiness, watchdog pings, and status reporting (silent no-op on non-systemd)
3. SQLCipher database encryption at rest with centralized key management, three-file safe migration, and doctor reporting
4. Minisign binary signature verification with embedded public key and CLI verify command
5. Self-update with rollback: version check, download, Minisign verify, atomic swap, health check, rollback
6. Audit gap closure: all 30 requirements verified, traceability complete, SUMMARY frontmatter populated

### Known Tech Debt
- Carry-forward from v1.1: 5 deferred MCP integration items, 4 human verification items
- No new tech debt introduced in v1.2

---

## v1.1 MCP Integration (Shipped: 2026-03-03)

**Delivered:** Full MCP citizen — Blufio exposes skills/memory as MCP tools and consumes external MCP servers, with security hardening across every layer.

**Phases completed:** 8 phases, 32 plans
**Timeline:** 2 days (2026-03-02 → 2026-03-03)
**Commits:** 42 total
**Lines added:** ~8,452 (total Rust LOC: 36,462 across 16 crates)
**Git range:** af82e42 → 4844998
**Requirements:** 48/48 satisfied (all formally verified)

**Key accomplishments:**
1. MCP server with stdio + Streamable HTTP transports — Claude Desktop connects and uses Blufio skills/memory as MCP tools
2. MCP client consuming external servers via TOML config — agent discovers and invokes external MCP tools in conversation
3. Security hardening chain: namespace enforcement, export allowlist, SHA-256 hash pinning, description sanitization, trust zone labeling
4. MCP resources exposing memory (search + lookup) and session history, prompt templates via prompts/list
5. Full observability: Prometheus MCP metrics, connection limits, health monitoring with exponential backoff
6. All 48 requirements formally verified with VERIFICATION.md reports, 4 E2E flow traces passing

### Known Tech Debt
- 5 deferred integration items: tools/list_changed notification (SRVR-13), progress notifications (SRVR-14), degraded tool unregistration (CLNT-06), per-server cost write path (CLNT-12), context utilization metric (INTG-04)
- 4 human verification items pending: live Telegram E2E, session persistence, SIGTERM drain, memory bounds 72h (runbooks exist)
- SUMMARY frontmatter gaps in phases 16, 18, 19 (bookkeeping only, all verified in VERIFICATION.md)

---

## v1.0 MVP (Shipped: 2026-03-02)

**Delivered:** A ground-up Rust AI agent platform shipping as a single static binary — 14 crates, 28,790 LOC, 70 requirements satisfied, all verified.

**Phases completed:** 14 phases, 43 plans
**Timeline:** 3 days (2026-02-28 → 2026-03-02)
**Commits:** 158 total (44 feat)
**Lines of code:** 28,790 Rust across 111 source files
**Git range:** a3b8361 → 4851130

**Key accomplishments:**
1. Cargo workspace with 14 crates, single static binary with jemalloc allocator
2. FSM-per-session agent loop with Anthropic streaming, Telegram adapter, persistent SQLite conversations
3. Three-zone context engine with Anthropic prompt cache alignment, unified cost ledger with budget caps
4. Local ONNX embedding model with hybrid search (vector + BM25) for long-term memory recall
5. WASM skill sandbox (wasmtime) with capability manifests, progressive discovery, skill registry
6. Plugin system with 7 adapter traits, HTTP/WebSocket gateway, Prometheus metrics, Ed25519 auth
7. Multi-agent delegation with signed messages, model routing (Haiku/Sonnet/Opus), smart heartbeats
8. Production hardening: systemd integration, memory bounds, secret redaction, TLS enforcement, SSRF protection

### Known Tech Debt
- 3 human verification items pending (live Telegram E2E, session persistence across restarts, SIGTERM drain timing)
- Memory bounds (CORE-07, CORE-08) verified by mechanism, not 72+ hour runtime measurement
- `GET /v1/sessions` returns hard-coded empty list (StorageAdapter not wired into GatewayState)
- No systemd unit file committed (deployment artifact)
- `#[allow(clippy::too_many_arguments)]` on SessionActor constructor

---

