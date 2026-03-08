# Requirements: Blufio v1.3

**Defined:** 2026-03-05
**Core Value:** An always-on personal AI agent that is secure enough to trust, efficient enough to afford, and simple enough to deploy by copying one file.

## v1.3 Requirements

Requirements for v1.3 Ecosystem Expansion. Each maps to roadmap phases.

### API Layer

- [x] **API-01**: User can send OpenAI-compatible chat completions via POST /v1/chat/completions
- [x] **API-02**: Chat completions endpoint supports SSE streaming responses
- [x] **API-03**: Chat completions endpoint supports tool calling (tools + tool_choice)
- [x] **API-04**: Chat completions endpoint supports response_format (JSON mode)
- [x] **API-05**: Chat completions responses include usage (token counts + cost)
- [x] **API-06**: OpenAI wire types are separate from internal ProviderResponse (finish_reason vs stop_reason)
- [x] **API-07**: User can send requests via OpenResponses POST /v1/responses
- [x] **API-08**: Responses endpoint streams semantic events (response.created, output_text.delta, response.completed)
- [x] **API-09**: User can invoke tools directly via POST /v1/tools/invoke
- [x] **API-10**: User can list available tools via GET /v1/tools with JSON schemas
- [x] **API-11**: User can create scoped API keys via POST /v1/api-keys
- [x] **API-12**: API keys support scope restrictions (chat.completions, tools.invoke, admin)
- [x] **API-13**: API keys support per-key rate limiting (requests per minute)
- [x] **API-14**: API keys support expiration and revocation
- [x] **API-15**: User can register webhooks via POST /v1/webhooks
- [x] **API-16**: Webhooks deliver events with HMAC-SHA256 signing and exponential backoff retry
- [x] **API-17**: User can submit batch requests via POST /v1/batch
- [x] **API-18**: Batch results available with per-item success/error status

### Providers

- [x] **PROV-01**: OpenAI provider with streaming and tool calling
- [x] **PROV-02**: OpenAI provider supports vision and structured outputs
- [x] **PROV-03**: OpenAI provider configurable via base_url (Azure OpenAI, Together, Fireworks)
- [x] **PROV-04**: Ollama provider using native /api/chat endpoint (not OpenAI compat shim)
- [x] **PROV-05**: Ollama auto-discovers local models via /api/tags
- [x] **PROV-06**: OpenRouter provider with streaming and X-Title/HTTP-Referer headers
- [x] **PROV-07**: OpenRouter supports provider fallback ordering
- [x] **PROV-08**: Google/Gemini provider with native API format (not OpenAI-compatible)
- [x] **PROV-09**: Gemini function calling mapped to provider-agnostic ToolDefinition
- [x] **PROV-10**: Provider-agnostic ToolDefinition type in blufio-core (replaces Anthropic-specific)
- [x] **PROV-11**: TTS provider trait (AudioProvider) defined with reference interface
- [x] **PROV-12**: Transcription provider trait defined with reference interface
- [x] **PROV-13**: Image generation provider trait (ImageProvider) defined with reference interface
- [x] **PROV-14**: Custom provider via TOML config (base_url + wire_protocol + api_key_env)

### Channels

- [x] **CHAN-01**: Discord adapter with Gateway WebSocket and REST via serenity
- [x] **CHAN-02**: Discord slash commands and ephemeral responses
- [x] **CHAN-03**: Discord MESSAGE_CONTENT privileged intent correctly handled
- [x] **CHAN-04**: Slack adapter with Events API and Socket Mode via slack-morphism
- [x] **CHAN-05**: Slack slash commands and Block Kit messages
- [x] **CHAN-06**: WhatsApp Cloud API adapter (official Meta Business API)
- [x] **CHAN-07**: WhatsApp Web adapter (experimental, behind feature flag, labeled unstable)
- [x] **CHAN-08**: Signal adapter via signal-cli JSON-RPC sidecar bridge
- [x] **CHAN-09**: IRC adapter with TLS and NickServ authentication via irc crate
- [x] **CHAN-10**: Matrix adapter with room join and messaging via matrix-sdk 0.11
- [x] **CHAN-11**: All new adapters implement ChannelAdapter trait with capabilities manifest
- [x] **CHAN-12**: Format degradation pipeline works across all new channel capabilities

### Infrastructure

- [x] **INFRA-01**: Internal event bus using tokio broadcast with lag handling
- [x] **INFRA-02**: Event bus publishes typed events (session, channel, skill, node, webhook, batch)
- [x] **INFRA-03**: Event bus uses mpsc for reliable subscribers (webhook delivery)
- [x] **INFRA-04**: Docker multi-stage build producing minimal image (distroless or scratch)
- [x] **INFRA-05**: docker-compose.yml with volume mounts, env injection, and health check
- [x] **INFRA-06**: Cross-channel bridging with configurable bridge rules in TOML
- [x] **INFRA-07**: Multi-instance systemd template (blufio@.service) with per-instance config

### Skills

- [x] **SKILL-01**: Local skill registry with install/list/remove/update commands
- [x] **SKILL-02**: Registry stores skill manifests with SHA-256 content hashes
- [x] **SKILL-03**: Ed25519 code signing for WASM skill artifacts
- [x] **SKILL-04**: Signature verification at install time and before execution
- [x] **SKILL-05**: Capability enforcement checked at every WASM host function call site

### Node System

- [x] **NODE-01**: Node pairing via Ed25519 mutual authentication (QR or shared token)
- [x] **NODE-02**: Node connection via WebSocket with capability declaration (camera, screen, location, exec)
- [x] **NODE-03**: Node heartbeat monitoring (battery, memory, connectivity, stale detection)
- [x] **NODE-04**: Node fleet management CLI (blufio nodes list/group/exec)
- [x] **NODE-05**: Approval routing broadcasts to all connected operator devices

### Migration

- [x] **MIGR-01**: blufio migrate --from-openclaw reads OpenClaw data directory
- [x] **MIGR-02**: Migration imports session history and cost records to SQLite
- [x] **MIGR-03**: Migration imports workspace personality files (SOUL.md, AGENTS.md, USER.md, etc.)
- [x] **MIGR-04**: blufio migrate preview shows dry-run report of what translates and what needs manual attention
- [x] **MIGR-05**: blufio config translate maps OpenClaw JSON config to Blufio TOML

### CLI Utilities

- [x] **CLI-01**: blufio bench runs built-in benchmarks (startup, context assembly, WASM, SQLite)
- [x] **CLI-02**: blufio privacy evidence-report enumerates outbound data flows and local stores
- [x] **CLI-03**: blufio config recipe generates config templates (personal/team/production/iot)
- [x] **CLI-04**: blufio uninstall removes binary, service files, and optionally data
- [x] **CLI-05**: blufio bundle creates Minisign-signed air-gapped deployment archive

## Future Requirements

Deferred beyond v1.3. Tracked but not in current roadmap.

### Workflow

- **WF-01**: DAG workflow engine for multi-step task orchestration (PRD §5-3.7: explicitly v2.0)

### Extensions

- **EXT-01**: Browser extension connecting to Blufio gateway via WebSocket (PRD §4-9.8: post-v1.0)
- **EXT-02**: Remote skill registry / marketplace with CDN distribution
- **EXT-03**: TTS reference implementation (OpenAI TTS, ElevenLabs)
- **EXT-04**: Transcription reference implementation (Whisper, Deepgram)
- **EXT-05**: Image generation reference implementation (DALL-E, Stable Diffusion)
- **EXT-06**: Matrix E2E encryption support via matrix-sdk-crypto

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| DAG workflow engine | PRD §5-3.7 explicitly marks as v2.0 feature |
| Browser extension | PRD §4-9.8 marks as post-v1.0 roadmap item |
| Native plugin system (libloading) | WASM-only per v1.0 decision; memory safety boundary |
| Automatic plugin updates on startup | Bypasses code-signing verification; security risk |
| OpenAI Assistants API compatibility | Deprecated by OpenAI in favor of Responses API |
| WhatsApp Web as default path | Unofficial; permanent ban risk; Cloud API is primary |
| External message broker (Redis/NATS) | Contradicts single-binary model; tokio channels sufficient |
| GUI/visual config builder | Out of scope per PROJECT.md |
| Full Gemini API parity | Target stable subset; full parity tracks upstream |
| Signal native Rust integration | No production Rust library; signal-cli sidecar is viable path |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Verification | Status |
|-------------|-------|--------------|--------|
| INFRA-01 | Phase 40 | - | Pending |
| INFRA-02 | Phase 40 | - | Pending |
| INFRA-03 | Phase 40 | - | Pending |
| PROV-10 | Phase 29 | 29-VERIFICATION.md | Verified |
| PROV-11 | Phase 29 | 29-VERIFICATION.md | Verified |
| PROV-12 | Phase 29 | 29-VERIFICATION.md | Verified |
| PROV-13 | Phase 29 | 29-VERIFICATION.md | Verified |
| PROV-14 | Phase 29 | 29-VERIFICATION.md | Verified |
| PROV-01 | Phase 41 | - | Pending |
| PROV-02 | Phase 41 | - | Pending |
| PROV-03 | Phase 41 | - | Pending |
| PROV-04 | Phase 41 | - | Pending |
| PROV-05 | Phase 41 | - | Pending |
| PROV-06 | Phase 41 | - | Pending |
| PROV-07 | Phase 41 | - | Pending |
| PROV-08 | Phase 41 | - | Pending |
| PROV-09 | Phase 41 | - | Pending |
| API-01 | Phase 41 | - | Pending |
| API-02 | Phase 41 | - | Pending |
| API-03 | Phase 41 | - | Pending |
| API-04 | Phase 41 | - | Pending |
| API-05 | Phase 41 | - | Pending |
| API-06 | Phase 41 | - | Pending |
| API-07 | Phase 41 | - | Pending |
| API-08 | Phase 41 | - | Pending |
| API-09 | Phase 41 | - | Pending |
| API-10 | Phase 41 | - | Pending |
| API-11 | Phase 42 | - | Pending |
| API-12 | Phase 42 | - | Pending |
| API-13 | Phase 42 | - | Pending |
| API-14 | Phase 42 | - | Pending |
| API-15 | Phase 42 | - | Pending |
| API-16 | Phase 43 | 43-01-SUMMARY.md | Complete |
| API-17 | Phase 42 | - | Pending |
| API-18 | Phase 42 | - | Pending |
| CHAN-01 | Phase 33 | 33-VERIFICATION.md | Verified |
| CHAN-02 | Phase 33 | 33-VERIFICATION.md | Verified |
| CHAN-03 | Phase 33 | 33-VERIFICATION.md | Verified |
| CHAN-04 | Phase 33 | 33-VERIFICATION.md | Verified |
| CHAN-05 | Phase 33 | 33-VERIFICATION.md | Verified |
| CHAN-11 | Phase 33 | 33-VERIFICATION.md | Verified |
| CHAN-12 | Phase 33 | 33-VERIFICATION.md | Verified |
| CHAN-06 | Phase 34 | 34-VERIFICATION.md | Verified |
| CHAN-07 | Phase 34 | 34-VERIFICATION.md | Verified |
| CHAN-08 | Phase 34 | 34-VERIFICATION.md | Verified |
| CHAN-09 | Phase 34 | 34-VERIFICATION.md | Verified |
| CHAN-10 | Phase 34 | 34-VERIFICATION.md | Verified |
| INFRA-06 | Phase 40 | - | Pending |
| SKILL-01 | Phase 35 | 35-VERIFICATION.md | Verified |
| SKILL-02 | Phase 35 | 35-VERIFICATION.md | Verified |
| SKILL-03 | Phase 35 | 35-VERIFICATION.md | Verified |
| SKILL-04 | Phase 35 | 35-VERIFICATION.md | Verified |
| SKILL-05 | Phase 35 | 35-VERIFICATION.md | Verified |
| INFRA-04 | Phase 36 | 36-VERIFICATION.md | Verified |
| INFRA-05 | Phase 36 | 36-VERIFICATION.md | Verified |
| INFRA-07 | Phase 36 | 36-VERIFICATION.md | Verified |
| NODE-01 | Phase 37 | 37-VERIFICATION.md | Verified |
| NODE-02 | Phase 37 | 37-VERIFICATION.md | Verified |
| NODE-03 | Phase 37 | 37-VERIFICATION.md | Verified |
| NODE-04 | Phase 37 | 37-VERIFICATION.md | Verified |
| NODE-05 | Phase 37 | 37-VERIFICATION.md | Verified |
| MIGR-01 | Phase 38 | 38-VERIFICATION.md | Verified |
| MIGR-02 | Phase 38 | 38-VERIFICATION.md | Verified |
| MIGR-03 | Phase 38 | 38-VERIFICATION.md | Verified |
| MIGR-04 | Phase 38 | 38-VERIFICATION.md | Verified |
| MIGR-05 | Phase 38 | 38-VERIFICATION.md | Verified |
| CLI-01 | Phase 38 | 38-VERIFICATION.md | Verified |
| CLI-02 | Phase 38 | 38-VERIFICATION.md | Verified |
| CLI-03 | Phase 38 | 38-VERIFICATION.md | Verified |
| CLI-04 | Phase 38 | 38-VERIFICATION.md | Verified |
| CLI-05 | Phase 38 | 38-VERIFICATION.md | Verified |

**Coverage:**
- v1.3 requirements: 71 total
- Verified: 40
- Pending (gap closure): 31 (30 doc-stale from Ph 40-42 + 1 API-16 partial from Ph 43)
- Unverified: 0
- By category: API: 0/18, PROV: 5/14, CHAN: 12/12, INFRA: 3/7, SKILL: 5/5, NODE: 5/5, MIGR: 5/5, CLI: 5/5

**Notes:**
- INFRA-04 (Docker build) verified statically only -- Docker daemon not available
- NODE-05 has 2 internal wiring gaps (approval event bus subscription, WebSocket forwarding) but core requirement (broadcast + first-wins + timeout) is satisfied
- All 71 requirements have formal evidence in per-phase VERIFICATION.md files

---
*Requirements defined: 2026-03-05*
*Last updated: 2026-03-07 after gap closure phases 43-45 added (API-16 reset to [ ], assigned to Phase 43)*
