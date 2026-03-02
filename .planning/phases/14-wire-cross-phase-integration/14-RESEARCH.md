# Phase 14: Wire Cross-Phase Integration - Research

**Researched:** 2026-03-02
**Domain:** Cross-phase runtime integration wiring (security, observability, logging)
**Confidence:** HIGH

## Summary

Phase 14 wires three integration points that exist as implemented but unconnected code:
(1) `build_secure_client()` in blufio-security replaces the plain `reqwest::Client` in AnthropicClient,
(2) `RedactingWriter` in blufio-security wraps the tracing subscriber's writer in `init_tracing()`,
and (3) Prometheus recording functions from blufio-prometheus are called at runtime in the agent loop.

All three building blocks are already implemented and tested. The work is purely integration wiring --
no new algorithms, no new crates, no new dependencies. The risk is low because the interfaces are
known and the patterns are established. The main complexity is threading `SecurityConfig` into the
Anthropic client constructor and threading `Arc<RwLock<Vec<String>>>` vault values into the tracing
subscriber initialization.

**Primary recommendation:** Three plans in a single wave -- each integration point is independent and can be implemented in parallel, but they share a startup validation task that should be in the first plan.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Wire ALL existing recording functions: messages_total, tokens_total, errors_total, active_sessions, budget_remaining, response_latency, memory gauges
- Token metrics use per-model labels (e.g., claude-sonnet-4, claude-haiku) for cost distribution visibility
- Measure BOTH LLM API call latency AND full message round-trip latency (two separate histogram metrics)
- Error metrics labeled by error type matching BlufioError variants: 'provider', 'security', 'storage', 'agent'
- Redact Anthropic API keys (sk-ant-* pattern), Telegram bot tokens, and all credential vault stored values
- NOT redacting session IDs, user identifiers, or message content
- Feed vault values to RedactingWriter via shared Arc<RwLock<Vec<String>>>
- Redaction happens at the fmt writer level
- Replacement is uniform `[REDACTED]` with no type metadata leakage
- Secure TLS client failure: REFUSE TO START
- Redaction writer failure: REFUSE TO START
- Prometheus metrics failure: WARN AND CONTINUE
- Validate all three integration points together at startup
- Use existing `allowed_private_ips` config for SSRF allowlisting
- Anthropic client retains its own timeout config (5min for streaming)
- Status and doctor localhost health-check clients stay as plain reqwest::Client
- No `--insecure` CLI flag

### Claude's Discretion
- Exact integration point for passing SecurityConfig to AnthropicClient
- How to thread the Arc<RwLock> vault values from credential vault to RedactingWriter at init
- Precise code locations for each metric recording call site in the agent loop
- Startup validation ordering and error message formatting

### Deferred Ideas (OUT OF SCOPE)
None
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| SEC-03 | AES-256-GCM encrypted credential vault stores all API keys and bot tokens | Vault already exists; this phase wires vault values to RedactingWriter for runtime redaction |
| SEC-04 | Vault key derived from passphrase via Argon2id -- never stored on disk | Already implemented; no changes needed for Phase 14 |
| SEC-08 | Secrets redacted from all logs and persisted data before storage | Wire RedactingWriter into init_tracing() -- currently logs use plain fmt writer |
| SEC-09 | SSRF prevention (private IP blocking) enabled by default | Wire build_secure_client() with SsrfSafeResolver into AnthropicClient |
| COST-04 | Prometheus metrics endpoint exports token usage, latency percentiles, error rates, memory usage | Wire record_message/record_tokens/record_error/record_latency/set_active_sessions call sites |
| INFRA-05 | HTTP/WebSocket gateway for API access alongside channel messaging | Gateway already exists; /metrics endpoint already works -- Phase 14 makes metrics non-zero |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| blufio-security | local | TLS enforcement, SSRF, redaction | Already implemented in Phase 2 |
| blufio-prometheus | local | Prometheus recording functions | Already implemented in Phase 9 |
| tracing-subscriber | workspace | Logging infrastructure | Already in use |
| reqwest | workspace | HTTP client (TLS-capable) | Already in use everywhere |
| metrics-rs | workspace | Metrics facade | Already installed by PrometheusAdapter |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tracing_subscriber::fmt::MakeWriter | 0.3 | Custom writer for RedactingWriter integration | Wrapping fmt subscriber writer |

## Architecture Patterns

### Pattern 1: Constructor Injection for Secure Client
**What:** Pass a pre-built `reqwest::Client` (from `build_secure_client()`) into `AnthropicClient::new()` instead of building one internally.
**When to use:** When replacing the internal client construction.
**Approach:**
- Add an optional `client: Option<reqwest::Client>` parameter to `AnthropicClient::new()`
- If provided, use it (with added default headers); if None, build internally (backward compat for tests)
- `AnthropicProvider::new()` calls `build_secure_client(&config.security)` and passes the client
- The 5-minute streaming timeout is set on the secure client builder

### Pattern 2: MakeWriter Trait for RedactingWriter
**What:** Use tracing-subscriber's `MakeWriter` trait to integrate `RedactingWriter`.
**When to use:** When wrapping the tracing subscriber's writer.
**Approach:**
- Create a `RedactingMakeWriter` struct that implements `MakeWriter`
- It holds `Arc<RwLock<Vec<String>>>` for vault values
- Each `make_writer()` call returns a `RedactingWriter<std::io::Stdout>` (or stderr)
- Pass to `tracing_subscriber::fmt().with_writer(redacting_make_writer)`

### Pattern 3: Feature-Gated Metric Calls
**What:** Use `#[cfg(feature = "prometheus")]` guards around metric recording calls.
**When to use:** At every call site in the agent loop.
**Approach:**
- Already used in memory_monitor in serve.rs
- Apply the same pattern to record_message, record_tokens, record_error, etc.
- This is zero-cost when prometheus feature is disabled

### Anti-Patterns to Avoid
- **Passing config through many layers:** Don't thread SecurityConfig through blufio-agent -- keep the secure client construction in serve.rs/AnthropicProvider::new()
- **Global mutable state for vault values:** Use Arc<RwLock<Vec<String>>> properly, not a static global
- **Blocking on vault read in tracing writer:** The RwLock read in RedactingWriter::write() is synchronous -- fine for tracing which is also sync

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| TLS enforcement | Custom reqwest middleware | `build_secure_client()` | Already handles TLS 1.2+, SSRF resolver |
| Secret redaction | Custom regex in tracing layer | `RedactingWriter` | Already handles regex + exact match |
| Metric recording | Custom metric infrastructure | `blufio_prometheus::record_*()` | Already defines all counters and gauges |

## Common Pitfalls

### Pitfall 1: tracing_subscriber::init() Already Called
**What goes wrong:** `init_tracing()` currently calls `.init()` which installs a global subscriber. If called twice, it panics.
**Why it happens:** Forgetting that init is a one-time operation.
**How to avoid:** The existing `init_tracing()` is the only place -- just modify it in place.
**Warning signs:** "tracing subscriber already set" panic at startup.

### Pitfall 2: Timeout Lost When Replacing Client
**What goes wrong:** The current AnthropicClient has a 300-second timeout. Replacing with build_secure_client() might lose this.
**Why it happens:** build_secure_client() doesn't set a timeout.
**How to avoid:** Set timeout on the secure client builder, or set it after construction on the AnthropicClient.
**Warning signs:** Streaming requests that hang indefinitely.

### Pitfall 3: RedactingWriter Write Length Mismatch
**What goes wrong:** `RedactingWriter::write()` returns `buf.len()` (original length) even though the redacted output may differ in length.
**Why it happens:** This is intentional for tracing compatibility -- the caller expects the full buffer was consumed.
**How to avoid:** This is already correctly handled in the existing implementation. Don't change it.

### Pitfall 4: Circular Dependency blufio-anthropic -> blufio-security
**What goes wrong:** Adding blufio-security as a dependency of blufio-anthropic could create a cycle.
**Why it happens:** Dependency graph is acyclic by design.
**How to avoid:** Check: blufio-security depends on blufio-core and blufio-config. blufio-anthropic depends on blufio-core and blufio-config. No cycle -- safe to add blufio-security to blufio-anthropic.
**Alternative:** Pass pre-built client from serve.rs into AnthropicProvider::new() to avoid any new inter-crate dependency.

### Pitfall 5: Vault Values Not Yet Available at Tracing Init
**What goes wrong:** init_tracing() is called before vault startup check, so vault values aren't decrypted yet.
**Why it happens:** Tracing must be initialized early for all subsequent logging.
**How to avoid:** Initialize RedactingWriter with empty vault values, then populate via Arc<RwLock<Vec<String>>> when vault is unlocked later. The dynamic nature of the Arc<RwLock> handles this.

## Code Examples

### Secure Client Integration in AnthropicClient
```rust
// In AnthropicClient::new() -- accept optional pre-built client
pub fn new(
    api_key: String,
    api_version: String,
    model: String,
    secure_client: Option<reqwest::Client>,
) -> Result<Self, BlufioError> {
    let mut headers = HeaderMap::new();
    headers.insert("x-api-key", HeaderValue::from_str(&api_key)?);
    headers.insert("anthropic-version", HeaderValue::from_str(&api_version)?);
    headers.insert("content-type", HeaderValue::from_static("application/json"));

    let client = match secure_client {
        Some(c) => c,  // Already has TLS + SSRF protection
        None => reqwest::Client::builder()
            .default_headers(headers.clone())
            .timeout(Duration::from_secs(300))
            .build()?,
    };
    // ... rest unchanged
}
```

### RedactingWriter MakeWriter Integration
```rust
use std::sync::{Arc, RwLock};
use tracing_subscriber::fmt::MakeWriter;

struct RedactingMakeWriter {
    vault_values: Arc<RwLock<Vec<String>>>,
}

impl<'a> MakeWriter<'a> for RedactingMakeWriter {
    type Writer = RedactingWriter<std::io::Stdout>;
    fn make_writer(&'a self) -> Self::Writer {
        RedactingWriter::new(std::io::stdout(), self.vault_values.clone())
    }
}
```

### Metric Call Sites in Agent Loop
```rust
// In handle_inbound() after receiving a message:
#[cfg(feature = "prometheus")]
blufio_prometheus::record_message(&channel_name);

// In persist_response() after recording cost:
#[cfg(feature = "prometheus")]
if let Some(ref usage) = usage {
    blufio_prometheus::record_tokens(&model_for_cost, usage.input_tokens, usage.output_tokens);
}

// In handle_inbound() error path:
#[cfg(feature = "prometheus")]
blufio_prometheus::record_error("provider");
```

## Open Questions

1. **MakeWriter trait lifetime requirements**
   - What we know: tracing_subscriber's MakeWriter trait requires `for<'a> MakeWriter<'a>`. RedactingWriter wraps stdout which is `'static`.
   - What's unclear: Whether the borrow checker will accept the Arc<RwLock> pattern in MakeWriter. May need `impl MakeWriter for RedactingMakeWriter` without lifetime parameter.
   - Recommendation: Test the MakeWriter integration early. If lifetimes are problematic, use `tracing_subscriber::fmt().with_writer(move || RedactingWriter::new(...))` closure pattern.

## Sources

### Primary (HIGH confidence)
- Codebase inspection: All files read directly from the repository
- blufio-security/src/tls.rs -- build_secure_client() implementation
- blufio-security/src/redact.rs -- RedactingWriter implementation
- blufio-prometheus/src/recording.rs -- all recording functions
- blufio-anthropic/src/client.rs -- current reqwest::Client construction
- blufio/src/serve.rs -- init_tracing() and startup sequence

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - all code already exists in the repo
- Architecture: HIGH - integration patterns are straightforward wiring
- Pitfalls: HIGH - identified from reading actual code, not speculation

**Research date:** 2026-03-02
**Valid until:** 2026-04-02 (stable -- internal codebase)
