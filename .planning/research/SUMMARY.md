# Project Research Summary

**Project:** Blufio v1.3 Ecosystem Expansion
**Domain:** Rust AI Agent Platform — multi-provider LLM, multi-channel adapters, OpenAI-compatible API, skill marketplace, node system, Docker deployment
**Researched:** 2026-03-05
**Confidence:** HIGH (stack and architecture), MEDIUM (channel adapters, node system, OpenClaw migration)

## Executive Summary

Blufio v1.3 is an ecosystem expansion of a production-quality Rust AI agent platform (39K LOC, 21 crates, shipped through v1.2). The goal is to achieve feature parity with OpenClaw (Node.js, the primary competitor) while exploiting Blufio's structural advantages: WASM skill sandboxing, SQLCipher encryption, Ed25519 code signing, bounded resource management, and a single static binary deployment model. The recommended approach builds on the existing well-tested foundation without architectural disruption — all new features extend existing traits (`ProviderAdapter`, `ChannelAdapter`) and reuse existing infrastructure (tokio, axum, SQLite, reqwest, ed25519-dalek) wherever possible. Only 9 net-new Rust crates are required across the entire expansion, keeping the workspace within its 80-dependency constraint.

The single most important architectural decision in v1.3 is building an event bus (`blufio-bus` wrapping `tokio::sync::broadcast`) first, because four other features depend on it: webhook delivery, cross-channel bridging, node synchronization, and batch completion notification. The second critical decision is implementing a strict wire-type separation between internal Blufio types (Anthropic-influenced, using `stop_reason`) and the external OpenAI-compatible API surface (which requires `finish_reason`) — failure here silently breaks every OpenAI SDK client. The third is that the OpenAI-compatible API layer must be defined before any provider plugin is wired up, because providers must produce responses conforming to the external schema.

The primary risk areas are channel adapter fragility (Signal has no stable Rust library; WhatsApp unofficial libraries result in permanent account bans; Discord requires explicit privileged intents that are silent when missing) and concurrency bottlenecks introduced by exposing a concurrent API layer on top of SQLite's single-writer model. These risks are well-understood and have concrete mitigations. The competitive opportunity is clear: neither OpenClaw nor ZeroClaw exposes an OpenAI-compatible server API — only Blufio will do this, positioning it as a drop-in replacement for any OpenAI-consuming tool or frontend.

## Key Findings

### Recommended Stack

The v1.2 stack (tokio, axum 0.8, rusqlite 0.37 + SQLCipher, reqwest 0.13, ed25519-dalek, rmcp 0.17) is fully validated and unchanged. v1.3 adds exactly 9 new direct Rust crates, bringing the workspace from ~42 to ~51 direct dependencies. The single most critical version constraint: `matrix-sdk` must be pinned to `0.11.0` — the highest version compatible with `rust-version = "1.85"` in the workspace. Version 0.12+ bumps MSRV to 1.88, which breaks the entire workspace. This pin must be enforced with a comment in Cargo.toml.

**Core new technologies:**
- `tower-governor 0.8.0`: Per-API-key rate limiting via GCRA algorithm — the only Tower-compatible rate limiter with custom key extractors, required for scoped API key rate limits; wraps the governor crate as a Tower Layer compatible with axum 0.8
- `async-openai 0.33.0`: OpenAI type definitions and HTTP client for OpenAI, Ollama (base URL override), and OpenRouter providers — one crate covers three providers; also covers TTS and Whisper transcription types; released 2026-02-18
- `genai 0.5.3`: Google Gemini provider — the only production-stable Rust crate with native Gemini support; Gemini is not OpenAI-compatible so a separate crate is unavoidable; released 2026-01-31
- `serenity 0.12.5`: Discord channel adapter — de facto standard Rust Discord library with automatic sharding and rate limiting; MSRV 1.74, compatible with workspace; released 2025-12-20
- `slack-morphism 2.18.0`: Slack adapter with Socket Mode support — Slack removed the legacy RTM API in March 2025; this is the only viable maintained async Rust Slack library; released 2026-02-21
- `matrix-sdk 0.11.0`: Matrix adapter — CRITICAL: must be pinned to 0.11.x; MSRV exactly 1.85; 0.12+ requires MSRV 1.88 and breaks the workspace
- `irc 1.1.0`: IRC adapter — RFC-compliant, tokio-async, lowest-complexity channel to implement; released 2025-03-24
- `serde_json 1` + `walkdir 2`: OpenClaw migration tooling only — minimal, well-established crates with no MSRV concerns

**What requires no new crates:** Event bus (tokio::sync::broadcast is already in workspace), WhatsApp Cloud API (reqwest direct implementation), Signal bridge (reqwest to signal-cli sidecar), skill registry/signing (minisign-verify + reqwest + flate2 + tar all in workspace), node system (ed25519-dalek + reqwest already in workspace), Docker (build tooling only — cargo-chef in Dockerfile).

See `.planning/research/STACK.md` for full dependency analysis, version compatibility matrix, workspace Cargo.toml additions, and anti-recommendations.

### Expected Features

**Must have (v1.3 launch — table stakes):**
- Event bus (internal pub/sub) — unblocks four other features; must be implemented first as infrastructure
- OpenAI-compatible `/v1/chat/completions` — de facto LLM API standard; every tool and SDK expects it; enables drop-in OpenAI replacement
- OpenAI, Ollama, OpenRouter, and Google/Gemini provider plugins — required for "full provider coverage" claim; each implements existing `ProviderAdapter` trait
- Discord and Slack channel adapters — largest developer community and enterprise standard respectively
- Scoped API keys with per-key rate limiting — required before any multi-user or multi-service deployment scenario
- Webhook management — async event delivery for channel integrations and batch job completion
- Docker image (Dockerfile + docker-compose) — hard deployment barrier without it; cloud channel webhook testing requires it
- Skill registry (local) + Ed25519 code signing — security prerequisite before any public skill sharing; signing without registry has no enforcement mechanism
- OpenClaw migration tool — the primary user acquisition trigger; users will not switch without a migration path

**Should have (v1.3.x after validation):**
- OpenResponses `/v1/responses` API — stateful agentic loop; add after `/v1/chat/completions` stabilizes
- Tools Invoke API (`/v1/tools/invoke`) — direct skill execution without agent loop; depends on Responses API shape
- Batch operations API — 50% cost reduction on async workloads; add when bulk demand is confirmed
- WhatsApp Cloud API adapter — official Meta Cloud API; requires Meta Business account; add when operator demand justifies the documentation investment
- Matrix channel adapter — high complexity (E2EE crypto) but high strategic value (bridges to 20+ other platforms natively)
- Cross-channel bridging — requires event bus and multiple stable adapters; implement after Discord and Slack ship
- Node system (paired devices) — highest dependency count of any v1.3 feature; implement when event bus and scoped keys are stable

**Defer to v1.4+:**
- IRC and Signal channel adapters — IRC has low business impact; Signal has no stable Rust library and no official bot API
- Skill marketplace (remote registry) — requires community infrastructure and moderation; local registry must stabilize first
- TTS/Transcription/Image provider implementations — define trait interfaces in v1.3.x; implementations in v1.4
- WhatsApp Web mode (unofficial) — permanent account ban risk; experimental at best, never as primary path

**Anti-features to explicitly reject:**
- Automatic plugin updates on startup (bypasses code signing; silent supply chain attack vector)
- Native plugin system via libloading (eliminates memory safety boundary; one bad plugin crashes the process)
- External message broker like Redis or NATS (contradicts single-binary deployment model; not needed at current scale)
- OpenAI Assistants API (deprecated by OpenAI; implement Responses API only)
- GUI or visual config builder (explicitly out of scope in PROJECT.md)
- WhatsApp unofficial protocol libraries (account ban within days to weeks; no recovery path)

See `.planning/research/FEATURES.md` for full feature tables, dependency graph, prioritization matrix, and competitor analysis.

### Architecture Approach

All v1.3 features integrate into the existing layered architecture without structural disruption. The `ChannelAdapter` trait already supports N channels via the existing `ChannelMultiplexer` — new channel crates require zero changes to the multiplexer. The `ProviderAdapter` trait already supports streaming — new provider crates implement the existing interface. The gateway (`blufio-gateway`) gains a new route group as a separate module with strict wire-type isolation from internal Blufio types. A new `blufio-bus` crate wraps `tokio::sync::broadcast` and becomes a cross-cutting dependency injected via `Arc<EventBus>` into the agent loop, gateway, and channel adapters.

**Major new components and their responsibilities:**
1. `blufio-bus` — typed pub/sub event backbone; capacity-bounded broadcast (1024+); use mpsc per critical subscriber (webhook delivery, audit log) and broadcast for fire-and-forget subscribers (metrics, debug logging)
2. `blufio-gateway` (modified) — adds OpenAI-compatible API surface (`/v1/chat/completions`, `/v1/responses`, `/v1/tools/invoke`, `/v1/webhooks`, `/v1/batch`) as a separate `openai/` module with explicit `OpenAiChatResponse` wire types never exposing internal `ProviderResponse`; also adds API key middleware and rate limiting
3. `blufio-openai`, `blufio-ollama`, `blufio-openrouter`, `blufio-gemini` — four new provider crates each implementing `ProviderAdapter`; a provider-agnostic `ToolDefinition` type must be added to `blufio-core` before any of these are implemented
4. `blufio-discord`, `blufio-slack`, `blufio-whatsapp`, `blufio-irc`, `blufio-matrix` — five new channel crates each implementing `ChannelAdapter`; all follow the same structure as `blufio-telegram` (lib.rs, client.rs, types.rs)
5. `blufio-registry` — skill registry with local JSON index, Ed25519 signature verification at install time AND at every WASM host function invocation (not just at install), CLI subcommands
6. `blufio-node` — paired device mesh over HTTPS + Ed25519 mutual auth; static peer list in TOML config; no P2P complexity (libp2p explicitly rejected); pairing tokens must have 15-minute expiry and single-use enforcement from day one
7. Migration subcommand in binary crate — `blufio migrate openclaw`; reads OpenClaw JSONL sessions, flat memory files (MEMORY.md, LESSONS.md, CONTEXT.md, workspace/YYYY-MM-DD.md), and config; maps to Blufio storage API; outputs migration-report.md with dry-run mode

See `.planning/research/ARCHITECTURE.md` for full integration analysis, module structure for each new crate, concrete Rust code patterns, and anti-patterns.

### Critical Pitfalls

The full pitfall inventory contains 14 critical items. The highest-priority five:

1. **OpenAI `finish_reason` vs internal `stop_reason` field name mismatch** — The existing `ProviderResponse` uses Anthropic's `stop_reason`; the OpenAI wire format requires `finish_reason`, and maps `"tool_use"` to `"tool_calls"` and `"end_turn"` to `"stop"`. Leaking internal field names silently breaks every OpenAI SDK client (LangChain, LlamaIndex, OpenWebUI all loop forever or drop tool calls). Prevention: define completely separate `OpenAiChatResponse` wire types in `blufio-gateway/src/openai/types.rs` with `#[serde(rename = "finish_reason")]`; write an integration test asserting `finish_reason` appears in both streaming chunks and final responses.

2. **Ollama streaming + tool calls silently drops tool invocations** — Ollama's OpenAI compatibility shim (`/v1/chat/completions`) does not correctly forward tool call data when streaming is enabled (confirmed GitHub issue #12557, still open). Blufio streams by default. Prevention: use Ollama's native API (`/api/chat`) for `blufio-ollama`, never the OpenAI shim; add an explicit tool-call + `stream: true` integration test before the Ollama adapter is declared complete.

3. **Provider tool format leaks Anthropic-specific structure to all providers** — The existing `ProviderRequest.tools` is `Option<Vec<serde_json::Value>>` with Anthropic format assumed. OpenAI requires `{type: "function", function: {...}}` wrapping; Gemini requires `{functionDeclarations: [...]}` format. Prevention: define a provider-agnostic `ToolDefinition` type in `blufio-core` before implementing any non-Anthropic provider; each provider adapter serializes to its own wire format independently.

4. **Event bus broadcast receiver lag causes silent message loss** — `tokio::sync::broadcast` drops oldest messages when a slow receiver lags; `RecvError::Lagged` is not visible by default. Webhook dispatchers that miss events have no failure signal. Prevention: log `RecvError::Lagged(n)` as `tracing::warn!` with receiver name and emit a Prometheus counter; use `mpsc` per critical subscriber (webhook delivery, audit log); use `broadcast` only for fire-and-forget subscribers (metrics, debug); set capacity to 1024+.

5. **WhatsApp unofficial library causes permanent phone number ban** — Meta detects non-standard clients within days to weeks and permanently bans the phone number with no appeal process. Prevention: use only the official Meta Cloud API (`graph.facebook.com`); never ship unofficial protocol crates even for local testing; document BSP approval requirement in config validation error messages.

See `.planning/research/PITFALLS.md` for all 14 pitfalls with prevention strategies, warning signs, recovery costs, and phase-to-pitfall mapping.

## Implications for Roadmap

Based on the dependency graph from FEATURES.md and the integration analysis from ARCHITECTURE.md, the following phase structure is recommended. Phase ordering follows the dependency chain: infrastructure before consumers, API surface before providers, local features before remote-dependent features, security features before distribution features.

### Phase 1: OpenAI-Compatible API Layer + Scoped API Keys

**Rationale:** The OpenAI `/v1/chat/completions` endpoint must be defined before any provider plugin is wired up — providers must produce responses conforming to this schema. Scoped API keys must be designed alongside the API surface (not retrofitted) because rate limiting, session scoping, and access control all interact. This phase also establishes the wire-type separation that prevents the `stop_reason` vs `finish_reason` class of bugs from propagating downstream to all providers.

**Delivers:** External callers can use Blufio as an OpenAI-compatible server; scoped API key management (create/list/revoke) with per-key TPM/RPM limits; webhook registration and delivery endpoints; rate limiting via tower-governor; explicit external session IDs distinct from internal SQLite primary keys

**Addresses features:** OpenAI-compatible `/v1/chat/completions`, scoped API keys with rate limiting, webhook management

**Avoids pitfalls:** `finish_reason` field mismatch (Pitfall 1), internal session ID exposure to external callers (Pitfall 14), scoped key session cross-access, SQLite write queue bottleneck under concurrent load (concurrent load testing must be in acceptance criteria)

**Stack additions:** `tower-governor 0.8.0`; `async-openai 0.33.0` (types reference only, not HTTP client)

**Research flag:** Standard patterns — axum route nesting is well-documented; OpenAI API spec is authoritative and public; no additional research needed during planning

### Phase 2: Event Bus

**Rationale:** Four features depend on the event bus: cross-channel bridging, webhook delivery queue, node sync, and batch completion. The event bus must immediately follow the API layer because webhook delivery (partially needed by Phase 1) requires it, and it must be designed with lag handling from day one — retrofitting broadcast-vs-mpsc selection is harder than getting it right initially.

**Delivers:** `blufio-bus` crate with typed `BlufioEvent` enum; `EventBus` wrapper over `tokio::sync::broadcast`; observable lag handling (`RecvError::Lagged` logged and metered); webhook delivery via dedicated mpsc subscriber; foundation for cross-channel bridging and batch completion notification

**Addresses features:** Event bus (internal pub/sub), webhook delivery queue

**Avoids pitfalls:** Broadcast receiver lag causing silent message loss (Pitfall 5); correct broadcast-vs-mpsc subscriber selection from day one

**Stack additions:** None — `tokio::sync::broadcast` is already in workspace

**Research flag:** Standard patterns — tokio broadcast is a core primitive with extensive documentation; no additional research needed

### Phase 3: Multi-Provider LLM Support

**Rationale:** Provider plugins depend on the OpenAI wire types defined in Phase 1. All four providers (OpenAI, Ollama, OpenRouter, Gemini) implement the existing `ProviderAdapter` trait. A provider-agnostic `ToolDefinition` type must be introduced in `blufio-core` before any non-Anthropic provider is implemented — this prevents Anthropic-format tool schema from leaking to all providers.

**Delivers:** OpenAI, Ollama, OpenRouter, and Gemini providers selectable as backends via `[providers.default]` config; `ToolDefinition` type in `blufio-core` with per-provider serialization; `TtsAdapter`, `TranscriptionAdapter`, and `ImageAdapter` trait interfaces defined in blufio-core (no implementations in this phase)

**Addresses features:** OpenAI provider, Ollama provider, OpenRouter provider, Gemini/Google provider, TTS/Transcription/Image provider trait definitions

**Avoids pitfalls:** Anthropic tool format leaking to all providers (Pitfall 3), Ollama streaming + tools silent drop (Pitfall 2 — native `/api/chat` required), Gemini native tool format differences (`{functionDeclarations: [...]}` format), reqwest Client as long-lived singleton per provider (not per-request)

**Stack additions:** `async-openai 0.33.0` (HTTP client for OpenAI/Ollama/OpenRouter), `genai 0.5.3` (Gemini native API)

**Research flag:** Gemini integration warrants brief research during planning — genai 0.5.3 has no official Google backing and is in active development (0.6.0-beta.3 exists); validate tool calling coverage before committing. OpenAI/Ollama/OpenRouter are standard patterns with async-openai.

### Phase 4: Docker Image

**Rationale:** Docker must be validated before channel adapters are added, while the binary is at its smallest and the TLS/SQLCipher musl build path can be cleanly verified. Docker is also a deployment prerequisite for cloud-based channel adapter testing — Discord and Slack webhook URLs require a publicly accessible endpoint.

**Delivers:** Multi-stage Dockerfile using cargo-chef for layer caching; distroless/static-debian12:nonroot runtime base (CA certificates included, non-root by default); docker-compose.yml with volume mounts for `/data`, `/config`, `/plugins` and environment variable injection; CI validation that TLS works inside the container via `docker run blufio blufio doctor`

**Addresses features:** Docker image + docker-compose

**Avoids pitfalls:** Docker TLS failure from missing CA bundle with FROM scratch (Pitfall 11 — use distroless instead); container running as root (security mistake); SQLCipher + musl static linking verified

**Stack additions:** None (cargo-chef in Dockerfile only, not a Rust dependency)

**Research flag:** Standard patterns — cargo-chef + distroless/static-debian12:nonroot is industry standard for Rust production containers; no additional research needed

### Phase 5: Multi-Channel Adapters (Discord + Slack)

**Rationale:** Discord and Slack are the two highest-value channels. Both require the event bus (Phase 2) for bridging and the Docker image (Phase 4) for webhook endpoint testing. WhatsApp Cloud API and Matrix are deferred to v1.3.x because they each have unique setup complexity (Meta Business account BSP approval; Matrix E2EE).

**Delivers:** Discord and Slack channel adapters; cross-channel bridging between any combination of Telegram, Discord, and Slack via BridgeRule config; observable metrics per channel

**Addresses features:** Discord channel adapter, Slack channel adapter, cross-channel bridging foundation

**Avoids pitfalls:** Discord `MESSAGE_CONTENT` privileged intent silent failure (Pitfall 7 — startup check must emit `tracing::error!` if guild messages arrive with empty content); Slack rate limit tier throttling (Pitfall 8 — Events API or Socket Mode for push delivery, not polling); WhatsApp unofficial library ban (Pitfall 6 — document official-only in config validation)

**Stack additions:** `serenity 0.12.5`, `slack-morphism 2.18.0`

**Research flag:** Standard patterns — serenity and slack-morphism are actively maintained with clear documentation; existing blufio-telegram provides structural reference. Discord Socket Mode vs Events API selection should be confirmed during planning.

### Phase 6: Skill Registry + Code Signing

**Rationale:** The local skill registry must precede code signing enforcement — signing without a registry has no enforcement point. Both must precede any public skill sharing. Capability enforcement (checking per-WASM-host-function-call, not just at install time) must be implemented in this phase because it cannot be retrofitted safely after skills are in production use.

**Delivers:** `blufio skill install/list/search/publish/verify` CLI; Ed25519 signature verification at install time AND at every WASM host function invocation; local registry JSON index with SHA-256 content hashes; `blufio bundle` for air-gapped deployment; key ID versioning in signatures (rotation support from day one)

**Addresses features:** Skill registry (local), code signing for skills, blufio bundle

**Avoids pitfalls:** Skill code signing not preventing capability escalation (Pitfall 10 — per-host-function capability check, not just at install); signing key without rotation mechanism (Technical Debt pattern — key ID versioning in signature format from day one)

**Stack additions:** None — reuses minisign-verify 0.2.5, reqwest 0.13, flate2, tar (all already in workspace from v1.2)

**Research flag:** Standard patterns — ed25519-dalek and minisign-verify are validated from v1.2; wasmtime capability enforcement integration should be reviewed against the existing blufio-skill host function dispatch before implementation begins

### Phase 7: Node System (Paired Devices)

**Rationale:** Node system has the highest dependency count of any v1.3 feature — it requires event bus (Phase 2) + scoped API keys (Phase 1) + Ed25519 (already exists). Implementing last among new infrastructure features ensures all dependencies are stable and lessons from earlier phases inform the protocol design. The design is deliberately minimal: static peer list + HTTPS + Ed25519 auth, not P2P.

**Delivers:** `blufio-node` crate; QR code or token-based node pairing; Ed25519 mutual authentication for all inter-node messages; session sharing and relay across paired instances; node health monitoring; pairing tokens with 15-minute expiry and single-use enforcement

**Addresses features:** Node system (paired devices), multi-instance coordination

**Avoids pitfalls:** Node pairing token with no expiry (Pitfall 12 — `expires_at` column required from day one); token reuse allowing permanent unauthorized access; libp2p complexity for a simple trusted cluster of 2-10 devices

**Stack additions:** None — reuses ed25519-dalek 2.1, reqwest 0.13 (both already in workspace)

**Research flag:** Node system has no precedent in the codebase. Pairing flow, relay protocol, sync conflict resolution, and NAT traversal edge cases all need explicit design decisions before implementation. This phase warrants a `/gsd:research-phase` during planning to validate the HTTPS + Ed25519 approach against real deployment scenarios.

### Phase 8: OpenClaw Migration Tool

**Rationale:** Migration tool is most valuable late in the milestone — the more providers and channels Blufio has, the better the migration coverage. Requires at minimum the OpenAI provider (most common OpenClaw config) and ideally Discord and Slack adapters for complete channel config migration.

**Delivers:** `blufio migrate openclaw [--source] [--dry-run]` subcommand; migrates config, conversation history (JSONL sessions), memory files (MEMORY.md, LESSONS.md, CONTEXT.md, workspace/YYYY-MM-DD.md daily files), and channel configs; outputs migration-report.md with migrated items, manual steps needed, and cost comparison estimate; npm skills flagged for manual conversion (cannot auto-convert to WASM)

**Addresses features:** OpenClaw migration tool

**Avoids pitfalls:** OpenClaw migration losing daily memory and long-term context files (Pitfall 13 — must explicitly enumerate and migrate MEMORY.md, LESSONS.md, CONTEXT.md, and all workspace/ files, not just sessions; dry-run output must include these files before any write operation)

**Stack additions:** `serde_json 1`, `walkdir 2`

**Research flag:** OpenClaw config JSON schema (exact field names, all flat file locations, memory file format) needs validation against a real openclaw.json instance before field mappings are finalized. Plan for a brief discovery step at the start of this phase.

### Phase Ordering Rationale

- **Event bus before channel adapters:** Bridging, webhook delivery, and batch completion all require pub/sub; building adapters before the bus forces a retrofit of lag handling and subscriber selection
- **OpenAI API surface before providers:** Providers must produce responses conforming to the external schema; defining the schema first prevents the entire class of `stop_reason` vs `finish_reason` field name bugs
- **Docker before channel adapters:** Discord and Slack webhook URLs require a publicly accessible endpoint; Docker is the standard solution; also validates the musl + TLS + SQLCipher build path while the binary is smallest
- **Local skill registry before code signing:** Signing without an enforcement point (the registry) is theater, not security; capability checks must be at host function call time, not just install time
- **Node system last among infrastructure:** Highest dependency count; benefits from event bus, scoped keys, and Ed25519 all being stable before its novel protocol design is implemented
- **Migration tool last:** Value scales with how many providers and channels are already implemented; more equivalents means better migration coverage

### Research Flags

Phases likely needing `/gsd:research-phase` during planning:
- **Phase 3 (Gemini provider):** genai 0.5.3 has no official Google backing; 0.6.0-beta.3 is in flight; Gemini API has its own authentication and streaming format that differs from all OpenAI-compatible providers. Validate tool calling coverage before committing.
- **Phase 7 (Node system):** No precedent in the codebase. Pairing flow, relay protocol, sync conflict resolution, and NAT traversal edge cases all need design validation before implementation begins.
- **Phase 8 (OpenClaw migration):** Exact OpenClaw config JSON schema and flat file locations need validation against real instances before field mappings are finalized.

Phases with well-documented patterns (skip research-phase):
- **Phase 1 (OpenAI API layer):** OpenAI API spec is authoritative and public; axum routing and tower-governor are well-documented
- **Phase 2 (Event bus):** tokio::sync::broadcast is a core primitive with extensive official documentation
- **Phase 4 (Docker):** cargo-chef + distroless is industry standard; musl static builds validated in v1.2
- **Phase 5 (Discord + Slack):** serenity and slack-morphism are actively maintained; blufio-telegram provides structural reference
- **Phase 6 (Skill registry):** Reuses v1.2 minisign-verify and ed25519-dalek patterns; wasmtime integration already established

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | 7 of 9 new crates verified on docs.rs with release dates and MSRV. matrix-sdk version constraint (pin to 0.11.0) is the single critical risk — mandatory, not optional. genai 0.5.3 rated MEDIUM individually (no official Google backing) but HIGH for the overall stack since it covers only Gemini. |
| Features | HIGH | OpenAI API contracts are authoritative public specs. Feature dependency graph is grounded in official API documentation. Competitor analysis (OpenClaw, ZeroClaw) documented from multiple sources. Anti-features are well-reasoned from known failure modes. |
| Architecture | HIGH | Based on direct codebase analysis of v1.2 (39K LOC, 21 crates). Integration points verified against existing trait boundaries. Provider and channel adapter patterns are well-established in the codebase. All 9 new crates' integration points are clear. |
| Pitfalls | HIGH for Rust/axum/SQLite specifics; MEDIUM for channel platform specifics | 14 pitfalls documented with specific prevention strategies, warning signs, recovery costs, and phase assignments. WhatsApp ban risk and Signal ecosystem fragility are the highest-impact MEDIUM-confidence items. Ollama tool-call drop is confirmed via public GitHub issue. |

**Overall confidence:** HIGH

### Gaps to Address

- **Gemini tool calling coverage in genai 0.5.3:** genai is in active development with 0.6.0-beta.3 in flight. Validate that genai 0.5.3 correctly handles Gemini function calling (not just text completion) before committing to it as the implementation path. Alternative: direct reqwest + serde implementation against the Gemini REST API spec.

- **OpenClaw exact JSON schema:** PITFALLS.md documents that migration must include MEMORY.md, LESSONS.md, and workspace/ daily files — but the exact JSON field names in openclaw.json need validation against a real instance before field mappings are implemented. Plan for a discovery step at the start of Phase 8.

- **Node protocol design details:** The node system is the only v1.3 feature with no existing precedent in the codebase. NAT traversal (when both nodes are behind NAT), connection failure handling, sync conflict resolution, and relay message format all need explicit design decisions before implementation begins. A research-phase during planning is warranted.

- **Signal ecosystem deferral:** Signal has no stable Rust library and no official bot API. Research confirms deferring Signal to v1.4+ or implementing as an explicit signal-cli bridge with a documented Java runtime dependency. This must be communicated clearly in v1.3 release notes to avoid operator expectations.

- **matrix-sdk MSRV lock:** matrix-sdk 0.11.0 is pinned due to the workspace `rust-version = "1.85"` constraint. If the workspace ever bumps to `rust-version = "1.88"`, matrix-sdk can be upgraded to the latest (0.16.0 as of 2025-12-04). Document the constraint in Cargo.toml with a comment explaining the MSRV lock and the upgrade path.

- **Cargo workspace feature unification:** Adding 12 new crates with potentially overlapping feature flags may trigger unexpected feature unification. Run `cargo build --features discord,slack` and `cargo build --features matrix` separately and check for unexpected feature activation before declaring any channel adapter phase complete.

- **WhatsApp v1.3.x sequencing:** WhatsApp Cloud API requires a Meta Business account, BSP verification, and a verified phone number — all operator prerequisites, not code. The implementation itself is straightforward (reqwest + serde, ~300 lines). Defer to v1.3.x to allow documentation of the operator setup path, but do not let the documentation burden block implementation.

## Sources

### Primary (HIGH confidence — verified on docs.rs or official documentation)
- [async-openai 0.33.0 on docs.rs](https://docs.rs/crate/async-openai/latest) — OpenAI API type definitions, TTS/transcription types; released 2026-02-18
- [serenity 0.12.5 on docs.rs](https://docs.rs/crate/serenity/latest) — Discord gateway + REST; MSRV 1.74; released 2025-12-20
- [slack-morphism 2.18.0 on docs.rs](https://docs.rs/crate/slack-morphism/latest) — Slack Events API + Socket Mode; released 2026-02-21
- [matrix-sdk CHANGELOG](https://github.com/matrix-org/matrix-rust-sdk/blob/main/crates/matrix-sdk/CHANGELOG.md) — MSRV 1.85 for 0.11.0; MSRV 1.88 from 0.12+; critical version constraint verified
- [tower-governor 0.8.0](https://docs.rs/tower-governor/latest/tower_governor/) — GCRA rate limiting with custom key extractors; tower 0.5 compatible
- [OpenAI Chat Completions API Reference](https://platform.openai.com/docs/api-reference/chat) — `finish_reason`, streaming format, tool calling spec
- [Ollama GitHub issue #12557](https://github.com/ollama/ollama/issues/12557) — confirmed streaming + tool call silent drop in OpenAI shim; native `/api/chat` required
- [Slack rate limit changes (May 2025)](https://docs.slack.dev/changelog/2025/05/29/rate-limit-changes-for-non-marketplace-apps/) — 1 req/min for `conversations.history` for non-Marketplace apps
- [tokio broadcast channel docs](https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html) — lag behavior and `RecvError::Lagged` documented
- [Blufio codebase v1.2 — direct analysis](https://github.com/mondalsuman/blufio) — `ProviderRequest.tools` Anthropic-format field comment, `GatewayState.response_map` DashMap pattern, existing trait boundaries

### Secondary (MEDIUM confidence — community sources, multiple sources agree)
- [genai 0.5.3 on docs.rs](https://docs.rs/crate/genai/latest) — Google Gemini provider; released 2026-01-31; no official Google backing
- [WhatsApp Business API unofficial vs official risk](https://wisemelon.ai/blog/whatsapp-business-api-vs-unofficial-whatsapp-tools) — account ban risk documented
- [ZeroClaw competitor analysis (DeepWiki)](https://deepwiki.com/zeroclaw-labs/zeroclaw/) — 28+ providers; competitor feature landscape
- [ZeroClaw migration guide](https://gist.github.com/yanji84/ebc72e9b02553786418c2c24829752c7) — OpenClaw to ZeroClaw migration patterns; informs OpenClaw data format understanding
- [cargo-chef GitHub](https://github.com/LukeMathWalker/cargo-chef) — three-stage Docker pattern; industry standard for Rust
- [Discord MESSAGE_CONTENT intent (Discord developer docs)](https://docs.discord.com/developers/topics/gateway#privileged-intents) — privileged intent requirement confirmed

### Tertiary (LOW confidence — inference or single source, needs validation)
- [OpenClaw migration guide](https://docs.openclaw.ai/install/migrating) — state directory structure; exact field mappings need validation against real openclaw.json instances
- [presage Signal library](https://github.com/whisperfish/presage) — unofficial; maintenance status uncertain; Signal may break without notice

---
*Research completed: 2026-03-05*
*Ready for roadmap: yes*
