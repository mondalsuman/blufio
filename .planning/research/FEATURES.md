# Feature Research

**Domain:** Rust AI Agent Platform — Ecosystem Expansion (v1.3)
**Researched:** 2026-03-05
**Confidence:** HIGH (OpenAI API contracts, Rust crate ecosystem), MEDIUM (channel adapters, node system, event bus patterns), LOW (Signal/IRC due to unofficial API volatility)

---

## Scope

This document covers only the NEW features targeted for v1.3. All features in the Active requirements list in PROJECT.md are analyzed. Existing shipped features (Anthropic provider, Telegram adapter, HTTP/WS gateway, MCP, WASM skills, cost ledger, SQLCipher, Minisign, self-update) are treated as the foundation — they are dependencies, not scope.

The incumbent is OpenClaw (Node.js, 15+ channels, community skills, 6 structural weaknesses). The secondary reference is ZeroClaw (Rust rewrite of OpenClaw, 28+ providers). The v1.3 goal is ecosystem parity plus structural superiority.

---

## Feature Landscape

### Table Stakes (Users Expect These)

Features that users assume exist for an "ecosystem-grade" AI agent platform. Missing any of these makes the platform feel incomplete or unshippable for v1.3.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| OpenAI-compatible `/v1/chat/completions` | De facto LLM API standard; every tool, SDK, proxy, and frontend expects it; enables drop-in OpenAI replacement in any system | HIGH | Must implement: streaming (SSE), tool calling, message roles (system/user/assistant/tool), model selection, finish_reason, usage tokens, structured response format, error format `{"error":{"message":...,"type":...,"code":...}}`. Translation layer: map incoming OpenAI-format to active provider native format and map response back |
| OpenResponses `/v1/responses` API | OpenAI Responses API launched March 2025, superseding Assistants API; Open Responses open-source spec makes it multi-provider standard; needed for agentic loop exposure | HIGH | Stateful multi-turn with preserved reasoning across turns; server-side tool invocation and agentic loops; reasoning items: content/encrypted_content/summary per Open Responses spec; do NOT implement deprecated Assistants API |
| Tools Invoke API (`/v1/tools/invoke`) | Direct skill/tool execution without full agent loop; required for MCP clients and orchestration systems that call tools independently | MEDIUM | Must respect existing WASM sandbox capability manifests; integrates with skill registry; subset of Responses API agentic loop |
| Scoped API keys with rate limiting | Any multi-user or multi-service deployment requires key isolation; standard for any API gateway; prevents one consumer from exhausting LLM budget | MEDIUM | Keys need: scope (read/write/admin), per-key TPM/RPM limits, key revocation, expiry; integrates with existing cost ledger and bearer auth; store keys in existing SQLCipher DB |
| Webhook management | Async event delivery to consumers; required for Discord/Slack integrations and batch job completion notification; any async API needs webhooks | MEDIUM | Create/list/delete webhooks; delivery with retry + exponential backoff; HMAC-SHA256 signature on payload; dead-letter after N failures; depends on event bus for delivery queue |
| Batch operations API | 50% cost reduction on async workloads (OpenAI Batch API precedent); required for bulk processing use cases; separate rate limit pool | MEDIUM | Submit batch → poll status → retrieve results; 24h turnaround SLA; separate rate limit pool (does not consume synchronous quota); completion notification via webhook |
| OpenAI provider plugin | OpenAI is the baseline LLM provider; most OpenClaw users use OpenAI; required for migration to work | HIGH | Full streaming, tool calling, vision, structured outputs, o1/o3 reasoning models; implement against OpenAI wire format (also covers Azure OpenAI, Together, Fireworks via base_url config) |
| Ollama provider plugin | Local/private LLM requirement for air-gapped and privacy-first deployments; increasingly standard for self-hosted agents | MEDIUM | Use native Ollama API `/api/chat` — NOT `/v1/chat` (OpenAI-compat mode breaks tool calling per Ollama issue #12557); streaming; auto-discover local models via `/api/tags` |
| OpenRouter provider plugin | Single endpoint to 500+ models (60+ providers); enables model fallback routing without managing multiple API keys; lowest friction onboarding path | MEDIUM | OpenAI-compatible wire format; add `X-Title` and `HTTP-Referer` headers per OpenRouter spec; `providers.order` field for fallback; streaming; auto-fallback on provider failure |
| Google/Gemini provider plugin | Second largest LLM provider; Gemini 2.x models competitive with Claude/GPT-4; required for "full provider coverage" claim | HIGH | Gemini API is NOT OpenAI-compatible; separate auth (API key vs. Vertex OAuth); function calling format differs; native SSE streaming; gmini or rust-genai crates as reference; separate implementation from OpenAI-compatible path |
| Discord channel adapter | Largest gaming/developer community chat; 500M+ users; expected for any agent platform targeting developers | HIGH | Requires Gateway WebSocket + REST — REST alone cannot receive messages; serenity crate (high-level, handles sharding + rate limiting); slash commands; ephemeral responses; message chunking for 2000-char limit; rate limit handling |
| Slack channel adapter | Enterprise standard; majority of team productivity integrations use Slack; expected for business use cases | HIGH | Events API + Socket Mode (HTTP endpoint alternative); slash commands; Block Kit messages; OAuth for workspace install; legacy RTM API removed March 2025 — must use new Slack platform; slack-morphism-rust crate |
| Docker image (Dockerfile + compose) | Standard deployment artifact; required for cloud/container deployment; users expect `docker pull blufio` | MEDIUM | Multi-stage build (musl static binary from v1.2); FROM scratch (~5-10MB image); docker-compose.yml with volume mounts for SQLite (`/data`), config (`/config`), plugins (`/plugins`); env var injection for secrets; SQLCipher already works in musl/scratch (vendored OpenSSL from v1.2) |
| Skill registry (local) | Skill discovery and installation required before code signing can be enforced; `blufio skill install` needs a registry backend | MEDIUM | Local registry: TOML index at `~/.local/share/blufio/registry/index.toml`; install/list/remove/update commands; version pinning; SHA-256 content hash; integrates with existing WASM sandbox and capability manifests |
| Code signing (Ed25519) for skills | Security requirement before any skill distribution; unsigned skills are a supply chain risk; ed25519 infra already in workspace | MEDIUM | Sign skill WASM artifacts with ed25519-dalek (already in workspace); embed signature in skill manifest alongside capability manifest; verify at install time AND before execution; reuse Minisign pattern from v1.2 |
| Event bus (internal pub/sub) | Required decoupling mechanism for cross-channel bridging, webhook delivery, node synchronization, and batch job completion; multiple v1.3 features depend on it | HIGH | Tokio `broadcast` for fan-out (multiple channel adapters watching same event type); `mpsc` for directed events (webhook delivery queue); typed event enum (`AgentEvent`, `ChannelEvent`, `SkillEvent`, `NodeEvent`, `WebhookEvent`, `BatchEvent`); capacity-bounded (project principle); in-process only — no external broker needed at current scale |
| OpenClaw migration tool | Directly enables acquisition from OpenClaw's installed base; the "kill shot" requires a migration path; no migration = no switchers | HIGH | Read OpenClaw config (TOML/JSONL) and memory stores; map channels/providers to Blufio equivalents; migrate conversation history to SQLite; import skills (flag npm skills for manual migration); output `migration-report.md` with what was migrated, what needs manual attention, cost comparison estimate |
| blufio config recipe | Guided config generation; essential for first-run experience; reduces operator support burden | LOW | Interactive or template-based; generate valid TOML for common setups (Telegram+Anthropic, Discord+OpenAI, etc.); validation before writing |
| blufio uninstall | Clean removal expected for any installable system tool | LOW | Remove binary, service files, data dirs; optional data purge with confirmation prompt; systemd unit disable |
| Multi-instance systemd template | Required for running multiple agent personas on one host; standard systemd `@instance` pattern | LOW | `blufio@.service` template unit; per-instance config path (`~/.config/blufio/%i.toml`); coexist without port conflicts (config must set different ports per instance) |

### Differentiators (Competitive Advantage)

Features that go beyond what users expect and create meaningful advantage over OpenClaw and ZeroClaw.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| WhatsApp channel adapter (Cloud API) | 2B+ users; business messaging channel; OpenClaw uses unstable unofficial libraries; official Cloud API is stable | HIGH | Cloud API mode: official Meta Cloud API, webhook-based incoming messages, requires Meta Business Account (production-grade); whatsapp-business-rs crate; label clearly as "requires Meta Business Account" |
| WhatsApp channel adapter (Web mode) | Personal use without Meta Business Account; ZeroClaw merged this 2026-02-19 | HIGH | Web mode: unofficial (wa-rs crate, links as secondary device via QR); volatile — WhatsApp can break without notice; label as "experimental"; do not enable by default; separate feature flag |
| Signal channel adapter | Privacy-first users; journalists, activists, security researchers; no other major agent platform supports Signal natively | HIGH | presage crate (Rust, links as secondary device via QR); unofficial — no official Signal bot API; link via QR code; API volatile (Signal has broken presage without notice); label "experimental"; evaluate signal-cli-rest-api REST sidecar as stabler alternative |
| IRC channel adapter | Open-source communities (Libera.chat, OFTC); longevity users; low complexity once base channel trait exists | LOW | irc crate (async, tokio); PRIVMSG in/out; TLS; NickServ auth; demonstrates extensibility of adapter trait; ships as example adapter |
| Matrix channel adapter | Decentralized, federated, E2E encrypted; growing developer adoption; bridges to 20+ other platforms via Matrix bridge ecosystem | HIGH | matrix-sdk crate (official, maintained by Element/Matrix.org, built on ruma); account registration on homeserver; join rooms; E2E encryption via matrix-sdk-crypto adds complexity — start without E2E, add in follow-up; strategic because Matrix bridges to Discord/Slack/IRC natively |
| TTS/Transcription/Image provider traits | Multimodal capability; enables voice-first agents and image generation; establishes plugin contract for third-party authors | MEDIUM | New adapter traits: `AudioProvider` (TTS + transcription) and `ImageProvider`; sibling traits to existing `Provider`; v1.3 ships trait definitions only — reference implementations (OpenAI TTS, ElevenLabs, Deepgram) in v1.3.x or v1.4 |
| Cross-channel bridging | Unique: relay conversations across platforms (Telegram user talks to Discord server); requires event bus | HIGH | Depends on event bus; configurable bridge rules in TOML; message normalization across platforms; explicit opt-in per bridge rule (privacy); implementation in blufio-gateway as bridge router component |
| Node system (paired devices) | Multi-device agent coordination; secondary devices extend reach without full stack; enables edge deployments | HIGH | Ed25519 mutual authentication for pairing (ed25519-dalek already in workspace); secondary node relays messages to primary; primary controls secondary's tool access allowlist; pairing via QR or shared token; depends on event bus + scoped API keys |
| Skill registry / marketplace (remote) | Community skill distribution; ecosystem network effects; switching costs | HIGH | Remote registry: HTTPS static file server hosting index.toml; Ed25519-signed WASM skills; signature verification before install; download stats; deferred to v1.3.x — local registry must be stable first |
| blufio bundle (air-gapped) | Enterprise and privacy deployment; package everything into self-contained archive for offline install | MEDIUM | TAR.GZ of: binary + plugin WASMs + ONNX model + default config; Minisign-signed manifest; `blufio bundle --output blufio-bundle.tar.gz`; extracts to well-known paths; enables regulated-environment deployment |
| blufio bench (built-in benchmarks) | Self-reported performance validation; builds trust for "single binary efficiency" claim; helps operators size hardware | LOW | Measure: startup time, context assembly latency, WASM skill invocation overhead, SQLite read/write throughput; `blufio bench --format json`; output Prometheus-compatible metrics |
| blufio privacy evidence-report | Compliance artifact; lets security-conscious operators audit outbound data flows | LOW | Enumerate: all outbound domains (LLM APIs, update server, MCP servers), local-only data stores (SQLite, ONNX), no telemetry assertion; `blufio privacy evidence-report --format json`; human-readable + machine-readable output |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Automatic plugin updates on startup | "Always latest" appeals; no manual intervention | Silent auto-update of third-party WASM bypasses code-signing verification; a compromised registry could push malicious skills without user awareness | Explicit `blufio skill update` command with signature verification; notify-on-new-version in `blufio doctor` output |
| Native plugin system (libloading) | Faster than WASM; "real" dynamic libraries feel more powerful | Undefined behavior from ABI mismatches; memory safety boundary eliminated; one bad plugin crashes the whole process; contradicts "secure by default" positioning | WASM-only continues; subprocess escape hatch already exists for native access needs |
| WhatsApp Web mode as primary path | No Meta Business Account required | Unofficial; WhatsApp breaks compatibility without notice; bans accounts without warning; makes Blufio look unreliable | WhatsApp Cloud API as primary (official, stable); Web mode as experimental opt-in with clear warnings in UI and docs |
| External message broker (Redis, NATS) | "Production grade" pub/sub; horizontal scaling language | Contradicts single-binary deployment model; adds operational dependencies; current scale (10-50 sessions, one host) does not require external broker | Internal tokio broadcast channels with bounded capacity; migrate to external broker only when multi-host deployment becomes an explicit product requirement |
| Auto-detect and bridge all channels by default | "Unified inbox" sounds compelling | Combinatorial testing surface; privacy implications of relaying messages without per-channel consent; rich media does not translate across all platforms | Explicit bridge configuration in TOML; user must opt in per bridge rule; message normalization with lossy-conversion warnings |
| OpenAI Assistants API compatibility | Backward compat for existing Assistants-based integrations | Assistants API is deprecated by OpenAI in favor of Responses API; implementing it creates two diverging agentic API surfaces to maintain | Implement Responses API only; provide migration guide from Assistants to Responses |
| Per-model fine-tuning API | "Customize your agent" sounds valuable | Requires GPU infrastructure, dataset management, training pipelines; orthogonal to single-binary-on-VPS model; no in-scope provider supports fine-tuning at the API abstraction level Blufio controls | Model selection via routing (Haiku/Sonnet/Opus pattern) is the right tuning lever; skill customization covers task-specific behavior |
| GUI/visual config builder | Lowers barrier for non-technical users | Contradicts PROJECT.md "Out of Scope: Visual builder / GUI — CLI and config files only"; `blufio config recipe` CLI is the right alternative | `blufio config recipe` interactive CLI; good defaults + clear TOML docs |
| In-process restart after self-update | "Zero downtime updates" | If new binary has startup bug, running agent is lost with no recovery; systemd restart model is safer | Binary swap + health check via subprocess (`blufio doctor`); operator or systemd controls restart timing |

---

## Feature Dependencies

```
[Event Bus]
    └──required by──> [Cross-Channel Bridging]
    └──required by──> [Webhook Management] (delivery queue)
    └──required by──> [Node System] (sync events)
    └──required by──> [Batch Operations API] (completion notification)

[Skill Registry (local)]
    └──required by──> [Code Signing for Skills] (signing enforcement happens at install/exec)
    └──required by──> [Skill Registry (remote/marketplace)] (extends local)
    └──required by──> [blufio bundle] (bundles installed skills)

[Code Signing for Skills]
    └──required by──> [Skill Registry (remote/marketplace)] (unsigned remote = supply chain risk)

[OpenAI-Compatible /v1/chat/completions]
    └──required by──> [OpenAI Provider Plugin] (provider must produce compatible response)
    └──required by──> [OpenRouter Provider Plugin] (same)
    └──required by──> [Ollama Provider Plugin] (same via native API)
    └──required by──> [Scoped API Keys] (keys gate access to completions endpoint)

[OpenResponses /v1/responses]
    └──required by──> [Tools Invoke API] (invoke is a subset of responses agentic loop)

[Scoped API Keys]
    └──required by──> [Webhook Management] (webhooks scoped to key)
    └──required by──> [Batch Operations API] (batch submissions tied to key)
    └──required by──> [Node System] (nodes authenticate via scoped keys or paired tokens)

[Discord Channel Adapter]
    └──enhances──> [Cross-Channel Bridging] (source or destination channel)
    └──required by──> [OpenClaw Migration Tool] (for Discord config migration)

[Slack Channel Adapter]
    └──enhances──> [Cross-Channel Bridging]
    └──required by──> [OpenClaw Migration Tool] (for Slack config migration)

[Matrix Channel Adapter]
    └──enhances──> [Cross-Channel Bridging] (Matrix natively bridges to other platforms)

[Node System]
    └──requires──> [Event Bus] (node sync messages are events)
    └──requires──> [Scoped API Keys] (nodes authenticate via scoped keys or paired tokens)

[Multi-Instance systemd Template]
    └──enhances──> [blufio config recipe] (instances need per-instance config generation)

[OpenClaw Migration Tool]
    └──requires──> [OpenAI Provider Plugin] (OpenClaw primarily uses OpenAI; migration maps providers)
    └──enhanced by──> [Discord Channel Adapter] (Discord migration path)
    └──enhanced by──> [Slack Channel Adapter] (Slack migration path)

[TTS/Transcription/Image Provider Traits]
    └──independent──> (defines adapter interfaces only; no implementations in v1.3)

[blufio bundle]
    └──requires──> [Skill Registry (local)] (bundles locally installed skills)

[blufio bench]
    └──independent──> (standalone command; no new feature dependencies)

[blufio privacy evidence-report]
    └──enhanced by──> [Multi-Provider LLM] (reports all outbound provider domains)

[IRC Channel Adapter]
    └──independent──> (simplest channel; good first adapter after event bus)

[Signal Channel Adapter]
    └──independent──> [presage crate] (unofficial; volatile; experimental only)

[WhatsApp Channel Adapter (Cloud API)]
    └──independent──> (official Meta Cloud API; webhook-based)

[WhatsApp Channel Adapter (Web mode)]
    └──independent──> (unofficial; experimental; separate from Cloud API mode)
```

### Dependency Notes

- **Event Bus blocks four other features.** Cross-channel bridging, webhook management, node system, and batch completion all require internal pub/sub. Event bus must be the first infrastructure feature implemented.
- **Skill Registry (local) must precede Code Signing.** The signing enforcement point is at install and execution in the registry. Signing without a registry has no enforcement mechanism.
- **OpenAI `/v1/chat/completions` must precede provider plugins.** The completions endpoint defines the response schema that all providers must produce. Define the endpoint first; providers implement to it.
- **Scoped API Keys depend on Completions API.** Keys are meaningless without an endpoint to protect.
- **OpenClaw migration is most useful late.** It maps to Blufio equivalents — the more providers and channels Blufio has, the better the migration coverage. Build after at least OpenAI provider + one additional channel.
- **TTS/Transcription/Image traits are pure interface in v1.3.** Define the adapter traits; implementations are v1.3.x or v1.4. Do not defer the trait definition itself — it establishes the contract for plugin authors.
- **Node System is highest dependency count.** Requires event bus + scoped keys + Ed25519 (already have). Implement last among new infrastructure features.
- **Signal and WhatsApp Web are experimental.** Neither has an official bot API. Both can break at any time. Keep behind explicit `--features` or config flags; do not ship in the default binary.

---

## MVP Definition

This is a subsequent milestone MVP. The question: what minimum subset of v1.3 features delivers the ecosystem expansion goal and enables user acquisition from OpenClaw?

### Launch With (v1.3 core)

- [ ] Event bus (internal pub/sub) — unblocks 4 other features; implement first as infrastructure
- [ ] OpenAI-compatible `/v1/chat/completions` — de facto standard; required before provider plugins
- [ ] OpenAI provider plugin — most common OpenClaw config; primary non-Anthropic provider
- [ ] Ollama provider plugin — privacy/air-gapped differentiation; strong contrast to OpenClaw
- [ ] OpenRouter provider plugin — 500+ models; lowest friction onboarding; single API key
- [ ] Google/Gemini provider plugin — required for "full provider coverage" claim
- [ ] Discord channel adapter — largest developer community; primary non-Telegram channel
- [ ] Slack channel adapter — enterprise standard; business use cases
- [ ] Scoped API keys with rate limiting — required before any multi-user deployment scenario
- [ ] Webhook management — required for async event delivery to channel integrations
- [ ] Docker image (Dockerfile + compose) — required for cloud deployment; hard barrier without it
- [ ] Skill registry (local) — required before code signing; enables `blufio skill install`
- [ ] Code signing (Ed25519) for skills — security requirement before any public skill sharing
- [ ] OpenClaw migration tool — primary acquisition trigger; users will not switch without it

### Add After Validation (v1.3.x)

- [ ] OpenResponses `/v1/responses` API — add once `/v1/chat/completions` stabilizes; stateful agentic parity
- [ ] Tools Invoke API (`/v1/tools/invoke`) — depends on responses API shape
- [ ] Batch operations API — add when bulk workload demand is confirmed
- [ ] WhatsApp adapter (Cloud API mode) — requires Meta Business Account; add when operator demand justifies documentation effort
- [ ] Matrix channel adapter — high complexity (E2E crypto); high strategic value; add when Discord/Slack are stable
- [ ] Cross-channel bridging — requires event bus + multiple adapters stable; add after both ship
- [ ] Node system (paired devices) — highest dependency count; add when event bus + scoped keys are stable
- [ ] TTS/Transcription/Image provider traits — define in v1.3.x; implementations in v1.4
- [ ] blufio bundle (air-gapped) — add when Docker covers primary use case and air-gapped demand surfaces
- [ ] blufio bench — add when performance regression risk from multi-provider work becomes real
- [ ] blufio privacy evidence-report — add when enterprise/compliance interest is confirmed
- [ ] blufio config recipe — add when operator onboarding friction data surfaces
- [ ] blufio uninstall — add when user complaints about manual removal surface

### Future Consideration (v1.4+)

- [ ] IRC channel adapter — low user volume; minimal business impact; easy to add post-v1.3.x
- [ ] Signal channel adapter — unofficial API risk; personal use only; evaluate presage stability
- [ ] WhatsApp Web mode (unofficial) — evaluate stability; experimental flag only if added
- [ ] Skill registry / marketplace (remote) — requires community + moderation infrastructure
- [ ] Multi-instance systemd template — niche; operators can achieve manually with config path overrides today

---

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Event Bus | HIGH (unblocks 4 features) | MEDIUM | P1 |
| OpenAI `/v1/chat/completions` | HIGH | HIGH | P1 |
| OpenAI Provider Plugin | HIGH | HIGH | P1 |
| Ollama Provider Plugin | HIGH | MEDIUM | P1 |
| OpenRouter Provider Plugin | HIGH | MEDIUM | P1 |
| Google/Gemini Provider Plugin | HIGH | HIGH | P1 |
| Discord Channel Adapter | HIGH | HIGH | P1 |
| Slack Channel Adapter | HIGH | HIGH | P1 |
| Docker Image | HIGH | MEDIUM | P1 |
| Scoped API Keys | HIGH | MEDIUM | P1 |
| Webhook Management | MEDIUM | MEDIUM | P1 |
| Skill Registry (local) | HIGH | MEDIUM | P1 |
| Code Signing for Skills | HIGH | MEDIUM | P1 |
| OpenClaw Migration Tool | HIGH | HIGH | P1 |
| OpenResponses `/v1/responses` | MEDIUM | HIGH | P2 |
| Tools Invoke API | MEDIUM | MEDIUM | P2 |
| Batch Operations API | MEDIUM | MEDIUM | P2 |
| WhatsApp Adapter (Cloud API) | MEDIUM | HIGH | P2 |
| Matrix Channel Adapter | MEDIUM | HIGH | P2 |
| Cross-Channel Bridging | MEDIUM | HIGH | P2 |
| Node System | MEDIUM | HIGH | P2 |
| TTS/Transcription/Image Traits | LOW (traits only) | LOW | P2 |
| blufio bundle | MEDIUM | MEDIUM | P2 |
| blufio bench | LOW | LOW | P3 |
| blufio privacy evidence-report | MEDIUM | LOW | P3 |
| blufio config recipe | MEDIUM | LOW | P3 |
| blufio uninstall | LOW | LOW | P3 |
| Multi-instance systemd template | LOW | LOW | P3 |
| IRC Channel Adapter | LOW | LOW | P3 |
| Signal Channel Adapter | LOW | HIGH (risk-adjusted) | P3 |
| Skill Registry (remote) | HIGH (long-term) | HIGH | P3 |

**Priority key:**
- P1: Must have for v1.3 launch
- P2: Ship in v1.3 if time permits, else v1.3.x
- P3: v1.4+ or when explicitly demanded

---

## Competitor Feature Analysis

Primary competitor: OpenClaw (Node.js, 15+ channels, community skills, 6 structural weaknesses documented in PROJECT.md). Secondary reference: ZeroClaw (Rust rewrite, 28+ providers, 17k GitHub stars).

| Feature | OpenClaw | ZeroClaw | Blufio v1.3 Approach |
|---------|----------|----------|----------------------|
| LLM Providers | OpenAI, Anthropic, Gemini + 10+ via OpenAI-compat | 28+ providers (Anthropic, OpenAI, Gemini, Ollama, OpenRouter + 20 OpenAI-compat) | OpenAI, Ollama, OpenRouter, Gemini as plugins; any OpenAI-compat endpoint via `base_url` config |
| OpenAI API compatibility | Consumes it; does NOT expose it | Consumes it; does NOT expose it | **Exposes it** — Blufio becomes an OpenAI-compatible server, not just a client. Neither competitor does this. |
| Channel adapters | Telegram, Discord, WhatsApp, Slack, Matrix, IRC + 9 more (Node.js-based) | Config-driven TOML; 15+ channels | Plugin architecture; Discord + Slack in v1.3 launch; WhatsApp + Matrix in v1.3.x |
| WhatsApp | Unofficial (baileys) — known instability, account ban risk | Dual: Cloud API + Web mode (wa-rs, merged 2026-02-19) | Cloud API primary (official); Web mode experimental, clearly labeled |
| Migration tooling | None | `zeroclaw migrate openclaw` — reads TOML/JSONL directly | `blufio migrate --from openclaw` — maps config + history + skills, outputs migration report |
| Skill distribution | NPM registry (npm supply chain risk) | Local files only; no public registry | Ed25519-signed WASM; local registry v1.3; remote marketplace v1.4 |
| Memory model | In-memory (grows to 300-800MB/24h) | SQLite with LRU embedding cache | Bounded LRFU + ONNX + hybrid search (already shipped v1.0) |
| Event bus | None (direct function calls) | Not documented | Internal tokio pub/sub; required for bridging and webhooks |
| Docker | No official image | Not documented | Official Dockerfile; scratch-based (~5-10MB image) |
| Cost tracking | Token counting only | Not documented | Full cost ledger with budget caps and kill switches (already shipped) |
| API key scoping | None | None | Scoped API keys with TPM/RPM limits per key (v1.3) |
| Security | Binds 0.0.0.0; auth optional; plaintext credentials | TOML scoping; allowlists; pairing | TLS enforced; SQLCipher; Ed25519; scoped keys (v1.3) |
| Deployment | npm install; Node.js runtime required | Single Rust binary (~16MB) | Single static binary (~25-50MB with plugins); Docker option added in v1.3 |
| Node/multi-device | None | None | Node system: paired secondary devices via Ed25519 mutual auth (v1.3) |
| Benchmark tooling | None | None | `blufio bench` — self-reported performance validation |
| Privacy audit | None | None | `blufio privacy evidence-report` — enumerates all outbound data flows |

**Structural advantages Blufio maintains that neither competitor has:**
1. Exposes an OpenAI-compatible server API (neither OpenClaw nor ZeroClaw do this — they only consume)
2. WASM skill sandbox with capability manifests (OpenClaw skills run as full Node.js processes; ZeroClaw not documented)
3. Three-zone context engine (68-84% token reduction vs. inject-everything)
4. SQLCipher encryption at rest (neither competitor encrypts the database)
5. Ed25519 code signing for skills and agents (zero trust by default)
6. Bounded everything — caches (LRU), channels (backpressure), locks (timeouts) — no memory leaks by design

---

## Feature-Specific Implementation Notes

### OpenAI-Compatible API Layer

Table stakes request fields for `/v1/chat/completions`:
- `model`, `messages[]` (role + content), `stream`, `tools[]`, `tool_choice`, `temperature`, `max_tokens`, `response_format`, `stop`

Table stakes response fields:
- `id`, `object`, `created`, `model`, `choices[]` (index + message + finish_reason), `usage` (prompt_tokens + completion_tokens + total_tokens)

Streaming format: SSE `data: {"choices":[{"delta":{...},"finish_reason":null}]}` chunks, terminated with `data: [DONE]`

Error format: `{"error":{"message":"...","type":"...","code":...}}`

The endpoint is a **translation layer** — incoming OpenAI-format requests are mapped to the active provider's native format, executed, and responses mapped back. The existing `Provider` adapter trait abstracts this. New work: axum HTTP endpoint + request/response translation + SSE streaming.

### Multi-Provider Implementation Pattern

OpenAI, OpenRouter, and most OpenAI-compatible providers share a wire format — one HTTP client module can serve all three with different base URLs and auth headers. Gemini requires a separate implementation (different request schema, tool format, streaming format). Ollama requires native API path (not OpenAI-compat).

All four providers implement the existing `Provider` adapter trait. No new adapter traits needed for text completion providers.

Cargo: `openrouter_api` crate (async Rust, streaming, typed config) or direct `reqwest` implementation against the OpenAI-compat spec. `rust-genai` crate provides normalized multi-provider client that handles Anthropic + Gemini + OpenAI natively.

### Event Bus Design

- Typed enum of all events: `AgentEvent`, `ChannelEvent`, `SkillEvent`, `NodeEvent`, `WebhookEvent`, `BatchEvent`
- `tokio::sync::broadcast` for fan-out (multiple adapters watching same event type); capacity-bounded
- `tokio::sync::mpsc` for directed events (webhook delivery queue); capacity-bounded
- Lives in `blufio-core` (shared traits crate) or new `blufio-events` crate
- No persistence: events are ephemeral; durability belongs in caller (SQLite queue already exists for messages)
- Backpressure: bounded channels + `try_send` with drop on overflow (log the drop; never block)

### Skill Registry Design

- Local registry: TOML index file listing name, version, SHA-256 content hash, Ed25519 signature
- Install flow: download WASM + manifest + .sig → verify signature → verify capability manifest → copy to skills dir → update index
- Ed25519 signing uses same key infrastructure as agent signing (ed25519-dalek already in workspace)
- Remote registry in v1.3.x: static HTTPS file server hosting `index.toml` — do not build a registry server

### OpenClaw Migration Tool

Expected mapping:
- OpenClaw `config.toml` provider/model → Blufio `[provider]` section
- OpenClaw `IDENTITY.md` / `SOUL.md` → Blufio `[context.static]` system prompt
- OpenClaw conversation JSONL → Blufio SQLite session history (batch insert via storage API)
- OpenClaw channel config (Discord, Slack) → Blufio channel plugin config
- OpenClaw npm skills → flagged in migration report (cannot auto-convert npm to WASM; user must rewrite or find equivalent WASM skill)
- Output: `migration-report.md` with migrated items, manual steps needed, cost comparison estimate (token waste reduction)

### Channel Adapter Implementation Notes

**Discord:** serenity crate (high-level, handles sharding + rate limiting automatically). Gateway WebSocket is required — REST alone cannot receive messages. Slash commands require application command registration. Message chunking for 2000-char limit is required. Rate limiting is automatic in serenity.

**Slack:** slack-morphism-rust crate supports Events API + Socket Mode + Block Kit. Must use new Slack platform (legacy RTM API removed March 2025). OAuth workspace install required for app distribution. Socket Mode avoids needing a public URL in development.

**WhatsApp Cloud API:** whatsapp-business-rs crate. Meta Business Account + webhook endpoint for incoming messages. Webhook verification (HMAC-SHA256 challenge response) required. Straightforward once account setup is done.

**Matrix:** matrix-sdk crate (official, Element/Matrix.org maintained, built on ruma). Start without E2E encryption (unencrypted room join + message send/receive); add matrix-sdk-crypto in follow-up. Long-term strategic because Matrix bridges to Discord/Slack/IRC/WhatsApp natively.

**Signal:** presage crate (unofficial, secondary device via QR). High risk: Signal has broken presage without notice. signal-cli-rest-api as REST sidecar is a more stable alternative (Java-based but stable). Evaluate sidecar approach before native presage. Label whatever ships as "experimental" and warn users of account ban risk.

**IRC:** irc crate (async, tokio). Simplest possible adapter: connect → join channel → PRIVMSG handling. TLS + NickServ auth. Good regression test for the adapter trait contract.

### Docker Image Notes

Build pattern:
1. Builder stage: `FROM rust:latest AS builder` — compile with `--target x86_64-unknown-linux-musl` (same as CI)
2. Runtime stage: `FROM scratch` — copy binary only
3. Result: ~5-10MB compressed image (binary is ~25-50MB, but scratch layers compress well)
4. Volumes: `/data` for SQLite, `/config` for TOML, `/plugins` for WASM skills
5. Env vars: `BLUFIO_DB_KEY`, `BLUFIO_VAULT_KEY`, provider API keys via env
6. docker-compose.yml: service + named volumes + `restart: unless-stopped` + health check

SQLCipher note: musl static build with vendored OpenSSL already works from v1.2. FROM scratch needs no shared libs. Validated in CI already.

---

## Sources

- [OpenAI Chat Completions API Reference](https://platform.openai.com/docs/api-reference/chat) — request/response schema, streaming format, tool calling spec
- [OpenAI Responses API Reference](https://platform.openai.com/docs/api-reference/responses) — stateful agentic loop API
- [Open Responses Specification (Hugging Face Blog)](https://huggingface.co/blog/open-responses) — multi-provider reasoning API standard; reasoning items spec
- [Why We Built the Responses API (OpenAI Blog)](https://developers.openai.com/blog/responses-api/) — rationale and design
- [OpenRouter API Documentation](https://openrouter.ai/docs/api/reference/overview) — provider routing, fallback, streaming
- [OpenRouter Provider Selection](https://openrouter.ai/docs/guides/routing/provider-selection) — `providers.order` fallback field spec
- [Ollama Blog: Streaming + Tool Calling](https://ollama.com/blog/streaming-tool) — streaming tool call support
- [Ollama Issue #12557](https://github.com/ollama/ollama/issues/12557) — known `/v1` path tool calling inconsistency; use native `/api/chat`
- [openrouter_api Rust crate (lib.rs)](https://lib.rs/crates/openrouter_api) — async Rust client for OpenRouter with streaming, typed config, retry
- [rust-genai multi-provider crate (GitHub)](https://github.com/jeremychone/rust-genai) — reference for Gemini + Anthropic native protocol implementations; reqwest 0.13
- [gmini Gemini Rust SDK (lib.rs)](https://lib.rs/crates/gmini) — reqwest-based Gemini API client with streaming
- [serenity Discord library (GitHub)](https://github.com/serenity-rs/serenity) — high-level Discord bot framework for Rust; auto-sharding + rate limiting
- [twilight Discord library](https://twilight.rs/) — low-level Discord API for Rust (alternative to serenity)
- [slack-morphism-rust (GitHub)](https://github.com/abdolence/slack-morphism-rust) — Slack Events API + Socket Mode + Block Kit for Rust
- [Slack Events API Documentation](https://docs.slack.dev/apis/events-api/) — Events API spec; Socket Mode; OAuth
- [matrix-sdk (crates.io)](https://crates.io/crates/matrix-sdk) — official Matrix Rust SDK; ruma-based; E2E via matrix-sdk-crypto
- [ruma Matrix Rust crates (GitHub)](https://github.com/ruma/ruma) — low-level Matrix types and events
- [presage Signal library (GitHub)](https://github.com/whisperfish/presage) — unofficial Signal Rust client; secondary device linking
- [signal-cli (GitHub)](https://github.com/AsamK/signal-cli) — stable Signal CLI with JSON-RPC interface; REST sidecar alternative
- [irc crate (GitHub)](https://github.com/aatxe/irc) — async IRC for Rust; tokio-based; v0.14 async/await API
- [whatsapp-business-rs (Rust Forum)](https://users.rust-lang.org/t/showcase-whatsapp-business-rs-a-full-featured-whatsapp-business-sdk-in-rust/132064) — WhatsApp Cloud API Rust SDK; webhook handler
- [ZeroClaw built-in providers (DeepWiki)](https://deepwiki.com/zeroclaw-labs/zeroclaw/) — competitor provider/channel landscape; 28+ providers
- [ZeroClaw vs OpenClaw (zeroclaw.net)](https://zeroclaw.net/openclaw-vs-zeroclaw) — migration patterns reference
- [ZeroClaw Migration Assessment (GitHub Gist)](https://gist.github.com/yanji84/ebc72e9b02553786418c2c24829752c7) — detailed OpenClaw → ZeroClaw migration analysis
- [Docker Rust minimal image guide (ITNEXT)](https://itnext.io/a-practical-guide-to-containerize-your-rust-application-with-docker-77e8a391b4a8) — multi-stage + scratch base pattern; 4.6MB result
- [Designing event-driven systems in Rust (2026)](https://oneuptime.com/blog/post/2026-02-01-rust-event-driven-systems/view) — event bus patterns; backpressure
- [OpenAI Batch API Guide](https://developers.openai.com/api/docs/guides/batch/) — batch API design; separate rate limit pool; 24h turnaround SLA
- [OpenAI Rate Limits Reference](https://platform.openai.com/docs/api-reference/project-rate-limits) — scoped key + project-level rate limit design
- [Matterbridge (GitHub)](https://github.com/42wim/matterbridge) — cross-platform bridge reference; 20+ platform connectors
- [systemd multi-instance template (Opensource.com)](https://opensource.com/article/20/12/multiple-service-instances-systemctl) — @instance unit pattern; per-instance config
- [Multi-agent AI fleet on single VPS (DEV Community)](https://dev.to/oguzhanatalay/architecting-a-multi-agent-ai-fleet-on-a-single-vps-3h4c) — systemd template units for AI agent deployment patterns

---
*Feature research for: Blufio v1.3 Ecosystem Expansion*
*Researched: 2026-03-05*
