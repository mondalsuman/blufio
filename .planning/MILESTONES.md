# Milestones

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

