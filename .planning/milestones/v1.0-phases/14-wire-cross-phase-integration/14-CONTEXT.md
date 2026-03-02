# Phase 14: Wire Cross-Phase Integration - Context

**Gathered:** 2026-03-02
**Status:** Ready for planning

<domain>
## Phase Boundary

Wire three cross-phase integration points identified by the v1.0 re-audit: use the secure TLS client for Anthropic API calls, install secret redaction in the tracing subscriber, and add Prometheus business metric call sites in agent code. Closes integration gaps INT-01, INT-02, INT-03 and flow gaps FLOW-01, FLOW-02.

</domain>

<decisions>
## Implementation Decisions

### Metric Instrumentation Depth
- Wire ALL existing recording functions: messages_total, tokens_total, errors_total, active_sessions, budget_remaining, response_latency, memory gauges
- Token metrics use per-model labels (e.g., claude-sonnet-4, claude-haiku) for cost distribution visibility
- Measure BOTH LLM API call latency AND full message round-trip latency (two separate histogram metrics)
- Error metrics labeled by error type matching BlufioError variants: 'provider', 'security', 'storage', 'agent'

### Redaction Sensitivity Scope
- Redact Anthropic API keys (sk-ant-* pattern), Telegram bot tokens, and all credential vault stored values
- NOT redacting session IDs, user identifiers, or message content — focused on SEC-08/SEC-09 requirements
- Feed vault values to RedactingWriter via shared Arc<RwLock<Vec<String>>> — dynamic updates when vault decrypts new secrets
- Redaction happens at the fmt writer level — structured tracing fields are covered automatically in formatted output
- Replacement is uniform `[REDACTED]` with no type metadata leakage

### Failure Behavior
- Secure TLS client failure: REFUSE TO START — no insecure fallback (SEC requirements mandate TLS)
- Redaction writer failure: REFUSE TO START — secrets could leak to logs without redaction
- Prometheus metrics failure: WARN AND CONTINUE — observability is nice-to-have, not a hard requirement (matches existing serve.rs behavior)
- Validate all three integration points together at startup and log a unified summary: "Security: OK | Redaction: OK | Metrics: WARN (disabled)"

### Backward Compatibility
- Use existing `allowed_private_ips` config for users running local API proxies — no new config surface needed
- Anthropic client retains its own timeout config (5min for streaming) — timeout is API-specific, not a security concern
- Status and doctor localhost health-check clients stay as plain reqwest::Client — SSRF protection is for outbound API calls only
- No `--insecure` CLI flag — keep security posture clean

### Claude's Discretion
- Exact integration point for passing SecurityConfig to AnthropicClient (constructor injection vs. separate builder method)
- How to thread the Arc<RwLock> vault values from credential vault to RedactingWriter at init
- Precise code locations for each metric recording call site in the agent loop
- Startup validation ordering and error message formatting

</decisions>

<specifics>
## Specific Ideas

- Startup health summary should be a single-glance diagnostic: "Security: OK | Redaction: OK | Metrics: WARN (disabled)"
- Per-model token labels enable future cost dashboards without code changes
- The dual-latency approach (API-only + end-to-end) lets operators distinguish provider slowness from internal processing overhead

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `build_secure_client()` in blufio-security/src/tls.rs — ready to replace plain reqwest::Client in AnthropicClient
- `RedactingWriter<W>` in blufio-security/src/redact.rs — wraps any Write, accepts Arc<RwLock<Vec<String>>> for vault values
- `record_message()`, `record_tokens()`, `record_error()`, `record_latency()`, `set_active_sessions()` in blufio-prometheus/src/recording.rs — all defined, none called at runtime yet
- `SsrfSafeResolver` in blufio-security/src/ssrf.rs — already integrated into build_secure_client

### Established Patterns
- Feature-gated compilation: `#[cfg(feature = "prometheus")]` pattern already used in serve.rs for Prometheus adapter
- PrometheusAdapter initialization in serve.rs:196 with warn-and-continue error handling
- SecurityConfig with `allowed_private_ips: Vec<String>` already supports SSRF allowlisting
- Credential vault already decrypts secrets at runtime — needs Arc sharing with RedactingWriter

### Integration Points
- `AnthropicClient::new()` in blufio-anthropic/src/client.rs:42 — currently builds own reqwest::Client (line 57-64)
- `init_tracing()` in blufio/src/serve.rs:616 — uses plain tracing_subscriber::fmt() writer
- Agent loop in serve.rs — where metric recording calls need to be placed
- `run_serve()` in serve.rs:78 — startup sequence where integration validation should happen

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 14-wire-cross-phase-integration*
*Context gathered: 2026-03-02*
