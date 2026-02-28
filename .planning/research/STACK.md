# Stack Research

**Domain:** Rust AI Agent Platform (always-on, multi-channel, LLM-powered)
**Researched:** 2026-02-28
**Confidence:** HIGH (all core crates verified via Context7, docs.rs, and crates.io)

---

## Recommended Stack

### Async Runtime & Core

| Technology | Version | Purpose | Why Recommended | Confidence |
|------------|---------|---------|-----------------|------------|
| **tokio** | 1.49.0 | Async runtime | The Rust async runtime. No alternatives worth considering. Powers axum, reqwest, teloxide, tracing, hyper. Multi-threaded scheduler with work-stealing. LTS releases (1.47.x through Sep 2026). Every async crate in this stack depends on it. | HIGH |
| **serde** | 1.0.228 | Serialization framework | Universal serialization. Every config, API response, database row, and wire format uses serde derives. Zero-cost abstractions via proc macros. | HIGH |
| **serde_json** | 1.0.149 | JSON serialization | LLM API payloads are JSON. serde_json is the only serious choice. | HIGH |
| **tikv-jemallocator** | 0.6.1 | Memory allocator | jemalloc reduces fragmentation in long-running processes. Critical for an always-on agent that must not grow memory over weeks. TiKV's fork is the maintained version (wraps jemalloc 5.2.1). Linux-only for production; macOS uses system allocator in dev. | HIGH |

### Web Framework & HTTP

| Technology | Version | Purpose | Why Recommended | Confidence |
|------------|---------|---------|-----------------|------------|
| **axum** | 0.8.8 | HTTP/WebSocket gateway | Built by tokio team, on tower middleware ecosystem. Native WebSocket support via tungstenite. Extractor-based design composes cleanly with plugin architecture. 0.8.x introduced `{param}` path syntax and `OptionalFromRequestParts`. | HIGH |
| **tower** | 0.5.x | Middleware framework | axum's middleware layer. Rate limiting, timeouts, load shedding, compression all composable via tower::Layer. | HIGH |
| **tower-http** | 0.6.x | HTTP-specific middleware | CORS, compression, request ID, tracing, timeout layers. Saves writing boilerplate. | HIGH |
| **reqwest** | 0.13.2 | HTTP client | For all outbound HTTP: LLM API calls, Telegram Bot API, webhook delivery. Built on hyper + tokio. Connection pooling, TLS, streaming responses, multipart. 0.13.x is the latest major line. | HIGH |

### Database

| Technology | Version | Purpose | Why Recommended | Confidence |
|------------|---------|---------|-----------------|------------|
| **rusqlite** | 0.38.0 | SQLite bindings | Synchronous but that's correct for embedded SQLite. Wraps SQLite 3.51.1. Feature flags for `bundled` (compile SQLite into binary), `bundled-sqlcipher` (encrypted SQLite), `backup`, `blob`, `hooks`, `functions`, `vtab`. Use `bundled-sqlcipher-vendored-openssl` for fully static builds. | HIGH |
| **rusqlite_migration** | 2.4.0 | Schema migrations | Simple, file-based migrations for rusqlite. No runtime overhead. | MEDIUM |

**Why NOT sqlx:** sqlx is async but adds complexity for embedded SQLite. SQLite operations are inherently synchronous (single-writer with WAL). sqlx's value is compile-time query checking against Postgres/MySQL schemas -- overkill for embedded use. rusqlite + `spawn_blocking` for the few hot paths is simpler, gives direct access to SQLite features (backup API, blob I/O, virtual tables), and avoids the semver hazard where sqlx and rusqlite fight over libsqlite3-sys versions. For a single-binary embedded database, rusqlite is the right call.

**SQLCipher integration:** rusqlite's `bundled-sqlcipher` feature compiles SQLCipher directly into the binary. Use `PRAGMA key = 'your-key';` after opening. The `bundled-sqlcipher-vendored-openssl` feature avoids system OpenSSL dependency for fully static musl builds.

### Telegram Bot

| Technology | Version | Purpose | Why Recommended | Confidence |
|------------|---------|---------|-----------------|------------|
| **teloxide** | 0.17.0 | Telegram Bot API framework | Full-featured, async (tokio), type-safe Telegram bot framework. Covers the complete Telegram Bot API: long polling, webhooks, inline keyboards, media, commands. Dialogue system for multi-step conversations. Active maintenance (released Jul 2025). | HIGH |

**Architecture note:** teloxide will be wrapped behind a `Channel` adapter trait. The agent core never imports teloxide directly. This allows Telegram-specific features (inline keyboards, reply markup) while keeping the channel abstraction clean.

### LLM API Client

| Technology | Version | Purpose | Why Recommended | Confidence |
|------------|---------|---------|-----------------|------------|
| **reqwest** | 0.13.2 | HTTP transport for LLM APIs | Build custom Anthropic/OpenAI clients on reqwest directly. LLM APIs are simple REST endpoints (POST with JSON body, streaming SSE response). A thin typed client over reqwest gives full control over retry logic, streaming, token counting, and cost tracking. | HIGH |

**Why NOT rig-core:** Rig (rig.rs) is a good LLM abstraction framework but introduces opinions about agent architecture, RAG patterns, and vector store integration that conflict with Blufio's custom agent loop, three-zone context engine, and FSM-per-session design. The LLM provider abstraction is simple enough (send messages, get response, stream tokens) that a custom `Provider` trait with reqwest gives more control and fewer dependencies. Rig adds ~15 transitive dependencies for abstractions we'd fight against.

**Why NOT genai/agentai:** Too new, LOW confidence in stability. Build the thin client, evaluate later.

**Streaming:** Use reqwest's streaming response + `eventsource-stream` crate for SSE parsing of streaming LLM responses (Anthropic and OpenAI both use SSE for streaming).

### WASM Runtime

| Technology | Version | Purpose | Why Recommended | Confidence |
|------------|---------|---------|-----------------|------------|
| **wasmtime** | 40.0.1 | WASM skill sandboxing | Bytecode Alliance project. Industry-standard WASM runtime. Component Model support for structured host-guest communication. WASI Preview 2 for filesystem/network capabilities. Cranelift JIT compiler. Fuel-based execution limits for CPU bounding. Memory limits per instance. | HIGH |
| **wasmtime-wasi** | 40.0.1 | WASI host implementation | Provides WASI system interface to WASM modules. Controls what filesystem, network, env capabilities each skill gets. | HIGH |

**Component Model:** wasmtime 40.x supports the WebAssembly Component Model. Use WIT (WebAssembly Interface Types) files to define the skill<->host interface. Skills implement WIT-defined exports; the host provides WIT-defined imports (LLM calls, storage, HTTP). This is strictly better than raw WASM imports/exports for structured APIs.

**Security model:** Each skill runs in its own `Store` with isolated linear memory. Fuel metering prevents infinite loops. Capability manifests (defined in SKILL.toml) map to WASI capabilities granted at instantiation.

### Embedding Model Inference

| Technology | Version | Purpose | Why Recommended | Confidence |
|------------|---------|---------|-----------------|------------|
| **ort** | 2.0.0-rc.11 | ONNX Runtime inference | Wraps Microsoft ONNX Runtime 1.23. 3-5x faster than Python equivalents. Hardware acceleration (CUDA, TensorRT, OpenVINO) when available. Battle-tested for production embedding inference. Supports all-MiniLM-L6-v2 and similar sentence transformer models in ONNX format. | HIGH |

**Why NOT candle:** Candle (huggingface/candle, 0.9.2) excels at small binary size and HuggingFace integration, but ort with ONNX Runtime is faster for production embedding inference and has broader hardware acceleration support. For an always-on server (not edge/WASM deployment), raw inference speed matters more than binary size. The ~80MB ONNX model file is a fixed cost either way.

**Why NOT candle for v1.0, consider for v2.0:** Candle could be the `Embedding` plugin alternative for edge deployments where ONNX Runtime's C++ dependency is undesirable. The plugin architecture makes this a future swap.

**Model:** Ship with `all-MiniLM-L6-v2` (80MB ONNX, 384-dim embeddings). Good quality/size tradeoff for semantic search and memory retrieval.

### Observability

| Technology | Version | Purpose | Why Recommended | Confidence |
|------------|---------|---------|-----------------|------------|
| **tracing** | 0.1.44 | Structured logging + spans | The Rust observability standard. Async-aware spans, structured fields, subscriber architecture. Integrates with tokio console for debugging. Every major Rust library emits tracing events. | HIGH |
| **tracing-subscriber** | 0.3.x | Log output formatting | JSON output for production, pretty-print for development. Layer composition for multiple outputs. | HIGH |
| **tracing-opentelemetry** | 0.28.x | OpenTelemetry bridge | Connects tracing spans to OpenTelemetry for export to Jaeger/Zipkin/Prometheus. Optional but important for production observability. | MEDIUM |
| **metrics** | 0.24.3 | Metrics facade | Lightweight facade (like `log` for metrics). Counters, gauges, histograms via macros. Decouples instrumentation from export backend. | HIGH |
| **metrics-exporter-prometheus** | 0.18.1 | Prometheus export | Exposes `/metrics` endpoint for Prometheus scraping. Pairs with `metrics` facade. Production-proven pattern. | HIGH |

**Why both tracing AND metrics:** tracing handles structured logs and distributed tracing (request flows, error context). metrics handles numerical time-series (tokens/sec, cost/day, queue depth, latency percentiles). Different concerns, different consumers (log aggregator vs Prometheus/Grafana). Using both is standard practice.

### Configuration

| Technology | Version | Purpose | Why Recommended | Confidence |
|------------|---------|---------|-----------------|------------|
| **toml** | 0.9.x | TOML parsing | Rust ecosystem default for config. serde integration. Human-readable, unambiguous syntax. | HIGH |
| **serde** (with `deny_unknown_fields`) | 1.0.228 | Config validation | `#[serde(deny_unknown_fields)]` on all config structs catches typos at load time instead of silently ignoring them. Critical for operational safety. | HIGH |

**Config layering:** TOML for file config, environment variables for secrets (via `std::env`), CLI flags for overrides (via clap). Priority: CLI > env > file > defaults. No need for a config framework crate -- this pattern is <100 lines of code.

### CLI

| Technology | Version | Purpose | Why Recommended | Confidence |
|------------|---------|---------|-----------------|------------|
| **clap** | 4.5.60 | CLI argument parsing | De facto standard. Derive macros for zero-boilerplate subcommand definitions. Powers ripgrep, bat, fd. Supports shell completions, colored help, env var fallback. | HIGH |

### Cryptography

| Technology | Version | Purpose | Why Recommended | Confidence |
|------------|---------|---------|-----------------|------------|
| **ring** | 0.17.14 | AES-256-GCM encryption | Audited, high-performance crypto. Uses hardware AES-NI when available. Preferred for AES-GCM because it's a single well-audited dependency rather than assembling multiple RustCrypto crates. Used by rustls, the Rust TLS implementation. | HIGH |
| **ed25519-dalek** | 2.2.0 | Ed25519 signing | RustCrypto ecosystem. Fast Ed25519 key generation, signing, verification. For inter-agent message signing and device keypair authentication. | HIGH |
| **rand** | 0.9.x | Cryptographic RNG | `rand::rngs::OsRng` for key generation. `rand::thread_rng()` for non-security randomness. | HIGH |

**Why ring for AES-GCM but ed25519-dalek for Ed25519:** ring's Ed25519 API is more restrictive (no keypair serialization without workarounds). ed25519-dalek provides a cleaner API for key management (generate, serialize, deserialize, sign, verify). For AES-GCM, ring is simpler and faster. Use the best tool for each primitive.

**Why NOT RustCrypto aes-gcm:** The aes-gcm crate (0.10.3 stable, 0.11.0-rc.2 pre-release) is pure Rust and audited, but ring's AES-GCM uses hardware intrinsics by default and is a single dependency. aes-gcm requires assembling aes + aes-gcm + cipher crates. ring is the simpler, faster path for this specific primitive.

### Plugin Architecture

| Technology | Version | Purpose | Why Recommended | Confidence |
|------------|---------|---------|-----------------|------------|
| **Trait objects** (`Box<dyn Trait>`) | N/A (language feature) | Plugin dispatch | Rust's native dynamic dispatch. Define adapter traits (`Channel`, `Provider`, `Storage`, `Embedding`, `Observability`, `Auth`, `SkillRuntime`). Plugins implement traits, registered at startup. ~2-5% overhead from vtable indirection -- negligible for I/O-bound adapters. | HIGH |

**Why NOT libloading/dynamic .so:** Rust has no stable ABI. Dynamic loading of .so/.dylib requires C FFI boundaries, `#[repr(C)]` structs, and manual vtable construction. Fragile across compiler versions. WASM-only for v1.0 means third-party code runs in wasmtime, not as native plugins. Built-in plugins (Telegram, Anthropic, SQLite) are compiled in with feature flags.

**Pattern:** Compile-time plugin composition via Cargo feature flags. `--features telegram,anthropic,sqlite,prometheus` controls what's included. Zero-cost when disabled. Runtime registration via a `PluginRegistry` that holds `Box<dyn Channel>`, `Box<dyn Provider>`, etc.

### Supporting Libraries

| Library | Version | Purpose | When to Use | Confidence |
|---------|---------|---------|-------------|------------|
| **tokio-util** | 0.7.x | Async utilities | Codec for framing, CancellationToken for graceful shutdown, sync utilities | HIGH |
| **futures** | 0.3.x | Future combinators | `Stream`, `Sink`, `select!`, `join!` for async composition | HIGH |
| **bytes** | 1.x | Byte buffer | Zero-copy byte handling for network I/O and WASM memory transfer | HIGH |
| **uuid** | 1.x | Unique IDs | Session IDs, message IDs, correlation IDs | HIGH |
| **chrono** | 0.4.x | Date/time | Timestamps for messages, cron scheduling, cost tracking | HIGH |
| **thiserror** | 2.x | Error types | Derive macro for custom error enums. Use for library error types. | HIGH |
| **anyhow** | 1.x | Error handling | Use in application code (main, handlers) where you don't need typed errors. Not in library code. | HIGH |
| **eventsource-stream** | 0.2.x | SSE parsing | Parse Server-Sent Events from LLM streaming responses | MEDIUM |
| **base64** | 0.22.x | Base64 encoding | Encoding binary data in JSON payloads, credential handling | HIGH |
| **url** | 2.x | URL parsing | Webhook URLs, API endpoints, Telegram file URLs | HIGH |
| **dashmap** | 6.x | Concurrent hash map | Session registry, LRU caches shared across tasks. Lock-free reads. | MEDIUM |
| **lru** | 0.12.x | LRU cache | Bounded caches for context, embeddings, rate limit counters | MEDIUM |
| **cron** | 0.13.x | Cron expressions | Parsing cron schedules for heartbeats and scheduled skills | MEDIUM |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| **cargo-deny** | Dependency auditing | License checking, advisory database, duplicate detection. Run in CI. |
| **cargo-audit** | Security advisories | Check deps against RustSec advisory database |
| **cargo-watch** | Dev reload | `cargo watch -x run` for development iteration |
| **cargo-nextest** | Test runner | Faster, better output than `cargo test`. Parallel by default. |
| **cargo-llvm-cov** | Code coverage | LLVM-based coverage. Accurate for async code. |
| **cross** | Cross-compilation | Build `x86_64-unknown-linux-musl` static binary from macOS |
| **cargo-bloat** | Binary size analysis | Track binary size budget (25-50MB target) |

---

## Alternatives Considered

| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| Async runtime | **tokio** | async-std | async-std has smaller ecosystem, less middleware, fewer integrations. tokio is the gravity well -- everything depends on it. |
| Web framework | **axum** | actix-web | actix-web is fast but uses its own runtime, fighting with tokio. axum IS the tokio web framework. warp is unmaintained. |
| SQLite | **rusqlite** | sqlx (SQLite mode) | sqlx adds async overhead for an inherently sync database. Compile-time query checking doesn't help with dynamic queries. SQLCipher integration is smoother with rusqlite. |
| SQLite | **rusqlite** | diesel | Diesel's ORM is overkill. We want raw SQL control for SQLite-specific features (WAL, PRAGMA, backup API). |
| HTTP client | **reqwest** | hyper (direct) | hyper is lower-level. reqwest adds connection pooling, TLS, redirects, cookies. For LLM API calls, reqwest's ergonomics win. |
| HTTP client | **reqwest** | ureq | ureq is blocking-only. We need async streaming for LLM responses. |
| LLM abstraction | **custom (reqwest)** | rig-core | Rig's abstractions conflict with our agent loop design. Too opinionated for a platform that IS the agent framework. |
| LLM abstraction | **custom (reqwest)** | genai | Too immature, LOW confidence in API stability. |
| Embedding | **ort** | candle | ort wraps ONNX Runtime with hardware accel. Candle is pure Rust but slower for server-side inference. Consider candle for edge plugin. |
| Embedding | **ort** | fastembed-rs | fastembed-rs wraps ort anyway. Just use ort directly for more control. |
| WASM | **wasmtime** | wasmer | wasmtime is Bytecode Alliance (Mozilla/Fastly/Intel backed). Better Component Model support. wasmer has licensing concerns and less WASI P2 support. |
| Crypto | **ring** (AES-GCM) | RustCrypto aes-gcm | ring uses hardware AES-NI by default, single dep. aes-gcm requires assembling multiple crates. |
| Crypto | **ed25519-dalek** | ring (Ed25519) | ring's Ed25519 API lacks easy key serialization. dalek's API is cleaner for key management workflows. |
| Config | **toml + serde** | config (crate) | The `config` crate adds layers of abstraction for something that's ~100 lines with toml + serde + env vars. Simpler is better. |
| Logging | **tracing** | log | log is text-only. tracing adds spans, structured fields, async context propagation. Standard for tokio ecosystem. |
| CLI | **clap** | structopt | structopt merged into clap 4.x derives. Use clap directly. |
| Allocator | **tikv-jemallocator** | mimalloc | Both good. jemalloc is better studied for long-running server processes. TiKV uses it in production for exactly this use case. |
| Plugin loading | **trait objects + features** | libloading (.so) | No stable ABI in Rust. Dynamic loading is fragile across compiler versions. WASM for third-party, feature flags for built-in. |

---

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| **async-std** | Fragmenting the async ecosystem. Most crates assume tokio. | tokio |
| **warp** | Effectively unmaintained since 2023. | axum |
| **diesel** | ORM layer hides SQLite-specific features we need (PRAGMA, backup, WAL tuning). Migration system is overkill. | rusqlite |
| **sea-orm** | Same ORM objection. Also pulls in sqlx which conflicts with rusqlite's libsqlite3-sys. | rusqlite |
| **hyper** (direct) | Too low-level for HTTP client use. Requires manual connection management. | reqwest (built on hyper) |
| **openssl** (direct) | Heavy system dependency. Complicates static/musl builds. | ring for crypto, rustls for TLS (reqwest default) |
| **native-tls** | System TLS varies across platforms. | rustls (reqwest `rustls-tls` feature) |
| **wasmer** | Less Component Model support. WASI P2 lagging behind wasmtime. Licensing ambiguity (WASIX). | wasmtime |
| **sled** | Unmaintained embedded database. Was never stable. | rusqlite + SQLite |
| **tonic** | gRPC framework. Blufio uses REST/WebSocket, not gRPC. Adds protobuf dependency. | axum |
| **rig-core** | Imposes agent architecture that conflicts with custom FSM agent loop. | Custom provider trait + reqwest |
| **config** (crate) | Unnecessary abstraction layer. TOML + env + CLI is simple enough without it. | toml + serde + std::env |

---

## Stack Patterns by Variant

**If targeting musl static binary (production):**
- Use `rusqlite` with `bundled-sqlcipher-vendored-openssl` feature
- Use `reqwest` with `rustls-tls` feature (NOT `native-tls`)
- Use `tikv-jemallocator` as global allocator
- Cross-compile with `cross` or `cargo-zigbuild` targeting `x86_64-unknown-linux-musl`
- Binary size target: 25-50MB

**If running on macOS (development):**
- Use system allocator (jemalloc optional on macOS)
- Use `rusqlite` with `bundled-sqlcipher` feature
- Use `reqwest` with `rustls-tls` feature (consistent with production)
- Enable `tracing-subscriber` pretty-print format

**If embedding model is optional (minimal install):**
- Make `ort` a Cargo feature flag (`--features embedding`)
- Core binary without embedding: ~25MB
- With embedding runtime + model: ~50MB + 80MB model file

---

## Version Compatibility Matrix

| Package | Compatible With | Notes |
|---------|-----------------|-------|
| tokio 1.49.x | axum 0.8.x, reqwest 0.13.x, teloxide 0.17.x | All built on tokio 1.x. No version conflicts. |
| axum 0.8.x | tower 0.5.x, tower-http 0.6.x | axum 0.8 requires tower 0.5. tower-http 0.6.x matches. |
| rusqlite 0.38.x | libsqlite3-sys 0.31.x | Bundled feature compiles SQLite 3.51.1. Do NOT add sqlx in same dependency tree. |
| wasmtime 40.x | wasmtime-wasi 40.x | Always match major versions. They release in lockstep. |
| metrics 0.24.x | metrics-exporter-prometheus 0.18.x | Facade + exporter versions must be compatible. Check changelog on upgrade. |
| tracing 0.1.x | tracing-subscriber 0.3.x | Stable API. 0.2.x is unreleased future version. |
| ring 0.17.x | rustls (via reqwest) | ring is rustls's crypto backend. No conflict. |

---

## Cargo.toml Skeleton

```toml
[package]
name = "blufio"
version = "0.1.0"
edition = "2024"
license = "MIT OR Apache-2.0"
rust-version = "1.85"

[dependencies]
# Async runtime
tokio = { version = "1.49", features = ["full"] }
tokio-util = { version = "0.7", features = ["codec"] }
futures = "0.3"

# Web framework
axum = { version = "0.8", features = ["ws"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "compression-gzip", "request-id", "trace", "timeout"] }

# HTTP client
reqwest = { version = "0.13", default-features = false, features = ["json", "stream", "rustls-tls"] }

# Database
rusqlite = { version = "0.38", features = ["bundled-sqlcipher-vendored-openssl", "backup", "blob", "hooks", "functions", "trace", "serde_json", "uuid"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.9"

# Telegram
teloxide = { version = "0.17", features = ["macros"] }

# WASM runtime
wasmtime = { version = "40", features = ["component-model"] }
wasmtime-wasi = "40"

# Embedding inference (optional)
ort = { version = "2.0.0-rc.11", optional = true }

# Observability
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
metrics = "0.24"
metrics-exporter-prometheus = "0.18"

# Crypto
ring = "0.17"
ed25519-dalek = { version = "2.2", features = ["serde"] }
rand = "0.9"

# CLI
clap = { version = "4.5", features = ["derive", "env"] }

# Utilities
anyhow = "1"
thiserror = "2"
bytes = "1"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
base64 = "0.22"
url = "2"
dashmap = "6"
lru = "0.12"

[target.'cfg(target_os = "linux")'.dependencies]
tikv-jemallocator = "0.6"

[features]
default = ["telegram", "embedding"]
telegram = ["dep:teloxide"]
embedding = ["dep:ort"]

[profile.release]
opt-level = "z"     # Optimize for size
lto = true          # Link-time optimization
codegen-units = 1   # Single codegen unit for better optimization
strip = true        # Strip debug symbols
panic = "abort"     # No unwinding
```

---

## Sources

### Context7 (HIGH confidence)
- `/websites/rs_tokio_1_49_0` -- tokio 1.49.0 runtime features, multi-threaded scheduler, `#[tokio::main]` macro
- `/tokio-rs/axum/axum_v0_8_4` -- axum 0.8.4 WebSocket support, middleware, routing, ecosystem
- `/bytecodealliance/wasmtime/v38.0.4` -- wasmtime Component Model, WASI, embedding API (verified 40.0.1 is latest via docs.rs)
- `/websites/rs_teloxide_0_17_0_teloxide` -- teloxide 0.17.0 Bot API, Requester trait, webhooks, long polling
- `/huggingface/candle` -- Candle ML framework capabilities (compared against ort)

### docs.rs (HIGH confidence)
- [rusqlite 0.38.0](https://docs.rs/crate/rusqlite/latest) -- Version, features including bundled-sqlcipher, WAL mode
- [tracing 0.1.44](https://docs.rs/crate/tracing/latest) -- Version, structured diagnostics
- [tikv-jemallocator 0.6.1](https://docs.rs/crate/tikv-jemallocator/latest) -- Version, jemalloc 5.2.1 backend
- [ort 2.0.0-rc.11](https://docs.rs/crate/ort/latest) -- ONNX Runtime 1.23 wrapper
- [metrics 0.24.3](https://docs.rs/crate/metrics/latest) -- Metrics facade version
- [teloxide 0.17.0](https://docs.rs/crate/teloxide/latest) -- Version and release date (Jul 2025)
- [clap 4.5.60](https://docs.rs/crate/clap/latest) -- Version (Feb 2026)
- [reqwest 0.13.2](https://docs.rs/crate/reqwest/latest) -- Version (Feb 2026)
- [ring 0.17.14](https://docs.rs/crate/ring/latest) -- Version
- [ed25519-dalek 2.2.0](https://docs.rs/crate/ed25519-dalek/latest) -- Version
- [wasmtime 40.0.1](https://docs.rs/crate/wasmtime/latest) -- Version (Jan 2026)
- [metrics-exporter-prometheus 0.18.1](https://docs.rs/crate/metrics-exporter-prometheus/latest) -- Version
- [aes-gcm 0.10.3](https://docs.rs/crate/aes-gcm/latest) -- Evaluated, chose ring instead

### Web Search (MEDIUM confidence, verified with official sources)
- [Announcing axum 0.8.0](https://tokio.rs/blog/2025-01-01-announcing-axum-0-8-0) -- axum 0.8 release notes
- [tokio releases](https://github.com/tokio-rs/tokio/releases) -- LTS version schedule (1.47.x through Sep 2026)
- [wasmtime releases](https://github.com/bytecodealliance/wasmtime/releases) -- Release cadence, LTS policy
- [Cryptography.rs](https://cryptography.rs/) -- Rust crypto ecosystem overview
- [Building Sentence Transformers in Rust](https://dev.to/mayu2008/building-sentence-transformers-in-rust-a-practical-guide-with-burn-onnx-runtime-and-candle-281k) -- ort vs candle comparison
- [Rig.rs](https://rig.rs/) -- Evaluated as LLM abstraction, decided against
- [rusqlite SQLCipher issue](https://github.com/rusqlite/rusqlite/issues/219) -- SQLCipher feature flag history
- [Rust ORMs 2026 comparison](https://aarambhdevhub.medium.com/rust-orms-in-2026-diesel-vs-sqlx-vs-seaorm-vs-rusqlite-which-one-should-you-actually-use-706d0fe912f3) -- sqlx vs rusqlite analysis
- [Rust Observability with OpenTelemetry and Tokio](https://dasroot.net/posts/2026/01/rust-observability-opentelemetry-tokio/) -- tracing + metrics patterns

---
*Stack research for: Rust AI Agent Platform (Blufio)*
*Researched: 2026-02-28*
