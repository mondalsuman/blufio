# Roadmap: Blufio

## Milestones

- ✅ **v1.0 MVP** — Phases 1-14 (shipped 2026-03-02)
- ✅ **v1.1 MCP Integration** — Phases 15-22 (shipped 2026-03-03)
- ✅ **v1.2 Production Hardening** — Phases 23-28 (shipped 2026-03-04)
- **v1.3 Ecosystem Expansion** — Phases 29-45 (gap closure in progress)

## Phases

<details>
<summary>✅ v1.0 MVP (Phases 1-14) — SHIPPED 2026-03-02</summary>

- [x] Phase 1: Project Foundation & Workspace (2/2 plans) — completed 2026-02-28
- [x] Phase 2: Persistence & Security Vault (2/2 plans) — completed 2026-02-28
- [x] Phase 3: Agent Loop & Telegram (4/4 plans) — completed 2026-03-01
- [x] Phase 4: Context Engine & Cost Tracking (3/3 plans) — completed 2026-03-01
- [x] Phase 5: Memory & Embeddings (3/3 plans) — completed 2026-03-01
- [x] Phase 6: Model Routing & Smart Heartbeats (3/3 plans) — completed 2026-03-01
- [x] Phase 7: WASM Skill Sandbox (4/4 plans) — completed 2026-03-01
- [x] Phase 8: Plugin System & Gateway (3/3 plans) — completed 2026-03-01
- [x] Phase 9: Production Hardening (3/3 plans) — completed 2026-03-01
- [x] Phase 10: Multi-Agent & Final Integration (3/3 plans) — completed 2026-03-01
- [x] Phase 11: Fix Critical Integration Bugs (4/4 plans) — completed 2026-03-01
- [x] Phase 12: Verify Unverified Phases (5/5 plans) — completed 2026-03-01
- [x] Phase 13: Sync Traceability & Documentation (1/1 plan) — completed 2026-03-02
- [x] Phase 14: Wire Cross-Phase Integration (3/3 plans) — completed 2026-03-02

</details>

<details>
<summary>✅ v1.1 MCP Integration (Phases 15-22) — SHIPPED 2026-03-03</summary>

- [x] Phase 15: MCP Foundation (4/4 plans) — completed 2026-03-02
- [x] Phase 16: MCP Server stdio (3/3 plans) — completed 2026-03-02
- [x] Phase 17: MCP Server HTTP + Resources (5/5 plans) — completed 2026-03-02
- [x] Phase 18: MCP Client (4/4 plans) — completed 2026-03-03
- [x] Phase 19: Integration Testing + Tech Debt (5/5 plans) — completed 2026-03-03
- [x] Phase 20: Verify Phase 15 & 16 Completeness (4/4 plans) — completed 2026-03-03
- [x] Phase 21: Fix MCP Wiring Gaps (4/4 plans) — completed 2026-03-03
- [x] Phase 22: Verify Phase 18 & 19 + Close Traceability (3/3 plans) — completed 2026-03-03

</details>

<details>
<summary>✅ v1.2 Production Hardening (Phases 23-28) — SHIPPED 2026-03-04</summary>

- [x] Phase 23: Backup Integrity Verification (1/1 plan) — completed 2026-03-03
- [x] Phase 24: sd_notify Integration (2/2 plans) — completed 2026-03-03
- [x] Phase 25: SQLCipher Database Encryption (4/4 plans) — completed 2026-03-03
- [x] Phase 26: Minisign Signature Verification (2/2 plans) — completed 2026-03-03
- [x] Phase 27: Self-Update with Rollback (2/2 plans) — completed 2026-03-03
- [x] Phase 28: Close Audit Gaps (2/2 plans) — completed 2026-03-04

</details>

### v1.3 Ecosystem Expansion (GAP CLOSURE IN PROGRESS)

**Milestone Goal:** Expand the platform ecosystem with OpenAI-compatible APIs, multi-provider LLM support, multi-channel adapters, Docker deployment, event bus, skill marketplace, node system, and migration tooling.
**Status:** Tech debt closure in progress -- 71/71 requirements satisfied, 2 event publisher gaps + node wiring + doc sync. Phases 40-42 closed runtime wiring; Phases 43-45 close remaining gaps.

- [x] **Phase 29: Event Bus & Core Trait Extensions** — Internal pub/sub backbone and provider-agnostic ToolDefinition
- [x] **Phase 30: Multi-Provider LLM Support** — OpenAI, Ollama, OpenRouter, and Gemini provider plugins (completed 2026-03-05)
- [x] **Phase 31: OpenAI-Compatible Gateway API** (3/3 plans) — completed 2026-03-05
- [ ] **Phase 32: Scoped API Keys, Webhooks & Batch** — API key management, webhook delivery, and batch operations
- [x] **Phase 33: Discord & Slack Channel Adapters** (3/3 plans) — completed 2026-03-06
- [x] **Phase 34: WhatsApp, Signal, IRC & Matrix Adapters** (5/5 plans) — completed 2026-03-06
- [x] **Phase 35: Skill Registry & Code Signing** (2/2 plans) — completed 2026-03-06
- [x] **Phase 36: Docker Image & Deployment** (2/2 plans) — completed 2026-03-07
- [x] **Phase 37: Node System** — Paired device mesh with Ed25519 mutual authentication (completed 2026-03-07)
- [x] **Phase 38: Migration & CLI Utilities** — OpenClaw migration tool, bench, privacy report, config recipe, uninstall, bundle (completed 2026-03-07)
- [x] **Phase 39: Integration Verification** — End-to-end validation across all v1.3 features (completed 2026-03-07)
- [x] **Phase 40: Wire Global EventBus & Bridge** — Global EventBus in serve.rs + bridge loop startup (completed 2026-03-07)
- [x] **Phase 41: Wire ProviderRegistry into Gateway** — Provider crates as binary deps + ProviderRegistry impl (completed 2026-03-07)
- [x] **Phase 42: Wire Gateway Stores** — ApiKeyStore, WebhookStore, BatchStore instantiation + webhook delivery (completed 2026-03-07)
- [x] **Phase 43: Wire EventBus Event Publishers** — AgentLoop + WasmSkillRuntime event publishing for webhook triggers (completed 2026-03-08)
- [ ] **Phase 44: Node Approval Wiring** — ApprovalRouter EventBus subscription + ConnectionManager forwarding
- [ ] **Phase 45: Documentation & Traceability Sync** — Fix 31 stale traceability entries + Phase 32 checkbox

## Phase Details

### Phase 29: Event Bus & Core Trait Extensions
**Goal**: Establish the internal pub/sub backbone that unblocks webhooks, bridging, nodes, and batch; extend blufio-core with provider-agnostic types and media provider traits
**Depends on**: Phase 28 (v1.2 complete)
**Requirements**: INFRA-01, INFRA-02, INFRA-03, PROV-10, PROV-11, PROV-12, PROV-13, PROV-14
**Success Criteria** (what must be TRUE):
  1. Any component can publish typed events (session, channel, skill, node, webhook, batch) via Arc<EventBus> and subscribers receive them
  2. Critical subscribers (webhook delivery, audit) use mpsc and never silently drop events; fire-and-forget subscribers use broadcast with logged lag warnings
  3. A provider-agnostic ToolDefinition type exists in blufio-core that each provider can serialize to its own wire format independently
  4. TTS, Transcription, and Image provider trait interfaces are defined in blufio-core (no implementations required yet)
  5. Custom providers can be declared via TOML config with base_url, wire_protocol, and api_key_env
**Plans**: TBD

Plans:
- [x] 29-01: Event bus crate (blufio-bus)
- [x] 29-02: Core trait extensions (ToolDefinition, media traits, custom provider config)

### Phase 30: Multi-Provider LLM Support
**Goal**: Users can select OpenAI, Ollama, OpenRouter, or Gemini as their LLM backend with streaming and tool calling
**Depends on**: Phase 29 (provider-agnostic ToolDefinition required)
**Requirements**: PROV-01, PROV-02, PROV-03, PROV-04, PROV-05, PROV-06, PROV-07, PROV-08, PROV-09
**Success Criteria** (what must be TRUE):
  1. User can set `providers.default = "openai"` in config and chat via Blufio with OpenAI models including vision and structured outputs
  2. User can set `providers.default = "ollama"` and chat with locally-running Ollama models discovered via /api/tags, using native /api/chat (not OpenAI compat shim)
  3. User can set `providers.default = "openrouter"` and chat via OpenRouter with provider fallback ordering and correct X-Title/HTTP-Referer headers
  4. User can set `providers.default = "gemini"` and chat via Google Gemini with function calling mapped to provider-agnostic ToolDefinition
  5. Tool calling works correctly with streaming enabled across all four providers (no silent tool call drops)
**Plans**: TBD

Plans:
- [ ] 30-01: OpenAI provider (blufio-openai)
- [ ] 30-02: Ollama provider (blufio-ollama)
- [ ] 30-03: OpenRouter provider (blufio-openrouter)
- [ ] 30-04: Gemini provider (blufio-gemini)

### Phase 31: OpenAI-Compatible Gateway API
**Goal**: External callers can use Blufio as a drop-in OpenAI-compatible server via standard API endpoints
**Depends on**: Phase 30 (providers must exist to serve completions)
**Requirements**: API-01, API-02, API-03, API-04, API-05, API-06, API-07, API-08, API-09, API-10
**Success Criteria** (what must be TRUE):
  1. User can POST to /v1/chat/completions with an OpenAI SDK and receive a valid response with finish_reason (not stop_reason), usage stats, and correct tool_calls format
  2. SSE streaming works with standard OpenAI clients (data: [DONE] termination, delta chunks with finish_reason)
  3. User can POST to /v1/responses and receive semantic streaming events (response.created, output_text.delta, response.completed)
  4. User can GET /v1/tools to list available tools with JSON schemas and POST /v1/tools/invoke to execute a tool directly
  5. OpenAI wire types (OpenAiChatResponse) are completely separate from internal ProviderResponse -- no Anthropic-specific field names leak to external callers
**Plans**: TBD

Plans:
- [ ] 31-01: OpenAI-compatible /v1/chat/completions with wire type separation
- [ ] 31-02: OpenResponses /v1/responses API
- [ ] 31-03: Tools API (/v1/tools, /v1/tools/invoke)

### Phase 32: Scoped API Keys, Webhooks & Batch
**Goal**: Multi-user and multi-service deployments are secure with scoped keys, async event delivery via webhooks, and cost-efficient batch processing
**Depends on**: Phase 31 (API endpoints must exist before access control)
**Requirements**: API-11, API-12, API-13, API-14, API-15, API-16, API-17, API-18
**Success Criteria** (what must be TRUE):
  1. User can create scoped API keys via POST /v1/api-keys with scope restrictions (chat.completions, tools.invoke, admin) and per-key rate limits
  2. API keys can be expired and revoked; revoked keys are immediately rejected on all endpoints
  3. User can register webhooks via POST /v1/webhooks; events are delivered with HMAC-SHA256 signatures and exponential backoff retry on failure
  4. User can submit batch requests via POST /v1/batch and retrieve per-item results with success/error status
**Plans**: TBD

Plans:
- [ ] 32-01: Scoped API key management and rate limiting
- [ ] 32-02: Webhook registration and event delivery
- [ ] 32-03: Batch operations API

### Phase 33: Discord & Slack Channel Adapters
**Goal**: Users can interact with Blufio through Discord servers and Slack workspaces
**Depends on**: Phase 29 (event bus for bridging foundation)
**Requirements**: CHAN-01, CHAN-02, CHAN-03, CHAN-04, CHAN-05, CHAN-11, CHAN-12
**Success Criteria** (what must be TRUE):
  1. User can add Blufio as a Discord bot and chat in channels/DMs with full message content (MESSAGE_CONTENT privileged intent correctly handled with startup warning if missing)
  2. Discord slash commands work and ephemeral responses are used where appropriate
  3. User can add Blufio to a Slack workspace and chat via Events API or Socket Mode with Block Kit formatted messages
  4. Slack slash commands route to Blufio and responses render correctly
  5. Both adapters implement ChannelAdapter trait with capabilities manifest and format degradation pipeline works across channel capabilities
**Plans**: 3 plans

Plans:
- [x] 33-01: Shared infrastructure (FormatPipeline, StreamingEditorOps trait, ChannelCapabilities extension, config structs)
- [x] 33-02: Discord adapter (blufio-discord) + serve.rs wiring
- [x] 33-03: Slack adapter (blufio-slack) + serve.rs wiring

### Phase 34: WhatsApp, Signal, IRC & Matrix Adapters
**Goal**: Users can interact with Blufio through WhatsApp, Signal, IRC, and Matrix, and messages can bridge across any combination of channels
**Depends on**: Phase 33 (adapter patterns established), Phase 29 (event bus for bridging)
**Requirements**: CHAN-06, CHAN-07, CHAN-08, CHAN-09, CHAN-10, INFRA-06
**Success Criteria** (what must be TRUE):
  1. User can chat with Blufio via WhatsApp Cloud API (official Meta Business API) with the WhatsApp Web experimental adapter available behind a feature flag
  2. User can chat with Blufio via Signal using signal-cli JSON-RPC sidecar bridge
  3. User can chat with Blufio via IRC with TLS and NickServ authentication
  4. User can chat with Blufio via Matrix with room join and messaging (matrix-sdk 0.11 pinned)
  5. Cross-channel bridging works with configurable bridge rules in TOML between any combination of active channels
**Plans**: 5 plans

Plans:
- [x] 34-01: WhatsApp Cloud API + Web adapter (blufio-whatsapp crate, webhook handlers, gateway integration)
- [x] 34-02: Signal adapter via signal-cli JSON-RPC sidecar (blufio-signal crate, TCP/Unix auto-detect, exponential backoff)
- [x] 34-03: IRC adapter (blufio-irc crate, SASL PLAIN + NickServ, flood protection, message splitting)
- [x] 34-04: Matrix adapter (blufio-matrix crate, matrix-sdk =0.11.0 pinned, room join, invite auto-accept)
- [x] 34-05: Cross-channel bridging (blufio-bridge crate, event bus subscription, attribution formatting, loop prevention)

### Phase 35: Skill Registry & Code Signing
**Goal**: Users can install, manage, and trust WASM skills with cryptographic verification at every execution boundary
**Depends on**: Phase 29 (event bus for skill events)
**Requirements**: SKILL-01, SKILL-02, SKILL-03, SKILL-04, SKILL-05
**Success Criteria** (what must be TRUE):
  1. User can run blufio skill install/list/remove/update to manage skills from a local registry
  2. Registry stores skill manifests with SHA-256 content hashes and verifies integrity on every load
  3. Ed25519 code signatures are verified at install time AND before every WASM execution
  4. Capability enforcement is checked at every WASM host function call site (not just at install time)
**Plans**: 2 plans

Plans:
- [x] 35-01: Signing infrastructure, V8 migration, store extensions, CLI commands (Sign, Update, Keygen, Verify, Info)
- [x] 35-02: Pre-execution verification gate and capability enforcement audit

### Phase 36: Docker Image & Deployment
**Goal**: Users can deploy Blufio via Docker with a single command and run multiple instances via systemd templates
**Depends on**: Phase 29 (event bus injected into container)
**Requirements**: INFRA-04, INFRA-05, INFRA-07
**Success Criteria** (what must be TRUE):
  1. docker build produces a minimal image (distroless/static-debian12:nonroot base) with TLS and SQLCipher working
  2. docker-compose up starts Blufio with volume mounts for data/config/plugins, env injection for secrets, and health check passing
  3. Multi-instance systemd template (blufio@.service) allows running N instances with per-instance config directories
**Plans**: 2 plans

Plans:
- [x] 36-01: Multi-stage Dockerfile, docker-compose, healthcheck CLI subcommand
- [x] 36-02: Multi-instance systemd template and instance setup helper

### Phase 37: Node System
**Goal**: Users can pair multiple Blufio instances as a trusted device mesh for session sharing and coordinated approvals
**Depends on**: Phase 29 (event bus for node sync), Phase 32 (scoped keys for node auth)
**Requirements**: NODE-01, NODE-02, NODE-03, NODE-04, NODE-05
**Success Criteria** (what must be TRUE):
  1. User can pair two Blufio instances via QR code or shared token with Ed25519 mutual authentication (pairing tokens expire in 15 minutes and are single-use)
  2. Paired nodes connect via WebSocket with capability declaration (camera, screen, location, exec) and maintain heartbeat monitoring
  3. Node fleet is manageable via blufio nodes list/group/exec CLI commands with battery, memory, and connectivity status
  4. Approval routing broadcasts to all connected operator devices and any device can approve
**Plans**: TBD

Plans:
- [ ] 37-01: Node pairing and mutual authentication (blufio-node)
- [ ] 37-02: Node WebSocket connection, heartbeat, and fleet CLI
- [ ] 37-03: Approval routing broadcast

### Phase 38: Migration & CLI Utilities
**Goal**: Users migrating from OpenClaw have a clear path, and operators have essential CLI tools for benchmarking, privacy auditing, config generation, cleanup, and air-gapped deployment
**Depends on**: Phase 30 (providers needed for config translation), Phase 33 (channels needed for migration coverage)
**Requirements**: MIGR-01, MIGR-02, MIGR-03, MIGR-04, MIGR-05, CLI-01, CLI-02, CLI-03, CLI-04, CLI-05
**Success Criteria** (what must be TRUE):
  1. User can run blufio migrate --from-openclaw and have session history, cost records, and personality files (SOUL.md, AGENTS.md, USER.md, etc.) imported to Blufio storage
  2. blufio migrate preview shows a dry-run report listing what translates, what needs manual attention, and estimated cost comparison
  3. blufio config translate converts OpenClaw JSON config to Blufio TOML
  4. blufio bench runs built-in benchmarks (startup, context assembly, WASM, SQLite) and reports results
  5. blufio privacy evidence-report, blufio config recipe, blufio uninstall, and blufio bundle all work as documented
**Plans**: 2 plans

Plans:
- [ ] 38-01-PLAN.md — OpenClaw migration tool (migrate, preview, config translate)
- [ ] 38-02-PLAN.md — CLI utilities (bench, privacy, recipe, uninstall, bundle)

### Phase 39: Integration Verification
**Goal**: All 71 v1.3 requirements are verified end-to-end with cross-feature integration validated
**Depends on**: All previous v1.3 phases (29-38)
**Requirements**: (verification phase -- validates all requirements from phases 29-38)
**Success Criteria** (what must be TRUE):
  1. All 71 v1.3 requirements have formal verification evidence in VERIFICATION.md
  2. Cross-feature flows work: OpenAI SDK -> chat completions -> OpenRouter provider -> Discord channel -> webhook delivery
  3. Docker deployment passes full integration: docker-compose up -> API key create -> chat completion -> webhook fires
  4. Traceability is complete: every requirement maps to a phase, every phase has verification evidence
**Plans**: 7 plans

Plans:
- [x] 39-01-PLAN.md — Verify Phases 29+30 (Event Bus + Providers, 17 requirements)
- [x] 39-02-PLAN.md — Verify Phases 31+32 (Gateway API + Keys/Webhooks/Batch, 18 requirements)
- [x] 39-03-PLAN.md — Verify Phases 33+34 (Channel Adapters + Bridging, 13 requirements)
- [x] 39-04-PLAN.md — Verify Phases 35+36 (Skills + Docker, 8 requirements)
- [x] 39-05-PLAN.md — Re-verify Phases 37+38 (Nodes + Migration/CLI, 15 requirements)
- [x] 39-06-PLAN.md — Cross-feature integration flows (4 E2E tests)
- [x] 39-07-PLAN.md — Traceability audit + documentation updates + readiness summary

### Phase 40: Wire Global EventBus & Bridge
**Goal:** Create a single global EventBus in serve.rs shared across all subsystems, and wire the bridge loop
**Depends on:** Phase 29 (EventBus crate), Phase 34 (bridge crate)
**Requirements:** INFRA-01, INFRA-02, INFRA-03, INFRA-06
**Gap Closure:** Closes runtime wiring gaps from v1.3 audit

Plans:
- [ ] 40-01: Global EventBus creation and subsystem sharing in serve.rs
- [ ] 40-02: Wire blufio-bridge import and run_bridge_loop() startup

### Phase 41: Wire ProviderRegistry into Gateway
**Goal:** Add Phase 30 provider crates as binary dependencies, implement ProviderRegistry, and wire into GatewayState
**Depends on:** Phase 30 (provider crates), Phase 31 (gateway with ProviderRegistry trait)
**Requirements:** API-01, API-02, API-03, API-04, API-05, API-06, API-07, API-08, API-09, API-10, PROV-01, PROV-02, PROV-03, PROV-04, PROV-05, PROV-06, PROV-07, PROV-08, PROV-09
**Gap Closure:** Closes runtime wiring gaps from v1.3 audit

Plans:
- [ ] 41-01: Add provider crate deps and implement concrete ProviderRegistry
- [ ] 41-02: Wire ProviderRegistry into GatewayState in serve.rs

### Phase 42: Wire Gateway Stores
**Goal:** Instantiate ApiKeyStore, WebhookStore, and BatchStore in serve.rs and wire into GatewayState
**Depends on:** Phase 32 (store implementations), Phase 40 (global EventBus for webhook delivery)
**Requirements:** API-11, API-12, API-13, API-14, API-15, API-16, API-17, API-18
**Gap Closure:** Closes runtime wiring gaps from v1.3 audit
**Plans:** 2/2 plans complete

Plans:
- [ ] 42-01-PLAN.md — Add store/event_bus setters to GatewayChannel, instantiate stores in serve.rs
- [ ] 42-02-PLAN.md — Spawn webhook delivery background task with EventBus

### Phase 43: Wire EventBus Event Publishers
**Goal:** Wire EventBus into AgentLoop and WasmSkillRuntime so chat.completed and tool.invoked webhook events actually fire
**Depends on:** Phase 42 (webhook delivery spawned), Phase 29 (EventBus)
**Requirements:** API-16
**Gap Closure:** Closes 2 event publisher gaps from v1.3 audit (AgentLoop → ChannelEvent::MessageSent, WasmSkillRuntime → SkillEvent::Invoked/Completed)

Plans:
- [x] 43-01-PLAN.md — Wire EventBus into AgentLoop and WasmSkillRuntime, publish chat.completed and tool.invoked events

### Phase 44: Node Approval Wiring
**Goal:** Wire ApprovalRouter into EventBus for event-driven triggering and fix ConnectionManager forwarding
**Depends on:** Phase 37 (node system), Phase 40 (global EventBus)
**Requirements:** NODE-05 (enhancement — core satisfied, wiring gaps remain)
**Gap Closure:** Closes Phase 37 tech debt from v1.3 audit

Plans:
- [ ] 44-01-PLAN.md — Subscribe ApprovalRouter to EventBus for automatic event-driven triggering
- [ ] 44-02-PLAN.md — Forward ApprovalResponse from ConnectionManager to handle_response()

### Phase 45: Documentation & Traceability Sync
**Goal:** Update stale REQUIREMENTS.md traceability entries and fix ROADMAP.md inaccuracies
**Depends on:** Phase 42 (all gap closure phases with VERIFICATION.md files)
**Requirements:** (documentation phase — no new requirements)
**Gap Closure:** Closes documentation staleness from v1.3 audit

Plans:
- [ ] 45-01-PLAN.md — Update 31 traceability entries from Pending to Verified with VERIFICATION.md references
- [ ] 45-02-PLAN.md — Fix ROADMAP.md Phase 32 checkbox and stale status lines

## Progress

**Execution Order:**
Phases execute in numeric order: 29 -> 30 -> 31 -> ... -> 39

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Project Foundation & Workspace | v1.0 | 2/2 | Complete | 2026-02-28 |
| 2. Persistence & Security Vault | v1.0 | 2/2 | Complete | 2026-02-28 |
| 3. Agent Loop & Telegram | v1.0 | 4/4 | Complete | 2026-03-01 |
| 4. Context Engine & Cost Tracking | v1.0 | 3/3 | Complete | 2026-03-01 |
| 5. Memory & Embeddings | v1.0 | 3/3 | Complete | 2026-03-01 |
| 6. Model Routing & Smart Heartbeats | v1.0 | 3/3 | Complete | 2026-03-01 |
| 7. WASM Skill Sandbox | v1.0 | 4/4 | Complete | 2026-03-01 |
| 8. Plugin System & Gateway | v1.0 | 3/3 | Complete | 2026-03-01 |
| 9. Production Hardening | v1.0 | 3/3 | Complete | 2026-03-01 |
| 10. Multi-Agent & Final Integration | v1.0 | 3/3 | Complete | 2026-03-01 |
| 11. Fix Critical Integration Bugs | v1.0 | 4/4 | Complete | 2026-03-01 |
| 12. Verify Unverified Phases | v1.0 | 5/5 | Complete | 2026-03-01 |
| 13. Sync Traceability & Documentation | v1.0 | 1/1 | Complete | 2026-03-02 |
| 14. Wire Cross-Phase Integration | v1.0 | 3/3 | Complete | 2026-03-02 |
| 15. MCP Foundation | v1.1 | 4/4 | Complete | 2026-03-02 |
| 16. MCP Server stdio | v1.1 | 3/3 | Complete | 2026-03-02 |
| 17. MCP Server HTTP + Resources | v1.1 | 5/5 | Complete | 2026-03-02 |
| 18. MCP Client | v1.1 | 4/4 | Complete | 2026-03-03 |
| 19. Integration Testing + Tech Debt | v1.1 | 5/5 | Complete | 2026-03-03 |
| 20. Verify Phase 15 & 16 Completeness | v1.1 | 4/4 | Complete | 2026-03-03 |
| 21. Fix MCP Wiring Gaps | v1.1 | 4/4 | Complete | 2026-03-03 |
| 22. Verify Phase 18 & 19 + Close Traceability | v1.1 | 3/3 | Complete | 2026-03-03 |
| 23. Backup Integrity Verification | v1.2 | 1/1 | Complete | 2026-03-03 |
| 24. sd_notify Integration | v1.2 | 2/2 | Complete | 2026-03-03 |
| 25. SQLCipher Database Encryption | v1.2 | 4/4 | Complete | 2026-03-03 |
| 26. Minisign Signature Verification | v1.2 | 2/2 | Complete | 2026-03-03 |
| 27. Self-Update with Rollback | v1.2 | 2/2 | Complete | 2026-03-03 |
| 28. Close Audit Gaps | v1.2 | 2/2 | Complete | 2026-03-04 |
| 29. Event Bus & Core Trait Extensions | v1.3 | 2/2 | Complete | 2026-03-05 |
| 30. Multi-Provider LLM Support | v1.3 | 4/4 | Complete | 2026-03-05 |
| 31. OpenAI-Compatible Gateway API | v1.3 | 3/3 | Complete | 2026-03-05 |
| 32. Scoped API Keys, Webhooks & Batch | v1.3 | 3/3 | Complete | 2026-03-06 |
| 33. Discord & Slack Channel Adapters | v1.3 | 3/3 | Complete | 2026-03-06 |
| 34. WhatsApp, Signal, IRC & Matrix Adapters | v1.3 | 5/5 | Complete | 2026-03-06 |
| 35. Skill Registry & Code Signing | v1.3 | 2/2 | Complete | 2026-03-06 |
| 36. Docker Image & Deployment | v1.3 | 2/2 | Complete | 2026-03-07 |
| 37. Node System | v1.3 | 3/3 | Complete | 2026-03-07 |
| 38. Migration & CLI Utilities | v1.3 | 2/2 | Complete | 2026-03-07 |
| 39. Integration Verification | v1.3 | Complete    | 2026-03-07 | 2026-03-07 |
| 40. Wire Global EventBus & Bridge | 2/2 | Complete    | 2026-03-07 | - |
| 41. Wire ProviderRegistry into Gateway | 2/2 | Complete    | 2026-03-07 | - |
| 42. Wire Gateway Stores | 2/2 | Complete    | 2026-03-07 | - |
| 43. Wire EventBus Event Publishers | v1.3 | 0/1 | Planned | - |
| 44. Node Approval Wiring | v1.3 | 0/2 | Planned | - |
| 45. Documentation & Traceability Sync | v1.3 | 0/2 | Planned | - |

---
*Roadmap created: 2026-02-28*
*Last updated: 2026-03-07 after gap closure phases 43-45 added from milestone audit*
