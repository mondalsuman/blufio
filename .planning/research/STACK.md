# Technology Stack: v1.3 Ecosystem Expansion

**Project:** Blufio
**Researched:** 2026-03-05
**Scope:** NEW crate additions for OpenAI-compatible API layer, multi-provider LLM support, multi-channel adapters, Docker, event bus, skill registry/marketplace, node system, and migration tooling.

> Existing stack validated through v1.2 (tokio, axum, rusqlite 0.37 with SQLCipher, reqwest 0.13, ed25519-dalek, rmcp 0.17, etc.) is UNCHANGED. This document covers ONLY what v1.3 adds.

---

## 1. OpenAI-Compatible API Endpoints

The `/v1/chat/completions`, `/v1/responses`, and `/v1/tools/invoke` endpoints are pure axum handlers. No new framework is needed — axum 0.8 already handles SSE streaming, typed headers, and JSON bodies.

### New Libraries for OpenAI API Surface

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| tower-governor | 0.8.0 | Rate limiting middleware for scoped API keys | Wraps governor (GCRA algorithm) as a Tower Layer. Integrates natively with axum 0.8's tower-compatible router. Supports custom key extractors (API key header, not just IP). GCRA is better than token-bucket for API rate limiting. |
| async-openai | 0.33.0 | Type definitions for OpenAI API schema (ChatCompletionRequest, ChatCompletionResponse, etc.) | The crate's types model the OpenAI OpenAPI spec exactly. Import the types only — use as schema reference and response serialization. Do NOT use its HTTP client for inbound requests. Version 0.33.0 released 2026-02-18. |

**Why tower-governor instead of tower::limit::RateLimit:** Tower's built-in RateLimiter is global, not per-key. tower-governor supports custom key extractors needed for scoped API key rate limiting.

**OpenAI API SSE streaming:** axum's built-in `axum::response::sse::Sse<S>` + `tokio::sync::broadcast` for internal fan-out. No extra crates needed. Already established pattern in codebase.

**Webhook management:** reqwest 0.13 (existing) for outbound delivery. Store webhook endpoints in SQLite. No additional crates.

**Batch operations:** tokio::task::JoinSet (existing, in tokio) for bounded parallel execution of batch requests.

**Confidence:** HIGH — axum 0.8 SSE is documented. tower-governor 0.8.0 verified on docs.rs. async-openai 0.33.0 version verified on docs.rs (released 2026-02-18).

---

## 2. Multi-Provider LLM Support

The existing `Provider` adapter trait abstracts LLM calls. New providers implement this trait as plugin crates.

### Provider Crates

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| async-openai | 0.33.0 | OpenAI provider client (also covers OpenRouter, Ollama via base URL override) | Single crate covers three providers. OpenAI-compatible endpoints (Ollama, OpenRouter) work by setting `OPENAI_BASE_URL`. async-openai supports `base_url` configuration. Version 0.33.0 released 2026-02-18 — verified. |
| genai | 0.5.3 | Google Gemini provider (and fallback multi-provider utility) | The only production-stable Rust crate with native Gemini support. Also covers Anthropic, OpenAI, Ollama, Groq, DeepSeek, etc. Version 0.5.3 released 2026-01-31. Actively maintained, semver-respecting. |

**Architecture decision — why NOT genai for everything:**

genai is a unified client. But blufio already has its own Provider trait with a built codebase. Using genai for Anthropic would displace the existing `blufio-anthropic` crate (v1.0-v1.2 investment). Use:
- `blufio-anthropic` — keep existing, no change
- `blufio-openai` — new crate, uses `async-openai` with configurable base URL
- `blufio-ollama` — new crate, uses `async-openai` with `http://localhost:11434/v1` base URL (Ollama speaks OpenAI API natively)
- `blufio-openrouter` — new crate, uses `async-openai` with `https://openrouter.ai/api/v1` base URL
- `blufio-gemini` — new crate, uses `genai` crate for Google's native Gemini API (not OpenAI-compatible)

**Why async-openai 0.33 for OpenAI/Ollama/OpenRouter:**

Ollama's `/api/chat` implements the OpenAI chat completions spec. OpenRouter is an OpenAI-compatible proxy. Using `async-openai` with a custom `OPENAI_BASE_URL` or `OpenAIConfig::with_api_base()` covers all three providers with one crate and no extra dependencies.

**Why genai 0.5.3 for Gemini:**

Google's Gemini API is NOT OpenAI-compatible (different request/response format, streaming protocol, auth mechanism). The only mature Rust options are community crates. genai 0.5.3 is the most complete, actively maintained, multi-provider crate. No official Google Rust SDK exists. genai has 0.6.0-beta.3 in flight — stick with 0.5.3 for stability.

**Confidence:** HIGH for async-openai 0.33 (verified docs.rs). MEDIUM for genai 0.5.3 (verified docs.rs, no official backing, active development).

---

## 3. TTS / Transcription / Image Provider Traits

These are new adapter traits on the existing plugin system — the `Provider` trait gains sibling traits: `TtsProvider`, `TranscriptionProvider`, `ImageProvider`.

### TTS/Transcription Crates

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| async-openai | 0.33.0 (already added above) | OpenAI TTS (tts-1, gpt-4o-mini-tts) and Whisper transcription HTTP API client | async-openai models `/v1/audio/speech` (TTS) and `/v1/audio/transcriptions` (Whisper) in its type system. Same crate, no new dependency. |

**No local TTS/STT by default:** Local whisper bindings (whisper-rs, whisper-cpp-plus) require building C/C++ code, adding significant compile time and binary size. They are appropriate as optional plugin crates that operators install separately. The default `TtsProvider` and `TranscriptionProvider` implementations target the OpenAI-compatible HTTP API.

**Why NOT whisper-rs in core:** whisper-rs 0.15 requires building whisper.cpp (C++, GGML), adding 5-15MB to binary and 3-5 minutes to builds. Violates the <50MB binary constraint. Add as a separate optional crate (`blufio-whisper`) in a future milestone if users request it.

**Image generation:** Same pattern. `async-openai` covers DALL-E via `/v1/images/generations`. No new crate.

**Confidence:** HIGH — async-openai TTS/transcription types verified in documentation.

---

## 4. Channel Adapters

Each adapter is a new `blufio-{name}` crate implementing the existing `Channel` trait. The trait already defines `send_message()`, `recv_message()`, and lifecycle hooks.

### Discord

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| serenity | 0.12.5 | Discord API client (Gateway + REST) | The de facto Rust Discord library. 0.12.5 released 2025-12-20. MSRV 1.74 — compatible with project's Rust 1.85. Supports tokio 1.0, async/await, Gateway websocket events, REST API. Used by thousands of Discord bots. |

**Why serenity over twilight:** twilight is more modular but lower-level. serenity provides a complete bot experience with a higher-level event system. For a channel adapter that needs to receive messages and send replies, serenity's EventHandler trait is a clean fit for the Channel trait.

**Confidence:** HIGH — serenity 0.12.5 verified on docs.rs, MSRV 1.74 confirmed.

---

### WhatsApp

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| reqwest | 0.13 (existing) | Meta WhatsApp Cloud API HTTP client | WhatsApp Business Cloud API is a standard REST+webhook API. No Rust-specific library needed. Use reqwest with serde for type-safe request/response. |

**Architecture:** Meta's WhatsApp Cloud API works via:
1. Inbound: Meta POSTs webhook events to our axum server (existing HTTP gateway handles this)
2. Outbound: We POST to `https://graph.facebook.com/v20.0/{phone_number_id}/messages`

**Why NOT whatsapp-cloud-api crate (0.5.4):** Last updated ~10 months ago (May 2025), targets Facebook Graph API v20.0 specifically, small ecosystem. Meta Cloud API is simple enough to implement directly with reqwest + serde in ~300 lines. Direct implementation avoids a dependency on an unmaintained community crate.

**Why NOT whatsapp-rust:** Uses unofficial WhatsApp Web protocol (reverse-engineered). Violates Meta ToS. Risk of account suspension. NOT suitable for a production platform.

**Setup requirement:** Operator needs a Meta Developer account, WhatsApp Business API app, verified phone number. This is a platform requirement, not a code issue. Document it clearly.

**Confidence:** HIGH for reqwest-based implementation. MEDIUM for Meta API stability (API is versioned and stable, but requires business verification).

---

### Slack

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| slack-morphism | 2.18.0 | Slack Web API + Events API + Socket Mode client | The most complete async Rust Slack library. Supports Events API (webhooks), Socket Mode (WebSocket-based, no public URL needed), Block Kit models, and OAuth. Version 2.18.0 released 2026-02-21. Actively maintained. |

**Socket Mode vs Events API:** Socket Mode lets the bot connect via WebSocket without exposing a public webhook URL. This is better for development and small deployments. The blufio HTTP gateway already handles webhook delivery, so Events API (webhook mode) is also viable. slack-morphism supports both.

**Why NOT slack-rs-api:** Last major update 2022. Does not support Socket Mode. slack-morphism is significantly more complete.

**Confidence:** HIGH — slack-morphism 2.18.0 verified on docs.rs (released 2026-02-21).

---

### Signal

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| reqwest | 0.13 (existing) | HTTP client to bridge with signal-cli JSON-RPC daemon | Signal has no public REST API. The only viable approach for Rust bots is to run signal-cli (Java daemon) and communicate with it via JSON-RPC. No Rust-native Signal crate is production-safe. |

**Architecture:** signal-cli runs as a sidecar daemon. Blufio communicates with it via:
- HTTP JSON-RPC (signal-cli `--output=json --http` mode) for message retrieval
- signal-cli REST API bridge (signal-cli-api crate, or direct JSON-RPC calls)

**Why NOT libsignal-service-rs (whisperfish):** Immature, no active maintenance for production use, implements the Signal protocol from scratch (complex, security-critical). Using signal-cli as a sidecar is the standard approach used by all open-source Signal bots.

**Why NOT direct Signal protocol:** Signal's protocol is reverse-engineered, not officially documented for third-party use. Implementing it directly creates maintenance burden and ToS risk.

**Operator requirement:** signal-cli must be installed separately. Document clearly. This is a soft dependency (not a Rust crate).

**Confidence:** MEDIUM — reqwest-based bridge to signal-cli is the established pattern. Signal ecosystem is fragile; signal-cli is Java (not a Rust concern) but is the only reliable option.

---

### IRC

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| irc | 1.1.0 | Async IRC client (RFC 2812, IRCv3.1/3.2) | The standard async Rust IRC library. Version 1.1.0 released 2025-03-24. 100% documentation coverage. Supports tokio async/await, TLS, rate limiting, SASL auth, multiple configuration formats. Thread-safe. |

**Confidence:** HIGH — irc 1.1.0 verified on docs.rs with release date 2025-03-24.

---

### Matrix

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| matrix-sdk | 0.11.0 | Matrix client (E2EE, sync, room management) | Official Matrix Rust SDK from matrix-org. Version 0.11.0 released 2025-04-11. MSRV 1.85 — exactly matches project's Rust 1.85. Version 0.12+ bumps MSRV to 1.88 (incompatible). |

**CRITICAL VERSION CONSTRAINT:** matrix-sdk 0.11.0 is the highest version compatible with the project's `rust-version = "1.85"` workspace setting. Version 0.12.0 (released 2025-06-10) bumped MSRV to 1.88. Do NOT use 0.12+. Pin to `matrix-sdk = "0.11"` and add a comment explaining the constraint.

**If MSRV is bumped to 1.88 later:** Can upgrade to matrix-sdk 0.16.0 (latest as of 2025-12-04). But that is a separate decision with workspace-wide implications.

**E2EE note:** matrix-sdk includes E2EE (matrix-sdk-crypto) via the `e2e-encryption` feature. Enable it — Matrix without E2EE is not suitable for a privacy-focused platform. This adds a build dependency on vodozemac (E2EE crypto library, pure Rust).

**Confidence:** HIGH — matrix-sdk 0.11.0 MSRV 1.85 confirmed from search results and changelog. Version pinning is mandatory.

---

## 5. Event Bus (Internal Pub/Sub)

**No new crate needed.** Use `tokio::sync::broadcast` channel directly.

**Rationale:**

The existing codebase already uses tokio channels extensively. The "event bus" pattern for an internal pub/sub system is a thin wrapper around `tokio::sync::broadcast::channel<BusEvent>`:
- Multi-producer, multi-consumer
- Backpressure via bounded capacity
- Clone the sender anywhere; subscribe by calling `.subscribe()` on the sender
- Events: `SessionStarted`, `SessionEnded`, `MessageReceived`, `SkillExecuted`, `CostThresholdReached`, etc.

A new crate `blufio-events` defines the `BusEvent` enum and a `EventBus` wrapper (essentially `Arc<tokio::sync::broadcast::Sender<BusEvent>>`). This is ~100 lines, zero new dependencies.

**Why NOT external event bus crates (event_bus_rs, tokio-pubsub):** These wrap what tokio already provides. Adding a dependency for a 100-line wrapper violates the minimal audit surface constraint.

**Slow receiver handling:** Broadcast channels drop messages to lagging receivers. Set capacity to 1024+ and document that adapters must process events promptly. Log lag metrics via existing Prometheus integration.

**Confidence:** HIGH — tokio::sync::broadcast is a core tokio primitive, documented and production-proven.

---

## 6. Skill Registry / Marketplace + Code Signing

### Registry Architecture

The skill registry is a static TOML/JSON index served over HTTPS, plus a CLI (`blufio skill search`, `blufio skill install`). No new database crates needed — SQLite tracks installed skills (already in blufio-skill).

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| minisign-verify | 0.2.5 (existing from v1.2) | Verify skill package Ed25519 signatures before installation | Already in workspace. Reuse the same Minisign verification flow used for binary self-update. Skills are signed with a developer keypair; public key embedded in the registry index. |
| reqwest | 0.13 (existing) | Download skill packages from registry | Already in workspace with stream support. |
| flate2 | 1 (existing from v1.2) | Decompress .wasm.gz skill packages | Already in workspace from self-update feature. |
| tar | 0.4 (existing from v1.2) | Extract skill archives | Already in workspace from self-update feature. |

**No new crates needed for skill registry.** All capabilities are in the existing stack.

**Code signing flow:**
1. Skill author signs their `.wasm` with `minisign -S` using their Ed25519 keypair
2. Registry index (TOML) stores: skill name, version, WASM URL, `.minisig` URL, author public key
3. `blufio skill install` downloads WASM + `.minisig`, verifies with minisign-verify, installs if valid
4. Verification uses the same `minisign_verify::PublicKey` + streaming verify pattern from v1.2 self-update

**Registry index format (TOML):**
```toml
[[skills]]
name = "weather"
version = "1.2.0"
author = "alice"
author_pubkey = "RWQ..."  # Ed25519 public key (base64)
wasm_url = "https://registry.blufio.dev/skills/weather-1.2.0.wasm.gz"
sig_url   = "https://registry.blufio.dev/skills/weather-1.2.0.wasm.gz.minisig"
description = "Get current weather for any city"
```

**Confidence:** HIGH — reuses existing dependencies and patterns.

---

## 7. Node System (Paired Devices)

The node system enables multiple Blufio instances to share sessions, route messages, and sync memory across a trusted network of paired devices.

**No new networking crates needed.** The node system uses the existing HTTP gateway (axum) for inbound connections and reqwest for outbound calls. Nodes discover each other via a static registry (TOML config), not P2P gossip.

**Design rationale — why NOT libp2p:**
libp2p (and p2panda-net) add significant complexity: DHT, gossipsub, NAT traversal, mDNS. For Blufio's use case (a small trusted cluster of 2-10 personal devices), the complexity is not justified. A static peer list in TOML config with mutual Ed25519 authentication covers the use case.

**Node protocol (built on existing stack):**
- Pairing: Ed25519 keypair exchange via QR code or manual public key entry
- Node-to-node: HTTPS POST to `/v1/node/relay` with `Authorization: Bearer {ed25519-signed-token}` (same signing as agent delegation in v1.0)
- Sync: periodic pull of session summaries via `/v1/node/sync`
- Config: `[nodes]` section in blufio TOML

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| ed25519-dalek | 2.1 (existing) | Sign/verify node-to-node messages | Already used for agent delegation. Same pattern. |
| reqwest | 0.13 (existing) | Outbound node HTTP calls | Already in workspace. |

**Confidence:** HIGH for reqwest+ed25519 approach. MEDIUM for overall node design (no precedent in codebase — first implementation of multi-instance coordination).

---

## 8. Docker Deployment

### Docker Build Strategy

| Tool | Purpose | Why |
|------|---------|-----|
| cargo-chef | Docker layer caching for Rust dependencies | Splits dependency build from source build. Dependencies (unchanged between code commits) cache as a Docker layer. Reduces incremental Docker build time from 5-15min to 30-90s. Industry standard for Rust Docker builds. |

**Dockerfile pattern (three-stage):**
```dockerfile
# Stage 1: compute recipe
FROM lukemathwalker/cargo-chef:latest-rust-1.85 AS planner
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Stage 2: build dependencies (cached layer)
FROM lukemathwalker/cargo-chef:latest-rust-1.85 AS builder
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
# Then build actual binary
COPY . .
RUN cargo build --release --target x86_64-unknown-linux-musl

# Stage 3: minimal runtime image
FROM gcr.io/distroless/static-debian12:nonroot AS runtime
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/blufio /blufio
ENTRYPOINT ["/blufio"]
```

**Runtime base image — distroless/static over scratch:**
- `gcr.io/distroless/static-debian12:nonroot` includes: CA certificates (for HTTPS), `/etc/passwd` (non-root user), timezone data
- `scratch` requires manually copying CA certs; missing them breaks all TLS connections
- distroless:nonroot runs as UID 65532 by default (security best practice)
- Final image size: 10-15MB (musl binary + distroless base)

**Musl target:** The existing `profile.release-musl` in Cargo.toml is already configured for static linking. Use `cross` (already likely in CI) with `x86_64-unknown-linux-musl` target.

**Docker Compose for development:**
```yaml
services:
  blufio:
    image: blufio:latest
    volumes:
      - ./data:/data
    environment:
      - BLUFIO_DB_KEY=${BLUFIO_DB_KEY}
      - BLUFIO_VAULT_KEY=${BLUFIO_VAULT_KEY}
    ports:
      - "3000:3000"
```

**No new Rust crates needed for Docker.** Docker is a build/deployment concern, not a code concern.

**Confidence:** HIGH — cargo-chef is the industry standard pattern for Rust Docker builds, well-documented and actively maintained.

---

## 9. OpenClaw Migration Tooling

### What Needs to Migrate

OpenClaw stores its state in a directory (`$OPENCLAW_STATE_DIR`) containing:
- `openclaw.json` — main config (JSON format)
- `credentials/` — per-provider credentials
- `agents/{agentId}/` — session history, memory, skills notes, channel state

### Crates for Migration

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| serde_json | 1 (via serde) | Parse openclaw.json config | serde_json is already transitively available via serde. Add as direct dep in the migration crate. |
| walkdir | 2 | Recursively walk OpenClaw state directory | Pure Rust, zero unsafe code. Standard for directory traversal. |

**New crate:** `blufio-openclaw-migrate` (or as a subcommand of the main binary: `blufio migrate openclaw`).

**Migration scope:**
1. Parse `openclaw.json` → emit equivalent `blufio.toml` sections
2. Map OpenClaw channel configs (Telegram, Discord, etc.) → blufio channel plugin configs
3. Map OpenClaw provider configs (OpenAI, Anthropic) → blufio provider configs
4. Convert OpenClaw session/memory JSONL files → INSERT into blufio SQLite schema
5. Emit clear warnings for OpenClaw features with no blufio equivalent (Node.js plugins)

**Why NOT openclaw-core crate:** The `openclaw-core` crate on crates.io is a community project, not official OpenClaw. Using it would add a dependency on an unofficial crate to read OpenClaw's format. Better to parse the JSON directly with serde_json — the format is simple.

**`--dry-run` mode:** Required. Show what would be migrated without writing anything. Users need confidence before committing.

**Confidence:** MEDIUM — OpenClaw config format is JSON (parseable), but specific schema details require testing against real openclaw.json files. The migration approach is sound; exact field mappings need implementation discovery.

---

## 10. Summary: All New Dependencies for v1.3

### New Crates

| Crate | Version | Used By | Purpose |
|-------|---------|---------|---------|
| tower-governor | 0.8.0 | blufio-gateway | API key rate limiting for OpenAI-compat layer |
| async-openai | 0.33.0 | blufio-openai, blufio-ollama, blufio-openrouter | OpenAI/Ollama/OpenRouter provider + TTS/Transcription types |
| genai | 0.5.3 | blufio-gemini | Google Gemini provider (not OpenAI-compatible) |
| serenity | 0.12.5 | blufio-discord | Discord channel adapter |
| slack-morphism | 2.18.0 | blufio-slack | Slack channel adapter with Socket Mode |
| irc | 1.1.0 | blufio-irc | IRC channel adapter |
| matrix-sdk | 0.11.0 | blufio-matrix | Matrix channel adapter (MSRV 1.85 compatible) |
| serde_json | 1 | blufio-openclaw-migrate | Parse OpenClaw JSON config |
| walkdir | 2 | blufio-openclaw-migrate | Walk OpenClaw state directory |

**Total new direct crates: 9**

### Reused Without Change (v1.2 additions already cover)

| Crate | Usage in v1.3 |
|-------|--------------|
| minisign-verify 0.2.5 | Skill code signing in registry |
| reqwest 0.13 | WhatsApp Cloud API, Signal JSON-RPC bridge, node HTTP calls |
| flate2 1 | Decompress skill packages |
| tar 0.4 | Extract skill archives |
| ed25519-dalek 2.1 | Node-to-node message signing |
| axum 0.8 | OpenAI-compatible endpoint handlers, SSE streaming |
| tokio (broadcast) | Internal event bus implementation |

### No New Crates For

- Event bus: `tokio::sync::broadcast` (zero new deps)
- WhatsApp: `reqwest` direct implementation (zero new deps)
- Signal: `reqwest` bridge to signal-cli sidecar (zero new deps)
- Skill registry: existing stack (minisign-verify + reqwest + flate2 + tar)
- Node system: existing stack (ed25519-dalek + reqwest)
- Docker: build tooling only (cargo-chef in CI/Dockerfile, not a Rust dependency)

### Dependency Budget

| Metric | v1.2 | v1.3 |
|--------|------|------|
| Direct workspace deps | ~42 | ~51 |
| Within <80 constraint | Yes | Yes |
| New crates with broad ecosystem use | All 9 | Minimal audit surface |

---

## 11. What NOT to Add

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| whatsapp-cloud-api crate (0.5.4) | Last updated May 2025, small ecosystem, targets Graph API v20.0 specifically | reqwest + serde with direct Meta API calls |
| whatsapp-rust crate | Unofficial WhatsApp Web protocol. Violates Meta ToS. Account suspension risk | Meta Cloud API (official) |
| libsignal-service-rs (whisperfish) | Immature, reverse-engineered Signal protocol, security-critical | signal-cli sidecar via JSON-RPC |
| matrix-sdk 0.12+ | MSRV 1.88 — incompatible with workspace rust-version 1.85 | matrix-sdk 0.11.0 (MSRV 1.85) |
| libp2p for node system | Full P2P stack (DHT, gossipsub, NAT) far exceeds needs for 2-10 trusted nodes | Static peer list in TOML + reqwest + ed25519 |
| whisper-rs / whisper-cpp-plus for local TTS | Requires building C++, adds 5-15MB to binary, violates <50MB constraint | OpenAI-compatible HTTP TTS/transcription API; optional local plugin later |
| genai for ALL providers | Displaces existing blufio-anthropic (v1.0-v1.2 investment), adds unnecessary abstraction layer | genai only for Gemini; async-openai for OpenAI/Ollama/OpenRouter |
| poise (Discord slash commands) | Framework on top of serenity, adds complexity. Channel adapter needs message recv/send, not slash commands | serenity directly |
| event_bus_rs / tokio-pubsub | Thin wrappers around tokio broadcast, zero value over direct use | tokio::sync::broadcast directly |
| openclaw-core crate | Unofficial community re-implementation of OpenClaw, not the real config format | Direct serde_json parsing of openclaw.json |
| self_update crate (jaemk) | Already rejected in v1.2. No Minisign support, outdated. Already using reqwest + self-replace | Continue v1.2 approach |
| slack-rs-api | Last updated 2022, no Socket Mode support | slack-morphism 2.18.0 |
| twilight (Discord) | Lower-level than serenity, requires more boilerplate for a Channel adapter | serenity 0.12.5 |

---

## 12. Version Compatibility Matrix

| Crate | Version Pinned | MSRV | Tokio Compat | Notes |
|-------|---------------|------|-------------|-------|
| serenity | 0.12.5 | 1.74 | tokio 1.x | Compatible with project Rust 1.85 |
| matrix-sdk | 0.11.0 | 1.85 | tokio 1.x | EXACT version match required. 0.12+ breaks. |
| slack-morphism | 2.18.0 | unknown | tokio 1.x | Actively maintained, no known MSRV conflicts |
| irc | 1.1.0 | unknown | tokio 1.x | tokio-based, released 2025-03-24 |
| async-openai | 0.33.0 | unknown | tokio 1.x | Released 2026-02-18 |
| genai | 0.5.3 | unknown | tokio 1.x | Released 2026-01-31 |
| tower-governor | 0.8.0 | unknown | tower 0.5 | tower 0.5 is in workspace already |

**Matrix-SDK version lock is the single most critical version constraint in v1.3.**

---

## 13. New Crates Structure

```
crates/
  blufio-events/          # Event bus (tokio broadcast wrapper, zero new deps)
  blufio-openai/          # OpenAI + Ollama + OpenRouter provider (async-openai)
  blufio-gemini/          # Gemini provider (genai)
  blufio-discord/         # Discord channel adapter (serenity)
  blufio-whatsapp/        # WhatsApp Cloud API adapter (reqwest)
  blufio-slack/           # Slack adapter (slack-morphism)
  blufio-signal/          # Signal adapter (reqwest + signal-cli bridge)
  blufio-irc/             # IRC adapter (irc crate)
  blufio-matrix/          # Matrix adapter (matrix-sdk 0.11)
  blufio-registry/        # Skill registry client (reqwest + minisign-verify)
  blufio-node/            # Node system (reqwest + ed25519-dalek)
  blufio-openclaw-migrate/ # OpenClaw migration (serde_json + walkdir)
```

Total new crates: 12 (bringing workspace from 22 to ~34 crates)

---

## 14. Workspace Cargo.toml Additions

```toml
[workspace.dependencies]
# === v1.3 ADDITIONS ===

# OpenAI-compatible API layer
tower-governor = "0.8"

# LLM providers
async-openai = "0.33"
genai = "0.5"

# Channel adapters
serenity = { version = "0.12", default-features = false, features = ["client", "gateway", "model", "rustls_backend"] }
slack-morphism = "2.18"
irc = { version = "1.1", default-features = false, features = ["tls-native"] }
# NOTE: Pin matrix-sdk to 0.11.x — MSRV 1.85. Do NOT upgrade to 0.12+.
matrix-sdk = { version = "0.11", default-features = false, features = ["rustls-tls", "e2e-encryption"] }

# Migration tooling
serde_json = "1"
walkdir = "2"
```

**serenity features:** Use `rustls_backend` (not `native_tls`) to stay consistent with the project's rustls-only TLS policy. Disable default features to exclude unused voice/collector support.

**irc features:** Use `tls-native` or `tls-rustls` depending on which compiles cleaner with the musl target. Verify at build time — IRC TLS via rustls is preferred for consistency.

**matrix-sdk features:** `rustls-tls` for TLS consistency. `e2e-encryption` for Matrix E2EE support (required for production Matrix use).

---

## Sources

### Verified via docs.rs (version + release date confirmed)
- [async-openai 0.33.0](https://docs.rs/crate/async-openai/latest) — Released 2026-02-18
- [serenity 0.12.5](https://docs.rs/crate/serenity/latest) — Released 2025-12-20
- [slack-morphism 2.18.0](https://docs.rs/crate/slack-morphism/latest) — Released 2026-02-21
- [irc 1.1.0](https://docs.rs/crate/irc/latest) — Released 2025-03-24
- [matrix-sdk 0.16.0](https://docs.rs/crate/matrix-sdk/latest) — 0.16 is latest; 0.11.0 used for MSRV 1.85
- [genai 0.5.3](https://docs.rs/crate/genai/latest) — Released 2026-01-31
- [tower-governor 0.8.0](https://docs.rs/tower-governor/latest/tower_governor/) — Version 0.8.0 confirmed

### Verified via WebSearch + official sources
- [matrix-sdk CHANGELOG](https://github.com/matrix-org/matrix-rust-sdk/blob/main/crates/matrix-sdk/CHANGELOG.md) — MSRV 1.85 for 0.11, MSRV 1.88 from 0.12+
- [serenity GitHub](https://github.com/serenity-rs/serenity) — MSRV 1.74, tokio 1.x compatible
- [OpenClaw migration guide](https://docs.openclaw.ai/install/migrating) — State directory structure verified
- [cargo-chef GitHub](https://github.com/LukeMathWalker/cargo-chef) — Three-stage Docker pattern
- [axum SSE docs](https://docs.rs/axum/latest/axum/response/sse/) — Built-in SSE, no extra crate needed
- [tokio broadcast](https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html) — Event bus primitive
- [genai crates.io](https://crates.io/crates/genai) — Multiprovider: OpenAI, Gemini, Anthropic, Ollama, etc.

### Confidence Assessment

| Area | Confidence | Reason |
|------|------------|--------|
| OpenAI API layer (axum + tower-governor) | HIGH | All crates verified, existing axum SSE proven |
| OpenAI/Ollama/OpenRouter provider (async-openai) | HIGH | Version verified, base URL override documented |
| Gemini provider (genai) | MEDIUM | Verified, actively maintained, but no official Google backing |
| Discord (serenity) | HIGH | De facto standard, version + MSRV verified |
| Slack (slack-morphism) | HIGH | Most complete Rust Slack library, version verified |
| IRC (irc crate) | HIGH | RFC-compliant, version verified |
| WhatsApp (reqwest direct) | MEDIUM | Meta API is stable, but business account setup is required |
| Signal (signal-cli bridge) | MEDIUM | signal-cli is the only realistic approach; Java sidecar adds ops complexity |
| Matrix (matrix-sdk 0.11) | HIGH for version, MEDIUM for E2EE | MSRV constraint verified; E2EE adds compile complexity |
| Event bus (tokio broadcast) | HIGH | Core tokio primitive |
| Skill registry/signing | HIGH | Reuses existing minisign-verify + reqwest |
| Node system | MEDIUM | Design sound, no precedent in codebase |
| Docker (cargo-chef) | HIGH | Industry standard Rust Docker pattern |
| OpenClaw migration | MEDIUM | JSON format parseable; field mapping needs implementation discovery |

---

*Stack research for: Blufio v1.3 Ecosystem Expansion*
*Researched: 2026-03-05*
