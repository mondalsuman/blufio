# Phase 62: Observability & API Surface - Research

**Researched:** 2026-03-13
**Domain:** OpenTelemetry tracing, OpenAPI spec generation, Litestream WAL replication
**Confidence:** HIGH

## Summary

Phase 62 implements three independent subsystems: (1) OpenTelemetry distributed tracing via the `tracing-opentelemetry` bridge with OTLP HTTP export, feature-gated behind a Cargo `otel` feature; (2) OpenAPI 3.1 spec auto-generation using `utoipa` annotations on existing axum handlers with optional Swagger UI; and (3) Litestream WAL replication support via CLI config templates and status checking, with SQLCipher incompatibility documentation.

The Rust OpenTelemetry ecosystem has stabilized at version 0.31.0 across all crates (`opentelemetry`, `opentelemetry-otlp`, `opentelemetry-sdk`, `tracing-opentelemetry`). The `utoipa` crate is at version 5.4.0 with full OpenAPI 3.1 and axum 0.8 support. Both ecosystems are mature and well-documented. The Litestream work is primarily config template generation and CLI commands -- no new library dependencies required.

**Primary recommendation:** Use opentelemetry 0.31.0 family with `reqwest-client` feature (reusing workspace reqwest), utoipa 5.4.0 with `axum_extras` feature, and utoipa-swagger-ui 9.0.2 with `axum` feature. All three subsystems are independent and can be implemented in parallel waves.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**OpenTelemetry Tracing:**
- Key spans only (5 required): agent loop iteration, LLM provider call, tool execution, memory retrieval, context assembly
- WASM skill execution gets tracing spans under "tool execution"
- Dotted namespace span naming: `blufio.agent.loop`, `blufio.llm.call`, `blufio.tool.execute`, `blufio.memory.retrieve`, `blufio.context.assemble`
- OTel GenAI semantic conventions for LLM attributes (gen_ai.request.model, gen_ai.usage.*)
- OTLP HTTP export only (no gRPC) via reqwest
- Single 'otel' Cargo feature on binary crate, opt-in
- #[cfg(feature = "otel")] guards at instrumentation sites with helper macros
- OTel layer added in serve.rs startup only
- W3C Traceparent header for MCP trace propagation (opentelemetry-http crate)
- Configurable sampling ratio, batch timeout, max batch size
- X-Trace-Id response header on HTTP responses
- trace_id and span_id injected into tracing log records via tracing-opentelemetry
- Init failure: warn and disable, never block startup
- Prometheus meta-metrics for OTel operations
- Config in [observability.opentelemetry] section
- Hot reload for sample_ratio and endpoint via ArcSwap

**OpenAPI Spec Generation:**
- utoipa annotations inline on handler functions
- OpenAPI 3.1 spec auto-generated
- Always compiled in (not feature-gated)
- /openapi.json always served (public, unauthenticated)
- /docs serves Swagger UI (config-toggled, disabled by default)
- Swagger UI feature-gated: `--features swagger-ui` separate Cargo feature
- Snapshot test of /openapi.json via insta crate
- Tags: Messages, Sessions, OpenAI Compatible, API Keys, Webhooks, Batch, Health
- Full auth documentation (SecurityScheme for Bearer token)
- Example request/response bodies

**Litestream WAL Replication:**
- Top-level `blufio litestream` subcommand (init, status)
- Config templates only (no unencrypted DB mode)
- SQLCipher incompatibility documented
- PRAGMA wal_autocheckpoint=0 when Litestream enabled
- Mock litestream CLI output for tests

**Feature Gate Design:**
- 'otel' + 'swagger-ui' separate features
- 'full' convenience feature enables all
- Docker built with --features full
- Binary size budget: +5MB default, +10MB full
- Features reported in `blufio --version`
- CI tests both default AND --features otel

**Config Recipe:**
- Observability recipe with docker-compose snippet (Blufio + Jaeger + Prometheus + Grafana)
- Pre-configured dashboards

### Claude's Discretion
- Exact OTel SDK configuration details (batch processor tuning, exporter timeouts)
- Litestream template YAML formatting and comments
- OpenAPI schema detail level per endpoint
- Grafana dashboard layout and panel arrangement
- Helper macro API design

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| OTEL-01 | OpenTelemetry tracing via tracing-opentelemetry bridge, feature-gated (disabled by default) | opentelemetry 0.31.0 + tracing-opentelemetry 0.32.1 + OpenTelemetryLayer; feature gate via Cargo `otel` feature with cfg guards |
| OTEL-02 | OTLP export (HTTP mode) to configurable endpoint | opentelemetry-otlp 0.31.0 with `http-proto` + `reqwest-client` features; SpanExporter::builder().with_http() API |
| OTEL-03 | Key spans: agent loop iteration, LLM provider call, tool execution, memory retrieval, context assembly | tracing::instrument or manual span creation with OTel attributes; GenAI semantic conventions for LLM spans |
| OTEL-04 | Trace context propagation through MCP calls | opentelemetry-http 0.31.0 HeaderInjector/HeaderExtractor; TraceContextPropagator for W3C traceparent |
| OTEL-05 | Zero overhead when disabled (feature-gate at compile time) | #[cfg(feature = "otel")] eliminates all OTel code paths; helper macros reduce boilerplate |
| OTEL-06 | Coexists with existing Prometheus metrics (OTel for traces only, Prometheus for metrics) | OTel TracerProvider is separate from metrics-rs recorder; no conflict -- OTel traces, Prometheus metrics |
| OAPI-01 | OpenAPI 3.1 spec auto-generated from axum route definitions via utoipa annotations | utoipa 5.4.0 with #[utoipa::path] macro and #[derive(ToSchema)] on types |
| OAPI-02 | Spec served at /openapi.json endpoint | Handler serving ApiDoc::openapi().to_pretty_json(); route on public_routes (unauthenticated) |
| OAPI-03 | Optional Swagger UI served at /docs when enabled in config | utoipa-swagger-ui 9.0.2 with `axum` feature; SwaggerUi::new("/docs"); config-gated + feature-gated |
| OAPI-04 | All existing gateway endpoints annotated with request/response schemas | 20+ handlers in blufio-gateway need #[utoipa::path] + ToSchema derives on types |
| LITE-01 | Litestream config template generation via `blufio litestream init` | CLI subcommand generating litestream.yml with S3-compatible replica stanza |
| LITE-02 | `blufio litestream status` checks replication lag | CLI subcommand shelling out to litestream binary, parsing output |
| LITE-03 | Documentation of SQLCipher incompatibility with mitigation | Litestream cannot read SQLCipher encrypted WAL; document `blufio backup` + cron as alternative |
| LITE-04 | WAL autocheckpoint disabled when Litestream mode active | PRAGMA wal_autocheckpoint=0 set on DB open when [litestream].enabled = true |
</phase_requirements>

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| opentelemetry | 0.31.0 | Core OTel API (TraceId, SpanId, KeyValue, propagation) | Official Rust OTel implementation, Apache-2.0 |
| opentelemetry-sdk | 0.31.0 | SdkTracerProvider, BatchSpanProcessor, resource attributes | Runtime SDK paired with API crate |
| opentelemetry-otlp | 0.31.0 | OTLP HTTP exporter (SpanExporter) | Standard OTLP export, reqwest HTTP client |
| tracing-opentelemetry | 0.32.1 | OpenTelemetryLayer bridge from tracing to OTel | Bridges existing tracing crate to OTel spans |
| opentelemetry-http | 0.31.0 | HeaderInjector/HeaderExtractor for W3C traceparent | HTTP propagation utilities |
| utoipa | 5.4.0 | OpenAPI 3.1 spec generation from annotations | De facto Rust OpenAPI crate, axum support |
| utoipa-swagger-ui | 9.0.2 | Swagger UI serving (axum router merge) | Official companion crate to utoipa |
| insta | latest | Snapshot testing for OpenAPI spec | Standard Rust snapshot testing |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| opentelemetry-semantic-conventions | 0.31.0 | Predefined attribute constants (if available) | For semantic convention attribute names |
| serde_json | 1 (workspace) | OpenAPI JSON serialization, Grafana dashboard validation | Already in workspace |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| utoipa | aide | aide has less mature axum 0.8 support; utoipa is dominant ecosystem choice |
| opentelemetry-otlp gRPC | opentelemetry-otlp HTTP | HTTP avoids tonic/protobuf dep tree; decision locked to HTTP-only |
| Custom OpenAPI generation | Manual spec file | Annotations stay in sync with code; manual spec drifts |

**Installation (workspace Cargo.toml additions):**
```toml
# OpenTelemetry family (all behind otel feature)
opentelemetry = { version = "0.31", default-features = false, features = ["trace"] }
opentelemetry-sdk = { version = "0.31", default-features = false, features = ["trace", "rt-tokio"] }
opentelemetry-otlp = { version = "0.31", default-features = false, features = ["trace", "http-proto", "reqwest-client", "reqwest-rustls-webpki-roots"] }
tracing-opentelemetry = "0.32"
opentelemetry-http = "0.31"

# OpenAPI (always compiled)
utoipa = { version = "5.4", features = ["axum_extras"] }
utoipa-swagger-ui = { version = "9", features = ["axum"] }

# Snapshot testing (dev-dependencies)
insta = { version = "1", features = ["json"] }
```

## Architecture Patterns

### Recommended Project Structure
```
crates/
├── blufio/src/
│   ├── serve.rs           # OTel layer init, Swagger UI router merge, shutdown flush
│   ├── main.rs            # Feature info in --version, Litestream subcommand
│   ├── doctor.rs          # OTel connectivity check, Litestream status check
│   ├── litestream.rs      # NEW: blufio litestream init/status subcommand
│   └── otel.rs            # NEW: OTel initialization, helper macros, config
├── blufio-config/src/
│   └── model.rs           # OpenTelemetryConfig, LitestreamConfig, OpenApiConfig sections
├── blufio-gateway/src/
│   ├── server.rs          # /openapi.json route, /docs Swagger UI route
│   ├── handlers.rs        # #[utoipa::path] annotations + ToSchema derives
│   ├── openapi.rs         # NEW: OpenApi doc struct, tag definitions
│   └── openai_compat/     # All handlers get utoipa annotations
├── blufio-agent/src/
│   └── session.rs         # OTel span instrumentation (agent loop, LLM call, tool exec)
├── blufio-memory/src/
│   └── retriever.rs       # OTel span for memory retrieval
├── blufio-context/src/
│   └── engine.rs          # OTel span for context assembly
└── blufio-mcp-client/src/
    └── external_tool.rs   # W3C traceparent injection on outbound MCP calls
```

### Pattern 1: Feature-Gated OTel Initialization

**What:** OTel layer conditionally compiled and added to tracing subscriber
**When to use:** serve.rs init_tracing function
**Example:**
```rust
// Current init_tracing signature changes to support layered subscriber
fn init_tracing(log_level: &str, _otel_config: &OpenTelemetryConfig) -> ... {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("blufio={log_level},warn")));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_names(false)
        .with_writer(redacting_writer);

    // Build subscriber with optional OTel layer
    #[cfg(feature = "otel")]
    {
        if let Some(otel_layer) = crate::otel::try_init_otel_layer(_otel_config) {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .with(otel_layer)
                .init();
        } else {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .init();
        }
    }
    #[cfg(not(feature = "otel"))]
    {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .init();
    }
}
```

**Critical note:** The current `init_tracing` uses `tracing_subscriber::fmt().init()` which is a convenience shorthand. Adding an OTel layer requires switching to the `tracing_subscriber::registry()` layered approach with `.with()` combinators. This is a required refactor -- the fmt shorthand cannot compose with additional layers.

### Pattern 2: OTel Helper Macro for Zero-Overhead Guards

**What:** Macros that compile to no-ops when otel feature is disabled
**When to use:** All instrumentation sites (session.rs, retriever.rs, engine.rs)
**Example:**
```rust
/// Create an OTel-enriched span. Compiles to a standard tracing span
/// when otel feature is disabled (still useful for structured logging).
#[cfg(feature = "otel")]
macro_rules! otel_span {
    ($name:expr, $($key:expr => $val:expr),* $(,)?) => {{
        let span = tracing::info_span!($name, $($key = %$val),*);
        span
    }};
}

#[cfg(not(feature = "otel"))]
macro_rules! otel_span {
    ($name:expr, $($key:expr => $val:expr),* $(,)?) => {{
        // No-op: tracing span still created for structured logging
        let span = tracing::info_span!($name);
        span
    }};
}
```

**Note:** The tracing spans themselves are always useful for structured logging. The difference is that with OTel enabled, the OpenTelemetryLayer converts them into OTel spans with attributes exported to OTLP. Without OTel, they remain standard tracing spans visible in logs only.

### Pattern 3: utoipa OpenAPI Doc Aggregation

**What:** Central OpenApi struct collecting all handler annotations
**When to use:** blufio-gateway openapi.rs module
**Example:**
```rust
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::post_messages,
        handlers::get_health,
        handlers::get_sessions,
        handlers::get_public_health,
        handlers::get_public_metrics,
        openai_compat::handlers::post_chat_completions,
        openai_compat::handlers::get_models,
        openai_compat::responses::post_responses,
        openai_compat::tools::get_tools,
        openai_compat::tools::post_tool_invoke,
        api_keys::handlers::post_create_api_key,
        api_keys::handlers::get_list_api_keys,
        api_keys::handlers::delete_api_key,
        webhooks::handlers::post_create_webhook,
        webhooks::handlers::get_list_webhooks,
        webhooks::handlers::delete_webhook,
        batch::handlers::post_create_batch,
        batch::handlers::get_batch_status,
    ),
    components(schemas(
        MessageRequest, MessageResponse, HealthResponse,
        SessionListResponse, SessionInfo, ErrorResponse,
        // ... all request/response types
    )),
    tags(
        (name = "Messages", description = "Message exchange API"),
        (name = "Sessions", description = "Session management"),
        (name = "OpenAI Compatible", description = "OpenAI-compatible endpoints"),
        (name = "API Keys", description = "API key management"),
        (name = "Webhooks", description = "Webhook management"),
        (name = "Batch", description = "Batch processing"),
        (name = "Health", description = "Health and monitoring"),
    ),
    security(
        ("bearer_auth" = [])
    ),
    modifiers(&SecurityAddon),
    info(
        title = "Blufio API",
        version = env!("CARGO_PKG_VERSION"),
        description = "Always-on personal AI agent REST API"
    )
)]
pub struct ApiDoc;
```

### Pattern 4: Litestream CLI Subcommand

**What:** Top-level `blufio litestream` subcommand following existing CLI patterns
**When to use:** main.rs Commands enum
**Example:**
```rust
/// Litestream WAL replication management.
Litestream {
    #[command(subcommand)]
    command: LitestreamCommands,
},

#[derive(Subcommand, Debug)]
enum LitestreamCommands {
    /// Generate Litestream config template alongside database file.
    Init,
    /// Check Litestream replication status and lag.
    Status,
}
```

### Anti-Patterns to Avoid

- **Never block agent loop on OTel failure:** All OTel operations must be non-blocking. BatchSpanProcessor runs on a background thread. If OTLP endpoint is unreachable, spans are dropped silently.
- **Never put sensitive data in spans:** No tool inputs/outputs, no message content, no PII. Only metadata (model name, token counts, tool name, session ID).
- **Never make OpenAPI spec manual:** Spec must be generated from code annotations. Manual spec files drift from actual API.
- **Never use gRPC for OTLP:** Decision locked to HTTP-only. gRPC adds tonic + protobuf dep tree.
- **Never couple OTel initialization to main.rs:** OTel is only active during `serve` command, not for CLI tools.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| OTel trace export | Custom HTTP exporter | opentelemetry-otlp SpanExporter | Handles batching, retries, protobuf encoding |
| Tracing-to-OTel bridge | Manual span conversion | tracing-opentelemetry OpenTelemetryLayer | Automatic conversion with zero manual mapping |
| W3C traceparent parsing | Custom header parser | opentelemetry-http HeaderExtractor | W3C spec-compliant parsing with edge cases handled |
| OpenAPI spec generation | Manual JSON/YAML spec | utoipa derive macros | Stays in sync with code, compile-time validation |
| Swagger UI serving | Custom static file hosting | utoipa-swagger-ui | Pre-bundled assets, hot-linked to spec endpoint |
| Batch span processing | Custom queue + export loop | opentelemetry-sdk BatchSpanProcessor | Background thread, bounded queue, configurable batch size |
| Litestream YAML generation | Manual string formatting | serde_yaml or inline template | Structured YAML generation avoids formatting bugs |

**Key insight:** The OTel Rust ecosystem is specifically designed for the tracing crate bridge pattern. The project already uses `tracing` across 15+ files -- the OpenTelemetryLayer automatically converts all existing tracing spans into OTel spans. No manual span creation is needed for the bridge.

## Common Pitfalls

### Pitfall 1: init_tracing Refactor Required
**What goes wrong:** Current `tracing_subscriber::fmt().init()` shorthand cannot compose with additional layers (OTel). Attempting to add OTel without refactoring causes compilation errors or double-initialization panics.
**Why it happens:** The `.init()` convenience method installs a global subscriber. The layered approach with `registry()` + `.with()` is needed for multiple layers.
**How to avoid:** Refactor init_tracing to use `tracing_subscriber::registry()` with `.with(fmt_layer).with(otel_layer)` pattern. This is backward-compatible -- fmt_layer replicates current behavior exactly.
**Warning signs:** Panic at runtime "a global default trace dispatcher has already been set"

### Pitfall 2: OTel SDK Version Alignment
**What goes wrong:** Mixing opentelemetry 0.28 with opentelemetry-otlp 0.31 causes trait bound mismatches and confusing compiler errors.
**Why it happens:** OTel Rust crates are released in lockstep. All must be the same minor version.
**How to avoid:** Pin all opentelemetry-* crates to 0.31.x in workspace dependencies. Use tracing-opentelemetry 0.32.x which is the matched version.
**Warning signs:** Trait bound errors mentioning `SpanExporter` or `TracerProvider`

### Pitfall 3: reqwest Feature Conflict
**What goes wrong:** opentelemetry-otlp brings its own reqwest with different features. Duplicate reqwest versions in lockfile.
**Why it happens:** Workspace reqwest is 0.13 with specific features. OTel's reqwest-client feature also depends on reqwest.
**How to avoid:** Use `reqwest-client` (not `reqwest-blocking-client`) feature which uses async reqwest compatible with workspace 0.13. Ensure `reqwest-rustls-webpki-roots` for TLS (matching workspace's `rustls` feature). Do NOT enable default features on opentelemetry-otlp.
**Warning signs:** Multiple reqwest versions in cargo tree, or TLS errors at runtime

### Pitfall 4: BatchSpanProcessor Shutdown Deadlock
**What goes wrong:** Calling `tracer_provider.shutdown()` from the tokio runtime's main thread on a current-thread runtime causes deadlock.
**Why it happens:** BatchSpanProcessor's shutdown is a blocking operation that waits for the background thread to flush.
**How to avoid:** Call shutdown in a dedicated tokio::task::spawn_blocking or ensure multi-thread runtime (which blufio uses). Place shutdown after agent_loop.run() returns, before audit trail cleanup.
**Warning signs:** Process hangs during graceful shutdown

### Pitfall 5: Swagger UI Binary Size
**What goes wrong:** utoipa-swagger-ui bundles ~2MB of JavaScript/CSS assets, inflating binary size.
**Why it happens:** Swagger UI assets are compiled into the binary.
**How to avoid:** Feature-gate behind `swagger-ui` Cargo feature. Measure binary size impact. The decision doc already accounts for this with the +5MB/+10MB budget and cut priority.
**Warning signs:** Binary size exceeds budget after adding swagger-ui feature

### Pitfall 6: utoipa Axum Version Mismatch
**What goes wrong:** utoipa-swagger-ui or utoipa-axum expects axum 0.7, but workspace uses axum 0.8.
**Why it happens:** Version lag between utoipa ecosystem releases.
**How to avoid:** Use utoipa 5.4.0+ which supports axum 0.8. Check `axum_extras` feature requirement. utoipa-swagger-ui 9.0.2 states "axum version >=0.7" -- axum 0.8 is compatible.
**Warning signs:** Trait bound errors on Router or State types

### Pitfall 7: OpenAPI Annotation on Handlers with Complex State
**What goes wrong:** Axum handlers using `State<GatewayState>` may not directly satisfy utoipa's state constraints.
**Why it happens:** utoipa needs to extract type information at compile time; complex state types need explicit schema exclusion.
**How to avoid:** State extractors don't need ToSchema derives -- only request body (Json<T>) and response types need it. Path/Query params use IntoParams. State is invisible to OpenAPI.
**Warning signs:** Compile errors about ToSchema on GatewayState

### Pitfall 8: Litestream Binary Detection
**What goes wrong:** `blufio litestream status` fails because litestream binary is not in PATH.
**Why it happens:** Litestream is a Go sidecar binary, not bundled with blufio.
**How to avoid:** Check for litestream binary existence before shelling out. Provide helpful error message with install instructions. Mock the binary for tests.
**Warning signs:** Command execution errors in CI/testing

## Code Examples

### OTel TracerProvider Setup (otel.rs module)
```rust
// Source: opentelemetry-otlp 0.31.0 docs + opentelemetry-sdk 0.31.0 docs
use opentelemetry::KeyValue;
use opentelemetry_otlp::SpanExporter;
use opentelemetry_sdk::trace::{BatchSpanProcessor, SdkTracerProvider, BatchConfigBuilder};
use opentelemetry_sdk::Resource;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::Registry;

pub fn try_init_otel_layer(
    config: &OpenTelemetryConfig,
) -> Option<OpenTelemetryLayer<Registry, opentelemetry_sdk::trace::SdkTracer>> {
    if !config.enabled {
        return None;
    }

    // Build OTLP HTTP exporter
    let exporter = SpanExporter::builder()
        .with_http()
        .with_endpoint(&config.endpoint) // e.g., "http://localhost:4318"
        .build()
        .map_err(|e| {
            tracing::warn!(error = %e, "failed to create OTLP exporter, disabling OTel");
        })
        .ok()?;

    // Build resource with service info
    let resource = Resource::builder()
        .with_service_name(config.service_name.clone())
        .with_attributes(vec![
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
            KeyValue::new("deployment.environment", config.environment.clone()),
        ])
        .build();

    // Build batch processor with configured limits
    let batch_config = BatchConfigBuilder::default()
        .with_max_queue_size(config.max_queue_size) // default 2048
        .with_max_export_batch_size(config.max_export_batch_size) // default 512
        .with_scheduled_delay(std::time::Duration::from_millis(config.batch_timeout_ms))
        .build();

    let processor = BatchSpanProcessor::builder(exporter)
        .with_batch_config(batch_config)
        .build();

    // Build TracerProvider
    let provider = SdkTracerProvider::builder()
        .with_span_processor(processor)
        .with_resource(resource)
        .build();

    // Set as global provider (for propagation utilities)
    let tracer = opentelemetry::global::tracer("blufio");

    // Set W3C TraceContext propagator
    opentelemetry::global::set_text_map_propagator(
        opentelemetry_sdk::propagation::TraceContextPropagator::new()
    );

    Some(OpenTelemetryLayer::new(tracer))
}
```

### W3C Traceparent Injection for MCP Calls
```rust
// Source: opentelemetry-http 0.31.0 docs + OTel propagation docs
use opentelemetry::global;
use opentelemetry::propagation::Injector;

struct HeaderMapInjector<'a> {
    headers: &'a mut reqwest::header::HeaderMap,
}

impl<'a> Injector for HeaderMapInjector<'a> {
    fn set(&mut self, key: &str, value: String) {
        if let Ok(name) = reqwest::header::HeaderName::from_bytes(key.as_bytes()) {
            if let Ok(val) = reqwest::header::HeaderValue::from_str(&value) {
                self.headers.insert(name, val);
            }
        }
    }
}

// In MCP client call site:
fn inject_trace_context(headers: &mut reqwest::header::HeaderMap) {
    #[cfg(feature = "otel")]
    {
        global::get_text_map_propagator(|propagator| {
            propagator.inject_context(
                &opentelemetry::Context::current(),
                &mut HeaderMapInjector { headers },
            );
        });
    }
}
```

### utoipa Handler Annotation
```rust
// Source: utoipa 5.4.0 docs
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub struct MessageRequest {
    /// Message content text.
    #[schema(example = "Hello, how are you?")]
    pub content: String,
    /// Optional session ID to continue an existing session.
    #[schema(example = "sess_abc123")]
    #[serde(default)]
    pub session_id: Option<String>,
    /// Optional sender identifier.
    #[serde(default)]
    pub sender_id: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MessageResponse {
    /// Request/message ID.
    #[schema(example = "msg_xyz789")]
    pub id: String,
    /// Response content from the agent.
    pub content: String,
    /// Session ID (may be newly created).
    pub session_id: Option<String>,
    /// ISO 8601 timestamp.
    #[schema(example = "2026-03-13T10:00:00Z")]
    pub created_at: String,
}

/// POST /v1/messages
#[utoipa::path(
    post,
    path = "/v1/messages",
    tag = "Messages",
    request_body = MessageRequest,
    responses(
        (status = 200, description = "Message processed", body = MessageResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 401, description = "Unauthorized"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn post_messages(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Json(request): Json<MessageRequest>,
) -> impl IntoResponse {
    // ... existing implementation unchanged
}
```

### OpenAPI JSON Serving Route
```rust
// Source: utoipa 5.4.0 docs
use utoipa::OpenApi;

// In server.rs, add to public_routes:
async fn get_openapi_json() -> impl IntoResponse {
    let spec = crate::openapi::ApiDoc::openapi().to_pretty_json()
        .unwrap_or_else(|_| "{}".to_string());
    (
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        spec,
    )
}

// In public_routes builder:
let public_routes = Router::new()
    .route("/health", get(handlers::get_public_health))
    .route("/metrics", get(handlers::get_public_metrics))
    .route("/openapi.json", get(get_openapi_json))
    .with_state(state.clone());

// Swagger UI (feature-gated):
#[cfg(feature = "swagger-ui")]
{
    if config.gateway.openapi.swagger_ui_enabled {
        app = app.merge(
            utoipa_swagger_ui::SwaggerUi::new("/docs")
                .url("/openapi.json", crate::openapi::ApiDoc::openapi())
        );
    }
}
```

### Config Sections
```rust
// Source: existing blufio-config pattern (serde(default) + deny_unknown_fields)

/// OpenTelemetry tracing configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct OpenTelemetryConfig {
    /// Enable OpenTelemetry tracing (requires otel feature compiled in).
    pub enabled: bool,
    /// OTLP HTTP endpoint URL.
    pub endpoint: String,
    /// Trace sampling ratio (0.0 = none, 1.0 = all).
    pub sample_ratio: f64,
    /// Service name reported in traces.
    pub service_name: String,
    /// Deployment environment.
    pub environment: String,
    /// Batch export timeout in milliseconds.
    pub batch_timeout_ms: u64,
    /// Maximum spans per export batch.
    pub max_export_batch_size: usize,
    /// Maximum span queue size.
    pub max_queue_size: usize,
    /// Custom resource attributes.
    pub resource_attributes: std::collections::HashMap<String, String>,
}

impl Default for OpenTelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "http://localhost:4318".to_string(),
            sample_ratio: 1.0,
            service_name: "blufio".to_string(),
            environment: "production".to_string(),
            batch_timeout_ms: 5000,
            max_export_batch_size: 512,
            max_queue_size: 2048,
            resource_attributes: std::collections::HashMap::new(),
        }
    }
}

/// Litestream WAL replication configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct LitestreamConfig {
    /// Enable Litestream integration (sets PRAGMA wal_autocheckpoint=0).
    pub enabled: bool,
}

impl Default for LitestreamConfig {
    fn default() -> Self {
        Self { enabled: false }
    }
}

/// OpenAPI configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct OpenApiConfig {
    /// Enable Swagger UI at /docs (requires swagger-ui feature).
    pub swagger_ui_enabled: bool,
}

impl Default for OpenApiConfig {
    fn default() -> Self {
        Self { swagger_ui_enabled: false }
    }
}
```

### Litestream YAML Template Generation
```rust
// Generated litestream.yml template
fn generate_litestream_config(db_path: &str, audit_db_path: &str) -> String {
    format!(r#"# Litestream configuration for Blufio
# Generated by: blufio litestream init
#
# WARNING: Litestream is INCOMPATIBLE with SQLCipher encrypted databases.
# If encryption is enabled, use `blufio backup` + cron instead.
# See: https://github.com/benbjohnson/litestream/issues/177

dbs:
  - path: {db_path}
    replicas:
      - type: s3
        bucket: your-bucket-name
        path: blufio/main
        region: us-east-1
        access-key-id: YOUR_ACCESS_KEY
        secret-access-key: YOUR_SECRET_KEY
        retention: 72h
        snapshot-interval: 24h

  - path: {audit_db_path}
    replicas:
      - type: s3
        bucket: your-bucket-name
        path: blufio/audit
        region: us-east-1
        access-key-id: YOUR_ACCESS_KEY
        secret-access-key: YOUR_SECRET_KEY
        retention: 168h
        snapshot-interval: 24h
"#, db_path = db_path, audit_db_path = audit_db_path)
}
```

### OpenAPI Snapshot Test
```rust
// Source: insta crate docs
#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_json_snapshot;

    #[test]
    fn openapi_spec_snapshot() {
        let spec = ApiDoc::openapi();
        let json: serde_json::Value = serde_json::from_str(
            &spec.to_pretty_json().unwrap()
        ).unwrap();
        assert_json_snapshot!("openapi_spec", json);
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| opentelemetry 0.20-0.27 | opentelemetry 0.31.0 | 2025 | SdkTracerProvider replaces TracerProvider; BatchSpanProcessor uses background thread (not async runtime) |
| Separate gRPC/HTTP features | Unified exporter builder | 0.28+ | SpanExporter::builder().with_http() replaces separate constructors |
| gen_ai.prompt / gen_ai.completion | Structured message events | OTel semconv v1.38.0 (Dec 2025) | Old attributes deprecated; use gen_ai.usage.* for token counts, avoid gen_ai.prompt |
| utoipa 4.x | utoipa 5.4.0 | 2025 | Automatic schema collection, improved axum 0.8 support |
| Manual Swagger UI hosting | utoipa-swagger-ui 9.x | 2025 | Single .merge() call for axum router integration |

**Deprecated/outdated:**
- `opentelemetry-jaeger` crate: Deprecated in favor of OTLP export to Jaeger (Jaeger now accepts OTLP natively)
- `gen_ai.prompt` / `gen_ai.completion` span attributes: Deprecated in semconv v1.38.0, replaced by structured events
- `TracerProvider` (old API): Replaced by `SdkTracerProvider` in opentelemetry-sdk 0.31

## Open Questions

1. **Exact opentelemetry-semantic-conventions crate version**
   - What we know: The OTel Rust repo has this crate for predefined attribute constants
   - What's unclear: Whether 0.31.0 version exists and includes GenAI conventions
   - Recommendation: Check crate availability; if GenAI conventions aren't in the crate, define constants manually (they're just string literals like `"gen_ai.request.model"`)

2. **reqwest feature unification**
   - What we know: Workspace uses reqwest 0.13 with `json`, `rustls`, `stream`. opentelemetry-otlp needs `reqwest-client` + `reqwest-rustls-webpki-roots`
   - What's unclear: Whether these unify to a single reqwest version in Cargo.lock without conflicts
   - Recommendation: Run `cargo tree -d` after adding deps to verify no duplicate reqwest versions. If conflict occurs, use `reqwest-blocking-client` as fallback.

3. **insta snapshot stability across utoipa versions**
   - What we know: OpenAPI spec snapshots capture the full JSON output
   - What's unclear: Whether utoipa version bumps cause snapshot churn due to field ordering changes
   - Recommendation: Use `insta::Settings::sort_maps(true)` for deterministic JSON output; accept that major utoipa updates may require snapshot updates

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in) + insta for snapshots |
| Config file | workspace Cargo.toml (test profiles) |
| Quick run command | `cargo test -p blufio-gateway --lib` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| OTEL-01 | OTel layer initializes with feature gate | unit | `cargo test -p blufio --lib otel --features otel` | Wave 0 |
| OTEL-02 | OTLP HTTP exporter builds successfully | unit | `cargo test -p blufio --lib otel --features otel` | Wave 0 |
| OTEL-03 | Key spans have correct names and attributes | unit | `cargo test -p blufio-agent --lib -- otel --features otel` | Wave 0 |
| OTEL-04 | Traceparent injected on MCP calls | unit | `cargo test -p blufio-mcp-client --lib -- trace --features otel` | Wave 0 |
| OTEL-05 | No OTel deps in default build | smoke | `cargo build 2>&1 && cargo tree -p blufio \| grep -c opentelemetry` | Wave 0 |
| OTEL-06 | Prometheus + OTel coexist | integration | `cargo test -p blufio --lib -- prometheus_otel --features otel,prometheus` | Wave 0 |
| OAPI-01 | OpenAPI spec generated from annotations | unit | `cargo test -p blufio-gateway --lib -- openapi` | Wave 0 |
| OAPI-02 | /openapi.json served at endpoint | unit | `cargo test -p blufio-gateway --lib -- openapi_json` | Wave 0 |
| OAPI-03 | Swagger UI served at /docs | unit | `cargo test -p blufio-gateway --lib -- swagger` | Wave 0 |
| OAPI-04 | All handlers annotated (snapshot) | snapshot | `cargo test -p blufio-gateway --lib -- openapi_spec_snapshot` | Wave 0 |
| LITE-01 | Litestream init generates valid YAML | unit | `cargo test -p blufio --lib -- litestream_init` | Wave 0 |
| LITE-02 | Litestream status parses mock output | unit | `cargo test -p blufio --lib -- litestream_status` | Wave 0 |
| LITE-03 | SQLCipher warning emitted | unit | `cargo test -p blufio --lib -- litestream_sqlcipher` | Wave 0 |
| LITE-04 | WAL autocheckpoint pragma set | unit | `cargo test -p blufio-config --lib -- litestream` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --workspace --lib` (quick, lib tests only)
- **Per wave merge:** `cargo test --workspace` (full suite)
- **Phase gate:** Full suite green + `cargo test --workspace --features otel` before verify

### Wave 0 Gaps
- [ ] OTel test infrastructure: tests need `--features otel` CI matrix entry
- [ ] insta crate added to workspace dev-dependencies
- [ ] Snapshot file directory (crates/blufio-gateway/src/snapshots/)
- [ ] CI workflow updated for otel + swagger-ui feature testing

## Sources

### Primary (HIGH confidence)
- [opentelemetry 0.31.0 docs](https://docs.rs/opentelemetry/0.31.0) - core API, propagation module
- [opentelemetry-otlp 0.31.0 docs](https://docs.rs/opentelemetry-otlp/0.31.0) - OTLP exporter API, feature flags
- [opentelemetry-sdk 0.31.0 docs](https://docs.rs/opentelemetry_sdk/0.31.0) - SdkTracerProvider, BatchSpanProcessor, BatchConfig
- [opentelemetry-http 0.31.0 docs](https://docs.rs/opentelemetry-http/0.31.0) - HeaderInjector, HeaderExtractor
- [tracing-opentelemetry 0.32.1 docs](https://docs.rs/tracing-opentelemetry/0.32.1) - OpenTelemetryLayer setup
- [utoipa 5.4.0 docs](https://docs.rs/utoipa/5.4.0) - OpenAPI annotation macros, ToSchema derive
- [utoipa-swagger-ui 9.0.2 docs](https://docs.rs/utoipa-swagger-ui/9.0.2) - SwaggerUi axum router merge
- [OTel GenAI Semantic Conventions](https://opentelemetry.io/docs/specs/semconv/gen-ai/gen-ai-spans/) - Span attributes, naming conventions
- [Litestream SQLCipher issue #177](https://github.com/benbjohnson/litestream/issues/177) - Confirmed incompatibility

### Secondary (MEDIUM confidence)
- [Uptrace OTel Rust propagation guide](https://uptrace.dev/get/opentelemetry-rust/propagation) - W3C traceparent injection/extraction patterns
- [OTel Rust exporters guide](https://opentelemetry.io/docs/languages/rust/exporters/) - OTLP setup patterns
- [utoipa GitHub examples](https://github.com/juhaku/utoipa) - axum integration examples
- [insta snapshot testing](https://insta.rs/) - Snapshot test macros

### Tertiary (LOW confidence)
- GenAI semantic conventions Rust crate availability: May need manual attribute constant definitions if crate doesn't include GenAI conventions yet

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All crate versions verified via docs.rs, APIs confirmed from official documentation
- Architecture: HIGH - Integration patterns well-understood; existing tracing infrastructure makes bridge straightforward
- Pitfalls: HIGH - init_tracing refactor, version alignment, and reqwest conflict are well-known issues documented in OTel Rust community
- OpenAPI: HIGH - utoipa is mature with clear axum 0.8 support
- Litestream: HIGH - SQLCipher incompatibility confirmed via official GitHub issue; CLI template pattern follows existing backup/cron commands

**Research date:** 2026-03-13
**Valid until:** 2026-04-13 (stable ecosystem, 30-day validity)
