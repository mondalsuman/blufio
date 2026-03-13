# Phase 62: Observability & API Surface - Context

**Gathered:** 2026-03-13
**Status:** Ready for planning

<domain>
## Phase Boundary

Operators can trace requests through the system with OpenTelemetry, browse the API with OpenAPI docs, and replicate data with Litestream. Three independent subsystems: OTel distributed tracing (feature-gated), OpenAPI spec generation with Swagger UI, and Litestream WAL replication support (config templates + CLI + SQLCipher incompatibility documentation).

</domain>

<decisions>
## Implementation Decisions

### OpenTelemetry Tracing

**Span Coverage:**
- Key spans only (5 required): agent loop iteration, LLM provider call, tool execution, memory retrieval, context assembly
- WASM skill execution also gets tracing spans (under "tool execution")
- Per-turn spans (one span per agent loop iteration, not per-session)
- Single span for full LLM streaming response (first token to last token)
- Rich attributes on all spans (model, provider, session_id, token counts, tool name, etc.)
- Full error details on error spans (otel.status = ERROR, error.type, error.message)

**Span Naming:**
- Dotted namespace convention: `blufio.agent.loop`, `blufio.llm.call`, `blufio.tool.execute`, `blufio.memory.retrieve`, `blufio.context.assemble`
- HTTP gateway spans use route-based names per OTel HTTP convention: `HTTP POST /v1/messages`
- OTel semantic conventions for attribute naming (gen_ai.request.model, gen_ai.usage.input_tokens)
- OTel GenAI semantic conventions for LLM-specific attributes (gen_ai.system, gen_ai.usage.*)

**Span Attributes (per span type):**
- Agent loop: blufio.session.id, blufio.channel (telegram/discord/etc.)
- LLM call: gen_ai.system (provider name), gen_ai.request.model, gen_ai.usage.input_tokens, gen_ai.usage.output_tokens
- Tool execution: tool_name, skill_name, signed (bool), fuel_consumed (WASM)
- Memory retrieval: blufio.memory.results_count, blufio.memory.top_score
- Context assembly: blufio.context.static_tokens, blufio.context.conditional_tokens, blufio.context.dynamic_tokens, blufio.context.total_tokens

**MCP Trace Propagation (OTEL-04):**
- W3C Traceparent header standard for propagation
- Span per MCP tool call (not per transport operation)
- Both directions: inject traceparent on outbound MCP client calls, extract on inbound MCP server requests
- MCP span attributes: tool_name, server_name, blufio.mcp.trust_zone -- metadata only, no input/output data
- opentelemetry-http crate for W3C traceparent injection/extraction

**Transport & Export:**
- OTLP HTTP export only (no gRPC) -- OTEL-02
- reqwest via opentelemetry-otlp reqwest-client feature (reuses existing workspace reqwest)
- Configurable sampling ratio (0.0-1.0, default 1.0)
- Configurable batch_timeout_ms (default 5000) and max_export_batch_size (default 512)
- Bounded span buffer (BatchSpanProcessor, default max queue 2048) -- follows "bounded everything"
- Configurable service_name (default "blufio")
- Configurable environment (default "production")
- Custom resource attributes via TOML key-value map
- X-Trace-Id response header on HTTP responses for API consumer debugging
- Honor incoming W3C traceparent headers (join distributed traces)

**Resource Attributes:**
- service.name (configurable), service.version (from Cargo), deployment.environment (configurable), host.name (from hostname), host.arch (auto-detected)
- Custom resource attributes configurable via [observability.opentelemetry.resource_attributes]

**Trace-Log Correlation:**
- trace_id and span_id injected into tracing log records when OTel active
- tracing-opentelemetry OpenTelemetryLayer handles this

**Initialization & Lifecycle:**
- OTel layer added in serve.rs startup (not main.rs globally) -- only active when serving
- Flush pending traces on graceful shutdown (tracer_provider.shutdown())
- Init failure: warn and disable tracing, never block startup
- OTLP endpoint unreachable: log warning, drop spans silently, never block agent loop
- Prometheus meta-metrics: otel_spans_exported_total, otel_spans_dropped_total, otel_export_errors_total

**Config:**
- Lives in [observability.opentelemetry] section of blufio.toml
- Parse-time validation for field types/ranges (sample_ratio 0.0-1.0)
- Startup verification of endpoint URL well-formedness and feature compilation
- Feature mismatch (config present, feature not compiled): warn at startup, non-fatal
- Hot reload supported for sample_ratio and endpoint changes (ArcSwap pattern)
- serde(default) on config section

**Doctor & Diagnostics:**
- Doctor check validates OTLP endpoint connectivity when feature compiled + config enabled
- Doctor reports compiled features section (otel: enabled/disabled, swagger-ui: enabled/disabled)

### OpenAPI Spec Generation

**Approach:**
- utoipa annotations inline on handler functions (not centralized spec file)
- OpenAPI 3.1 spec auto-generated from axum route definitions
- Latest stable utoipa version with axum integration
- Always compiled in (not feature-gated) -- annotations are lightweight

**Spec Content:**
- Full auth documentation (SecurityScheme for Bearer token, per-endpoint auth requirements)
- Example request/response bodies via #[schema(example = ...)]
- Tags to group endpoints by domain: Messages, Sessions, OpenAI Compatible, API Keys, Webhooks, Batch, Health
- SSE streaming endpoint documented with event format description (within OpenAPI limits)
- REST endpoints only (WebSocket documented as note, not spec'd)
- Spec version from env!("CARGO_PKG_VERSION")

**Endpoints:**
- /openapi.json always served (public, unauthenticated, like /health and /metrics)
- /docs serves Swagger UI (config-toggled, disabled by default)
- Config toggle lives in [gateway.openapi] section

**Swagger UI:**
- Bundled via utoipa-swagger-ui with axum feature for seamless router merging
- Feature-gated: `--features swagger-ui` Cargo feature (separate from otel)
- Disabled by default in config -- operators opt in

**Testing:**
- Snapshot test of /openapi.json via insta crate
- insta added to workspace dev dependencies

### Litestream WAL Replication

**Scope:**
- Config templates + docs only (no unencrypted DB mode)
- SQLCipher incompatibility documented with existing `blufio backup` + cron as recommended alternative

**CLI:**
- Top-level `blufio litestream` subcommand (init, status)
- `blufio litestream init`: generates litestream.yml alongside DB file
  - Checks if litestream binary installed, offers install guidance if not
  - S3-compatible replica stanza with placeholder credentials
  - Retention settings with sensible defaults (24h snapshots, 72h WAL segments)
  - Separate entries for both blufio.db and audit.db
- `blufio litestream status`: shells out to litestream CLI, parses output
- Integrated into `blufio doctor` as summary check

**Config:**
- LitestreamConfig added to blufio-config with serde(default)
- Auto-detect: if [litestream] enabled, set PRAGMA wal_autocheckpoint=0 on DB open
- Startup warning if litestream config exists AND SQLCipher active
- Application-level backup alternative: document existing `blufio backup` + cron scheduler

**Testing:**
- Mock litestream CLI output for tests (no real binary required)
- YAML parse validation of generated config templates
- Grafana dashboard JSON parse validation

### Feature Gate Design

**OTel Feature:**
- Single 'otel' Cargo feature on blufio binary crate, propagated via workspace features
- Opt-in: default `cargo build` has no OTel deps
- #[cfg(feature = "otel")] guards at instrumentation sites
- Helper macros (e.g., otel_span!()) to reduce #[cfg] boilerplate
- CI tests both modes: default AND --features otel

**Swagger UI Feature:**
- Separate 'swagger-ui' Cargo feature
- Feature-gated bundled assets (~2MB)

**Convenience:**
- 'full' feature enables all optional features (otel + swagger-ui)
- Docker image built with --features full by default
- Dockerfile accepts BUILD_FEATURES build arg for customization

**Binary Size:**
- Budget: up to +5MB default build, +10MB full build
- Binary size measured before/after in VERIFICATION.md
- cargo-bloat analysis to identify largest contributors
- Cut priority if over budget: Swagger UI first (largest, least critical)
- Features reported in `blufio --version` output (e.g., "blufio 1.5.0 (otel, swagger-ui)")

**Release & Update:**
- Release CI produces both variants: blufio-linux-amd64 (default) and blufio-linux-amd64-full
- Self-update detects compiled features, downloads matching variant

**Dependencies:**
- OTel deps in workspace [workspace.dependencies] (consistent with pattern)
- Latest stable opentelemetry (0.28+), opentelemetry-otlp, tracing-opentelemetry, opentelemetry-http
- Latest stable utoipa with axum integration, utoipa-swagger-ui with axum feature
- Pre-audit new dep licenses for cargo-deny before CI runs

### Config Recipe

- Observability recipe in `blufio config recipe` sets up Prometheus + OTel + Litestream
- Includes docker-compose snippet with Blufio + Jaeger all-in-one + Prometheus + Grafana with pre-configured dashboards
- Pre-configured prometheus.yml with scrape target for blufio /metrics
- Core operational Grafana dashboard: request rate, latency p50/p95/p99, active sessions, token usage, cost, memory, error rate, degradation level
- Recipe detects if otel feature compiled in, warns if not
- Output written to files (config additions + docker-compose-observability.yml)
- Grafana dashboard JSON validated in tests

### Claude's Discretion
- Exact OTel SDK configuration details (batch processor tuning, exporter timeouts)
- Litestream template YAML formatting and comments
- OpenAPI schema detail level per endpoint
- Grafana dashboard layout and panel arrangement
- Helper macro API design

</decisions>

<specifics>
## Specific Ideas

- OTel should coexist with Prometheus (OTEL-06): OTel for traces only, Prometheus for metrics
- "Observability never blocks core" -- all OTel failures are non-fatal
- Follow OTel GenAI semantic conventions for LLM attributes (emerging standard, compatible with Langfuse)
- No sensitive data in spans (tool inputs/outputs excluded, PII/classification concerns)
- Docker-compose observability recipe should feel like a one-command monitoring stack

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- blufio-prometheus: Existing Prometheus metrics adapter (metrics-rs facade + exporter). OTel must coexist, not replace.
- tracing crate: Already used across 15 files (36 occurrences). tracing-opentelemetry bridges naturally.
- blufio-gateway/src/server.rs: Gateway router with ~20+ route handlers needing OpenAPI annotations. HealthState, GatewayState already defined.
- blufio-gateway/src/handlers.rs: Request/response types (MessageRequest, MessageResponse, HealthResponse) ready for utoipa derive macros.
- blufio-config/src/model.rs: Config model with existing [observability] section for Prometheus. New [observability.opentelemetry] nests here.
- backup.rs: Existing backup system -- documented as SQLCipher-compatible Litestream alternative.
- blufio-resilience: Circuit breaker and degradation patterns available if needed.
- ArcSwap pattern from Phase 59 hot reload: Reusable for OTel config hot reload.

### Established Patterns
- Config sections use serde(default) + deny_unknown_fields (CronConfig, HookConfig, GdprConfig)
- EventBus events use String fields to avoid cross-crate deps
- Feature detection via cfg! macros at runtime
- CLI subcommands follow top-level pattern (backup, cron, gdpr)
- Doctor checks return HealthStatus with description
- Workspace deps in root Cargo.toml

### Integration Points
- serve.rs: OTel layer initialization after config load, before agent loop
- serve.rs: Swagger UI router merge (existing pattern for MCP router merge)
- server.rs: /openapi.json and /docs route registration on public router
- doctor.rs: New checks for OTel connectivity and Litestream status
- main.rs: Feature info in --version output
- Dockerfile: BUILD_FEATURES arg and multi-variant builds
- CI: Feature matrix for testing both default and otel builds
- config recipe: New observability recipe alongside existing recipes

</code_context>

<deferred>
## Deferred Ideas

None -- discussion stayed within phase scope

</deferred>

---

*Phase: 62-observability-api-surface*
*Context gathered: 2026-03-13*
